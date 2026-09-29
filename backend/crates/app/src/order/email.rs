//! Order status e-mails (`EnqueueStatusEmailTaskIfEligible`, `handleOrderStatusEmail`,
//! NTF-01/02/10).

use serde::{Deserialize, Serialize};
use zs_domain::identity::email::is_placeholder;
use zs_domain::identity::mailer::BrandScope;
use zs_domain::order::email::{
    StatusMailInput, fulfillment_text, instructions_text, mail_locale, should_attach, status_mail,
};
use zs_domain::order::model::{Order, OrderStatus};
use zs_domain::queue::{NewJob, kinds};
use zs_domain::settings::keys as setting_keys;
use zs_domain::settings::schema::order_email::OrderEmailTemplateSetting;
use zs_domain::{ErrorKind, Id, Result};

pub use zs_domain::order::email::{OrderMail, OrderMailer};

use super::OrderService;

/// Retry budget of status e-mail jobs (NTF-01).
const STATUS_EMAIL_ATTEMPTS: i32 = 3;

/// Payload of `order:status_email`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusEmailPayload {
    pub order_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refund_record_id: Option<Id>,
    pub status: String,
}

impl OrderService {
    /// Receiver and locale of an order's e-mails (user e-mail, else guest e-mail).
    async fn receiver(&self, order: &Order) -> Result<(String, String)> {
        if order.user_id > 0 {
            let user = self
                .deps
                .repo
                .users(&[order.user_id])
                .await?
                .remove(&order.user_id);
            return Ok(user
                .map(|u| (u.email.trim().to_owned(), u.locale.trim().to_owned()))
                .unwrap_or_default());
        }
        Ok((
            order.guest_email.trim().to_owned(),
            order.guest_locale.trim().to_owned(),
        ))
    }

    /// Queues a status e-mail when SMTP and order notifications are on and the order has a
    /// real receiver (system-generated placeholder addresses never get mail).
    pub async fn enqueue_status_email(
        &self,
        order_id: Id,
        status: OrderStatus,
        refund_record_id: Option<Id>,
    ) {
        match self.smtp_setting().await {
            Ok(s) if s.enabled && s.order_notification_enabled => {}
            Ok(_) => return,
            Err(error) => {
                tracing::warn!(%error, order_id, "order_enqueue_status_email_setting_failed");
                return;
            }
        }
        let Ok(Some(order)) = self.deps.repo.get(order_id).await else {
            return;
        };
        match self.receiver(&order).await {
            Ok((email, _)) if !email.is_empty() && !is_placeholder(&email) => {}
            Ok(_) => return,
            Err(error) => {
                tracing::warn!(%error, order_id, "order_status_email_receiver_lookup_failed")
            }
        }
        let payload = StatusEmailPayload {
            order_id,
            refund_record_id,
            status: status.as_str().to_owned(),
        };
        let job = NewJob::new(kinds::ORDER_STATUS_EMAIL, payload)
            .map(|j| j.attempts(STATUS_EMAIL_ATTEMPTS));
        match job {
            Ok(job) => {
                if let Err(error) = self.deps.queue.enqueue(job).await {
                    tracing::warn!(%error, order_id, status = status.as_str(), "order_enqueue_status_email_failed");
                }
            }
            Err(error) => tracing::warn!(%error, "order_status_email_payload_invalid"),
        }
    }

    /// `order:status_email`: switches are re-checked (fail closed), canceled orders never
    /// mail, large deliveries become attachments.
    pub async fn send_status_email(&self, payload: &StatusEmailPayload) -> Result<()> {
        if payload.order_id <= 0 {
            return Ok(());
        }
        let wanted = OrderStatus::parse(&payload.status);
        if wanted == Some(OrderStatus::Canceled) {
            return Ok(());
        }
        match self.smtp_setting().await {
            Ok(s) if s.enabled && s.order_notification_enabled => {}
            Ok(_) => return Ok(()),
            Err(error) => {
                tracing::warn!(%error, order_id = payload.order_id, "worker_order_status_email_load_smtp_setting_failed");
                return Ok(());
            }
        }
        let Some(order) = self.deps.repo.get(payload.order_id).await? else {
            return Ok(());
        };
        let status = wanted.unwrap_or(order.status);
        if status == OrderStatus::Canceled {
            return Ok(());
        }
        let (to, locale) = self.receiver(&order).await?;
        if to.is_empty() || is_placeholder(&to) {
            return Ok(());
        }
        let locale = mail_locale(&locale);
        let raw = self
            .deps
            .settings
            .get(setting_keys::ORDER_EMAIL_TEMPLATE_CONFIG)
            .await
            .ok()
            .flatten();
        let templates =
            OrderEmailTemplateSetting::decode(raw.as_ref(), OrderEmailTemplateSetting::defaults());

        // White-label orders never fall back to the main brand; a failing lookup is
        // retried rather than sent with the wrong identity (NTF-02).
        let scope = match order.reseller_id.filter(|id| *id > 0) {
            Some(id) => BrandScope {
                reseller_id: Some(id),
                host: order.reseller_domain.trim().to_owned(),
            },
            None => BrandScope::default(),
        };
        let brand = self.deps.mail_brands.resolve(&scope).await?;
        let (site_name, site_url) = (brand.site_name.clone(), brand.site_url.clone());
        let (mut refund_amount, mut refund_reason) = (order.refunded_amount, String::new());
        if let Some(record_id) = payload.refund_record_id.filter(|id| *id > 0)
            && let Some(record) = self.deps.repo.refund(record_id).await?
        {
            let belongs = record.order_id == order.id
                || self
                    .deps
                    .repo
                    .get(record.order_id)
                    .await?
                    .is_some_and(|o| o.parent_id == Some(order.id));
            if belongs {
                refund_amount = record.amount;
                refund_reason = record.remark.trim().to_owned();
            }
        }
        let delivered = fulfillment_text(&order);
        let attach = should_attach(&delivered);
        let input = StatusMailInput {
            order_no: order.order_no.clone(),
            status: Some(status),
            amount: order.total_amount,
            refund_amount,
            refund_reason,
            currency: order.currency.clone(),
            site_name: site_name.clone(),
            site_url,
            is_guest: order.user_id == 0,
            fulfillment: if attach {
                String::new()
            } else {
                delivered.clone()
            },
            instructions: if matches!(status, OrderStatus::Delivered | OrderStatus::Completed) {
                instructions_text(&order, locale)
            } else {
                String::new()
            },
        };
        let (subject, body) = status_mail(&input, locale, &templates, attach);
        let mail = OrderMail {
            to,
            subject,
            body,
            attachment: attach
                .then(|| (format!("order_{}_delivery.txt", order.order_no), delivered)),
            from_name: Some(brand.from_name).filter(|n| !n.is_empty()),
            reply_to: Some(brand.reply_to).filter(|r| !r.is_empty()),
        };
        match self.deps.mailer.send(&mail).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() != ErrorKind::Internal => {
                tracing::debug!(order_id = order.id, reason = %e, "worker_order_status_email_skip");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}
