//! Admin order management (`admin_handler.go`, `UpdateOrderStatus`).

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use zs_domain::order::model::{Order, OrderStatus, keys};
use zs_domain::order::ports::{AdminOrderFilter, CancelReason};
use zs_domain::order::status::{can_complete_parent, is_transition_allowed};
use zs_domain::payment::model::{AdminPayment, PaymentRefs, admin_payment};
use zs_domain::{Error, Id, Result};
use zs_shared::page::Page;

use super::OrderService;

/// Admin list row (`AdminOrderListItem`).
#[derive(Debug, Clone, Serialize)]
pub struct AdminOrderRow {
    #[serde(flatten)]
    pub order: Order,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user_email: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user_display_name: String,
    /// LQA-I3: status of a purchase order that failed while the (child) order is still
    /// paid and undelivered (`rejected` / `manual_review` / `canceled`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub procurement_issue: String,
}

/// Admin detail (`AdminOrderDetail`).
#[derive(Debug, Clone, Serialize)]
pub struct AdminOrderDetail {
    #[serde(flatten)]
    pub order: Order,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user_email: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user_display_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub coupon_code: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub promotion_name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub payments: Vec<AdminPayment>,
}

impl OrderService {
    /// `GET /admin/orders`.
    pub async fn admin_orders(&self, filter: &AdminOrderFilter) -> Result<Page<AdminOrderRow>> {
        let page = self
            .deps
            .repo
            .list_admin(filter)
            .await
            .map_err(|e| e.or_internal(keys::ORDER_FETCH_FAILED))?;
        let mut orders = Vec::with_capacity(page.items.len());
        for order in page.items {
            orders.push(
                self.refresh(order)
                    .await
                    .map_err(|e| e.or_internal(keys::ORDER_UPDATE_FAILED))?,
            );
        }
        let ids: Vec<Id> = orders
            .iter()
            .map(|o| o.user_id)
            .filter(|id| *id > 0)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let users = self.deps.repo.users(&ids).await?;
        let issues = self.procurement_issues(&orders).await;
        Ok(Page {
            items: orders
                .into_iter()
                .map(|order| {
                    let user = users.get(&order.user_id);
                    AdminOrderRow {
                        user_email: user.map(|u| u.email.clone()).unwrap_or_default(),
                        user_display_name: user.map(|u| u.display_name.clone()).unwrap_or_default(),
                        procurement_issue: issues.get(&order.id).cloned().unwrap_or_default(),
                        order,
                    }
                })
                .collect(),
            total: page.total,
        })
    }

