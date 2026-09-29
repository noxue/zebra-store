//! Legacy supplier adapter:
//! HMAC-signed requests (`zs_shared::sign`, path without query), SSRF-safe transport,
//! structured error mapping, and verification of its signed order callbacks.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use zs_domain::Id;
use zs_domain::integration::adapter::{
    AdapterMeta, Capabilities, Capability, ConfigField, FieldKind, HandshakeInfo, InboundError,
    InboundEvent, InboundKind, InboundRequest, OrderNotice, OrderRef, PlaceOrder, SupplierAdapter,
    text3,
};
use zs_domain::integration::connection::{Endpoint, PROTOCOL_DUJIAO_NEXT};
use zs_domain::integration::downstream::CALLBACK_SIGN_PATH;
use zs_domain::integration::protocol::{
    CallbackPayload, CategoryList, CreateOrderRequest, CreateOrderResponse, Download, PingInfo,
    ProductPage, ProductQuery, RemoteCategory, RemoteOrder, RemoteProduct, UpstreamClient,
    UpstreamError,
};
use zs_shared::clock::Clock;
use zs_shared::sign;

use super::http::{AddressPolicy, build_client, check_url, is_forbidden, read_limited};

/// Whole-request timeout of supplier calls (original client timeout 30 s).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Largest accepted supplier answer.
pub(crate) const MAX_RESPONSE_BYTES: usize = 16 << 20;
/// Default largest downloaded image (overridden by `upload.max_size`).
const DEFAULT_MAX_DOWNLOAD_BYTES: usize = 10 << 20;

/// Outbound HTTP shared by every adapter: SSRF policy, clock and download cap.
#[derive(Clone)]
pub struct HttpConnector {
    pub(crate) http: reqwest::Client,
    pub(crate) policy: AddressPolicy,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) max_download: usize,
}

impl std::fmt::Debug for HttpConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpConnector")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl HttpConnector {
    /// `policy` must be [`AddressPolicy::PublicOnly`] in production; tests against a
    /// local mock server use `AllowPrivate` (redirects stay refused).
    pub fn new(policy: AddressPolicy, clock: Arc<dyn Clock>, max_download: u64) -> Self {
        Self {
            http: build_client(policy, REQUEST_TIMEOUT),
            policy,
            clock,
            max_download: usize::try_from(max_download)
                .ok()
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_MAX_DOWNLOAD_BYTES),
        }
    }

    /// Downloads a supplier image: same origin as `base_url` only, size-capped (UPS-14).
    pub(crate) async fn download(
        &self,
        base_url: &str,
        raw: &str,
    ) -> Result<Download, UpstreamError> {
        let raw = raw.trim();
        let full = if raw.starts_with('/') && !raw.starts_with("//") {
            format!("{base_url}{raw}")
        } else {
            raw.to_owned()
        };
        let url = check_url(self.policy, &full).map_err(UpstreamError::Forbidden)?;
        let base = check_url(self.policy, base_url).map_err(UpstreamError::Forbidden)?;
        if url.origin() != base.origin() {
            return Err(UpstreamError::Forbidden("cross-origin image".into()));
        }
        let res = self
            .http
            .get(url.clone())
            .send()
            .await
            .map_err(|e| transport(&e))?;
        if res.status().as_u16() != 200 {
            return Err(UpstreamError::Http {
                status: res.status().as_u16(),
                code: String::new(),
                message: "download image".into(),
            });
        }
        let bytes = read_limited(res, self.max_download)
            .await
            .map_err(UpstreamError::Transport)?;
        let filename = url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .filter(|s| !s.is_empty())
            .unwrap_or("image.jpg")
            .to_owned();
        Ok(Download { filename, bytes })
    }
}

