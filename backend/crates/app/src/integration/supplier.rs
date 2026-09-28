//! The legacy upstream API we serve (`/api/v1/upstream/*`) and the catalog
//! projection shared with the `zebra-store` API. Supplier callbacks we receive as a
//! buyer go through [`super::inbound::InboundService`].

use std::sync::Arc;

use serde_json::{Value, json};
use zs_domain::catalog::category::Category;
use zs_domain::catalog::product::{JsonMap, Product};
use zs_domain::integration::downstream::OrderRefRepo;
use zs_domain::integration::protocol::{ProductQuery, RemoteProduct};
use zs_domain::integration::supplier::{
    Buyer, PRODUCTS_MAX_PAGE_SIZE, PROTOCOL_VERSION, PlaceOutcome, PlaceUpstreamOrder, SiteInfo,
    SupplierCatalog, UpstreamOrderError, UpstreamOrderSummary, UpstreamOrderView, UpstreamOrdering,
};
use zs_domain::{Id, Result};

use super::credential::Caller;
use super::procurement::ProcurementService;
use super::provide::desk::{DEFAULT_CURRENCY, SupplyDesk};

/// A protocol failure: real HTTP status, `error_code`, `error_message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}

impl Failure {
    pub fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    fn internal(message: &str) -> Self {
        Self::new(500, "internal_error", message)
    }
}

/// `POST /upstream/orders` request (already parsed).
#[derive(Debug, Clone, Default)]
pub struct OrderRequest {
    pub sku_id: Id,
    pub quantity: i32,
    pub manual_form_data: Option<JsonMap>,
    pub downstream_order_no: String,
    pub trace_id: String,
    pub callback_url: String,
    pub client_ip: String,
}

/// Longest accepted downstream order number / trace id (`varchar(64)` columns).
const MAX_REF_LEN: usize = 64;
/// Longest accepted callback URL (`varchar(500)`).
const MAX_CALLBACK_URL_LEN: usize = 500;

/// Literal-address check of a buyer's callback URL (UPS-10). The delivery client
/// re-checks every resolved address at send time (UPS-01).
pub fn validate_callback_url(raw: &str) -> std::result::Result<(), &'static str> {
    let url = raw.trim();
    if url.len() > MAX_CALLBACK_URL_LEN {
        return Err("callback url is too long");
    }
    let rest = if let Some(r) = url.strip_prefix("https://") {
        r
    } else if let Some(r) = url.strip_prefix("http://") {
        r
    } else {
        return Err("callback url must use http or https");
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority.rsplit('@').next().unwrap_or_default();
    let host = if let Some(stripped) = host_port.strip_prefix('[') {
        stripped.split(']').next().unwrap_or_default()
    } else {
        host_port.split(':').next().unwrap_or_default()
    };
    if host.is_empty() {
        return Err("callback url must have a host");
    }
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        return Err("callback url must not point to localhost");
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>()
        && !is_public_literal(ip)
    {
        return Err("callback url must not point to private network");
    }
    Ok(())
}

fn is_public_literal(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || a == 0
                || (a == 100 && (64..128).contains(&b)))
        }
        std::net::IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_literal(std::net::IpAddr::V4(v4));
            }
            let seg = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg[0] & 0xfe00) == 0xfc00
                || (seg[0] & 0xffc0) == 0xfe80)
        }
    }
}

/// Upstream API use cases.
#[derive(Clone)]
pub struct SupplierService {
    /// The provider core shared with the other served protocols.
    desk: SupplyDesk,
    catalog: Arc<dyn SupplierCatalog>,
    ordering: Arc<dyn UpstreamOrdering>,
    refs: Arc<dyn OrderRefRepo>,
    procurement: ProcurementService,
}

impl std::fmt::Debug for SupplierService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SupplierService")
    }
}

/// A listing answer.
#[derive(Debug, Clone)]
pub struct ProductListing {
    pub items: Vec<RemoteProduct>,
    pub total: u64,
    pub page: i64,
    pub page_size: i64,
    pub includes_inactive: bool,
}

