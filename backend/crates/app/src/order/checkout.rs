//! Price preview and order creation (`PreviewOrder`, `CreateOrder`, `CreateGuestOrder`).

use std::collections::{HashMap, HashSet};

use chrono::Duration;
use serde_json::{Map, Value};
use zs_domain::catalog::manual_form::validate_and_normalize;
use zs_domain::catalog::product::manual_sku_available;
use zs_domain::catalog::product::{
    FulfillmentType, Product, ProductSku, PurchaseType, should_enforce_manual_sku_stock,
    validate_purchase_quantity,
};
use zs_domain::marketing::coupon::{CouponBuyer, keys as coupon_keys};
use zs_domain::order::guest::{validate_guest_email, validate_guest_password};
use zs_domain::order::model::{
    JsonMap, Order, OrderItem, OrderStatus, child_order_no, item_key, keys, new_order_no,
};
use zs_domain::order::ports::{NewOrder, Reservation, RiskGate};
use zs_domain::order::pricing::{
    CouponContext, MemberContext, PricingInput, PricingLine, PricingResult, apply_reseller_prices,
    child_profit, price_order,
};
use zs_domain::order::risk::{RiskInput, RiskItem, RiskPrep};
use zs_domain::order::view::{LineDisplay, Preview};
use zs_domain::queue::{NewJob, kinds};
use zs_domain::reseller::pricing::{
    OrderLine, OrderPricingContext, SettingIndex, price_order_line,
};
use zs_domain::reseller::tenant::ResellerTenant;
use zs_domain::{Error, ErrorKind, Id, Result};
use zs_shared::money::Amount;

use super::OrderService;

/// Locked unit prices per `(product_id, sku_id)`.
pub type PriceCaps = HashMap<(Id, Id), Amount>;

/// A requested order line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemRequest {
    pub product_id: Id,
    pub sku_id: Id,
    pub quantity: i32,
}

/// Guest identity of a guest order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuestInfo {
    pub email: String,
    pub password: String,
    pub locale: String,
}

/// A preview / create request (user or guest).
#[derive(Debug, Clone, Default)]
pub struct CheckoutRequest {
    /// `0` for guests.
    pub user_id: Id,
    pub guest: Option<GuestInfo>,
    pub tenant: ResellerTenant,
    pub items: Vec<ItemRequest>,
    pub coupon_code: String,
    pub affiliate_code: String,
    pub affiliate_visitor_key: String,
    pub client_ip: String,
    /// `{"<product_id>:<sku_id>" | "<product_id>": {field: value}}`.
    pub manual_form_data: JsonMap,
    /// Downstream API orders skip risk control entirely.
    pub skip_risk: bool,
    /// Channel / Bot orders skip the IP dimensions.
    pub skip_ip_risk: bool,
}

impl CheckoutRequest {
    fn is_guest(&self) -> bool {
        self.user_id == 0
    }

    fn risk_input(&self) -> RiskInput {
        RiskInput {
            user_id: self.user_id,
            client_ip: self.client_ip.trim().to_owned(),
            is_guest: self.is_guest(),
            skip_ip: self.skip_ip_risk,
            items: self
                .items
                .iter()
                .map(|i| RiskItem {
                    product_id: i.product_id,
                    quantity: i.quantity,
                })
                .collect(),
        }
    }
}

/// One line resolved against the catalog.
#[derive(Debug, Clone)]
struct LineMeta {
    product: Product,
    sku: ProductSku,
    fulfillment: FulfillmentType,
    schema: JsonMap,
    submission: JsonMap,
}

/// Result of [`OrderService::build`].
#[derive(Debug, Clone)]
struct Built {
    result: PricingResult,
    lines: Vec<LineMeta>,
    currency: String,
    member_level_id: Option<Id>,
    reseller: Option<OrderPricingContext>,
}