    /// Any order by id, refreshed (lazy expiry / refund sync).
    pub async fn admin_order(&self, id: Id) -> Result<Order> {
        if id <= 0 {
            return Err(Error::not_found(keys::ORDER_NOT_FOUND));
        }
        let order = self
            .deps
            .repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(keys::ORDER_FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        self.refresh(order)
            .await
            .map_err(|e| e.or_internal(keys::ORDER_UPDATE_FAILED))
    }

    /// LQA-I3: per listed order, the failed purchase of a still-paid order or child
    /// (best effort: a lookup failure only hides the marker).
    async fn procurement_issues(&self, orders: &[Order]) -> HashMap<Id, String> {
        let Some(integration) = &self.deps.integration else {
            return HashMap::new();
        };
        let mut owner: HashMap<Id, Id> = HashMap::new();
        for o in orders {
            if o.status == OrderStatus::Paid || o.status == OrderStatus::Fulfilling {
                owner.insert(o.id, o.id);
            }
            for c in &o.children {
                if c.status == OrderStatus::Paid || c.status == OrderStatus::Fulfilling {
                    owner.insert(c.id, o.id);
                }
            }
        }
        if owner.is_empty() {
            return HashMap::new();
        }
        let ids: Vec<Id> = owner.keys().copied().collect();
        match integration.procurement_issues(&ids).await {
            Ok(found) => found
                .into_iter()
                .filter_map(|(id, status)| owner.get(&id).map(|root| (*root, status)))
                .collect(),
            Err(error) => {
                tracing::warn!(%error, "procurement issue lookup failed");
                HashMap::new()
            }
        }
    }

    /// `GET /admin/orders/:id`: user, coupon, promotion names and payments (redacted, PAY-45).
    pub async fn admin_order_detail(&self, id: Id) -> Result<AdminOrderDetail> {
        let mut order = self.admin_order(id).await?;
        let (user_email, user_display_name) = if order.user_id > 0 {
            self.deps
                .repo
                .users(&[order.user_id])
                .await?
                .remove(&order.user_id)
                .map(|u| (u.email, u.display_name))
                .unwrap_or_default()
        } else {
            (String::new(), String::new())
        };
        let coupon_code = match order.coupon_id.filter(|id| *id > 0) {
            Some(id) => self.deps.repo.coupon_code(id).await?.unwrap_or_default(),
            None => String::new(),
        };
        let mut promotion_ids: Vec<Id> = order.promotion_id.into_iter().collect();
        promotion_ids.extend(order.items.iter().filter_map(|i| i.promotion_id));
        for child in &order.children {
            promotion_ids.extend(child.items.iter().filter_map(|i| i.promotion_id));
        }
        let names = self.deps.repo.promotion_names(&promotion_ids).await?;
        let promotion_name = order
            .promotion_id
            .and_then(|id| names.get(&id).cloned())
            .unwrap_or_default();
        for item in &mut order.items {
            if let Some(id) = item.promotion_id {
                item.promotion_name = names.get(&id).cloned().unwrap_or_default();
            }
        }
        for child in &mut order.children {
            for item in &mut child.items {
                if let Some(id) = item.promotion_id {
                    item.promotion_name = names.get(&id).cloned().unwrap_or_default();
                }
            }
        }
        let payments = self.deps.repo.payments_of(order.id).await?;
        let channel_ids: Vec<Id> = payments
            .iter()
            .map(|p| p.channel_id)
            .filter(|id| *id > 0)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let channels = self.deps.repo.channels(&channel_ids).await?;
        let refs = PaymentRefs::default();
        let payments = payments
            .iter()
            .map(|p| {
                let name = channels
                    .iter()
                    .find(|c| c.id == p.channel_id)
                    .map(|c| c.name.as_str())
                    .unwrap_or_default();
                admin_payment(p, name, &refs)
            })
            .collect();
        order.truncate_fulfillment_payload();
        Ok(AdminOrderDetail {
            order,
            user_email,
            user_display_name,
            coupon_code,
            promotion_name,
            payments,
        })
    }

    /// `PATCH /admin/orders/:id` (`UpdateOrderStatus`): `paid` is never accepted here —
    /// only verified payments or wallet debits pay an order. Likewise the refunded
    /// states: the original allowed them, which marked an order refunded without
    /// moving money or stock (live QA I-2) — refunds go through the refund endpoints.
    pub async fn admin_set_status(&self, id: Id, raw_status: &str) -> Result<Order> {
        let order = self.admin_order(id).await?;
        let target = OrderStatus::parse(raw_status)
            .ok_or_else(|| Error::bad_request(keys::ORDER_STATUS_INVALID))?;
        if order.status == target {
            return Ok(order);
        }
        if target == OrderStatus::Paid {
            return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
        }
        if matches!(
            target,
            OrderStatus::Refunded | OrderStatus::PartiallyRefunded
        ) {
            return Err(Error::bad_request(keys::ORDER_STATUS_REFUND_REQUIRED));
        }
        let now = self.deps.clock.now();
        let update_failed = |e: Error| e.or_internal(keys::ORDER_UPDATE_FAILED);
        let is_parent = order.parent_id.is_none() && !order.children.is_empty();
        if is_parent {
            match target {
                OrderStatus::Canceled => {
                    let canceled = self.deps.store.cancel(order.id, now).await.map_err(|e| {
                        if e.kind() == zs_domain::ErrorKind::Internal {
                            e.or_internal(keys::ORDER_UPDATE_FAILED)
                        } else {
                            Error::internal_msg(e.to_string())
                                .or_internal(keys::ORDER_UPDATE_FAILED)
                        }
                    })?;
                    self.after_cancel(&canceled, CancelReason::Admin).await;
                }
                OrderStatus::Completed => {
                    if !can_complete_parent(&order) {
                        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
                    }
                    self.deps
                        .store
                        .set_parent_status(order.id, target, now)
                        .await
                        .map_err(update_failed)?;
                    self.enqueue_status_email(order.id, target, None).await;
                }
                _ => return Err(Error::bad_request(keys::ORDER_STATUS_INVALID)),
            }
        } else {
            if !is_transition_allowed(order.status, target) {
                return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
            }
            if target == OrderStatus::Canceled {
                let canceled = self
                    .deps
                    .store
                    .cancel(order.id, now)
                    .await
                    .map_err(update_failed)?;
                self.after_cancel(&canceled, CancelReason::Admin).await;
            } else {
                self.deps
                    .store
                    .set_status(order.id, target, now)
                    .await
                    .map_err(update_failed)?;
            }
            match order.parent_id {
                Some(parent) => {
                    let status = self
                        .deps
                        .repo
                        .get(parent)
                        .await?
                        .map(|p| p.status)
                        .unwrap_or(target);
                    if status != OrderStatus::Canceled {
                        self.enqueue_status_email(parent, status, None).await;
                    }
                }
                None if target != OrderStatus::Canceled => {
                    self.enqueue_status_email(order.id, target, None).await;
                }
                None => {}
            }
        }
        if let Some(integration) = &self.deps.integration
            && let Err(error) = integration.order_status_changed(order.id).await
        {
            tracing::warn!(%error, order_id = order.id, "order_status_downstream_callback_failed");
        }
        let mut updated = self.admin_order(order.id).await?;
        updated.fill_items_from_children();
        Ok(updated)
    }
}
