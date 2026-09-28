//! mcy-shop OpenApi plugin provider facade (`POST /plugin/open-api/*`), the protocol
//! of acg-faka's "萌次元(V4.0)" store type and gmshop-edge (docs/protocol/third-party/
//! mcy-shop.md §2, reverse-engineered from those two clients).
//!
//! Wire rules: `Api-Id` = user id, `Api-Signature` = md5 over the form's top-level
//! scalar fields (arrays left out, mcy `Str::generateSignature`) + `&key=<app_key>`;
//! always HTTP 200, `{"code":200,"msg":"success","data":…}` / `{"code":0,"msg":…}`.
//!
//! Mapping: product → item (`id` = product id, `category` = `{name}`), SKU → `sku[]`
//! (`stock_price` = the caller's unit price). Only automatically delivered products
//! are offered: acg-faka stores the `trade` response `contents` as the final
//! delivery. `trade_no` is the idempotency key: a repeated `trade_no` answers the
//! original `contents` (gmshop-edge re-sends `trade` to reconcile).

use serde_json::{Value, json};
use zs_domain::Id;

use super::access::{AccessDenied, CompatAccess};
use super::desk::{DeliveryPolicy, DeskError, Offer, Purchase, SupplyDesk};
use super::form::{PhpForm, SignScope, signature_eq};
use super::sku_labels;
use crate::identity::rate_limit::RateLimiter;
use crate::integration::credential::Caller;

const POLICY: DeliveryPolicy = DeliveryPolicy::Synchronous;

/// `/plugin/open-api/…` endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    Connect,
    Items,
    Item,
    SkuStock,
    SkuState,
    Amount,
    Trade,
}

/// A raw request.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub api_id: &'a str,
    pub signature: &'a str,
    pub body: &'a [u8],
    pub client_ip: &'a str,
    pub origin: &'a str,
}

fn ok(data: Value) -> Value {
    json!({"code": 200, "msg": "success", "data": data})
}

fn fail(msg: &str) -> Value {
    json!({"code": 0, "msg": msg})
}

fn denied(d: AccessDenied) -> Value {
    fail(match d {
        AccessDenied::UnknownApp => "API-ID不存在或未开通对接",
        AccessDenied::BadSignature => "签名错误",
        AccessDenied::Disabled => "对接已关闭，请在供货站个人中心开启",
        AccessDenied::IpDenied => "当前IP不在对接白名单内",
        AccessDenied::Internal => "系统繁忙，请稍后重试",
    })
}

fn desk_fail(e: &DeskError) -> Value {
    fail(match e {
        DeskError::NotFound => "商品不存在",
        DeskError::Unavailable => "商品未开放对接",
        DeskError::QuantityLimit => "购买数量不在允许范围内",
        DeskError::OutOfStock => "库存不足",
        DeskError::InsufficientBalance => "余额不足",
        DeskError::Invalid(m) => m,
        DeskError::PriceInvalid => "商品价格异常，暂停对接",
        DeskError::OrderNotFound => "订单不存在",
        DeskError::OrderFailed(_) => "该订单未支付成功",
        DeskError::Internal => "系统繁忙，请稍后重试",
    })
}

fn positive_id(raw: &str) -> Option<Id> {
    raw.parse::<Id>().ok().filter(|id| *id > 0)
}

fn quantity(raw: &str) -> Option<i32> {
    if raw.is_empty() {
        return Some(1);
    }
    raw.parse::<i32>().ok().filter(|n| *n > 0)
}

/// Item object (`gm:acg.ts` schema + acg-faka `createV4Item`).
fn item_json(offer: &Offer, base: &str) -> Value {
    let skus: Vec<Value> = sku_labels(offer)
        .into_iter()
        .map(|(name, s)| {
            json!({
                "id": s.sku_id,
                "name": name,
                "stock_price": s.price.to_string(),
                "stock": s.stock,
            })
        })
        .collect();
    json!({
        "id": offer.product.id,
        "name": offer.title(),
        "introduce": offer.description(),
        "picture_url": offer.cover(base),
        "category": {"name": offer
            .product
            .category
            .as_ref()
            .map(|c| c.name.resolve(super::desk::TEXT_LOCALE).to_owned())
            .unwrap_or_default()},
        "widget": "[]",
        "sku": skus,
    })
}

/// The facade.
#[derive(Clone)]
pub struct McyOpenApi {
    desk: SupplyDesk,
    access: CompatAccess,
    limiter: RateLimiter,
}

impl std::fmt::Debug for McyOpenApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("McyOpenApi")
    }
}

impl McyOpenApi {
    pub fn new(desk: SupplyDesk, access: CompatAccess, limiter: RateLimiter) -> Self {
        Self {
            desk,
            access,
            limiter,
        }
    }

    /// Serves one request; the result is the JSON body (always HTTP 200).
    pub async fn serve(&self, endpoint: Endpoint, req: Request<'_>) -> Value {
        let limit_key = format!("mcy|{}|{}", req.client_ip, req.api_id.trim());
        if self.limiter.hit(&limit_key).is_err() {
            return fail("请求过于频繁，请稍后再试");
        }
        let form = PhpForm::parse(req.body);
        let verify = |app_key: &str| {
            signature_eq(
                &form.signature(SignScope::ScalarsOnly, app_key),
                req.signature.trim(),
            )
        };
        let caller = match self
            .access
            .authenticate(req.api_id, req.client_ip, verify)
            .await
        {
            Ok(c) => c,
            Err(d) => return denied(d),
        };
        let base = self.desk.base_url(req.origin).await;
        let r = match endpoint {
            Endpoint::Connect => self.connect(&caller).await,
            Endpoint::Items => self.items(&caller, &base).await,
            Endpoint::Item => self.item(&caller, &form, &base).await,
            Endpoint::SkuStock => self.sku_stock(&caller, &form).await,
            Endpoint::SkuState => self.sku_state(&caller, &form).await,
            Endpoint::Amount => self.amount(&caller, &form).await,
            Endpoint::Trade => self.trade(&caller, &form, req.client_ip).await,
        };
        r.unwrap_or_else(|v| v)
    }