/// Classifies a failed send by whether the request may have reached the supplier:
/// the SSRF guard, request building and connection establishment (DNS, refused /
/// connect timeout, TLS handshake — all `is_connect`) happen before anything is sent;
/// every later failure (timeout while waiting, reset / closed mid-answer) is
/// [`UpstreamError::Uncertain`].
pub(crate) fn transport(e: &reqwest::Error) -> UpstreamError {
    if is_forbidden(e) {
        UpstreamError::Forbidden("resolved to a non-public address".into())
    } else if e.is_connect() || e.is_builder() {
        UpstreamError::Transport(e.to_string())
    } else {
        UpstreamError::Uncertain(e.to_string())
    }
}

/// The answer arrived but could not be read completely (reset, truncated, too large):
/// the supplier received the request.
pub(crate) fn body_lost(reason: String) -> UpstreamError {
    UpstreamError::Uncertain(format!("read answer: {reason}"))
}

/// Turns an HTTP redirect into an actionable error. Supplier API calls must not
/// follow redirects: doing so would change the signed path and could cross an
/// SSRF trust boundary. In practice redirects here usually come from a login
/// page or a browser-only WAF / anti-bot challenge in front of the API.
pub(crate) fn redirect_error(res: &reqwest::Response) -> Option<UpstreamError> {
    let status = res.status();
    if !status.is_redirection() {
        return None;
    }
    let destination = res
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let message = if destination.is_empty() {
        "API request was redirected; configure the upstream reverse proxy or WAF to allow API paths without a browser challenge".to_owned()
    } else {
        format!(
            "API request was redirected to {destination}; configure the upstream reverse proxy or WAF to allow API paths without a browser challenge"
        )
    };
    Some(UpstreamError::Http {
        status: status.as_u16(),
        code: "api_redirected".into(),
        message,
    })
}

/// A received answer that is not the expected JSON (truncated, proxy error page …).
pub(crate) fn unreadable(raw: &[u8]) -> UpstreamError {
    UpstreamError::Uncertain(format!(
        "unreadable answer: {}",
        String::from_utf8_lossy(raw)
            .chars()
            .take(200)
            .collect::<String>()
    ))
}

/// Connection form fields shared by the key/secret based systems.
pub(crate) fn key_secret_fields() -> Vec<ConfigField> {
    vec![
        ConfigField {
            key: "base_url",
            label: text3("站点地址", "站點地址", "Site URL"),
            kind: FieldKind::Url,
            required: true,
            placeholder: text3(
                "https://supplier.example.com",
                "https://supplier.example.com",
                "https://supplier.example.com",
            ),
            options: Vec::new(),
        },
        ConfigField {
            key: "api_key",
            label: text3("API Key", "API Key", "API key"),
            kind: FieldKind::Text,
            required: true,
            placeholder: text3(
                "供货方签发的 API Key",
                "供貨方簽發的 API Key",
                "API key issued by the supplier",
            ),
            options: Vec::new(),
        },
        ConfigField {
            key: "api_secret",
            label: text3("API Secret", "API Secret", "API secret"),
            kind: FieldKind::Secret,
            required: true,
            placeholder: text3(
                "供货方签发的 API Secret",
                "供貨方簽發的 API Secret",
                "API secret issued by the supplier",
            ),
            options: Vec::new(),
        },
    ]
}

/// The legacy compatibility system.
#[derive(Debug, Clone)]
pub struct DujiaoNextAdapter {
    ctx: HttpConnector,
}

impl DujiaoNextAdapter {
    pub fn new(ctx: HttpConnector) -> Self {
        Self { ctx }
    }

    fn capabilities() -> Capabilities {
        Capabilities::of(&[Capability::Categories])
    }
}

