//! Configuration and protocol-neutral types for third-party card converters.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Id, Result};

#[derive(Debug, Clone, Serialize)]
pub struct Converter {
    pub id: Id,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    pub token_configured: bool,
    pub types: Vec<ConverterType>,
    pub health: String,
    pub consecutive_failures: i32,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub last_error: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip)]
    pub token_enc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConverterType {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Dynamic converter-specific settings, rendered as a form by the shop admin.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ConverterField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConverterField {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub required: bool,
    /// `text`, `number`, `boolean`, or `select`.
    #[serde(default = "default_field_kind")]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub options: Vec<String>,
}

fn default_field_kind() -> String {
    "text".to_owned()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Binding {
    pub id: Id,
    pub product_id: Id,
    /// 0 means the product default; a positive SKU id overrides it.
    pub sku_id: Id,
    pub converter_id: Id,
    pub type_id: String,
    pub fields: Vec<String>,
    pub extra_template: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct ConverterInput {
    pub name: String,
    pub base_url: String,
    /// Empty on update means keep the current token.
    pub token: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExchangeItem {
    pub index: usize,
    pub upstream_card: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExchangeRequest {
    pub protocol_version: String,
    pub idempotency_key: String,
    pub type_id: String,
    pub order: Value,
    pub site: Value,
    pub product: Value,
    pub sku: Value,
    pub items: Vec<ExchangeItem>,
    pub extra: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeResultItem {
    pub index: usize,
    pub card: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeResponse {
    pub protocol_version: String,
    pub items: Vec<ExchangeResultItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConverterEvent {
    pub id: Id,
    pub converter_id: Id,
    pub event_type: String,
    pub status: String,
    pub error_code: String,
    pub duration_ms: i64,
    pub order_no: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewConverterEvent {
    pub converter_id: Id,
    pub event_type: String,
    pub status: String,
    pub error_code: String,
    pub duration_ms: i64,
    pub order_no: String,
    pub created_at: DateTime<Utc>,
}

#[async_trait]
pub trait ConverterRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<Converter>>;
    async fn get(&self, id: Id) -> Result<Option<Converter>>;
    async fn save(&self, converter: &Converter) -> Result<Converter>;
    async fn delete(&self, id: Id) -> Result<()>;
    async fn bindings(&self, product_id: Option<Id>) -> Result<Vec<Binding>>;
    async fn get_binding(&self, product_id: Id, sku_id: Id) -> Result<Option<Binding>>;
    async fn save_binding(&self, binding: &Binding) -> Result<Binding>;
    async fn delete_binding(&self, product_id: Id, sku_id: Id) -> Result<()>;
    async fn health_result(
        &self,
        id: Id,
        ok: bool,
        at: DateTime<Utc>,
        safe_error: &str,
    ) -> Result<(Converter, bool)>;
    async fn record_event(&self, event: &NewConverterEvent) -> Result<()>;
    async fn events(&self, converter_id: Id, limit: u64) -> Result<Vec<ConverterEvent>>;
}

#[async_trait]
pub trait ConverterHttp: Send + Sync {
    async fn types(&self, base_url: &str, token: &str) -> Result<Vec<ConverterType>>;
    async fn health(&self, base_url: &str, token: &str) -> Result<()>;
    async fn exchange(
        &self,
        base_url: &str,
        token: &str,
        request: &ExchangeRequest,
    ) -> Result<ExchangeResponse>;
}
