//! `mcy-shop` (萌次元商城) supplier adapter: buyer-side client of the OpenApi plugin
//! `/plugin/open-api/*` (docs/protocol/third-party/mcy-shop.md §2; the protocol is
//! reverse-engineered from its two open clients). Business fields are sent as a form,
//! authenticated by the `Api-Id` / `Api-Signature` headers (md5 signature without
//! array fields); full catalog, SKU-level prices and stock, synchronous `trade`.
//!
//! Protocol limits handled here (spec §4.1):
//! - categories are names only: their ids are a stable hash of the name;
//! - there is no order lookup and no callback: the `contents` returned by `trade` is
//!   handed to the procurement core as the order's synchronous delivery
//!   (`CreateOrderResponse::fulfillment`), which persists it with the supplier order
//!   number; after-sale texts instead of goods hold the purchase for manual review;
//! - `trade_no` deduplication is unverified, so a `trade` whose answer is lost is never
//!   retried: it is reported as [`RESULT_UNKNOWN`] for a manual check.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use zs_domain::Id;
use zs_domain::catalog::product::JsonMap;
use zs_domain::catalog::stock::StockPolicy;
use zs_domain::integration::adapter::{
    AdapterMeta, Capabilities, Capability, HandshakeInfo, InboundError, InboundEvent,
    InboundRequest, OrderRef, PlaceOrder, SupplierAdapter, text3,
};
use zs_domain::integration::connection::Endpoint;
use zs_domain::integration::procurement::RESULT_UNKNOWN;
use zs_domain::integration::protocol::{
    CategoryList, CreateOrderResponse, Download, PingInfo, ProductPage, ProductQuery,
    RemoteCategory, RemoteFulfillment, RemoteOrder, RemoteProduct, RemoteSku, UpstreamClient,
    UpstreamError,
};

use super::acg_faka::{
    cover, currency_of, form_schema, form_value, int, lizhipay_fields, money, not_found, scalar,
    text, widget_names, zh,
};
use super::client::{HttpConnector, MAX_RESPONSE_BYTES, body_lost, transport, unreadable};
use super::http::{check_url, read_limited};
use super::php_form::{self, ArrayFields, Form, form, md5_hex};

/// Protocol id stored in `site_connections.protocol`.
pub const PROTOCOL_ID: &str = "mcy-shop";

/// Route prefix of the OpenApi plugin (`OpenApi` → `open-api`, spec §0).
const PREFIX: &str = "/plugin/open-api";
pub const HEADER_ID: &str = "Api-Id";
pub const HEADER_SIGNATURE: &str = "Api-Signature";
/// Success `code` of the envelope.
const OK_CODE: i64 = 200;
/// `trade_no` length: acg-faka's client sends `substr(md5(order no), 0, 24)` (spec §2.3).
const TRADE_NO_LEN: usize = 24;
/// Status reported by `get_order`: the plugin has no order lookup, deliveries only
/// come with the `trade` answer (stored by the procurement core).
const STATUS_UNKNOWN: &str = "delivery_unknown";
/// Longest supplier text kept in logs and error messages.
const MSG_MAX_CHARS: usize = 300;
/// Texts mcy-shop / its clients put in `contents` instead of goods
/// (`RepertoryOrder.php:209-235`, acg-faka `Bind/Shared.php:417`).
const AFTER_SALE_MARKERS: [&str; 5] = [
    "申请售后",
    "发货失败",
    "没有发货信息",
    "正在发货中",
    "库存不足",
];
/// Label of the SKU dimension in spec values.
const SKU_LABEL: &str = "规格";

/// Failure of one call: transport / HTTP / protocol, or a business refusal.
#[derive(Debug)]
enum Fail {
    Upstream(UpstreamError),
    Refused(String),
}

impl From<UpstreamError> for Fail {
    fn from(e: UpstreamError) -> Self {
        Self::Upstream(e)
    }
}

impl From<Fail> for UpstreamError {
    fn from(f: Fail) -> Self {
        match f {
            Fail::Upstream(e) => e,
            Fail::Refused(message) => UpstreamError::Http {
                status: 200,
                code: error_code(&message).to_owned(),
                message,
            },
        }
    }
}

