//! acg-faka (异次元发卡) "共享店铺" provider facade: `POST /shared/authentication/connect`
//! and `POST /shared/commodity/*` (docs/protocol/third-party/acg-faka.md §3, §7.2).
//!
//! Wire rules: form bodies signed with `md5` over the received fields
//! ([`super::form`], nested `sku[…]` included), always HTTP 200, success
//! `{"code":200,"msg":"success","data":…}` (`msg` absent on `items`, `item`, `stock`,
//! `valuation`, `draftCard`, `draft` like the original), failure `{"code":0,"msg":…}`.
//!
//! Mapping: our product → one acg commodity (`code` = product id); SKUs → `[category]`
//! "races" (a product whose only SKU has no spec values has none); `price` = retail,
//! `user_price` / `factory_price` / `[category_factory]` = the caller's price from the
//! storefront pricing engine. Only automatically delivered products are offered
//! ([`DeliveryPolicy::Synchronous`]): acg-faka stores the `trade` response `secret`
//! as the final delivery and never queries again.

use serde_json::{Map, Value, json};
use zs_domain::Id;

use super::access::{AccessDenied, CompatAccess};
use super::desk::{DeliveryPolicy, DeskError, DeskOrder, Offer, OfferSku, Purchase, SupplyDesk};
use super::form::{PhpForm, SignScope, signature_eq};
use super::{money_number, sku_labels};
use crate::identity::rate_limit::RateLimiter;
use crate::integration::credential::Caller;

/// Delivery text when the inline delivery did not complete (acg-faka's own wording).
const PENDING_TEXT: &str = "正在发货中，请耐心等待，如有疑问，请联系客服。";
/// Most categories acg-faka accepts in an `items` tree (`Store.php:254`).
const MAX_CATEGORIES: usize = 200;
/// Longest `shopName` acg-faka accepts.
const MAX_SHOP_NAME: usize = 128;
const POLICY: DeliveryPolicy = DeliveryPolicy::Synchronous;

/// `/shared/…` endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    Connect,
    Items,
    Item,
    Inventory,
    InventoryState,
    Stock,
    Valuation,
    Trade,
    Query,
    DraftCard,
    Draft,
}

/// A raw request.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub body: &'a [u8],
    pub client_ip: &'a str,
    /// Request origin (`scheme://host`), used for absolute URLs without a brand URL.
    pub origin: &'a str,
}

fn ok(data: Value) -> Value {
    json!({"code": 200, "msg": "success", "data": data})
}

/// Success without `msg` (endpoints whose original response has none).
fn ok_bare(data: Value) -> Value {
    json!({"code": 200, "data": data})
}

fn fail(msg: &str) -> Value {
    json!({"code": 0, "msg": msg})
}

fn denied(d: AccessDenied) -> Value {
    fail(match d {
        AccessDenied::UnknownApp => "商户ID不存在",
        AccessDenied::BadSignature => "密钥错误",
        AccessDenied::Disabled => "对接已关闭，请在供货站个人中心开启",
        AccessDenied::IpDenied => "当前IP不在对接白名单内",
        AccessDenied::Internal => "系统繁忙，请稍后重试",
    })
}

fn desk_fail(e: &DeskError) -> Value {
    match e {
        DeskError::NotFound => fail("商品不存在"),
        DeskError::Unavailable => fail("该商品未开放对接"),
        DeskError::QuantityLimit => fail("购买数量不在允许范围内"),
        DeskError::OutOfStock => fail("库存不足"),
        DeskError::InsufficientBalance => fail("余额不足"),
        DeskError::Invalid(m) => fail(m),
        DeskError::PriceInvalid => fail("商品价格异常，暂停对接"),
        DeskError::OrderNotFound => fail("订单不存在"),
        DeskError::OrderFailed(_) => fail("该订单未支付成功"),
        DeskError::Internal => fail("系统繁忙，请稍后重试"),
    }
}

/// Product id of a commodity code (our codes are canonical positive ids).
fn product_id(code: &str) -> Option<Id> {
    code.parse::<Id>()
        .ok()
        .filter(|id| *id > 0 && id.to_string() == code)
}

/// `num` / `quantity`: a positive integer (missing → 1).
fn quantity(raw: &str) -> Option<i32> {
    if raw.is_empty() {
        return Some(1);
    }
    raw.parse::<i32>().ok().filter(|n| *n > 0)
}

