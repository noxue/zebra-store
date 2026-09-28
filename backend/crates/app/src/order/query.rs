//! Storefront order reads, cancellation and the timeout job (`order_service_query.go`,
//! `order_service_child.go`).

use std::collections::{BTreeMap, HashSet};

use zs_domain::order::guest::{hash_credential, normalize_email};
use zs_domain::order::model::{Order, OrderStatus, keys};
use zs_domain::order::ports::{CancelReason, Owner, Scope};
use zs_domain::order::status::{calc_parent_status, expected_refund_status};
use zs_domain::order::view::{OrderDetail, RefundView};
use zs_domain::payment::eligibility::{decode_channel_ids, product_channel_intersection};
use zs_domain::reseller::tenant::ResellerTenant;
use zs_domain::{Error, Id, Result};
use zs_shared::page::{Page, PageRequest};

use super::OrderService;

/// Tenant scope of storefront queries.
pub fn scope_of(tenant: &ResellerTenant) -> Scope {
    match tenant.reseller_id {
        Some(id) if tenant.is_reseller() => Scope::Reseller(id),
        _ => Scope::Main,
    }
}

/// Who is asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Viewer {
    User(Id),
    /// Guest credentials from the `Authorization: Guest` header.
    Guest {
        email: String,
        password: String,
    },
}

impl OrderService {
    fn owner(&self, viewer: &Viewer) -> Owner {
        match viewer {
            Viewer::User(id) => Owner::User(*id),
            Viewer::Guest { email, password } => {
                let email = normalize_email(email);
                Owner::Guest {
                    credential: hash_credential(&self.deps.guest_secret, &email, password),
                    email,
                }
            }
        }
    }

    fn not_found(viewer: &Viewer) -> Error {
        match viewer {
            Viewer::User(_) => Error::not_found(keys::ORDER_NOT_FOUND),
            Viewer::Guest { .. } => Error::not_found(keys::GUEST_ORDER_NOT_FOUND),
        }
    }

    /// Lazily cancels an expired pending order (`ensureOrderCanceledIfExpired`) and syncs
    /// refund statuses (`ensureOrderRefundStatusSynced`).
    pub(crate) async fn refresh(&self, mut order: Order) -> Result<Order> {
        let now = self.deps.clock.now();
        if order.status == OrderStatus::PendingPayment
            && order.parent_id.is_none()
            && order.expires_at.is_some_and(|e| e <= now)
        {
            match self.deps.store.cancel(order.id, now).await {
                Ok(canceled) => {
                    self.after_cancel(&canceled, CancelReason::Expired).await;
                    order = canceled;
                }
                Err(e) if e.key() == keys::ORDER_CANCEL_NOT_ALLOWED => {
                    // A concurrent payment won the race; reload below.
                    if let Some(fresh) = self.deps.repo.get(order.id).await? {
                        order = fresh;
                    }
                }
                Err(e) => return Err(e.or_internal(keys::ORDER_UPDATE_FAILED)),
            }
        }
        self.sync_refund_statuses(&mut order).await?;
        order.fill_items_from_children();
        Ok(order)
    }

    async fn sync_refund_statuses(&self, order: &mut Order) -> Result<()> {
        let now = self.deps.clock.now();
        let mut changed = false;
        if let Some(target) = expected_refund_status(order).filter(|t| *t != order.status) {
            self.deps.repo.set_status(order.id, target, now).await?;
            order.status = target;
            changed = true;
        }
        for child in &mut order.children {
            if let Some(target) = expected_refund_status(child).filter(|t| *t != child.status) {
                self.deps.repo.set_status(child.id, target, now).await?;
                child.status = target;
                changed = true;
            }
        }
        if !order.children.is_empty() {
            let statuses: Vec<OrderStatus> = order.children.iter().map(|c| c.status).collect();
            let target = expected_refund_status(order)
                .unwrap_or_else(|| calc_parent_status(&statuses, order.status));
            if target != order.status {
                self.deps.repo.set_status(order.id, target, now).await?;
                order.status = target;
            }
        } else if let (Some(parent), true) = (order.parent_id, changed) {
            self.deps.store.sync_parent(parent, now).await?;
        }
        Ok(())
    }

    /// A parent order of the viewer by number.
    pub async fn get_order(
        &self,
        viewer: &Viewer,
        tenant: &ResellerTenant,
        order_no: &str,
    ) -> Result<Order> {
        let order_no = order_no.trim();
        if order_no.is_empty() {
            return Err(Self::not_found(viewer));
        }
        let order = self
            .deps
            .repo
            .find_parent(order_no, &self.owner(viewer), scope_of(tenant))
            .await
            .map_err(|e| e.or_internal(keys::ORDER_FETCH_FAILED))?
            .ok_or_else(|| Self::not_found(viewer))?;
        self.refresh(order).await
    }

    /// A parent order of the viewer by id (payment capture ownership checks).
    pub async fn get_order_by_id(
        &self,
        viewer: &Viewer,
        tenant: &ResellerTenant,
        id: Id,
    ) -> Result<Order> {
        let order = self
            .deps
            .repo
            .find_parent_by_id(id, &self.owner(viewer), scope_of(tenant))
            .await
            .map_err(|e| e.or_internal(keys::ORDER_FETCH_FAILED))?
            .ok_or_else(|| Self::not_found(viewer))?;
        self.refresh(order).await
    }