/// Neutral error code of a refusal (the plugin's codes are unknown, only `msg` texts):
/// every refusal is permanent — its `trade` must never be repeated blindly.
pub(crate) fn error_code(msg: &str) -> &'static str {
    let has = |needles: &[&str]| needles.iter().any(|n| msg.contains(n));
    if has(&["余额"]) {
        "insufficient_balance"
    } else if has(&["库存", "售罄"]) {
        "product_out_of_stock"
    } else if has(&["签名", "密钥", "Signature", "Api-Id", "API"]) {
        "unauthorized"
    } else if has(&["不存在", "下架", "停售", "未上架"]) {
        "product_unavailable"
    } else {
        "invalid_request"
    }
}

/// Deterministic `trade_no` of a purchase (24 hex chars, like acg-faka's client).
pub(crate) fn trade_no(order_key: &str) -> String {
    md5_hex(order_key.as_bytes())[..TRADE_NO_LEN].to_owned()
}

/// Stable positive id of a category name (FNV-1a, 31 bits).
pub(crate) fn category_id(name: &str) -> Id {
    let mut h: u32 = 0x811c_9dc5;
    for b in name.trim().bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    i64::from(h & 0x7fff_ffff) + 1
}

/// SKU stock: integer or numeric text; `null` / non-numeric / negative = unlimited
/// (`-1`, like gmshop-edge), spec §2.2.
pub(crate) fn stock_of(v: Option<&Value>) -> i64 {
    let n = match v {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.trim().parse::<i64>().ok(),
        _ => None,
    };
    match n {
        Some(q) if q >= 0 => q,
        _ => -1,
    }
}

fn is_after_sale(contents: &str) -> bool {
    let c = contents.trim();
    c.is_empty() || AFTER_SALE_MARKERS.iter().any(|m| c.contains(m))
}

/// mcy widget (`title,name,placeholder,type,regex,error,data`) in acg-faka's shape
/// (`cn`, options as `dict`) so both share one schema mapping.
fn acg_widgets(raw: Option<&Value>) -> Value {
    let list = match raw {
        Some(Value::String(s)) => serde_json::from_str::<Vec<Value>>(s).unwrap_or_default(),
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    };
    Value::Array(
        list.iter()
            .map(|w| {
                json!({
                    "cn": text(w, "title"),
                    "name": text(w, "name"),
                    "placeholder": text(w, "placeholder"),
                    "type": text(w, "type"),
                    "regex": text(w, "regex"),
                    "dict": text(w, "data")
                        .split(['\r', '\n'])
                        .map(str::trim)
                        .filter(|o| !o.is_empty())
                        .collect::<Vec<_>>()
                        .join(","),
                })
            })
            .collect(),
    )
}

/// Supplier text shortened for logs and stored messages.
fn clip(s: &str) -> String {
    s.chars().take(MSG_MAX_CHARS).collect()
}

/// The `mcy-shop` system.
#[derive(Debug, Clone)]
pub struct McyShopAdapter {
    ctx: HttpConnector,
}

impl McyShopAdapter {
    pub fn new(ctx: HttpConnector) -> Self {
        Self { ctx }
    }

    /// Only categories (synthesised from names): `amount` does not lock prices, the
    /// `trade_no` deduplication is unverified, no feed / push / multi-item / encryption.
    fn capabilities() -> Capabilities {
        Capabilities::of(&[Capability::Categories])
    }
}

impl SupplierAdapter for McyShopAdapter {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: PROTOCOL_ID,
            name: text3("萌次元商城", "萌次元商城", "MCY-Shop"),
            description: text3(
                "对接萌次元商城（mcy-shop）安装了 OpenApi 插件的站点：填写个人中心的 API-ID 与 API 密钥。全量目录同步、SKU 级价格与库存、余额下单同步交付；对方没有查单与回调，结果不明的订单会标记为待人工核对，绝不重复下单。",
                "對接萌次元商城（mcy-shop）安裝了 OpenApi 外掛的站點：填寫個人中心的 API-ID 與 API 密鑰。全量目錄同步、SKU 級價格與庫存、餘額下單同步交付；對方沒有查單與回調，結果不明的訂單會標記為待人工核對，絕不重複下單。",
                "Buys from mcy-shop sites running the OpenApi plugin: enter the API-ID and API key of your account there. Full catalog sync, SKU prices and stock, balance-paid orders delivered synchronously; the supplier has no order lookup or callbacks, orders with an unknown outcome are flagged for a manual check and never bought twice.",
            ),
            fields: lizhipay_fields(
                ["API-ID（用户 ID）", "API-ID（用戶 ID）", "API-ID (user id)"],
                ["API 密钥", "API 密鑰", "API key"],
            ),
            capabilities: Self::capabilities(),
            supports_connection_code: false,
            inbound_path: "",
        }
    }

    fn open(&self, endpoint: &Endpoint) -> zs_domain::Result<Arc<dyn UpstreamClient>> {
        Ok(Arc::new(McyClient {
            ctx: self.ctx.clone(),
            base_url: endpoint.base_url.trim().trim_end_matches('/').to_owned(),
            api_id: endpoint.api_key.trim().to_owned(),
            app_key: endpoint.api_secret.clone(),
            currency: currency_of(endpoint),
            items: Mutex::new(None),
        }))
    }

    /// mcy-shop never calls its buyers back.
    fn inbound_key(
        &self,
        _req: &InboundRequest<'_>,
        _now: DateTime<Utc>,
    ) -> Result<String, InboundError> {
        Err(InboundError::MissingHeaders)
    }

    fn parse_inbound(
        &self,
        _req: &InboundRequest<'_>,
        _secret: &str,
        _now: DateTime<Utc>,
    ) -> Result<Vec<InboundEvent>, InboundError> {
        Err(InboundError::InvalidBody)
    }
}

