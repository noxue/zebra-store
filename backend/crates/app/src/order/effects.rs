//! Side effects of a paid order (`enqueueOrderPaidAsync`): they run after the settlement
//! transaction committed, only for the first success, and never fail the payment.

use serde_json::{Map, Value, json};
use zs_domain::notify::center::{
    NotifyEvent, OrderItemLine, biz_types, build_order_item_summaries, delivery_summary, events,
    payment_channel_label,
};
use zs_domain::notify::channel::{BOT_NOTIFY_MAX_RETRY, BotNotifyPayload, bot_events};
use zs_domain::order::model::{Order, OrderItem, OrderStatus};
use zs_domain::order::status::{has_manual_items, is_fully_auto, should_auto_fulfill};
use zs_domain::payment::model::Payment;
use zs_domain::queue::{NewJob, kinds};
use zs_domain::settings::keys as setting_keys;
use zs_domain::settings::schema::notification::NotificationCenterSetting;

use super::OrderService;

fn item_lines(items: &[&OrderItem]) -> Vec<OrderItemLine> {
    items
        .iter()
        .map(|i| OrderItemLine {
            title: Value::Object(i.title.clone()),
            sku_snapshot: Value::Object(i.sku_snapshot.clone()),
            quantity: i64::from(i.quantity),
            fulfillment_type: i.fulfillment_type.clone(),
        })
        .collect()
}

impl OrderService {
    async fn notification_locale(&self) -> String {
        let raw = self
            .deps
            .settings
            .get(setting_keys::NOTIFICATION_CENTER_CONFIG)
            .await
            .ok()
            .flatten();
        NotificationCenterSetting::decode(raw.as_ref(), NotificationCenterSetting::default())
            .default_locale
    }

    /// `buildOrderNotificationPayload` (NTF-11: parent items fall back to the children's).
    pub(crate) async fn order_notification_data(
        &self,
        order: &Order,
        payment: Option<&Payment>,
    ) -> Map<String, Value> {
        let locale = self.notification_locale().await;
        let (customer_email, customer_label, customer_type) = if order.user_id == 0 {
            (
                order.guest_email.clone(),
                order.guest_email.clone(),
                "guest",
            )
        } else {
            let user = self
                .deps
                .repo
                .users(&[order.user_id])
                .await
                .ok()
                .and_then(|mut m| m.remove(&order.user_id));
            let (email, name) = user
                .map(|u| (u.email.trim().to_owned(), u.display_name.trim().to_owned()))
                .unwrap_or_default();
            let email = if email.is_empty() {
                order.guest_email.clone()
            } else {
                email
            };
            let label = match (name.is_empty(), email.is_empty()) {
                (false, false) => format!("{name} <{email}>"),
                (true, false) => email.clone(),
                (false, true) => name,
                (true, true) => format!("user#{}", order.user_id),
            };
            (email, label, "registered")
        };
        let items = item_lines(&order.all_items());
        let (items_summary, pending_summary, counts) = build_order_item_summaries(&items, &locale);
        let (provider, channel) = match payment {
            Some(p) => {
                let display = p.display_channel_type();
                (
                    p.provider_type.clone(),
                    if display.is_empty() {
                        p.channel_type.clone()
                    } else {
                        display
                    },
                )
            }
            None => (String::new(), String::new()),
        };
        let channel_label =
            payment_channel_label(&provider, &channel, order.wallet_paid_amount.is_positive());
        let mut data = json!({
            "order_id": order.id.to_string(),
            "order_no": order.order_no.trim(),
            "user_id": order.user_id.to_string(),
            "guest_email": order.guest_email.trim(),
            "amount": order.total_amount.to_string(),
            "currency": order.currency.trim().to_uppercase(),
            "order_status": order.status.as_str(),
            "customer_email": customer_email,
            "customer_label": customer_label,
            "customer_type": customer_type,
            "items_summary": items_summary,
            "fulfillment_items_summary": pending_summary,
            "delivery_summary": delivery_summary(&locale, counts),
            "item_count": counts.total.to_string(),
            "auto_item_count": counts.auto.to_string(),
            "manual_item_count": counts.manual.to_string(),
            "upstream_item_count": counts.upstream.to_string(),
            "payment_channel": channel_label,
        });
        if let Some(obj) = data.as_object_mut() {
            if let Some(p) = payment {
                obj.insert("payment_id".into(), Value::String(p.id.to_string()));
            }
            if !provider.is_empty() {
                obj.insert("provider_type".into(), Value::String(provider));
            }
            if !channel.is_empty() {
                obj.insert("channel_type".into(), Value::String(channel));
            }
        }
        data.as_object().cloned().unwrap_or_default()
    }