    /// `connect`: `username` carries the site name (acg-faka shows it as the store
    /// name; the buyer's own account name / e-mail is not disclosed).
    async fn connect(&self, caller: &Caller) -> Result<Value, Value> {
        let site = self
            .desk
            .site()
            .await
            .map_err(|_| desk_fail(&DeskError::Internal))?;
        let balance = self
            .desk
            .balance(caller)
            .await
            .map_err(|_| desk_fail(&DeskError::Internal))?;
        let name = if site.site_name.trim().is_empty() {
            format!("user-{}", caller.user_id)
        } else {
            site.site_name.trim().to_owned()
        };
        Ok(ok(
            json!({"username": name, "balance": balance.to_string()}),
        ))
    }

    async fn items(&self, caller: &Caller, base: &str) -> Result<Value, Value> {
        let tree = self
            .desk
            .offers(caller, POLICY)
            .await
            .map_err(|_| desk_fail(&DeskError::Internal))?;
        let items: Vec<Value> = tree
            .iter()
            .flat_map(|c| {
                c.offers.iter().map(|o| {
                    let mut v = item_json(o, base);
                    v["category"] =
                        json!({"name": c.category.name.resolve(super::desk::TEXT_LOCALE)});
                    v
                })
            })
            .collect();
        Ok(ok(Value::Array(items)))
    }

    async fn item(&self, caller: &Caller, form: &PhpForm, base: &str) -> Result<Value, Value> {
        let id = positive_id(form.text("id")).ok_or_else(|| fail("商品不存在"))?;
        let offer = self
            .desk
            .offer(caller, id, POLICY)
            .await
            .map_err(|e| desk_fail(&e))?;
        Ok(ok(item_json(&offer, base)))
    }

    async fn sku_offer(&self, caller: &Caller, form: &PhpForm) -> Result<(Offer, Id), Value> {
        let sku_id = positive_id(form.text("sku_id")).ok_or_else(|| fail("SKU不存在"))?;
        let offer = self
            .desk
            .offer_of_sku(caller, sku_id, POLICY)
            .await
            .map_err(|e| desk_fail(&e))?;
        Ok((offer, sku_id))
    }

    async fn sku_stock(&self, caller: &Caller, form: &PhpForm) -> Result<Value, Value> {
        let (offer, sku_id) = self.sku_offer(caller, form).await?;
        let stock = offer.sku(sku_id).map_or(0, |s| s.stock);
        Ok(ok(json!({"stock": stock})))
    }

    /// `sku/state`: `false` for any business refusal (sold out, limits, delisted).
    async fn sku_state(&self, caller: &Caller, form: &PhpForm) -> Result<Value, Value> {
        let Some(qty) = quantity(form.text("quantity")) else {
            return Ok(ok(json!({"state": false})));
        };
        let state = match self.sku_offer(caller, form).await {
            Ok((offer, sku_id)) => SupplyDesk::check(&offer, sku_id, qty).is_ok(),
            Err(_) => false,
        };
        Ok(ok(json!({"state": state})))
    }

    async fn amount(&self, caller: &Caller, form: &PhpForm) -> Result<Value, Value> {
        let qty = quantity(form.text("quantity")).ok_or_else(|| fail("购买数量不正确"))?;
        let (offer, sku_id) = self.sku_offer(caller, form).await?;
        let quote = self
            .desk
            .quote(caller, &offer, sku_id, qty)
            .await
            .map_err(|e| desk_fail(&e))?;
        Ok(ok(json!({"amount": quote.total.to_string()})))
    }

    async fn trade(
        &self,
        caller: &Caller,
        form: &PhpForm,
        client_ip: &str,
    ) -> Result<Value, Value> {
        let trade_no = form.text("trade_no");
        if trade_no.is_empty() {
            return Err(fail("trade_no 不能为空"));
        }
        let order = match self
            .desk
            .resume(caller, trade_no)
            .await
            .map_err(|e| desk_fail(&e))?
        {
            Some(order) => order,
            None => {
                let qty = quantity(form.text("quantity")).ok_or_else(|| fail("购买数量不正确"))?;
                let sku_id = positive_id(form.text("sku_id")).ok_or_else(|| fail("SKU不存在"))?;
                self.desk
                    .place(
                        caller,
                        &Purchase {
                            sku_id,
                            quantity: qty,
                            request_no: trade_no.to_owned(),
                            client_ip: client_ip.to_owned(),
                        },
                        POLICY,
                    )
                    .await
                    .map_err(|e| desk_fail(&e))?
            }
        };
        let mut data = json!({
            "trade_no": trade_no,
            "order_no": order.order_no,
            "amount": order.amount.to_string(),
        });
        // Missing `contents` = still processing (gmshop-edge); never a placeholder.
        if let Some(contents) = &order.contents {
            data["contents"] = Value::String(contents.clone());
        }
        Ok(ok(data))
    }
}