impl SupplierService {
    pub fn new(
        catalog: Arc<dyn SupplierCatalog>,
        ordering: Arc<dyn UpstreamOrdering>,
        refs: Arc<dyn OrderRefRepo>,
        procurement: ProcurementService,
    ) -> Self {
        Self {
            desk: SupplyDesk::new(catalog.clone(), ordering.clone(), refs.clone()),
            catalog,
            ordering,
            refs,
            procurement,
        }
    }

    /// The provider core (catalog projection, pricing, idempotent wallet orders).
    pub fn desk(&self) -> &SupplyDesk {
        &self.desk
    }

    async fn currency(&self) -> String {
        self.desk.currency().await
    }

    async fn buyer(&self, caller: &Caller) -> Option<Buyer> {
        self.desk.buyer(caller).await
    }

    /// `POST /upstream/ping`.
    pub async fn ping(&self, caller: &Caller) -> Result<Value> {
        let site = self.catalog.site_info().await.unwrap_or_default();
        let buyer = self.buyer(caller).await;
        let member_level = buyer
            .as_ref()
            .and_then(|b| b.member_level.as_ref())
            .map(|l| json!({"id": l.id, "name": l.name, "slug": l.slug, "icon": l.icon}));
        let currency = if site.currency.trim().is_empty() {
            DEFAULT_CURRENCY.to_owned()
        } else {
            site.currency
        };
        Ok(json!({
            "ok": true,
            "site_name": site.site_name,
            "protocol_version": PROTOCOL_VERSION,
            "user_id": caller.user_id,
            "balance": buyer.map(|b| b.balance.to_string()).unwrap_or_else(|| "0.00".into()),
            "currency": currency,
            "member_level": member_level,
        }))
    }

    pub async fn categories(&self) -> Result<Vec<Category>> {
        self.catalog.categories().await
    }

    /// Site identity (name, currency with default, brand URL).
    pub async fn site(&self) -> Result<SiteInfo> {
        self.desk.site().await
    }

    /// Projects products for the caller (member prices, effective types, real stock).
    pub(crate) async fn project(
        &self,
        caller: &Caller,
        products: &[Product],
    ) -> Result<Vec<RemoteProduct>> {
        self.desk.project(caller, products).await
    }

    /// `GET /upstream/products` (page size ≤ 50, `updated_after` RFC3339, `include_inactive`).
    pub async fn products(
        &self,
        caller: &Caller,
        page: i64,
        page_size: i64,
        updated_after: Option<chrono::DateTime<chrono::Utc>>,
        include_inactive: bool,
    ) -> Result<ProductListing> {
        let page = page.max(1);
        let page_size = if page_size <= 0 {
            PRODUCTS_MAX_PAGE_SIZE
        } else {
            page_size.min(PRODUCTS_MAX_PAGE_SIZE)
        };
        let result = self
            .catalog
            .products(&ProductQuery {
                page,
                page_size,
                updated_after,
                include_inactive,
                cursor: None,
            })
            .await?;
        Ok(ProductListing {
            items: self.project(caller, &result.items).await?,
            total: result.total,
            page,
            page_size,
            includes_inactive: include_inactive,
        })
    }

    /// `GET /upstream/products/:id`: delisted products answer 200 with
    /// `is_active=false`, deleted ones are `None` (UPS-14).
    pub async fn product(&self, caller: &Caller, id: Id) -> Result<Option<RemoteProduct>> {
        let Some(p) = self.catalog.product(id).await? else {
            return Ok(None);
        };
        Ok(self.project(caller, std::slice::from_ref(&p)).await?.pop())
    }