    /// Parent or child order of the viewer (downloads, DLV-05).
    pub async fn get_any_order(
        &self,
        viewer: &Viewer,
        tenant: &ResellerTenant,
        order_no: &str,
    ) -> Result<Order> {
        let order = self
            .deps
            .repo
            .find_any(order_no.trim(), &self.owner(viewer), scope_of(tenant))
            .await
            .map_err(|e| e.or_internal(keys::ORDER_FETCH_FAILED))?
            .ok_or_else(|| Self::not_found(viewer))?;
        self.refresh(order).await
    }

    /// Full delivered content of an order (`respondFulfillmentDownload`).
    pub fn fulfillment_payload(order: &Order) -> Result<String> {
        let payload = order.collect_fulfillment_payload();
        if payload.is_empty() {
            return Err(Error::not_found(keys::FULFILLMENT_NOT_FOUND));
        }
        Ok(payload)
    }

    /// Parent orders of the viewer.
    pub async fn list_orders(
        &self,
        viewer: &Viewer,
        tenant: &ResellerTenant,
        status: &str,
        order_no: &str,
        page: PageRequest,
    ) -> Result<Page<Order>> {
        let page = self
            .deps
            .repo
            .list_for_owner(
                &self.owner(viewer),
                scope_of(tenant),
                status,
                order_no,
                page,
            )
            .await
            .map_err(|e| e.or_internal(keys::ORDER_FETCH_FAILED))?;
        let mut items = Vec::with_capacity(page.items.len());
        for order in page.items {
            items.push(
                self.refresh(order)
                    .await
                    .map_err(|e| e.or_internal(keys::ORDER_UPDATE_FAILED))?,
            );
        }
        Ok(Page {
            items,
            total: page.total,
        })
    }

    /// Order counts per status of a user (`/orders/stats`).
    pub async fn order_stats(
        &self,
        user_id: Id,
        tenant: &ResellerTenant,
        order_no: &str,
    ) -> Result<BTreeMap<String, i64>> {
        self.deps
            .repo
            .stats_for_user(user_id, scope_of(tenant), order_no)
            .await
            .map_err(|e| e.or_internal(keys::ORDER_FETCH_FAILED))
    }

    /// Channel whitelist of the order's products (PAY-11): `None` = unrestricted.
    pub async fn allowed_channel_ids(&self, product_ids: &[Id]) -> Result<Option<Vec<Id>>> {
        let ids: Vec<Id> = product_ids
            .iter()
            .copied()
            .filter(|id| *id > 0)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if ids.is_empty() {
            return Ok(None);
        }
        let lists: Vec<Vec<Id>> = self
            .deps
            .repo
            .product_channel_ids(&ids)
            .await?
            .iter()
            .map(|raw| decode_channel_ids(raw))
            .collect();
        Ok(product_channel_intersection(&lists))
    }

    /// Detail DTO with the channel whitelist and refund records (`enrichOrderWith*`).
    pub async fn order_detail(&self, order: &Order, truncate: bool) -> Result<OrderDetail> {
        let mut detail = OrderDetail::new(order, truncate);
        let product_ids: Vec<Id> = order.all_items().iter().map(|i| i.product_id).collect();
        detail.allowed_payment_channel_ids = self.allowed_channel_ids(&product_ids).await?;
        if matches!(
            detail.status,
            OrderStatus::Refunded | OrderStatus::PartiallyRefunded
        ) {
            match self.deps.repo.refunds_of(&order.family_ids()).await {
                Ok(records) => {
                    detail.refund_records = records.iter().map(RefundView::from).collect()
                }
                Err(error) => {
                    tracing::warn!(%error, order_id = order.id, "public_order_refund_records_fetch_failed")
                }
            }
        }
        Ok(detail)
    }

    /// User cancellation of a pending order (`CancelOrder`, ORD-01): coupon usage returned.
    pub async fn cancel_order(
        &self,
        user_id: Id,
        tenant: &ResellerTenant,
        order_no: &str,
    ) -> Result<Order> {
        let viewer = Viewer::User(user_id);
        let order = self.get_order(&viewer, tenant, order_no).await?;
        if order.status != OrderStatus::PendingPayment {
            return Err(Error::bad_request(keys::ORDER_CANCEL_NOT_ALLOWED));
        }
        let mut canceled = self
            .deps
            .store
            .cancel(order.id, self.deps.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::ORDER_UPDATE_FAILED))?;
        self.after_cancel(&canceled, CancelReason::User).await;
        canceled.fill_items_from_children();
        Ok(canceled)
    }

    /// Affiliate reaction to a cancellation (best effort).
    pub(crate) async fn after_cancel(&self, order: &Order, reason: CancelReason) {
        if let Err(error) = self
            .deps
            .affiliate
            .order_canceled(order.id, reason.as_str())
            .await
        {
            tracing::warn!(%error, order_id = order.id, "affiliate_handle_order_canceled_failed");
        }
    }

    /// `order:timeout_cancel` (`CancelExpiredOrder`, ORD-01/ORD-04): only a still-pending,
    /// expired order is canceled; a paid order is left alone.
    pub async fn cancel_expired(&self, order_id: Id) -> Result<()> {
        let Some(order) = self.deps.repo.get(order_id).await? else {
            return Ok(());
        };
        let now = self.deps.clock.now();
        if order.status != OrderStatus::PendingPayment || order.expires_at.is_none_or(|e| e > now) {
            return Ok(());
        }
        match self.deps.store.cancel(order.id, now).await {
            Ok(canceled) => {
                self.after_cancel(&canceled, CancelReason::Expired).await;
                Ok(())
            }
            Err(e) if e.key() == keys::ORDER_CANCEL_NOT_ALLOWED => Ok(()),
            Err(e) => Err(e),
        }
    }
}
