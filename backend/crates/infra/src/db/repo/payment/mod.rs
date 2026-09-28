//! Payment repositories, the payment-row settlement and the alert adapter.

pub mod channel;
pub mod records;
pub mod settlement;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};
use zs_domain::Result;
use zs_domain::payment::alert::{PaymentAlert, PaymentAlerts};
use zs_domain::queue::{JobQueue, NewJob, kinds};
use zs_shared::clock::Clock;

/// Queues payment exception alerts as `notification:dispatch` jobs (`exceptionAlerterAdapter`).
#[derive(Clone)]
pub struct QueuedPaymentAlerts {
    queue: Arc<dyn JobQueue>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for QueuedPaymentAlerts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QueuedPaymentAlerts")
    }
}

impl QueuedPaymentAlerts {
    pub fn new(queue: Arc<dyn JobQueue>, clock: Arc<dyn Clock>) -> Self {
        Self { queue, clock }
    }
}

/// Offset of `occurred_at` (the original formats local China time).
const ALERT_OFFSET_SECS: i32 = 8 * 3600;

#[async_trait]
impl PaymentAlerts for QueuedPaymentAlerts {
    async fn alert(&self, alert: PaymentAlert) -> Result<()> {
        let occurred_at = chrono::FixedOffset::east_opt(ALERT_OFFSET_SECS)
            .map(|o| {
                self.clock
                    .now()
                    .with_timezone(&o)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();
        let mut data = Map::new();
        data.insert("source".into(), Value::String("payment_callback".into()));
        data.insert("method".into(), Value::String(alert.method));
        data.insert("path".into(), Value::String(alert.path));
        data.insert("client_ip".into(), Value::String(alert.client_ip));
        data.insert("occurred_at".into(), Value::String(occurred_at));
        data.extend(alert.data);
        let payload = serde_json::json!({
            "event_type": "exception_alert",
            "biz_type": "payment_callback",
            "biz_id": 0,
            "data": Value::Object(data),
        });
        self.queue
            .enqueue(NewJob::new(kinds::NOTIFICATION_DISPATCH, payload)?)
            .await
    }
}
