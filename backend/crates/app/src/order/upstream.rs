//! Ports the order group provides to the integration group: API orders of downstream
//! shops ([`UpstreamOrdering`], `POST /upstream/orders`) and the order-side effects of
//! procurement ([`ProcurementLifecycle`]).

use async_trait::async_trait;
use serde_json::{Map, Value};
use zs_domain::catalog::manual_form;
use zs_domain::catalog::product::keys as product_keys;
use zs_domain::integration::downstream::NewOrderRef;
use zs_domain::integration::hooks::{FailureRefund, ProcurementLifecycle, UpstreamDelivery};
use zs_domain::integration::keys as integration_keys;
use zs_domain::integration::protocol::RemoteFulfillment;
use zs_domain::integration::supplier::{
    LinePrice, PlaceOutcome, PlaceUpstreamLines, PlaceUpstreamOrder, PriceLine,
    UpstreamOrderDetail, UpstreamOrderError, UpstreamOrderItem, UpstreamOrderLine,
    UpstreamOrderSummary, UpstreamOrderView, UpstreamOrdering,
};
use zs_domain::order::model::{
    JsonMap, Order, OrderStatus, fulfillment_status, fulfillment_type, item_key, keys,
};
use zs_domain::order::ports::{CancelReason, RefundRequest};
use zs_domain::reseller::tenant::ResellerTenant;
use zs_domain::wallet::model::keys as wallet_keys;
use zs_domain::{Error, ErrorKind, Id, Result};

use super::OrderService;
use super::checkout::{CheckoutRequest, ItemRequest};
use super::payment::PayRequest;
use crate::integration::card_converter::CardConverterService;

/// The order group's implementation of the integration ports.
#[derive(Debug, Clone)]
pub struct OrderIntegrationPorts(pub OrderService, pub Option<CardConverterService>);

/// Maps a checkout error to the protocol's error codes (`mapOrderError`).
fn place_error(e: Error) -> UpstreamOrderError {
    let key = e.key();
    if key == keys::DOWNSTREAM_ORDER_DUPLICATE {
        return UpstreamOrderError::DuplicateDownstreamNo;
    }
    if e.kind() == ErrorKind::Internal {
        return UpstreamOrderError::Internal(e);
    }
    match key {
        k if k == keys::CARD_SECRET_INSUFFICIENT
            || k == keys::MANUAL_STOCK_INSUFFICIENT
            || k == product_keys::MANUAL_STOCK_INSUFFICIENT
            || k == integration_keys::UPSTREAM_STOCK_INSUFFICIENT =>
        {
            UpstreamOrderError::InsufficientStock
        }
        k if k == wallet_keys::PAYMENT_AMOUNT_MISMATCH => UpstreamOrderError::InsufficientBalance,
        k if k == keys::PRODUCT_NOT_AVAILABLE => UpstreamOrderError::ProductUnavailable,
        k if k == keys::ORDER_ITEM_INVALID => UpstreamOrderError::InvalidItem,
        k if [
            manual_form::REQUIRED_MISSING,
            manual_form::FIELD_INVALID,
            manual_form::TYPE_INVALID,
            manual_form::OPTION_INVALID,
        ]
        .contains(&k) =>
        {
            UpstreamOrderError::ManualFormInvalid(e.to_string())
        }
        _ => UpstreamOrderError::Internal(e),
    }
}

/// Upstream items are shown as manual to buyers (`MaskUpstreamFulfillmentType`, UPS-19).
fn masked(kind: &str) -> String {
    if kind.trim() == fulfillment_type::UPSTREAM {
        fulfillment_type::MANUAL.to_owned()
    } else {
        kind.to_owned()
    }
}

fn summary(order: &Order, status: OrderStatus) -> UpstreamOrderSummary {
    UpstreamOrderSummary {
        order_id: order.id,
        order_no: order.order_no.clone(),
        status: status.as_str().to_owned(),
        amount: order.total_amount,
        currency: order.currency.clone(),
    }
}

