//! Operational alerts raised by callback handling (`ExceptionAlerter`).

use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::Result;

/// A callback that could not be processed.
#[derive(Debug, Clone, Default)]
pub struct PaymentAlert {
    pub method: String,
    pub path: String,
    pub client_ip: String,
    /// `alert_type`, `alert_level`, `message`, `provider`, … (original keys).
    pub data: Map<String, Value>,
}

impl PaymentAlert {
    /// Builds an alert with the standard `alert_type` / `alert_level` / `message` keys.
    pub fn new(alert_type: &str, level: &str, message: &str) -> Self {
        let mut data = Map::new();
        data.insert("alert_type".into(), Value::String(alert_type.into()));
        data.insert("alert_level".into(), Value::String(level.into()));
        data.insert("message".into(), Value::String(message.into()));
        Self {
            data,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }
}

/// Queues payment exception alerts (best effort; failures are only logged).
#[async_trait]
pub trait PaymentAlerts: Send + Sync {
    async fn alert(&self, alert: PaymentAlert) -> Result<()>;
}