/// True when the product has `[category]` races (not a single default SKU).
fn is_category(offer: &Offer) -> bool {
    offer.default_sku().is_none()
}

/// The SKU named by `race` (products without races ignore it).
fn race_sku<'a>(offer: &'a Offer, race: &str) -> Result<&'a OfferSku, Value> {
    if let Some(only) = offer.default_sku() {
        return Ok(only);
    }
    if race.is_empty() {
        return Err(fail("请选择商品种类"));
    }
    sku_labels(offer)
        .into_iter()
        .find(|(label, _)| label == race)
        .map(|(_, s)| s)
        .ok_or_else(|| fail("商品种类不存在"))
}

/// `key=value` line of an INI section.
type IniRow = (String, String);

fn ini(sections: &[(&str, Vec<IniRow>)]) -> String {
    let mut out = String::new();
    for (name, rows) in sections {
        if rows.is_empty() {
            continue;
        }
        out.push('[');
        out.push_str(name);
        out.push_str("]\n");
        for (k, v) in rows {
            out.push_str(k);
            out.push('=');
            out.push_str(v);
            out.push('\n');
        }
    }
    out
}

/// `[category]` (retail) and `[category_factory]` (caller price) rows.
fn race_rows(offer: &Offer) -> (Vec<IniRow>, Vec<IniRow>) {
    if !is_category(offer) {
        return (Vec::new(), Vec::new());
    }
    let labels = sku_labels(offer);
    (
        labels
            .iter()
            .map(|(l, s)| (l.clone(), s.retail.to_string()))
            .collect(),
        labels
            .iter()
            .map(|(l, s)| (l.clone(), s.price.to_string()))
            .collect(),
    )
}

fn rows_object(rows: &[IniRow]) -> Value {
    Value::Object(
        rows.iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect::<Map<String, Value>>(),
    )
}

/// Lowest retail / caller price of the offer (the commodity's base prices).
fn base_prices(offer: &Offer) -> (zs_shared::money::Amount, zs_shared::money::Amount) {
    let retail = offer
        .skus
        .iter()
        .map(|s| s.retail)
        .min()
        .unwrap_or_default();
    let price = offer.skus.iter().map(|s| s.price).min().unwrap_or_default();
    (retail, price)
}

fn limit(v: i32) -> i64 {
    i64::from(v.max(0))
}

/// Commodity row of `items` (acg-faka `SharedPayload` whitelist).
fn commodity_row(offer: &Offer, base: &str) -> Value {
    let (retail, price) = base_prices(offer);
    let (category, _) = race_rows(offer);
    json!({
        "id": offer.product.id,
        "category_id": offer.product.category_id,
        "name": offer.title(),
        "description": offer.description(),
        "cover": offer.cover(base),
        "price": money_number(retail),
        "user_price": money_number(price),
        "status": 1,
        "code": offer.product.id.to_string(),
        "sort": offer.product.sort_order,
        "delivery_way": 0,
        "contact_type": 0,
        "password_status": 0,
        "coupon": 0,
        "seckill_status": 0,
        "seckill_start_time": null,
        "seckill_end_time": null,
        "draft_status": 0,
        "draft_premium": 0,
        "inventory_hidden": 0,
        "only_user": 0,
        "purchase_count": 0,
        "widget": "[]",
        "minimum": limit(offer.product.min_purchase_quantity),
        "maximum": limit(offer.product.max_purchase_quantity),
        "config": ini(&[("category", category)]),
        "stock": offer.stock(),
        "tags": offer.product.tags.join(","),
    })
}

/// Detail of `item` (3.1.2+ shape with the caller's `factory_price`, 3.6.5+).
fn commodity_detail(offer: &Offer, base: &str) -> Value {
    let (retail, price) = base_prices(offer);
    let (category, factory) = race_rows(offer);
    let mut config = Map::new();
    if !category.is_empty() {
        config.insert("category".into(), rows_object(&category));
        config.insert("category_factory".into(), rows_object(&factory));
    }
    let factory_price = if is_category(offer) {
        zs_shared::money::Amount::ZERO
    } else {
        price
    };
    json!({
        "id": offer.product.id,
        "name": offer.title(),
        "description": offer.description(),
        "only_user": 0,
        "purchase_count": 0,
        "category_id": offer.product.category_id,
        "cover": offer.cover(base),
        "price": money_number(retail),
        "user_price": money_number(price),
        "factory_price": money_number(factory_price),
        "status": 1,
        "owner": null,
        "delivery_way": 0,
        "contact_type": 0,
        "password_status": 0,
        "coupon": 0,
        "seckill_status": 0,
        "seckill_start_time": null,
        "seckill_end_time": null,
        "draft_status": 0,
        "draft_premium": 0,
        "inventory_hidden": 0,
        "widget": [],
        "minimum": limit(offer.product.min_purchase_quantity),
        "maximum": limit(offer.product.max_purchase_quantity),
        "config": Value::Object(config),
        "stock": offer.stock(),
        "code": offer.product.id.to_string(),
        "tags": offer.product.tags,
        "order_sold": 0,
        "service_url": "",
        "service_qq": "",
        "share_url": "",
        "login": false,
        "trade_captcha": 0,
    })
}