/// Client bound to one connection (`items` fetched once per client).
struct McyClient {
    ctx: HttpConnector,
    base_url: String,
    api_id: String,
    app_key: String,
    currency: String,
    items: Mutex<Option<Arc<Vec<Value>>>>,
}

impl McyClient {
    /// Signed form POST; `data` of `code == 200`, [`Fail::Refused`] with `msg` else.
    async fn post(&self, endpoint: &str, fields: Form) -> Result<Value, Fail> {
        let url = check_url(
            self.ctx.policy,
            &format!("{}{PREFIX}/{endpoint}", self.base_url),
        )
        .map_err(UpstreamError::Forbidden)?;
        let signature = php_form::signature(&fields, &self.app_key, ArrayFields::Excluded);
        let res = self
            .ctx
            .http
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(HEADER_ID, &self.api_id)
            .header(HEADER_SIGNATURE, signature)
            .body(php_form::encode(&fields))
            .send()
            .await
            .map_err(|e| transport(&e))?;
        let status = res.status().as_u16();
        let raw = read_limited(res, MAX_RESPONSE_BYTES)
            .await
            .map_err(body_lost)?;
        if status != 200 {
            tracing::warn!(endpoint, status, "mcy-shop request error");
            return Err(Fail::Upstream(UpstreamError::Http {
                status,
                code: String::new(),
                message: String::from_utf8_lossy(&raw).chars().take(200).collect(),
            }));
        }
        let parsed: Value = serde_json::from_slice(&raw).map_err(|_| unreadable(&raw))?;
        if parsed.get("code").is_none() {
            return Err(Fail::Upstream(unreadable(&raw)));
        }
        if int(&parsed, "code") == OK_CODE {
            return Ok(parsed.get("data").cloned().unwrap_or(Value::Null));
        }
        let msg = clip(&text(&parsed, "msg"));
        tracing::info!(endpoint, msg, "mcy-shop refused");
        Err(Fail::Refused(if msg.is_empty() {
            "unknown error".into()
        } else {
            msg
        }))
    }

    async fn items(&self) -> Result<Arc<Vec<Value>>, Fail> {
        if let Some(i) = self
            .items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            return Ok(i);
        }
        let data = self.post("items", Vec::new()).await?;
        let items = Arc::new(data.as_array().cloned().unwrap_or_default());
        *self.items.lock().unwrap_or_else(PoisonError::into_inner) = Some(items.clone());
        Ok(items)
    }
}