impl SupplierAdapter for DujiaoNextAdapter {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: PROTOCOL_DUJIAO_NEXT,
            name: text3("独角数卡 Next", "獨角數卡 Next", "Dujiao-Next"),
            description: text3(
                "兼容传统上游协议：定时全量/增量同步，单商品下单，回调通知。",
                "相容 dujiao-next 原專案及其所有站點：定時全量/增量同步，單商品下單，回調通知。",
                "Compatible with dujiao-next and every site running it: periodic full/incremental sync, single-item orders, signed callbacks.",
            ),
            fields: key_secret_fields(),
            capabilities: Self::capabilities(),
            supports_connection_code: false,
            inbound_path: CALLBACK_SIGN_PATH,
        }
    }

    fn open(&self, endpoint: &Endpoint) -> zs_domain::Result<Arc<dyn UpstreamClient>> {
        Ok(Arc::new(DujiaoNextClient {
            ctx: self.ctx.clone(),
            base_url: endpoint.base_url.trim_end_matches('/').to_owned(),
            api_key: endpoint.api_key.clone(),
            api_secret: endpoint.api_secret.clone(),
        }))
    }

    fn inbound_key(
        &self,
        req: &InboundRequest<'_>,
        now: DateTime<Utc>,
    ) -> Result<String, InboundError> {
        let key = req.header(sign::HEADER_API_KEY);
        let ts = req.header(sign::HEADER_TIMESTAMP);
        if key.is_empty() || ts.is_empty() || req.header(sign::HEADER_SIGNATURE).is_empty() {
            return Err(InboundError::MissingHeaders);
        }
        let ts: i64 = ts.parse().map_err(|_| InboundError::InvalidTimestamp)?;
        if !sign::timestamp_valid(ts, now.timestamp()) {
            return Err(InboundError::TimestampExpired);
        }
        Ok(key)
    }

    fn parse_inbound(
        &self,
        req: &InboundRequest<'_>,
        secret: &str,
        _now: DateTime<Utc>,
    ) -> Result<Vec<InboundEvent>, InboundError> {
        let ts: i64 = req
            .header(sign::HEADER_TIMESTAMP)
            .parse()
            .map_err(|_| InboundError::InvalidTimestamp)?;
        let signature = req.header(sign::HEADER_SIGNATURE);
        if !sign::verify(secret, "POST", CALLBACK_SIGN_PATH, &signature, ts, req.body) {
            return Err(InboundError::InvalidSignature);
        }
        let payload: CallbackPayload =
            serde_json::from_slice(req.body).map_err(|_| InboundError::InvalidBody)?;
        if payload.downstream_order_no.is_empty() || payload.status.is_empty() {
            return Err(InboundError::MissingFields);
        }
        Ok(vec![InboundEvent {
            event_id: None,
            kind: InboundKind::Order(OrderNotice {
                downstream_order_no: payload.downstream_order_no,
                upstream: OrderRef {
                    id: payload.order_id,
                    no: payload.order_no,
                },
                status: payload.status,
                fulfillment: payload.fulfillment,
            }),
        }])
    }
}

/// Client bound to one connection.
struct DujiaoNextClient {
    ctx: HttpConnector,
    base_url: String,
    api_key: String,
    api_secret: String,
}