    fn order_failure(e: UpstreamOrderError) -> Failure {
        match e {
            UpstreamOrderError::InsufficientBalance => Failure::new(
                402,
                "insufficient_balance",
                "wallet balance is insufficient",
            ),
            UpstreamOrderError::InsufficientStock => {
                Failure::new(409, "insufficient_stock", "product stock is insufficient")
            }
            UpstreamOrderError::ProductUnavailable => {
                Failure::new(400, "product_unavailable", "product is not available")
            }
            UpstreamOrderError::SkuUnavailable => {
                Failure::new(400, "sku_unavailable", "sku is invalid or not available")
            }
            UpstreamOrderError::InvalidItem => {
                Failure::new(400, "bad_request", "invalid order parameters")
            }
            UpstreamOrderError::ManualFormInvalid(m) => Failure::new(
                400,
                "bad_request",
                format!("manual form data is invalid: {m}"),
            ),
            UpstreamOrderError::NotFound => Failure::new(404, "order_not_found", "order not found"),
            UpstreamOrderError::CancelNotAllowed => Failure::new(
                409,
                "cancel_not_allowed",
                "order cannot be canceled in current status",
            ),
            UpstreamOrderError::DuplicateDownstreamNo => {
                Failure::new(409, "duplicate_order", "downstream order already exists")
            }
            UpstreamOrderError::Internal(error) => {
                tracing::error!(%error, "upstream order failed");
                Failure::internal("failed to create order")
            }
        }
    }

    fn summary_json(s: &UpstreamOrderSummary) -> Value {
        json!({
            "ok": true,
            "order_id": s.order_id,
            "order_no": s.order_no,
            "status": s.status,
            "amount": s.amount.to_string(),
            "currency": s.currency,
        })
    }

    /// Existing order of a `(credential, downstream_order_no)` pair (idempotency, UPS-10).
    async fn existing_order(&self, caller: &Caller, downstream_no: &str) -> Option<Value> {
        if downstream_no.is_empty() {
            return None;
        }
        let r = self
            .refs
            .find_by_downstream_no(caller.credential_id, downstream_no)
            .await
            .ok()
            .flatten()?;
        let view = self
            .ordering
            .get(caller.user_id, r.order_id)
            .await
            .ok()
            .flatten()?;
        let currency = self.currency().await;
        Some(json!({
            "ok": true,
            "order_id": view.order_id,
            "order_no": view.order_no,
            "status": view.status,
            "amount": view.amount.to_string(),
            "currency": currency,
        }))
    }