fn refund_json(idx: usize, r: &zs_domain::order::model::RefundRecord) -> JsonMap {
    let mut m = Map::new();
    // The internal refund id is not exposed: records are numbered.
    m.insert("id".into(), Value::from(idx + 1));
    m.insert("user_id".into(), Value::from(r.user_id));
    m.insert("guest_email".into(), Value::String(r.guest_email.clone()));
    m.insert("order_id".into(), Value::from(r.order_id));
    m.insert("type".into(), Value::String(r.kind.clone()));
    m.insert("amount".into(), Value::String(r.amount.to_string()));
    m.insert("currency".into(), Value::String(r.currency.clone()));
    m.insert("remark".into(), Value::String(r.remark.clone()));
    m.insert(
        "created_at".into(),
        serde_json::to_value(r.created_at).unwrap_or(Value::Null),
    );
    m.insert(
        "updated_at".into(),
        serde_json::to_value(r.updated_at).unwrap_or(Value::Null),
    );
    m
}

impl OrderIntegrationPorts {
    async fn convert_delivery(
        &self,
        order: &Order,
        delivery: &UpstreamDelivery,
    ) -> Result<UpstreamDelivery> {
        if let Some(integration) = &self.0.deps.integration {
            return integration.convert_delivery(order, delivery).await;
        }
        let Some(converters) = &self.1 else {
            return Ok(delivery.clone());
        };
        let config = self
            .0
            .deps
            .settings
            .get(zs_domain::settings::keys::SITE_CONFIG)
            .await?
            .unwrap_or_default();
        let brand = config.get("brand").cloned().unwrap_or_default();
        let domain = if order.reseller_domain.trim().is_empty() {
            brand
                .get("site_url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        } else {
            order.reseller_domain.clone()
        };
        let name = brand
            .get("site_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        converters
            .convert_delivery(
                order,
                delivery,
                serde_json::json!({"domain":domain,"name":name}),
            )
            .await
    }

    /// A root order of the buyer (`GetOrderByUser`).
    async fn owned(&self, user_id: Id, order_id: Id) -> Result<Option<Order>> {
        if user_id <= 0 || order_id <= 0 {
            return Ok(None);
        }
        let Some(order) = self.0.deps.repo.get(order_id).await? else {
            return Ok(None);
        };
        if order.user_id != user_id || order.parent_id.is_some() {
            return Ok(None);
        }
        Ok(Some(order))
    }

    async fn cancel_unpaid(&self, order_id: Id) {
        match self
            .0
            .deps
            .store
            .cancel(order_id, self.0.deps.clock.now())
            .await
        {
            Ok(canceled) => self.0.after_cancel(&canceled, CancelReason::User).await,
            Err(error) => tracing::warn!(%error, order_id, "upstream_cancel_unpaid_order_failed"),
        }
    }
}

fn delivered(f: Option<&zs_domain::order::model::Fulfillment>) -> Option<RemoteFulfillment> {
    f.filter(|f| f.status == fulfillment_status::DELIVERED)
        .map(|f| RemoteFulfillment {
            kind: masked(&f.kind),
            status: f.status.clone(),
            payload: f.payload.clone(),
            delivery_data: Some(f.logistics.clone()),
            delivered_at: f.delivered_at,
        })
}

impl OrderIntegrationPorts {
    /// Pays a freshly created API order from the buyer's wallet; a failed payment
    /// cancels it (stock released).
    async fn pay_or_cancel(&self, order: Order, client_ip: &str) -> PlaceOutcome {
        let paid = self
            .0
            .pay(&PayRequest {
                order_id: order.id,
                channel_id: 0,
                channel_type: String::new(),
                use_balance: true,
                client_ip: client_ip.to_owned(),
                tenant: ResellerTenant::default(),
                scheme: "https".into(),
            })
            .await;
        match paid {
            Ok(view) if view.order_paid => PlaceOutcome::Placed(summary(&order, OrderStatus::Paid)),
            Ok(_) => {
                self.cancel_unpaid(order.id).await;
                PlaceOutcome::PaymentFailed {
                    order_id: order.id,
                    order_no: order.order_no,
                    message: "wallet balance is insufficient".into(),
                }
            }
            Err(error) => {
                tracing::error!(%error, order_id = order.id, "upstream_auto_wallet_pay_failed");
                self.cancel_unpaid(order.id).await;
                PlaceOutcome::PaymentFailed {
                    order_id: order.id,
                    order_no: order.order_no,
                    message: error.to_string(),
                }
            }
        }
    }
}

#[async_trait]
impl UpstreamOrdering for OrderIntegrationPorts {
    async fn price_lines(
        &self,
        user_id: Id,
        lines: &[PriceLine],
    ) -> std::result::Result<Vec<LinePrice>, UpstreamOrderError> {
        let checkout = CheckoutRequest {
            user_id,
            items: lines
                .iter()
                .map(|l| ItemRequest {
                    product_id: l.product_id,
                    sku_id: l.sku_id,
                    quantity: l.quantity,
                })
                .collect(),
            skip_risk: true,
            skip_ip_risk: true,
            ..CheckoutRequest::default()
        };
        let preview = self.0.preview(&checkout).await.map_err(place_error)?;
        Ok(preview
            .items
            .iter()
            .map(|i| LinePrice {
                product_id: i.product_id,
                sku_id: i.sku_id,
                quantity: i.quantity,
                unit_price: i.unit_price,
                total_price: i.total_price,
            })
            .collect())
    }

    async fn place_lines(
        &self,
        req: &PlaceUpstreamLines,
    ) -> std::result::Result<PlaceOutcome, UpstreamOrderError> {
        if req.user_id <= 0 || req.lines.is_empty() {
            return Err(UpstreamOrderError::InvalidItem);
        }
        let mut manual_form_data = JsonMap::new();
        let mut caps = super::checkout::PriceCaps::new();
        for line in &req.lines {
            if line.product_id <= 0 || line.quantity <= 0 {
                return Err(UpstreamOrderError::InvalidItem);
            }
            if let Some(data) = &line.manual_form_data {
                manual_form_data.insert(
                    item_key(line.product_id, line.sku_id),
                    Value::Object(data.clone()),
                );
            }
            if let Some(cap) = line.price_cap {
                caps.insert((line.product_id, line.sku_id), cap);
            }
        }
        let checkout = CheckoutRequest {
            user_id: req.user_id,
            items: req
                .lines
                .iter()
                .map(|l| ItemRequest {
                    product_id: l.product_id,
                    sku_id: l.sku_id,
                    quantity: l.quantity,
                })
                .collect(),
            client_ip: req.client_ip.clone(),
            manual_form_data,
            skip_risk: true,
            skip_ip_risk: true,
            ..CheckoutRequest::default()
        };
        let downstream_ref = NewOrderRef {
            order_id: 0,
            api_credential_id: req.credential_id,
            downstream_order_no: req.downstream_order_no.clone(),
            // Push events replace dujiao-style callbacks for these orders.
            callback_url: String::new(),
            trace_id: req.trace_id.clone(),
        };
        let order = self
            .0
            .create_order_capped(&checkout, Some(downstream_ref), &caps)
            .await
            .map_err(place_error)?;
        Ok(self.pay_or_cancel(order, &req.client_ip).await)
    }

    async fn detail(&self, user_id: Id, order_id: Id) -> Result<Option<UpstreamOrderDetail>> {
        let Some(order) = self.owned(user_id, order_id).await? else {
            return Ok(None);
        };
        let line = |o: &Order, i: &zs_domain::order::model::OrderItem| UpstreamOrderLine {
            product_id: i.product_id,
            sku_id: i.sku_id,
            quantity: i.quantity,
            unit_price: i.unit_price,
            total_price: i.total_price,
            status: o.status.as_str().to_owned(),
            fulfillment: delivered(o.fulfillment.as_ref()),
        };
        let lines = if order.children.is_empty() {
            order.items.iter().map(|i| line(&order, i)).collect()
        } else {
            order
                .children
                .iter()
                .flat_map(|c| c.items.iter().map(move |i| (c, i)))
                .map(|(c, i)| line(c, i))
                .collect()
        };
        Ok(Some(UpstreamOrderDetail {
            order_id: order.id,
            order_no: order.order_no.clone(),
            user_id: order.user_id,
            status: order.status.as_str().to_owned(),
            total: order.total_amount,
            currency: order.currency.clone(),
            created_at: order.created_at,
            lines,
        }))
    }

    async fn place(
        &self,
        req: &PlaceUpstreamOrder,
    ) -> std::result::Result<PlaceOutcome, UpstreamOrderError> {
        if req.user_id <= 0 || req.product_id <= 0 || req.quantity <= 0 {
            return Err(UpstreamOrderError::InvalidItem);
        }
        let product = self
            .0
            .deps
            .catalog
            .orderable_product(req.product_id)
            .await?
            .ok_or(UpstreamOrderError::ProductUnavailable)?;
        if req.sku_id > 0
            && !product
                .skus
                .iter()
                .any(|s| s.id == req.sku_id && s.is_active)
        {
            return Err(UpstreamOrderError::SkuUnavailable);
        }
        let mut manual_form_data = JsonMap::new();
        if let Some(data) = &req.manual_form_data {
            manual_form_data.insert(
                item_key(req.product_id, req.sku_id),
                Value::Object(data.clone()),
            );
        }
        // API orders skip risk control: the caller is authenticated by signature and pays
        // from its wallet right away.
        let checkout = CheckoutRequest {
            user_id: req.user_id,
            guest: None,
            tenant: ResellerTenant::default(),
            items: vec![ItemRequest {
                product_id: req.product_id,
                sku_id: req.sku_id,
                quantity: req.quantity,
            }],
            coupon_code: String::new(),
            affiliate_code: String::new(),
            affiliate_visitor_key: String::new(),
            client_ip: req.client_ip.clone(),
            manual_form_data,
            skip_risk: true,
            skip_ip_risk: true,
        };
        let downstream_ref = NewOrderRef {
            order_id: 0,
            api_credential_id: req.credential_id,
            downstream_order_no: req.downstream_order_no.clone(),
            callback_url: req.callback_url.clone(),
            trace_id: req.trace_id.clone(),
        };
        let order = self
            .0
            .create_order_with(&checkout, Some(downstream_ref))
            .await
            .map_err(place_error)?;

        Ok(self.pay_or_cancel(order, &req.client_ip).await)
    }

    async fn get(&self, user_id: Id, order_id: Id) -> Result<Option<UpstreamOrderView>> {
        let Some(order) = self.owned(user_id, order_id).await? else {
            return Ok(None);
        };
        let mut ids = vec![order.id];
        ids.extend(order.children.iter().map(|c| c.id));
        let mut records = self.0.deps.repo.refunds_of(&ids).await?;
        records.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        let fulfillment = order
            .fulfillment
            .as_ref()
            .or_else(|| order.children.iter().find_map(|c| c.fulfillment.as_ref()))
            .filter(|f| f.status == fulfillment_status::DELIVERED)
            .map(|f| RemoteFulfillment {
                kind: masked(&f.kind),
                status: f.status.clone(),
                payload: f.payload.clone(),
                delivery_data: Some(f.logistics.clone()),
                delivered_at: f.delivered_at,
            });
        let mut items_source = order.clone();
        items_source.fill_items_from_children();
        let items = items_source
            .items
            .iter()
            .map(|i| UpstreamOrderItem {
                product_id: i.product_id,
                sku_id: i.sku_id,
                title: i.title.clone(),
                quantity: i.quantity,
                original_unit_price: i.original_unit_price,
                unit_price: i.unit_price,
                original_total_price: i.original_total_price,
                total_price: i.total_price,
                fulfillment_type: masked(&i.fulfillment_type),
            })
            .collect();
        Ok(Some(UpstreamOrderView {
            order_id: order.id,
            order_no: order.order_no.clone(),
            status: order.status.as_str().to_owned(),
            amount: order.total_amount,
            refunded_amount: order.refunded_amount,
            currency: order.currency.clone(),
            refund_records: records
                .iter()
                .enumerate()
                .map(|(i, r)| refund_json(i, r))
                .collect(),
            fulfillment,
            items,
        }))
    }

    async fn cancel(
        &self,
        user_id: Id,
        order_id: Id,
    ) -> std::result::Result<UpstreamOrderSummary, UpstreamOrderError> {
        let order = self
            .owned(user_id, order_id)
            .await?
            .ok_or(UpstreamOrderError::NotFound)?;
        if order.status != OrderStatus::PendingPayment {
            return Err(UpstreamOrderError::CancelNotAllowed);
        }
        match self
            .0
            .deps
            .store
            .cancel(order.id, self.0.deps.clock.now())
            .await
        {
            Ok(canceled) => {
                self.0.after_cancel(&canceled, CancelReason::User).await;
                Ok(summary(&canceled, OrderStatus::Canceled))
            }
            Err(e) if e.key() == keys::ORDER_CANCEL_NOT_ALLOWED => {
                Err(UpstreamOrderError::CancelNotAllowed)
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Err(UpstreamOrderError::NotFound),
            Err(e) => Err(UpstreamOrderError::Internal(e)),
        }
    }

    async fn deliver_now(&self, order_id: Id) -> Result<()> {
        let Some(order) = self.0.deps.repo.get(order_id).await? else {
            return Ok(());
        };
        if order.children.is_empty() {
            return self.0.auto_fulfill(order.id).await;
        }
        // Paid orders are split per line: the children carry the fulfillments.
        for child in &order.children {
            self.0.auto_fulfill(child.id).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl ProcurementLifecycle for OrderIntegrationPorts {
    async fn mark_fulfilling(&self, order_id: Id) -> Result<()> {
        self.0
            .deps
            .store
            .upstream_fulfilling(order_id, self.0.deps.clock.now())
            .await?;
        Ok(())
    }

    async fn deliver_upstream(&self, order_id: Id, delivery: &UpstreamDelivery) -> Result<()> {
        let order = self
            .0
            .deps
            .repo
            .get(order_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        let converted = self.convert_delivery(&order, delivery).await?;
        let done = self
            .0
            .deps
            .store
            .upstream_deliver(order_id, &converted, self.0.deps.clock.now())
            .await?;
        if let Some((notify_id, status)) = done.and_then(|d| d.notify) {
            self.0.enqueue_status_email(notify_id, status, None).await;
        }
        Ok(())
    }

    async fn rollback_failed(&self, order_id: Id) -> Result<()> {
        self.0
            .deps
            .store
            .upstream_rollback(order_id, self.0.deps.clock.now())
            .await?;
        Ok(())
    }

    async fn refund_failed(&self, order_id: Id, reason: &str) -> Result<FailureRefund> {
        let Some(order) = self.0.deps.repo.get(order_id).await? else {
            return Ok(FailureRefund::None);
        };
        let left = order.total_amount - order.refunded_amount;
        if order.paid_at.is_none() || !left.is_positive() {
            return Ok(FailureRefund::None);
        }
        let to_wallet = order.user_id > 0;
        let done = self
            .0
            .deps
            .store
            .refund(&RefundRequest {
                order_id,
                amount: left,
                remark: reason.to_owned(),
                to_wallet,
                payment_fee_refunded: false,
                // The buyer never received the goods: no refund window.
                max_refund_days: 0,
                reseller_confirm_days: self.0.deps.reseller_confirm_days,
                now: self.0.deps.clock.now(),
            })
            .await?;
        self.0.after_refund(&done).await;
        Ok(if to_wallet {
            FailureRefund::Wallet(left)
        } else {
            FailureRefund::Manual(left)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UPS-10: the unique index race becomes the duplicate outcome; stock and balance
    /// failures map to their protocol codes.
    #[test]
    fn ups10_place_error_mapping() {
        assert!(matches!(
            place_error(Error::bad_request(keys::DOWNSTREAM_ORDER_DUPLICATE)),
            UpstreamOrderError::DuplicateDownstreamNo
        ));
        assert!(matches!(
            place_error(Error::bad_request(keys::CARD_SECRET_INSUFFICIENT)),
            UpstreamOrderError::InsufficientStock
        ));
        assert!(matches!(
            place_error(Error::bad_request(wallet_keys::PAYMENT_AMOUNT_MISMATCH)),
            UpstreamOrderError::InsufficientBalance
        ));
        assert!(matches!(
            place_error(Error::bad_request(manual_form::REQUIRED_MISSING)),
            UpstreamOrderError::ManualFormInvalid(_)
        ));
        assert!(matches!(
            place_error(Error::internal_msg("boom")),
            UpstreamOrderError::Internal(_)
        ));
    }

    /// UPS-19: buyers never see the `upstream` type.
    #[test]
    fn ups19_masks_upstream_type() {
        assert_eq!(masked("upstream"), "manual");
        assert_eq!(masked("auto"), "auto");
    }
}