/// Merges duplicate `(product, sku)` lines (`mergeCreateOrderItems`).
fn merge_items(items: &[ItemRequest]) -> Result<Vec<ItemRequest>> {
    let mut merged: Vec<ItemRequest> = Vec::new();
    let mut index: HashMap<(Id, Id), usize> = HashMap::new();
    for item in items {
        if item.product_id <= 0 || item.quantity <= 0 {
            return Err(Error::bad_request(keys::ORDER_ITEM_INVALID));
        }
        match index.get(&(item.product_id, item.sku_id)) {
            Some(&i) => merged[i].quantity += item.quantity,
            None => {
                index.insert((item.product_id, item.sku_id), merged.len());
                merged.push(*item);
            }
        }
    }
    Ok(merged)
}

/// Picks the ordered SKU among the product's active SKUs (`resolveProductOrderSKU`).
fn resolve_sku(product: &Product, sku_id: Id) -> Result<ProductSku> {
    let invalid = || Error::bad_request(keys::ORDER_ITEM_INVALID);
    let active: Vec<&ProductSku> = product.skus.iter().filter(|s| s.is_active).collect();
    if sku_id > 0 {
        return active
            .into_iter()
            .find(|s| s.id == sku_id)
            .cloned()
            .ok_or_else(invalid);
    }
    match active.as_slice() {
        [only] => Ok((*only).clone()),
        _ => Err(invalid()),
    }
}

/// Manual form answers of a line: `"product:sku"` first, then the legacy `"product"` key.
fn form_submission(data: &JsonMap, product_id: Id, sku_id: Id) -> JsonMap {
    for key in [item_key(product_id, sku_id), product_id.to_string()] {
        if let Some(v) = data.get(&key) {
            return match v {
                Value::Object(m) => m.clone(),
                _ => Map::new(),
            };
        }
    }
    Map::new()
}

fn first_image(images: &[String]) -> String {
    images
        .iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .unwrap_or_default()
        .to_owned()
}

fn sku_snapshot(sku: &ProductSku, product: &Product) -> JsonMap {
    let mut m = Map::new();
    m.insert("sku_id".into(), Value::from(sku.id));
    m.insert("sku_code".into(), Value::String(sku.sku_code.clone()));
    m.insert("spec_values".into(), Value::Object(sku.spec_values.clone()));
    m.insert("image".into(), Value::String(first_image(&product.images)));
    m
}

fn queue_unavailable() -> Error {
    Error::new(ErrorKind::Internal, keys::QUEUE_UNAVAILABLE)
}