/// The facade.
#[derive(Clone)]
pub struct AcgFaka {
    desk: SupplyDesk,
    access: CompatAccess,
    limiter: RateLimiter,
}

impl std::fmt::Debug for AcgFaka {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AcgFaka")
    }
}

impl AcgFaka {
    pub fn new(desk: SupplyDesk, access: CompatAccess, limiter: RateLimiter) -> Self {
        Self {
            desk,
            access,
            limiter,
        }
    }

    /// Serves one request; the result is the JSON body (always HTTP 200).
    pub async fn serve(&self, endpoint: Endpoint, req: Request<'_>) -> Value {
        let form = PhpForm::parse(req.body);
        // PRV-01: `app_id` must be a scalar (`app_id[]=…` is refused, not a 500).
        let Some(app_id) = form.scalar("app_id") else {
            return denied(AccessDenied::UnknownApp);
        };
        let limit_key = format!("acg|{}|{}", req.client_ip, app_id.trim());
        if self.limiter.hit(&limit_key).is_err() {
            return fail("请求过于频繁，请稍后再试");
        }
        let Some(sign) = form.scalar("sign").map(str::to_owned) else {
            return denied(AccessDenied::BadSignature);
        };
        let verify =
            |app_key: &str| signature_eq(&form.signature(SignScope::WithArrays, app_key), &sign);
        let caller = match self
            .access
            .authenticate(app_id, req.client_ip, verify)
            .await
        {
            Ok(c) => c,
            Err(d) => return denied(d),
        };
        let r = match endpoint {
            Endpoint::Connect => self.connect(&caller, req.origin).await,
            Endpoint::Items => self.items(&caller, req.origin).await,
            Endpoint::Item => self.item(&caller, &form, req.origin).await,
            Endpoint::Inventory => self.inventory(&caller, &form).await,
            Endpoint::InventoryState => self.inventory_state(&caller, &form).await,
            Endpoint::Stock => self.stock(&caller, &form).await,
            Endpoint::Valuation => self.valuation(&caller, &form).await,
            Endpoint::Trade => self.trade(&caller, &form, &req).await,
            Endpoint::Query => self.query(&caller, &form).await,
            // PRV-04: no pre-selectable cards; filters are never evaluated.
            Endpoint::DraftCard => Ok(ok_bare(json!({"list": [], "total": 0}))),
            Endpoint::Draft => Ok(ok_bare(json!({"draft_premium": 0}))),
        };
        r.unwrap_or_else(|v| v)
    }