impl DujiaoNextClient {
    /// Sends a signed request and returns the 200 body; other statuses become
    /// [`UpstreamError::Http`] with the parsed `error_code` / `error_message`.
    async fn request(
        &self,
        method: reqwest::Method,
        path_and_query: &str,
        body: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, UpstreamError> {
        let url = check_url(
            self.ctx.policy,
            &format!("{}{path_and_query}", self.base_url),
        )
        .map_err(UpstreamError::Forbidden)?;
        let sign_path = path_and_query.split('?').next().unwrap_or(path_and_query);
        let ts = self.ctx.clock.now().timestamp();
        let body_bytes = body.clone().unwrap_or_default();
        let signature = sign::sign(
            &self.api_secret,
            method.as_str(),
            sign_path,
            ts,
            &body_bytes,
        );
        let mut req = self
            .ctx
            .http
            .request(method.clone(), url)
            .header(sign::HEADER_API_KEY, &self.api_key)
            .header(sign::HEADER_TIMESTAMP, ts.to_string())
            .header(sign::HEADER_SIGNATURE, signature);
        if let Some(b) = body {
            req = req
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(b);
        }
        let res = req.send().await.map_err(|e| transport(&e))?;
        if let Some(error) = redirect_error(&res) {
            return Err(error);
        }
        let status = res.status().as_u16();
        let bytes = read_limited(res, MAX_RESPONSE_BYTES)
            .await
            .map_err(body_lost)?;
        if status != 200 {
            let parsed: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            let text = |k: &str| {
                parsed
                    .get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            tracing::warn!(method = %method, path = sign_path, status, "upstream request error");
            let mut message = text("error_message");
            if message.is_empty() {
                message = String::from_utf8_lossy(&bytes).chars().take(500).collect();
            }
            return Err(UpstreamError::Http {
                status,
                code: text("error_code"),
                message,
            });
        }
        Ok(bytes)
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path_and_query: &str,
        body: Option<&impl Serialize>,
    ) -> Result<T, UpstreamError> {
        let body = match body {
            Some(b) => {
                Some(serde_json::to_vec(b).map_err(|e| UpstreamError::Protocol(e.to_string()))?)
            }
            None => None,
        };
        let bytes = self.request(method, path_and_query, body).await?;
        serde_json::from_slice(&bytes).map_err(|_| unreadable(&bytes))
    }
}

fn ok_flag(v: &Value) -> bool {
    v.get("ok").and_then(Value::as_bool).unwrap_or(false)
}

#[async_trait]
impl UpstreamClient for DujiaoNextClient {
    fn capabilities(&self) -> Capabilities {
        DujiaoNextAdapter::capabilities()
    }

    async fn handshake(&self) -> Result<HandshakeInfo, UpstreamError> {
        let ping = self.ping().await?;
        Ok(HandshakeInfo {
            protocol: PROTOCOL_DUJIAO_NEXT.to_owned(),
            version: ping.protocol_version,
            site_name: ping.site_name,
            site_url: self.base_url.clone(),
            currency: ping.currency.clone(),
            features: DujiaoNextAdapter::capabilities()
                .names()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            limits: serde_json::Map::new(),
            user_id: ping.user_id,
            balance: ping.balance,
            account_currency: ping.currency,
            member_level: ping.member_level,
            change_head: None,
            server_time: None,
        })
    }

    async fn ping(&self) -> Result<PingInfo, UpstreamError> {
        let v: Value = self
            .call(reqwest::Method::POST, "/api/v1/upstream/ping", None::<&()>)
            .await?;
        if !ok_flag(&v) {
            return Err(UpstreamError::Protocol("ping failed".into()));
        }
        serde_json::from_value(v).map_err(|e| UpstreamError::Protocol(e.to_string()))
    }

    async fn list_categories(&self) -> Result<CategoryList, UpstreamError> {
        match self
            .call::<Value>(
                reqwest::Method::GET,
                "/api/v1/upstream/categories",
                None::<&()>,
            )
            .await
        {
            Ok(v) => {
                let categories: Vec<RemoteCategory> = v
                    .get("categories")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|c| serde_json::from_value(c.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(CategoryList {
                    supported: true,
                    categories,
                })
            }
            // Old suppliers have no category endpoint.
            Err(UpstreamError::Http { status: 404, .. }) => Ok(CategoryList {
                supported: false,
                categories: Vec::new(),
            }),
            Err(e) => Err(e),
        }
    }

    async fn list_products(&self, q: &ProductQuery) -> Result<ProductPage, UpstreamError> {
        let mut path = format!(
            "/api/v1/upstream/products?page={}&page_size={}",
            q.page, q.page_size
        );
        if let Some(after) = q.updated_after {
            path.push_str("&updated_after=");
            path.push_str(
                &after
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                    .replace('+', "%2B"),
            );
        }
        if q.include_inactive {
            path.push_str("&include_inactive=true");
        }
        self.call(reqwest::Method::GET, &path, None::<&()>).await
    }

    async fn get_product(&self, id: Id) -> Result<RemoteProduct, UpstreamError> {
        let path = format!("/api/v1/upstream/products/{id}");
        match self
            .call::<Value>(reqwest::Method::GET, &path, None::<&()>)
            .await
        {
            Ok(v) => serde_json::from_value(v.get("product").cloned().unwrap_or(Value::Null))
                .map_err(|e| UpstreamError::Protocol(e.to_string())),
            Err(UpstreamError::Http { code, .. })
                if code == "product_deleted" || code == "product_not_found" =>
            {
                Err(UpstreamError::ProductDeleted)
            }
            Err(UpstreamError::Http { code, .. }) if code == "product_unavailable" => {
                Err(UpstreamError::ProductUnavailable)
            }
            Err(e) => Err(e),
        }
    }

    async fn place_order(&self, req: &PlaceOrder) -> Result<CreateOrderResponse, UpstreamError> {
        let [line] = req.lines.as_slice() else {
            return Err(UpstreamError::Protocol(
                "dujiao-next orders take exactly one item".into(),
            ));
        };
        let body = CreateOrderRequest {
            sku_id: line.sku_id,
            quantity: line.quantity,
            manual_form_data: line.manual_form_data.clone(),
            downstream_order_no: req.downstream_order_no.clone(),
            trace_id: req.trace_id.clone(),
            callback_url: req.callback_url.clone(),
        };
        self.call(
            reqwest::Method::POST,
            "/api/v1/upstream/orders",
            Some(&body),
        )
        .await
    }

    async fn get_order(&self, order: &OrderRef) -> Result<RemoteOrder, UpstreamError> {
        let path = format!("/api/v1/upstream/orders/{}", order.id);
        self.call(reqwest::Method::GET, &path, None::<&()>).await
    }

    async fn cancel_order(&self, order: &OrderRef) -> Result<(), UpstreamError> {
        let path = format!("/api/v1/upstream/orders/{}/cancel", order.id);
        let v: Value = self.call(reqwest::Method::POST, &path, None::<&()>).await?;
        if ok_flag(&v) {
            Ok(())
        } else {
            Err(UpstreamError::Protocol("cancel order failed".into()))
        }
    }

    async fn download(&self, raw: &str) -> Result<Download, UpstreamError> {
        self.ctx.download(&self.base_url, raw).await
    }
}

#[cfg(test)]
mod transport_tests {

    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::integration::http::{AddressPolicy, build_client};

    async fn post(port: u16) -> UpstreamError {
        let http = build_client(AddressPolicy::AllowPrivate, Duration::from_millis(300));
        let e = http
            .post(format!("http://127.0.0.1:{port}/trade"))
            .body("sku_id=1")
            .send()
            .await
            .unwrap_err();
        transport(&e)
    }

    /// A supplier that accepts the request, optionally hangs, sends `reply` and closes.
    async fn supplier(reply: &'static [u8], hang: bool) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0_u8; 1024];
            let _ = sock.read(&mut buf).await;
            if hang {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            let _ = sock.write_all(reply).await;
            // dropped: connection closed / reset mid-answer
        });
        port
    }

    #[tokio::test]
    async fn refused_connection_was_never_sent() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let e = post(port).await;
        assert!(matches!(e, UpstreamError::Transport(_)), "{e:?}");
        assert!(!e.may_have_executed());
    }

    #[tokio::test]
    async fn accepted_then_hanging_may_have_executed() {
        let e = post(supplier(b"", true).await).await;
        assert!(matches!(e, UpstreamError::Uncertain(_)), "{e:?}");
        assert!(e.may_have_executed());
    }

    #[tokio::test]
    async fn accepted_then_closed_may_have_executed() {
        let e = post(supplier(b"", false).await).await;
        assert!(e.may_have_executed(), "{e:?}");
        let e = post(supplier(b"HTTP/1.1 200 OK\r\nContent-Le", false).await).await;
        assert!(e.may_have_executed(), "{e:?}");
    }

    #[test]
    fn unreadable_answers_may_have_executed() {
        assert!(unreadable(b"{\"code\":200").may_have_executed());
        assert!(body_lost("connection reset".into()).may_have_executed());
    }
}