impl OrderService {
    /// Resolves, validates and prices the lines (`buildOrderResult` + reseller pricing).
    async fn build(
        &self,
        req: &CheckoutRequest,
        skip_manual_form: bool,
        caps: &PriceCaps,
    ) -> Result<Built> {
        if req.items.is_empty() {
            return Err(Error::bad_request(keys::ORDER_ITEM_INVALID));
        }
        if let Some(guest) = &req.guest {
            if guest.email.trim().is_empty() {
                return Err(Error::bad_request(keys::GUEST_EMAIL_REQUIRED));
            }
            validate_guest_password(&guest.password)?;
        }
        let reseller = req.tenant.is_reseller();
        if reseller && !req.coupon_code.trim().is_empty() {
            return Err(Error::bad_request(keys::RESELLER_COUPON_NOT_ALLOWED));
        }
        let items = merge_items(&req.items)?;
        let currency = self.site_currency().await;
        let now = self.deps.clock.now();
        let product_ids: Vec<Id> = items
            .iter()
            .map(|i| i.product_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let mut member_level_id = None;
        let mut member = None;
        if !reseller && req.user_id > 0 {
            let level_id = self.deps.repo.member_level_of(req.user_id).await?;
            if level_id > 0 {
                member_level_id = Some(level_id);
                if let Some(level) = self.deps.levels.get(level_id).await? {
                    let prices = self
                        .deps
                        .levels
                        .list_prices_for_level(level_id, &product_ids)
                        .await?;
                    member = Some(MemberContext { level, prices });
                }
            }
        }

        let mut metas = Vec::with_capacity(items.len());
        let mut lines = Vec::with_capacity(items.len());
        for item in &items {
            let product = self
                .deps
                .catalog
                .orderable_product(item.product_id)
                .await?
                .ok_or_else(|| Error::bad_request(keys::PRODUCT_NOT_AVAILABLE))?;
            validate_purchase_quantity(
                product.min_purchase_quantity,
                product.max_purchase_quantity,
                item.quantity,
            )?;
            if req.is_guest() && product.purchase_type == PurchaseType::Member {
                return Err(Error::bad_request(keys::PRODUCT_PURCHASE_NOT_ALLOWED));
            }
            let sku = resolve_sku(&product, item.sku_id)?;
            let fulfillment = product.fulfillment_type;
            if fulfillment == FulfillmentType::Manual
                && should_enforce_manual_sku_stock(&product, &sku)
                && manual_sku_available(&sku) < item.quantity
            {
                return Err(Error::bad_request(keys::MANUAL_STOCK_INSUFFICIENT));
            }
            if fulfillment == FulfillmentType::Upstream
                && let Some(integration) = &self.deps.integration
            {
                integration
                    .ensure_upstream_stock(sku.id, item.quantity)
                    .await?;
            }
            let (schema, submission) = if !skip_manual_form
                && (fulfillment == FulfillmentType::Manual
                    || (fulfillment == FulfillmentType::Upstream
                        && !product.manual_form_schema.is_empty()))
            {
                let answers = form_submission(&req.manual_form_data, product.id, sku.id);
                validate_and_normalize(&product.manual_form_schema, &answers)?
            } else {
                (Map::new(), Map::new())
            };
            let promotions = if reseller {
                Vec::new()
            } else {
                self.deps.promotions.list_effective(product.id, now).await?
            };
            lines.push(PricingLine {
                product_id: product.id,
                sku_id: sku.id,
                sku_code: sku.sku_code.clone(),
                quantity: item.quantity,
                base_price: sku.price_amount,
                promotions,
                wholesale_tiers: product.wholesale_prices.clone(),
                price_cap: caps.get(&(product.id, sku.id)).copied(),
            });
            metas.push(LineMeta {
                product,
                sku,
                fulfillment,
                schema,
                submission,
            });
        }

        let coupon = if !reseller && !req.coupon_code.trim().is_empty() {
            let coupon = self
                .deps
                .coupons
                .get_by_code(req.coupon_code.trim())
                .await?
                .ok_or_else(|| Error::bad_request(coupon_keys::NOT_FOUND))?;
            let used_by_user = if coupon.per_user_limit > 0 && req.user_id > 0 {
                self.deps
                    .coupons
                    .count_user_usages(coupon.id, req.user_id)
                    .await?
            } else {
                0
            };
            Some(CouponContext {
                coupon,
                buyer: CouponBuyer {
                    user_id: req.user_id,
                    is_guest: req.is_guest(),
                    member_level_id: member_level_id.unwrap_or(0),
                    used_by_user,
                },
            })
        } else {
            None
        };

        let mut result = price_order(&PricingInput {
            lines,
            reseller,
            member,
            coupon,
            now,
        })?;
        result.member_level_id = if reseller { None } else { member_level_id };

        let reseller_ctx = if reseller {
            Some(
                self.reseller_pricing(req, &metas, &currency, &mut result)
                    .await?,
            )
        } else {
            None
        };
        Ok(Built {
            result,
            lines: metas,
            currency,
            member_level_id: if reseller { None } else { member_level_id },
            reseller: reseller_ctx,
        })
    }

    /// Reseller site prices (`ApplyToOrderBuildResult`): hidden items are rejected, every
    /// discount is replaced by the reseller unit price, self-dealing blocks the profit.
    async fn reseller_pricing(
        &self,
        req: &CheckoutRequest,
        metas: &[LineMeta],
        currency: &str,
        result: &mut PricingResult,
    ) -> Result<OrderPricingContext> {
        let reseller_id = req
            .tenant
            .reseller_id
            .ok_or_else(|| Error::bad_request(zs_domain::reseller::keys::PRODUCT_NOT_LISTED))?;
        let pricing = &self.deps.reseller_pricing;
        let profile = pricing.active_profile(reseller_id).await?;
        let product_ids: Vec<Id> = metas.iter().map(|m| m.product.id).collect();
        let sku_ids: Vec<Id> = metas.iter().map(|m| m.sku.id).collect();
        let settings = pricing
            .repo
            .settings_for_pricing(reseller_id, &product_ids, &sku_ids)
            .await?;
        let index = SettingIndex::new(&settings);
        let mut ctx = OrderPricingContext::new(
            reseller_id,
            &req.tenant.snapshot_domain(),
            currency,
            profile.user_id,
            req.user_id,
        );
        for (meta, line) in metas.iter().zip(&result.lines) {
            ctx.push(price_order_line(
                &profile,
                &index,
                &OrderLine {
                    product_id: meta.product.id,
                    sku_id: meta.sku.id,
                    quantity: line.quantity,
                    base_unit: meta.sku.price_amount,
                    cost: meta.sku.cost_price_amount,
                },
            )?);
        }
        let related = if req.user_id > 0 && req.user_id != profile.user_id {
            pricing
                .repo
                .is_related_account(reseller_id, req.user_id)
                .await?
        } else {
            false
        };
        ctx.apply_self_dealing_risk(profile.user_id, related);
        apply_reseller_prices(result, &ctx.items)?;
        Ok(ctx)
    }

    /// `POST /orders/preview` and `/guest/orders/preview`: risk precheck without consuming
    /// the rate limit; manual forms are not required (PRC-04).
    pub async fn preview(&self, req: &CheckoutRequest) -> Result<Preview> {
        if !req.skip_risk {
            self.check_risk(&req.risk_input(), false).await?;
        }
        let built = self.build(req, true, &PriceCaps::new()).await?;
        let display: Vec<LineDisplay> = built
            .lines
            .iter()
            .map(|m| LineDisplay {
                title: m.product.title.clone(),
                sku_snapshot: sku_snapshot(&m.sku, &m.product),
                tags: m.product.tags.clone(),
                fulfillment_type: m.fulfillment.as_str().to_owned(),
            })
            .collect();
        Ok(Preview::new(&built.currency, &built.result, &display))
    }

    /// Creates an order (`createOrder`) and schedules its timeout cancellation.
    pub async fn create_order(&self, req: &CheckoutRequest) -> Result<Order> {
        self.create_order_with(req, None).await
    }

    /// [`Self::create_order`] that also records the downstream reference of an API order
    /// in the creation transaction (UPS-10).
    pub(crate) async fn create_order_with(
        &self,
        req: &CheckoutRequest,
        downstream_ref: Option<zs_domain::integration::downstream::NewOrderRef>,
    ) -> Result<Order> {
        self.create_order_capped(req, downstream_ref, &PriceCaps::new())
            .await
    }

    /// [`Self::create_order_with`] with locked unit prices per `(product, sku)` (API
    /// quotes).
    pub(crate) async fn create_order_capped(
        &self,
        req: &CheckoutRequest,
        downstream_ref: Option<zs_domain::integration::downstream::NewOrderRef>,
        caps: &PriceCaps,
    ) -> Result<Order> {
        let mut req = req.clone();
        let guest = match &req.guest {
            Some(g) => Some(GuestInfo {
                email: validate_guest_email(&g.email)?,
                password: validate_guest_password(&g.password)?,
                locale: g.locale.trim().to_owned(),
            }),
            None if req.user_id <= 0 => return Err(Error::bad_request(keys::ORDER_ITEM_INVALID)),
            None => None,
        };
        req.guest.clone_from(&guest);
        let risk_input = req.risk_input();
        let prep: Option<RiskPrep> = if req.skip_risk {
            None
        } else {
            Some(self.check_risk(&risk_input, true).await?)
        };
        let built = self.build(&req, false, caps).await?;

        let wallet = self.wallet_setting().await;
        if wallet.wallet_only_payment {
            if req.is_guest() {
                return Err(Error::bad_request(keys::WALLET_ONLY_PAYMENT_REQUIRED));
            }
            let balance = self.deps.wallet.balance(req.user_id).await?;
            if balance < built.result.total_amount {
                return Err(Error::from(
                    zs_domain::wallet::LedgerError::InsufficientBalance,
                ));
            }
        }

        let (affiliate_profile_id, affiliate_code) = if built.reseller.is_some() {
            (None, String::new())
        } else {
            self.deps
                .affiliate
                .resolve_snapshot(req.user_id, &req.affiliate_code, &req.affiliate_visitor_key)
                .await?
        };

        let setting = self.order_setting().await;
        let mut expire_minutes = setting.payment_expire_minutes;
        if let Some(p) = prep.as_ref().filter(|p| p.payment_expire_minutes > 0) {
            expire_minutes = p.payment_expire_minutes;
        }
        let now = self.deps.clock.now();
        let expires_at = now + Duration::minutes(expire_minutes);
        let risk_ip = prep
            .as_ref()
            .map(|p| p.risk_ip.clone())
            .unwrap_or_else(|| zs_domain::order::risk::normalize_risk_ip(&req.client_ip));
        let new = self.new_order(
            &req,
            guest.as_ref(),
            &built,
            affiliate_profile_id,
            &affiliate_code,
            &risk_ip,
            expires_at,
            now,
        );
        let new = NewOrder {
            risk: prep.map(|prep| RiskGate {
                prep,
                input: risk_input,
            }),
            downstream_ref,
            ..new
        };
        let mut order = self
            .deps
            .store
            .create(&new, now)
            .await
            .map_err(|e| e.or_internal(keys::ORDER_CREATE_FAILED))?;

        let job = NewJob::new(
            kinds::ORDER_TIMEOUT_CANCEL,
            serde_json::json!({"order_id": order.id}),
        )?
        .at(expires_at);
        if let Err(error) = self.deps.queue.enqueue(job).await {
            tracing::error!(%error, order_id = order.id, order_no = %order.order_no, "order_enqueue_timeout_cancel_failed");
            if let Err(cancel) = self
                .deps
                .store
                .cancel(order.id, self.deps.clock.now())
                .await
            {
                tracing::error!(error = %cancel, order_id = order.id, "order_timeout_rollback_cancel_failed");
            }
            return Err(queue_unavailable());
        }
        order.fill_items_from_children();
        Ok(order)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "plain assembly of the order rows"
    )]
    fn new_order(
        &self,
        req: &CheckoutRequest,
        guest: Option<&GuestInfo>,
        built: &Built,
        affiliate_profile_id: Option<Id>,
        affiliate_code: &str,
        risk_ip: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> NewOrder {
        let r = &built.result;
        let order_no = new_order_no(now);
        let (reseller_id, reseller_domain, profit, eligible) = match &built.reseller {
            Some(ctx) => (
                Some(ctx.reseller_id),
                ctx.domain.clone(),
                Amount::new(ctx.effective_profit),
                ctx.profit_eligible,
            ),
            None => (None, String::new(), Amount::ZERO, false),
        };
        let coupon_id = r.applied_coupon.as_ref().map(|c| c.id);
        let base = |no: String| Order {
            id: 0,
            order_no: no,
            parent_id: None,
            user_id: req.user_id,
            guest_email: guest.map(|g| g.email.clone()).unwrap_or_default(),
            guest_password: guest.map(|g| g.password.clone()).unwrap_or_default(),
            guest_locale: guest.map(|g| g.locale.clone()).unwrap_or_default(),
            status: OrderStatus::PendingPayment,
            currency: built.currency.clone(),
            original_amount: Amount::ZERO,
            discount_amount: Amount::ZERO,
            member_discount_amount: Amount::ZERO,
            promotion_discount_amount: Amount::ZERO,
            wholesale_discount_amount: Amount::ZERO,
            total_amount: Amount::ZERO,
            wallet_paid_amount: Amount::ZERO,
            online_paid_amount: Amount::ZERO,
            refunded_amount: Amount::ZERO,
            member_level_id: None,
            coupon_id: None,
            promotion_id: None,
            affiliate_profile_id,
            affiliate_code: affiliate_code.to_owned(),
            reseller_id,
            reseller_domain: reseller_domain.clone(),
            reseller_profit_amount: Amount::ZERO,
            client_ip: req.client_ip.trim().to_owned(),
            risk_ip: risk_ip.to_owned(),
            expires_at: Some(expires_at),
            paid_at: None,
            canceled_at: None,
            created_at: now,
            updated_at: now,
            items: Vec::new(),
            fulfillment: None,
            children: Vec::new(),
        };
        let mut parent = Order {
            original_amount: r.original_amount,
            discount_amount: r.discount_amount,
            member_discount_amount: r.member_discount_amount,
            promotion_discount_amount: r.promotion_discount_amount,
            wholesale_discount_amount: r.wholesale_discount_amount,
            total_amount: r.total_amount,
            online_paid_amount: r.total_amount,
            member_level_id: built.member_level_id,
            coupon_id,
            promotion_id: r.promotion_id,
            reseller_profit_amount: profit,
            ..base(order_no.clone())
        };
        let mut reservations = Vec::with_capacity(r.lines.len());
        for (idx, (line, meta)) in r.lines.iter().zip(&built.lines).enumerate() {
            let payable = line.payable();
            let pricing_item = built.reseller.as_ref().and_then(|c| c.items.get(idx));
            let mut child = Order {
                original_amount: line.total_price,
                discount_amount: line.coupon_discount,
                member_discount_amount: line.member_discount,
                promotion_discount_amount: line.promotion_discount,
                wholesale_discount_amount: line.wholesale_discount,
                total_amount: payable,
                online_paid_amount: payable,
                coupon_id: coupon_id.filter(|_| line.coupon_discount.is_positive()),
                promotion_id: line.promotion_id,
                reseller_profit_amount: child_profit(pricing_item, eligible),
                ..base(child_order_no(&order_no, idx + 1))
            };
            child.items.push(OrderItem {
                id: 0,
                order_id: 0,
                product_id: meta.product.id,
                sku_id: meta.sku.id,
                title: meta.product.title.clone(),
                sku_snapshot: sku_snapshot(&meta.sku, &meta.product),
                tags: meta.product.tags.clone(),
                original_unit_price: line.original_unit_price,
                unit_price: line.unit_price,
                cost_price: meta.sku.cost_price_amount,
                quantity: line.quantity,
                original_total_price: line.original_total_price,
                total_price: line.total_price,
                coupon_discount: line.coupon_discount,
                member_discount: line.member_discount,
                promotion_discount: line.promotion_discount,
                wholesale_discount: line.wholesale_discount,
                promotion_id: line.promotion_id,
                promotion_name: String::new(),
                fulfillment_type: meta.fulfillment.as_str().to_owned(),
                manual_form_schema: meta.schema.clone(),
                manual_form_submission: meta.submission.clone(),
                instructions: meta.product.instructions.clone(),
                created_at: now,
                updated_at: now,
            });
            reservations.push(match meta.fulfillment {
                FulfillmentType::Auto => Reservation::Secrets {
                    product_id: meta.product.id,
                    sku_id: meta.sku.id,
                    quantity: u64::try_from(line.quantity).unwrap_or(0),
                },
                FulfillmentType::Manual
                    if should_enforce_manual_sku_stock(&meta.product, &meta.sku) =>
                {
                    Reservation::ManualSku {
                        sku_id: meta.sku.id,
                        quantity: line.quantity,
                    }
                }
                _ => Reservation::None,
            });
            parent.children.push(child);
        }
        NewOrder {
            order: parent,
            reservations,
            coupon: r.applied_coupon.as_ref().map(|c| (c.id, r.discount_amount)),
            reseller: built.reseller.clone(),
            risk: None,
            downstream_ref: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_duplicate_lines() {
        let items = [
            ItemRequest {
                product_id: 1,
                sku_id: 2,
                quantity: 1,
            },
            ItemRequest {
                product_id: 1,
                sku_id: 2,
                quantity: 2,
            },
            ItemRequest {
                product_id: 1,
                sku_id: 3,
                quantity: 1,
            },
        ];
        let merged = merge_items(&items).unwrap_or_default();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].quantity, 3);
        assert!(
            merge_items(&[ItemRequest {
                product_id: 1,
                sku_id: 0,
                quantity: 0
            }])
            .is_err()
        );
    }

    #[test]
    fn form_submission_prefers_item_key() {
        let data: JsonMap = serde_json::json!({"1:2": {"a": 1}, "1": {"a": 2}})
            .as_object()
            .cloned()
            .unwrap_or_default();
        assert_eq!(form_submission(&data, 1, 2)["a"], 1);
        assert_eq!(form_submission(&data, 1, 3)["a"], 2);
        assert!(form_submission(&data, 9, 0).is_empty());
    }
}