    async fn connect(&self, caller: &Caller, origin: &str) -> Result<Value, Value> {
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
        let mut name: String = site
            .site_name
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_SHOP_NAME)
            .collect();
        if name.trim().is_empty() {
            name = self.desk.base_url(origin).await;
        }
        Ok(ok(
            json!({"shopName": name.trim(), "balance": money_number(balance)}),
        ))
    }

    async fn items(&self, caller: &Caller, origin: &str) -> Result<Value, Value> {
        let base = self.desk.base_url(origin).await;
        let tree = self
            .desk
            .offers(caller, POLICY)
            .await
            .map_err(|_| desk_fail(&DeskError::Internal))?;
        let data: Vec<Value> = tree
            .iter()
            .take(MAX_CATEGORIES)
            .map(|c| {
                json!({
                    "id": c.category.id,
                    "name": c.category.name.resolve(super::desk::TEXT_LOCALE),
                    "sort": c.category.sort_order,
                    "icon": c.category.icon,
                    "status": 1,
                    "pid": 0,
                    "children": c.offers.iter().map(|o| commodity_row(o, &base)).collect::<Vec<_>>(),
                })
            })
            .collect();
        Ok(ok_bare(Value::Array(data)))
    }

    async fn offer_by_code(&self, caller: &Caller, code: &str) -> Result<Offer, Value> {
        let id = product_id(code).ok_or_else(|| fail("商品不存在"))?;
        self.desk
            .offer(caller, id, POLICY)
            .await
            .map_err(|e| desk_fail(&e))
    }

    /// `item(code)`; a caller sending only `sharedCode` is acg-faka ≤ 3.1.1 and gets
    /// the category tree of that product.
    async fn item(&self, caller: &Caller, form: &PhpForm, origin: &str) -> Result<Value, Value> {
        let base = self.desk.base_url(origin).await;
        let code = form.text("code");
        if code.is_empty() && !form.text("sharedCode").is_empty() {
            let offer = self.offer_by_code(caller, form.text("sharedCode")).await?;
            let category = json!({
                "id": offer.product.category_id,
                "name": "",
                "sort": 0,
                "icon": "",
                "status": 1,
                "pid": 0,
                "children": [commodity_row(&offer, &base)],
            });
            return Ok(ok_bare(json!([category])));
        }
        let offer = self.offer_by_code(caller, code).await?;
        Ok(ok_bare(commodity_detail(&offer, &base)))
    }

    /// `inventory(sharedCode, race)`: stock + the caller's price (all versions).
    async fn inventory(&self, caller: &Caller, form: &PhpForm) -> Result<Value, Value> {
        let code = if form.text("sharedCode").is_empty() {
            form.text("code")
        } else {
            form.text("sharedCode")
        };
        let offer = self.offer_by_code(caller, code).await?;
        let race = form.text("race");
        let sku = if race.is_empty() {
            offer.default_sku().or_else(|| offer.skus.first())
        } else {
            Some(race_sku(&offer, race)?)
        };
        let (retail, price) = base_prices(&offer);
        let (category, factory) = race_rows(&offer);
        let factory_price = if is_category(&offer) {
            zs_shared::money::Amount::ZERO
        } else {
            price
        };
        Ok(ok(json!({
            "count": sku.map_or(0, |s| s.stock),
            "delivery_way": 0,
            "draft_status": 0,
            "price": money_number(retail),
            "user_price": money_number(price),
            "config": ini(&[("category", category), ("category_factory", factory)]),
            "factory_price": money_number(factory_price),
            "is_category": is_category(&offer),
        })))
    }

    fn no_card(form: &PhpForm) -> Result<(), Value> {
        match form.text("card_id") {
            "" | "0" => Ok(()),
            _ => Err(fail("该商品不支持预选卡密")),
        }
    }

    /// `inventoryState(shared_code, card_id, num, race)`.
    async fn inventory_state(&self, caller: &Caller, form: &PhpForm) -> Result<Value, Value> {
        Self::no_card(form)?;
        let offer = self.offer_by_code(caller, form.text("shared_code")).await?;
        let sku = race_sku(&offer, form.text("race"))?;
        let num = quantity(form.text("num")).ok_or_else(|| fail("购买数量不正确"))?;
        SupplyDesk::check(&offer, sku.sku_id, num).map_err(|e| desk_fail(&e))?;
        Ok(ok(json!([])))
    }

    /// `stock(code, race, sku[…])`: stock as a string.
    async fn stock(&self, caller: &Caller, form: &PhpForm) -> Result<Value, Value> {
        let offer = self.offer_by_code(caller, form.text("code")).await?;
        let race = form.text("race");
        let stock = if race.is_empty() && is_category(&offer) {
            offer.stock()
        } else {
            race_sku(&offer, race)?.stock
        };
        Ok(ok_bare(json!({"stock": stock.to_string()})))
    }

    /// `valuation(code, num, race, sku[…], card_id)`: total for the caller.
    async fn valuation(&self, caller: &Caller, form: &PhpForm) -> Result<Value, Value> {
        Self::no_card(form)?;
        let offer = self.offer_by_code(caller, form.text("code")).await?;
        let sku = race_sku(&offer, form.text("race"))?;
        let num = quantity(form.text("num")).ok_or_else(|| fail("购买数量不正确"))?;
        let quote = self
            .desk
            .quote(caller, &offer, sku.sku_id, num)
            .await
            .map_err(|e| desk_fail(&e))?;
        let currency = self.desk.currency().await;
        Ok(ok_bare(
            json!({"price": quote.total.to_string(), "currency_code": currency}),
        ))
    }

    fn trade_data(&self, order: &DeskOrder, base: &str) -> Value {
        json!({
            "url": format!("{base}/orders/{}", order.order_no),
            "amount": order.amount.to_string(),
            "tradeNo": order.order_no,
            "secret": order.contents.clone().unwrap_or_else(|| PENDING_TEXT.to_owned()),
            "leave_message": null,
            "stock": order.stock_left.unwrap_or(0).to_string(),
        })
    }

    /// `trade(shared_code, num, race, request_no, …)`: paid from the caller's wallet,
    /// card secrets returned synchronously. A repeated `request_no` answers the
    /// original order instead of acg-faka's "The request ID already exists" (never a
    /// second charge, UPS-10 / PRV-03).
    async fn trade(
        &self,
        caller: &Caller,
        form: &PhpForm,
        req: &Request<'_>,
    ) -> Result<Value, Value> {
        Self::no_card(form)?;
        let request_no = form.text("request_no");
        if request_no.is_empty() {
            return Err(fail("request_no 不能为空"));
        }
        let base = self.desk.base_url(req.origin).await;
        // A replay needs no catalog lookup (the product may be sold out by now).
        if let Some(mut order) = self
            .desk
            .resume(caller, request_no)
            .await
            .map_err(|e| desk_fail(&e))?
        {
            // PRV-09: `stock` is always a string in the original response; a replay
            // reports the current stock of the addressed race (0 when gone).
            if order.stock_left.is_none() {
                let stock = match self.offer_by_code(caller, form.text("shared_code")).await {
                    Ok(offer) => race_sku(&offer, form.text("race")).map_or(0, |s| s.stock),
                    Err(_) => 0,
                };
                order.stock_left = Some(stock);
            }
            return Ok(ok(self.trade_data(&order, &base)));
        }
        let offer = self.offer_by_code(caller, form.text("shared_code")).await?;
        let sku = race_sku(&offer, form.text("race"))?;
        let num = quantity(form.text("num")).ok_or_else(|| fail("购买数量不正确"))?;
        let order = self
            .desk
            .place(
                caller,
                &Purchase {
                    sku_id: sku.sku_id,
                    quantity: num,
                    request_no: request_no.to_owned(),
                    client_ip: req.client_ip.to_owned(),
                },
                POLICY,
            )
            .await
            .map_err(|e| desk_fail(&e))?;
        Ok(ok(self.trade_data(&order, &base)))
    }

    /// `query(tradeNo)`: own orders only; `status` 1 = paid.
    async fn query(&self, caller: &Caller, form: &PhpForm) -> Result<Value, Value> {
        let trade_no = form.text("tradeNo");
        let not_found = || fail("订单不存在");
        let id = self
            .access
            .order_id_by_no(trade_no)
            .await
            .map_err(|_| desk_fail(&DeskError::Internal))?
            .ok_or_else(not_found)?;
        let order = self.desk.order(caller, id).await.map_err(|e| match e {
            DeskError::OrderNotFound => not_found(),
            e => desk_fail(&e),
        })?;
        let secret = match (&order.contents, order.paid) {
            (Some(c), _) => c.clone(),
            (None, true) => PENDING_TEXT.to_owned(),
            (None, false) => String::new(),
        };
        Ok(ok(json!({
            "secret": secret,
            "widget": null,
            "status": i32::from(order.paid),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_rendering() {
        let s = ini(&[
            (
                "category",
                vec![
                    ("月卡".into(), "10.00".into()),
                    ("季卡".into(), "28.00".into()),
                ],
            ),
            ("category_factory", vec![]),
        ]);
        assert_eq!(s, "[category]\n月卡=10.00\n季卡=28.00\n");
        assert_eq!(ini(&[("category", vec![])]), "");
    }

    #[test]
    fn codes_and_quantities() {
        assert_eq!(product_id("12"), Some(12));
        for bad in ["", "0", "012", "-1", "1 ", "abc"] {
            assert_eq!(product_id(bad), None, "{bad}");
        }
        assert_eq!(quantity(""), Some(1));
        assert_eq!(quantity("3"), Some(3));
        assert_eq!(quantity("0"), None);
        assert_eq!(quantity("x"), None);
    }
}
