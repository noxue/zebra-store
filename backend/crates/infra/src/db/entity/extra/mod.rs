//! Tables that are not part of the original GORM schema dump.

pub mod api_compat_keys;
pub mod api_credential_rotations;
pub mod api_request_nonces;
pub mod card_converter_bindings;
pub mod card_converter_events;
pub mod card_converters;
pub mod casbin_rule;
pub mod integration_connection_states;
pub mod integration_processed_events;
pub mod jobs;
pub mod procurement_deliveries;
pub mod zs_catalog_snapshots;
pub mod zs_change_log;
pub mod zs_order_requests;
pub mod zs_quotes;
pub mod zs_webhook_events;
pub mod zs_webhooks;
