//! Delivery use cases (`fulfillment/application/service.go`).

use zs_domain::integration::hooks::UpstreamDelivery;
use zs_domain::notify::channel::bot_events;
use zs_domain::order::delivery::{delivery_payload, normalize_delivery_data};
use zs_domain::order::model::{Fulfillment, JsonMap, OrderStatus, keys};
use zs_domain::{Error, ErrorKind, Id, Result};

use super::OrderService;

impl OrderService {
    /// Replays pending local conversions after process restart or converter recovery.
    /// The reserved cards and pending fulfillment make the attempt idempotent.
    pub async fn retry_pending_conversions(&self) -> Result<()> {
        for order_id in self.deps.repo.pending_auto_fulfillment_ids().await? {
            if let Err(error) = self.auto_fulfill(order_id).await {
                tracing::warn!(%error, order_id, "card_converter_pending_local_retry_failed");
            }
        }
        Ok(())
    }

    pub async fn pending_conversion_count(&self) -> Result<usize> {
        Ok(self.deps.repo.pending_auto_fulfillment_ids().await?.len())
    }

    /// After a child was delivered: recompute the parent, queue the status email of the
    /// parent (never for canceled), notify the bot and the integration group.
    async fn after_delivery(&self, order_id: Id, own_status: OrderStatus) {
        let Ok(Some(order)) = self.deps.repo.get(order_id).await else {
            return;
        };
        let now = self.deps.clock.now();
        let notify_id = order.parent_id.unwrap_or(order.id);
        match order.parent_id {
            Some(parent) => match self.deps.store.sync_parent(parent, now).await {
                Ok(status) => {
                    let status = status.unwrap_or(own_status);
                    if status != OrderStatus::Canceled {
                        self.enqueue_status_email(parent, status, None).await;
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, order_id, parent, "fulfillment_sync_parent_status_failed")
                }
            },
            None => self.enqueue_status_email(order.id, own_status, None).await,
        }
        self.bot_notify(notify_id, order.user_id, bot_events::ORDER_FULFILLED)
            .await;
        if let Some(integration) = &self.deps.integration
            && let Err(error) = integration.order_status_changed(order_id).await
        {
            tracing::warn!(%error, order_id, "fulfillment_downstream_callback_failed");
        }
    }

    /// `order:auto_fulfill` (`CreateAuto`, DLV-09): business rejections (already delivered,
    /// not auto, not paid, missing order) end the job; only internal errors are retried.
    pub async fn auto_fulfill(&self, order_id: Id) -> Result<()> {
        if order_id <= 0 {
            return Ok(());
        }
        let result: Result<()> = async {
            let order = self
                .deps
                .repo
                .get(order_id)
                .await?
                .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
            let pending_conversion = order.status == OrderStatus::Fulfilling
                && order
                    .fulfillment
                    .as_ref()
                    .is_some_and(|f| f.status == "pending" && f.kind == "auto");
            let has_converter = if let Some(integration) = &self.deps.integration {
                integration.requires_converter(&order).await?
            } else {
                false
            };
            if pending_conversion && !has_converter {
                return Err(Error::internal_msg(
                    "pending card conversion has no active product binding",
                ));
            }
            if !has_converter {
                self.deps
                    .store
                    .auto_fulfill(order_id, self.deps.clock.now())
                    .await?;
                return Ok(());
            }
            let raw = self
                .deps
                .store
                .prepare_auto_fulfill(order_id, self.deps.clock.now())
                .await?;
            let integration =
                self.deps.integration.as_ref().ok_or_else(|| {
                    Error::internal_msg("card converter integration is unavailable")
                })?;
            let delivery = zs_domain::integration::hooks::UpstreamDelivery {
                kind: "auto".to_owned(),
                status: "delivered".to_owned(),
                payload: raw,
                delivery_data: Default::default(),
                delivered_at: Some(self.deps.clock.now()),
            };
            let converted = integration
                .convert_delivery(&order, &delivery)
                .await
                .map_err(|e| e.or_internal("error.card_converter_conversion_failed"))?;
            self.deps
                .store
                .finalize_auto_fulfill(order_id, &converted.payload, self.deps.clock.now())
                .await?;
            Ok(())
        }
        .await;
        match result {
            Ok(_) => {
                self.after_delivery(order_id, OrderStatus::Completed).await;
                Ok(())
            }
            Err(e) if e.kind() != ErrorKind::Internal => {
                tracing::debug!(order_id, reason = %e, "worker_order_auto_fulfill_skip");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// `POST /admin/fulfillments` (`CreateManual`).
    pub async fn manual_fulfill(
        &self,
        order_id: Id,
        admin_id: Id,
        payload: &str,
        delivery_data: &JsonMap,
    ) -> Result<Fulfillment> {
        if order_id <= 0 || admin_id <= 0 {
            return Err(Error::bad_request(keys::FULFILLMENT_INVALID));
        }
        let data = normalize_delivery_data(delivery_data);
        let mut payload = payload.trim().to_owned();
        if payload.is_empty() && data.is_empty() {
            return Err(Error::bad_request(keys::FULFILLMENT_INVALID));
        }
        if payload.is_empty() {
            payload = delivery_payload(&data);
        }
        if let Some(integration) = &self.deps.integration
            && let Some(order) = self.deps.repo.get(order_id).await?
            && integration.requires_converter(&order).await?
        {
            let converted = integration
                .convert_delivery(
                    &order,
                    &UpstreamDelivery {
                        kind: "auto".to_owned(),
                        status: "delivered".to_owned(),
                        payload,
                        delivery_data: data.clone(),
                        delivered_at: Some(self.deps.clock.now()),
                    },
                )
                .await?;
            payload = converted.payload;
        }
        let now = self.deps.clock.now();
        let fulfillment = self
            .deps
            .store
            .manual_fulfill(order_id, admin_id, &payload, &data, now, now)
            .await
            .map_err(|e| e.or_internal(keys::FULFILLMENT_CREATE_FAILED))?;
        self.after_delivery(order_id, OrderStatus::Delivered).await;
        Ok(fulfillment)
    }
}