fn skus_of(item: &Value) -> Vec<Value> {
    item.get("sku")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn category_name(item: &Value) -> String {
    item.get("category")
        .map(|c| text(c, "name"))
        .unwrap_or_default()
}

/// Neutral product of an OpenApi item.
fn product_of(item: &Value) -> RemoteProduct {
    let widgets = acg_widgets(item.get("widget"));
    let mut skus = Vec::new();
    let mut seen = HashSet::new();
    for s in skus_of(item) {
        let id = int(&s, "id");
        if id <= 0 || !seen.insert(id) {
            continue;
        }
        let stock = stock_of(s.get("stock"));
        let name = text(&s, "name");
        let mut spec = JsonMap::new();
        if !name.is_empty() {
            spec.insert(SKU_LABEL.into(), Value::String(name.clone()));
        }
        skus.push(RemoteSku {
            id,
            sku_code: name,
            spec_values: spec,
            price_amount: money(&s.get("stock_price").and_then(scalar).unwrap_or_default()),
            stock_status: StockPolicy::UPSTREAM.status(stock).as_str().to_owned(),
            stock_quantity: stock,
            is_active: true,
            ..RemoteSku::default()
        });
    }
    let name = category_name(item);
    RemoteProduct {
        id: int(item, "id"),
        title: zh(&text(item, "name")),
        content: zh(&text(item, "introduce")),
        images: cover(&text(item, "picture_url")),
        price_amount: skus
            .iter()
            .filter_map(|s| s.price_amount.parse::<Decimal>().ok())
            .filter(|p| *p > Decimal::ZERO)
            .min()
            .map(|p| format!("{p:.2}"))
            .unwrap_or_default(),
        // The repertory order delivers synchronously through the supplier's plugin.
        fulfillment_type: "auto".into(),
        manual_form_schema: Some(form_schema(Some(&widgets))),
        is_active: true,
        category_id: if name.is_empty() {
            0
        } else {
            category_id(&name)
        },
        skus,
        ..RemoteProduct::default()
    }
}

#[async_trait]
impl UpstreamClient for McyClient {
    fn capabilities(&self) -> Capabilities {
        McyShopAdapter::capabilities()
    }

    async fn handshake(&self) -> Result<HandshakeInfo, UpstreamError> {
        let d = self.post("connect", Vec::new()).await?;
        Ok(HandshakeInfo {
            protocol: PROTOCOL_ID.to_owned(),
            version: String::new(),
            site_name: text(&d, "username"),
            site_url: self.base_url.clone(),
            currency: self.currency.clone(),
            features: McyShopAdapter::capabilities()
                .names()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            limits: JsonMap::new(),
            user_id: self.api_id.parse().unwrap_or(0),
            balance: money(&text(&d, "balance")),
            account_currency: self.currency.clone(),
            member_level: None,
            change_head: None,
            server_time: None,
        })
    }

    async fn ping(&self) -> Result<PingInfo, UpstreamError> {
        let h = self.handshake().await?;
        Ok(PingInfo {
            site_name: h.site_name,
            protocol_version: h.version,
            user_id: h.user_id,
            balance: h.balance,
            currency: h.account_currency,
            member_level: None,
        })
    }

    async fn list_categories(&self) -> Result<CategoryList, UpstreamError> {
        let items = self.items().await?;
        let mut seen = HashSet::new();
        let mut categories = Vec::new();
        for name in items.iter().map(category_name) {
            if name.is_empty() || !seen.insert(name.clone()) {
                continue;
            }
            let id = category_id(&name);
            categories.push(RemoteCategory {
                id,
                parent_id: 0,
                slug: format!("mcy-{id}"),
                name: zh(&name),
                icon: String::new(),
                sort_order: i64::try_from(categories.len()).unwrap_or(0),
            });
        }
        Ok(CategoryList {
            supported: true,
            categories,
        })
    }

    /// Full `items` paged locally (items missing from it may be delisted: no
    /// deletions are inferred).
    async fn list_products(&self, q: &ProductQuery) -> Result<ProductPage, UpstreamError> {
        let items = self.items().await?;
        let size = usize::try_from(q.page_size)
            .ok()
            .filter(|s| *s > 0)
            .unwrap_or(20);
        let page = usize::try_from(q.page).ok().filter(|p| *p > 0).unwrap_or(1);
        Ok(ProductPage {
            total: i64::try_from(items.len()).unwrap_or(i64::MAX),
            items: items
                .iter()
                .skip((page - 1).saturating_mul(size))
                .take(size)
                .map(product_of)
                .collect(),
            includes_inactive: false,
            next_cursor: String::new(),
            has_more: false,
        })
    }

    async fn get_product(&self, id: Id) -> Result<RemoteProduct, UpstreamError> {
        match self.post("item", form([("id", id.to_string())])).await {
            Ok(item) if item.is_object() => Ok(product_of(&item)),
            Ok(_) => Err(UpstreamError::ProductUnavailable),
            Err(Fail::Refused(msg)) => Err(not_found(&msg)),
            Err(Fail::Upstream(e)) => Err(e),
        }
    }

    /// `sku/state` + `amount` pre-checks, then one `trade`. Any failure of the `trade`
    /// call itself is final: a lost answer is [`RESULT_UNKNOWN`] (manual check).
    async fn place_order(&self, req: &PlaceOrder) -> Result<CreateOrderResponse, UpstreamError> {
        let refused = |code: &str, message: String| CreateOrderResponse {
            ok: false,
            error_code: code.to_owned(),
            error_message: message,
            ..CreateOrderResponse::default()
        };
        let [line] = req.lines.as_slice() else {
            return Err(UpstreamError::Protocol(
                "mcy-shop orders take exactly one item".into(),
            ));
        };
        // Pre-checks are read-only: their failures never bought anything.
        let items = self
            .items()
            .await
            .map_err(|f| UpstreamError::from(f).not_executed())?;
        let Some(item) = items
            .iter()
            .find(|i| skus_of(i).iter().any(|s| int(s, "id") == line.sku_id))
        else {
            return Ok(refused(
                "sku_unavailable",
                "SKU no longer offered by the supplier".into(),
            ));
        };
        let sku = line.sku_id.to_string();
        let quantity = line.quantity.to_string();
        let pre = form([("sku_id", sku.clone()), ("quantity", quantity.clone())]);
        match self.post("sku/state", pre.clone()).await {
            Ok(d) if d.get("state").and_then(Value::as_bool) == Some(false) => {
                return Ok(refused(
                    "product_out_of_stock",
                    "sku/state: not purchasable".into(),
                ));
            }
            Ok(_) => {}
            Err(Fail::Refused(msg)) => return Ok(refused(error_code(&msg), msg)),
            Err(Fail::Upstream(e)) => return Err(e.not_executed()),
        }
        let amount = match self.post("amount", pre).await {
            Ok(d) => money(&text(&d, "amount")),
            Err(Fail::Refused(msg)) => return Ok(refused(error_code(&msg), msg)),
            Err(Fail::Upstream(e)) => return Err(e.not_executed()),
        };
        let key = if req.downstream_order_no.is_empty() {
            &req.idempotency_key
        } else {
            &req.downstream_order_no
        };
        let trade_no = trade_no(key);
        let mut fields = form([
            ("sku_id", sku),
            ("quantity", quantity),
            ("trade_no", trade_no.clone()),
        ]);
        let widgets = acg_widgets(item.get("widget"));
        let names = widget_names(Some(&widgets));
        for (k, v) in line.manual_form_data.iter().flatten() {
            if let Some((_, name)) = names.iter().find(|(lk, _)| lk == &k.to_lowercase())
                && let Some(v) = form_value(v)
            {
                fields.push((name.clone(), v));
            }
        }
        match self.post("trade", fields).await {
            Ok(d) => {
                let contents = text(&d, "contents").replace("\r\n", "\n");
                // MCY-03: after-sale texts are not goods (paid, needs a human).
                let (fulfillment, review) = if is_after_sale(&contents) {
                    (
                        None,
                        Some(format!(
                            "trade {trade_no} was charged but returned no goods ({}); check the order at the supplier",
                            clip(contents.trim())
                        )),
                    )
                } else {
                    (
                        Some(RemoteFulfillment {
                            kind: "auto".into(),
                            status: "delivered".into(),
                            payload: contents.trim().to_owned(),
                            delivery_data: None,
                            delivered_at: None,
                        }),
                        None,
                    )
                };
                Ok(CreateOrderResponse {
                    ok: true,
                    order_id: 0,
                    order_no: trade_no,
                    status: "paid".into(),
                    amount,
                    currency: self.currency.clone(),
                    fulfillment,
                    review,
                    ..CreateOrderResponse::default()
                })
            }
            Err(Fail::Refused(msg)) => Ok(refused(error_code(&msg), msg)),
            // Never sent (DNS, refused connection …): safe to retry later.
            Err(Fail::Upstream(e)) if !e.may_have_executed() => Err(e),
            Err(Fail::Upstream(e)) => Ok(refused(
                RESULT_UNKNOWN,
                format!(
                    "trade {trade_no}: {e}; the supplier may have charged it, check the order manually"
                ),
            )),
        }
    }

    fn addressable(&self, order: &OrderRef) -> bool {
        !order.no.is_empty()
    }

    /// The plugin has no order lookup: the delivery comes with `trade` and is kept by
    /// the procurement core, so this only reports an unknown (non-terminal) status.
    async fn get_order(&self, order: &OrderRef) -> Result<RemoteOrder, UpstreamError> {
        Ok(RemoteOrder {
            order_id: 0,
            order_no: order.no.clone(),
            status: STATUS_UNKNOWN.to_owned(),
            amount: String::new(),
            refunded_amount: String::new(),
            currency: self.currency.clone(),
            fulfillment: None,
            refund_records: Vec::new(),
        })
    }

    async fn cancel_order(&self, _order: &OrderRef) -> Result<(), UpstreamError> {
        Err(UpstreamError::Unsupported)
    }

    async fn download(&self, raw: &str) -> Result<Download, UpstreamError> {
        self.ctx.download(&self.base_url, raw).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zs_domain::integration::procurement::is_retryable_error_code as retryable;

    #[test]
    fn items_map_to_neutral_products() {
        let item = json!({
            "id": 12, "name": "游戏点卡", "introduce": "<p>说明</p>", "picture_url": "https://s.example/a.png",
            "category": {"name": "游戏"},
            "widget": "[{\"title\":\"账号\",\"name\":\"Account\",\"placeholder\":\"\",\"type\":\"text\",\"regex\":\"\",\"error\":\"\",\"data\":\"\"},{\"title\":\"区服\",\"name\":\"server\",\"type\":\"select\",\"data\":\"亚服\\n美服\"}]",
            "sku": [
                {"id": 34, "name": "月卡", "stock_price": "8.5", "stock": 12},
                {"id": 35, "name": "季卡", "stock_price": 25, "stock": null},
                {"id": 36, "name": "年卡", "stock_price": "90.00", "stock": "7"},
                {"id": 34, "name": "dup", "stock_price": "1", "stock": 1}
            ]
        });
        let p = product_of(&item);
        assert_eq!(p.id, 12);
        assert_eq!(p.title["zh-CN"], "游戏点卡");
        assert_eq!(p.category_id, category_id("游戏"));
        assert_eq!(p.price_amount, "8.50");
        assert_eq!(p.images, vec!["https://s.example/a.png"]);
        assert_eq!(p.skus.len(), 3);
        assert_eq!(
            (
                p.skus[0].id,
                p.skus[0].price_amount.as_str(),
                p.skus[0].stock_quantity
            ),
            (34, "8.50", 12)
        );
        assert_eq!(p.skus[1].stock_quantity, -1, "null stock is unlimited");
        assert_eq!(p.skus[1].price_amount, "25.00");
        assert_eq!(p.skus[2].stock_quantity, 7);
        let schema = Value::Object(p.manual_form_schema.unwrap());
        assert_eq!(schema["fields"][0]["key"], "account");
        assert_eq!(schema["fields"][1]["options"], json!(["亚服", "美服"]));
    }

    #[test]
    fn category_ids_are_stable_and_positive() {
        assert_eq!(category_id("游戏"), category_id(" 游戏 "));
        assert_ne!(category_id("游戏"), category_id("软件"));
        assert!(category_id("") > 0);
    }

    #[test]
    fn refusals_are_never_retried() {
        for (msg, code) in [
            ("余额不足", "insufficient_balance"),
            ("库存不足", "product_out_of_stock"),
            ("签名错误", "unauthorized"),
            ("商品不存在", "product_unavailable"),
            ("什么东西", "invalid_request"),
        ] {
            assert_eq!(error_code(msg), code, "{msg}");
            assert!(!retryable(code));
        }
        assert!(!retryable(RESULT_UNKNOWN));
    }

    #[test]
    fn trade_numbers_and_after_sale_texts() {
        // mcy-shop.md §2.4 vector
        assert_eq!(trade_no("123456789012345678"), "9efebb3d7d059bff092842bf");
        assert!(is_after_sale("库存不足，请申请售后"));
        assert!(is_after_sale("此商品没有发货信息或正在发货中"));
        assert!(is_after_sale(""));
        assert!(!is_after_sale("CODE-1\nCODE-2"));
    }

    #[test]
    fn stock_values() {
        assert_eq!(stock_of(Some(&json!(3))), 3);
        assert_eq!(stock_of(Some(&json!("4"))), 4);
        assert_eq!(stock_of(Some(&json!(null))), -1);
        assert_eq!(stock_of(Some(&json!("很多"))), -1);
        assert_eq!(stock_of(Some(&json!(-1))), -1);
        assert_eq!(stock_of(None), -1);
    }

    #[test]
    fn supplier_texts_are_clipped() {
        let long = "错".repeat(MSG_MAX_CHARS + 10);
        assert_eq!(clip(&long).chars().count(), MSG_MAX_CHARS);
        assert_eq!(clip("short"), "short");
    }
}
