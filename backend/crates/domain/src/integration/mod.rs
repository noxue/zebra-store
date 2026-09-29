//! `integration` domain: site-to-site trading.
//!
//! - [`adapter`]: the protocol-neutral supplier adapter port (one adapter per system).
//! - [`credential`]: API credentials of downstream shops buying from us.
//! - [`connection`]: site connections to upstream suppliers we buy from.
//! - [`pricing`]: exchange rate / markup / rounding of imported prices.
//! - [`protocol`]: the legacy supplier wire format and the outbound client port.
//! - [`mapping`]: local product ↔ supplier product mappings and their sync plans.
//! - [`procurement`]: purchase orders placed with suppliers and their state machine.
//! - [`downstream`]: callbacks to downstream shops about their orders.
//! - [`reconciliation`]: purchase order vs. supplier comparison.
//! - [`supplier`]: ports of the upstream API we serve (`/api/v1/upstream/*`),
//!   including [`supplier::UpstreamOrdering`] implemented by the order group.
//! - [`hooks`]: ports between the order group and this group.
//! - [`provide`]: compat keys and rules of the provider-compat facades (acg-faka, mcy).

pub mod adapter;
pub mod card_converter;
pub mod connection;
pub mod credential;
pub mod downstream;
pub mod hooks;
pub mod mapping;
pub mod pricing;
pub mod procurement;
pub mod protocol;
pub mod provide;
pub mod reconciliation;
pub mod supplier;
pub mod zs;

/// Shared message keys of the group (identical to the original handlers; keys the
/// original never translated are rendered verbatim, like the original).
pub mod keys {
    pub const CREDENTIAL_NOT_FOUND: &str = "error.api_credential_not_found";
    pub const CREDENTIAL_NOT_APPROVED: &str = "error.api_credential_not_approved";
    pub const CREDENTIAL_FETCH_FAILED: &str = "error.api_credential_fetch_failed";
    pub const CREDENTIAL_APPLY_FAILED: &str = "error.api_credential_apply_failed";
    pub const CREDENTIAL_APPROVE_FAILED: &str = "error.api_credential_approve_failed";
    pub const CREDENTIAL_REJECT_FAILED: &str = "error.api_credential_reject_failed";
    pub const CREDENTIAL_UPDATE_FAILED: &str = "error.api_credential_update_failed";
    pub const CREDENTIAL_DELETE_FAILED: &str = "error.api_credential_delete_failed";
    pub const CREDENTIAL_REGENERATE_FAILED: &str = "error.api_credential_regenerate_failed";
    /// Raw messages of the original user handler (`RespondErrorWithMsg`).
    pub const MSG_CREDENTIAL_EXISTS: &str = "API credential already exists";
    pub const MSG_CREDENTIAL_PENDING: &str = "Application is pending review";
    pub const MSG_CREDENTIAL_NOT_APPROVED: &str = "API credential is not approved";

    pub const CONNECTION_NOT_FOUND: &str = "error.connection_not_found";
    pub const CONNECTION_INVALID: &str = "error.connection_invalid";
    pub const CONNECTION_CODE_INVALID: &str = "error.connection_code_invalid";
    pub const CONNECTION_HANDSHAKE_FAILED: &str = "error.connection_handshake_failed";
    pub const CONNECTION_FETCH_FAILED: &str = "error.connection_fetch_failed";
    pub const CONNECTION_CREATE_FAILED: &str = "error.connection_create_failed";
    pub const CONNECTION_UPDATE_FAILED: &str = "error.connection_update_failed";
    pub const CONNECTION_DELETE_FAILED: &str = "error.connection_delete_failed";
    pub const REAPPLY_MARKUP_FAILED: &str = "error.reapply_markup_failed";

    pub const MAPPING_NOT_FOUND: &str = "error.mapping_not_found";
    pub const MAPPING_EXISTS: &str = "error.mapping_already_exists";
    pub const MAPPING_FETCH_FAILED: &str = "error.mapping_fetch_failed";
    pub const MAPPING_IMPORT_FAILED: &str = "error.mapping_import_failed";
    pub const MAPPING_SYNC_FAILED: &str = "error.mapping_sync_failed";
    pub const MAPPING_UPDATE_FAILED: &str = "error.mapping_update_failed";
    pub const MAPPING_DELETE_FAILED: &str = "error.mapping_delete_failed";
    pub const UPSTREAM_PRODUCT_NOT_FOUND: &str = "error.upstream_product_not_found";
    pub const UPSTREAM_PRODUCTS_FETCH_FAILED: &str = "error.upstream_products_fetch_failed";
    pub const UPSTREAM_CATEGORIES_FETCH_FAILED: &str = "error.upstream_categories_fetch_failed";
    pub const CATEGORY_IMPORT_FAILED: &str = "error.category_import_failed";
    pub const INVALID_UPSTREAM_STATUS: &str = "error.invalid_upstream_status";
    pub const INVALID_PRODUCT_STATUS: &str = "error.invalid_product_status";
    pub const UPSTREAM_STOCK_INSUFFICIENT: &str = "error.upstream_stock_insufficient";

    pub const PROCUREMENT_NOT_FOUND: &str = "error.procurement_not_found";
    pub const PROCUREMENT_FETCH_FAILED: &str = "error.procurement_fetch_failed";
    pub const PROCUREMENT_RETRY_FAILED: &str = "error.procurement_retry_failed";
    pub const PROCUREMENT_CANCEL_FAILED: &str = "error.procurement_cancel_failed";
    /// Raw message of `procurementcontract.ErrStatusInvalid`.
    pub const MSG_PROCUREMENT_STATUS_INVALID: &str = "procurement order status invalid";
    pub const FULFILLMENT_NOT_FOUND: &str = "error.fulfillment_not_found";

    pub const RECONCILIATION_CREATE_FAILED: &str = "error.reconciliation_create_failed";
    pub const RECONCILIATION_FETCH_FAILED: &str = "error.reconciliation_fetch_failed";
    pub const RECONCILIATION_ITEM_NOT_FOUND: &str = "error.reconciliation_item_not_found";
    pub const RECONCILIATION_JOB_NOT_FOUND: &str = "error.reconciliation_job_not_found";
    pub const RECONCILIATION_RESOLVE_FAILED: &str = "error.reconciliation_resolve_failed";
}