    async fn notify_event(&self, event_type: &str, order: &Order, data: Map<String, Value>) {
        let event = NotifyEvent {
            event_type: event_type.to_owned(),
            biz_type: biz_types::ORDER.to_owned(),
            biz_id: order.id,
            data,
            ..NotifyEvent::default()
        };
        if let Err(error) = self.deps.notifier.notify(event).await {
            tracing::warn!(%error, order_id = order.id, event_type, "notification_enqueue_failed");
        }
    }

    async fn manual_pending(&self, order: &Order, parent: Option<&Order>) {
        let mut data = self.order_notification_data(order, None).await;
        if let Some(p) = parent {
            data.insert("parent_order_id".into(), Value::String(p.id.to_string()));
            data.insert(
                "parent_order_no".into(),
                Value::String(p.order_no.trim().to_owned()),
            );
        }
        self.notify_event(events::MANUAL_FULFILLMENT_PENDING, order, data)
            .await;
    }

    /// Queues the Telegram bot notification of a user's order.
    pub(crate) async fn bot_notify(
        &self,
        order_id: zs_domain::Id,
        user_id: zs_domain::Id,
        event_type: &str,
    ) {
        if user_id <= 0 {
            return;
        }
        let telegram = match self.deps.repo.telegram_user_id(user_id).await {
            Ok(Some(id)) => id,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(%error, order_id, "order_notify_bot_fetch_identity_failed");
                return;
            }
        };
        let payload = BotNotifyPayload {
            event_type: event_type.to_owned(),
            order_id,
            telegram_user_id: telegram,
            ..BotNotifyPayload::default()
        };
        match NewJob::new(kinds::BOT_NOTIFY, payload) {
            Ok(job) => {
                if let Err(error) = self
                    .deps
                    .queue
                    .enqueue(job.attempts(BOT_NOTIFY_MAX_RETRY))
                    .await
                {
                    tracing::warn!(%error, order_id, "order_notify_bot_enqueue_failed");
                }
            }
            Err(error) => tracing::warn!(%error, "order_notify_bot_payload_invalid"),
        }
    }

    async fn enqueue_auto_fulfill(&self, order_id: zs_domain::Id) {
        let job = NewJob::new(kinds::ORDER_AUTO_FULFILL, json!({"order_id": order_id}))
            .map(|j| j.unique(format!("order:auto_fulfill:{order_id}")));
        match job {
            Ok(job) => {
                if let Err(error) = self.deps.queue.enqueue(job).await {
                    tracing::warn!(%error, order_id, "payment_enqueue_auto_fulfill_failed");
                }
            }
            Err(error) => tracing::warn!(%error, "auto_fulfill_payload_invalid"),
        }
    }

    /// Everything that follows the first successful payment of an order.
    pub async fn after_order_paid(&self, order: &Order, payment: Option<&Payment>) {
        if let Err(error) = self.deps.affiliate.order_paid(order.id).await {
            tracing::warn!(%error, order_id = order.id, "affiliate_handle_order_paid_failed");
        }
        // Fully auto-delivered orders get the "completed" mail right after (NTF-01).
        if !is_fully_auto(order) {
            self.enqueue_status_email(order.id, OrderStatus::Paid, None)
                .await;
        }
        let data = self.order_notification_data(order, payment).await;
        self.notify_event(events::ORDER_PAID_SUCCESS, order, data)
            .await;
        self.bot_notify(order.id, order.user_id, bot_events::ORDER_PAID)
            .await;
        if order.user_id > 0
            && let Err(error) = self
                .deps
                .member_levels
                .on_order_paid(order.user_id, order.total_amount)
                .await
        {
            tracing::warn!(%error, order_id = order.id, "member_level_order_paid_failed");
        }
        if order.children.is_empty() {
            if order.status == OrderStatus::Fulfilling && has_manual_items(&order.items) {
                self.manual_pending(order, None).await;
            }
            if should_auto_fulfill(&order.items) {
                self.enqueue_auto_fulfill(order.id).await;
            }
        } else {
            for child in &order.children {
                if child.status == OrderStatus::Fulfilling && has_manual_items(&child.items) {
                    self.manual_pending(child, Some(order)).await;
                }
                if should_auto_fulfill(&child.items) {
                    self.enqueue_auto_fulfill(child.id).await;
                }
            }
        }
        if let Some(integration) = &self.deps.integration
            && let Err(error) = integration.order_paid(order.id).await
        {
            tracing::warn!(%error, order_id = order.id, "payment_enqueue_procurement_failed");
        }
    }
}