    /// `POST /upstream/orders`: validates, dedupes on the downstream order number,
    /// checks SKU / product availability and delegates to the order group.
    pub async fn create_order(
        &self,
        caller: &Caller,
        req: &OrderRequest,
    ) -> std::result::Result<Value, Failure> {
        if req.sku_id <= 0 || req.quantity < 1 {
            return Err(Failure::new(
                400,
                "bad_request",
                "invalid request body: sku_id and quantity are required",
            ));
        }
        let downstream_no = req.downstream_order_no.trim();
        if downstream_no.len() > MAX_REF_LEN || req.trace_id.len() > MAX_REF_LEN {
            return Err(Failure::new(
                400,
                "bad_request",
                "invalid request body: field too long",
            ));
        }
        if !req.callback_url.trim().is_empty()
            && let Err(msg) = validate_callback_url(&req.callback_url)
        {
            return Err(Failure::new(400, "invalid_callback_url", msg));
        }
        if let Some(existing) = self.existing_order(caller, downstream_no).await {
            return Ok(existing);
        }
        let product = self
            .catalog
            .product_of_sku(req.sku_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "upstream sku lookup failed");
                Failure::internal("failed to create order")
            })?;
        let Some((product, sku_active)) = product else {
            return Err(Failure::new(400, "sku_unavailable", "sku not found"));
        };
        if !sku_active {
            return Err(Failure::new(400, "sku_unavailable", "sku is not active"));
        }
        if !product.is_active {
            return Err(Failure::new(
                400,
                "product_unavailable",
                "product is not available",
            ));
        }
        let manual_form_data =
            if product.fulfillment_type == zs_domain::catalog::product::FulfillmentType::Manual {
                req.manual_form_data.clone()
            } else {
                None
            };
        let place = PlaceUpstreamOrder {
            user_id: caller.user_id,
            credential_id: caller.credential_id,
            product_id: product.id,
            sku_id: req.sku_id,
            quantity: req.quantity,
            manual_form_data,
            downstream_order_no: downstream_no.to_owned(),
            trace_id: req.trace_id.clone(),
            callback_url: req.callback_url.trim().to_owned(),
            client_ip: req.client_ip.clone(),
        };
        match self.ordering.place(&place).await {
            Ok(PlaceOutcome::Placed(s)) => Ok(Self::summary_json(&s)),
            Ok(PlaceOutcome::PaymentFailed {
                order_id,
                order_no,
                message,
            }) => Ok(json!({
                "ok": false,
                "order_id": order_id,
                "order_no": order_no,
                "status": "canceled",
                "error_code": "payment_failed",
                "error_message": format!("wallet payment failed: {message}"),
            })),
            Err(UpstreamOrderError::DuplicateDownstreamNo) => self
                .existing_order(caller, downstream_no)
                .await
                .ok_or_else(|| Self::order_failure(UpstreamOrderError::DuplicateDownstreamNo)),
            Err(e) => Err(Self::order_failure(e)),
        }
    }

    /// `GET /upstream/orders/:id` (supplier-side refund status wins, UPS-16).
    pub async fn get_order(
        &self,
        caller: &Caller,
        order_id: Id,
    ) -> std::result::Result<Value, Failure> {
        let view: UpstreamOrderView = match self.ordering.get(caller.user_id, order_id).await {
            Ok(Some(v)) => v,
            Ok(None) => return Err(Failure::new(404, "order_not_found", "order not found")),
            Err(error) => {
                tracing::error!(%error, order_id, "upstream get order failed");
                return Err(Failure::internal("failed to get order"));
            }
        };
        let mut status = view.status.trim().to_lowercase();
        if let Ok(Some(p)) = self
            .procurement
            .find_by_local_order_no(&view.order_no)
            .await
        {
            use zs_domain::integration::procurement::ProcurementStatus as S;
            if matches!(p.status, S::Refunded | S::PartiallyRefunded) {
                status = p.status.as_str().to_owned();
            }
        }
        let mut out = json!({
            "ok": true,
            "order_id": view.order_id,
            "order_no": view.order_no,
            "status": status,
            "amount": view.amount.to_string(),
            "refunded_amount": view.refunded_amount.to_string(),
            "currency": view.currency,
            "refund_records": view.refund_records,
        });
        if let Some(obj) = out.as_object_mut() {
            if let Some(f) = view
                .fulfillment
                .as_ref()
                .filter(|f| f.status == "delivered")
            {
                obj.insert(
                    "fulfillment".into(),
                    json!({
                        "type": f.kind,
                        "status": f.status,
                        "payload": f.payload,
                        "delivery_data": f.delivery_data,
                        "delivered_at": f.delivered_at,
                    }),
                );
            }
            if !view.items.is_empty() {
                obj.insert("items".into(), json!(view.items));
            }
        }
        Ok(out)
    }

    /// `POST /upstream/orders/:id/cancel`.
    pub async fn cancel_order(
        &self,
        caller: &Caller,
        order_id: Id,
    ) -> std::result::Result<Value, Failure> {
        match self.ordering.cancel(caller.user_id, order_id).await {
            Ok(s) => Ok(json!({
                "ok": true,
                "order_id": s.order_id,
                "order_no": s.order_no,
                "status": s.status,
            })),
            Err(UpstreamOrderError::Internal(error)) => {
                tracing::error!(%error, order_id, "upstream cancel failed");
                Err(Failure::internal("failed to cancel order"))
            }
            Err(e) => Err(Self::order_failure(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validate_callback_url;

    // UPS-10: callback URL literal checks.
    #[test]
    fn ups10_callback_url_validation() {
        for bad in [
            "http://10.0.0.1/x",
            "http://localhost/",
            "ftp://a.com",
            "http://127.0.0.1:6379",
            "http://169.254.169.254/latest",
            "http://[::1]/cb",
            "http://0.0.0.0/",
            "http://100.64.1.1/",
            "http://user@192.168.1.1/",
            "https:///nohost",
        ] {
            assert!(validate_callback_url(bad).is_err(), "{bad}");
        }
        for good in [
            "https://shop.example.com/api/v1/upstream/callback",
            "http://8.8.8.8:8080/cb",
        ] {
            assert!(validate_callback_url(good).is_ok(), "{good}");
        }
    }
}
