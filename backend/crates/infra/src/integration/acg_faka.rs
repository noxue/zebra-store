//! `acg-faka` (异次元发卡) supplier adapter: buyer-side client of its 「共享店铺」
//! protocol `/shared/*` (docs/protocol/third-party/acg-faka.md): md5 form signature
//! (`app_id` + `sign`, the key is never sent), one catalog tree, 「种类」 (race) and
//! 「规格」 (`[sku]`) variants mapped to neutral SKUs, synchronous `trade` with a
//! deduplicating `request_no`, `query` by supplier trade number.
//!
//! Protocol limits handled here (spec §7.1):
//! - no SKU ids: a SKU id is `commodity id << 20 | variant hash` (race + spec options),
//!   resolved back to race / options against the live product when ordering;
//! - `items` has no buy price (3.7.x): every listed product is read with `item`
//!   (`factory_price`, `config.category_factory`), older suppliers through `inventory`;
//! - `request_no` is refused when repeated (never answered with the original order) and
//!   there is no lookup by it: a retried purchase whose first attempt went through comes
//!   back as [`RESULT_UNKNOWN`] (manual check), so a timed-out `trade` can be retried
//!   without ever buying twice;
//! - `query` has no delivery status: a manual-delivery `secret` is the seller's notice
//!   until a human delivers; it is reported as unconfirmed content and the core
//!   delivers it once it differs from the trade-time notice (never a known pending
//!   notice); automatic products are delivered unless the secret is a pending notice;
//! - the cards of an automatic product come with the `trade` answer: they are handed to
//!   the procurement core as a synchronous delivery (persisted with the trade number),
//!   `query` stays the fallback for pending notices; a trade charged after the stock
//!   ran out (「抢走了商品」) is held for manual review.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};
use zs_domain::Id;
use zs_domain::catalog::product::{DEFAULT_SKU_CODE, JsonMap};
use zs_domain::catalog::stock::StockPolicy;
use zs_domain::integration::adapter::{
    AdapterMeta, Capabilities, Capability, ConfigField, FieldKind, HandshakeInfo, InboundError,
    InboundEvent, InboundRequest, OrderRef, PlaceOrder, SupplierAdapter, text3,
};
use zs_domain::integration::connection::Endpoint;
use zs_domain::integration::procurement::{FULFILLMENT_UNCONFIRMED, RESULT_UNKNOWN};
use zs_domain::integration::protocol::{
    CategoryList, CreateOrderResponse, Download, PingInfo, ProductPage, ProductQuery,
    RemoteCategory, RemoteFulfillment, RemoteOrder, RemoteProduct, RemoteSku, UpstreamClient,
    UpstreamError,
};

use super::client::{HttpConnector, MAX_RESPONSE_BYTES, body_lost, transport, unreadable};
use super::http::{check_url, read_limited};
use super::php_form::{self, ArrayFields, Form, form, md5_hex};

/// Protocol id stored in `site_connections.protocol`.
pub const PROTOCOL_ID: &str = "acg-faka";

const CONNECT_PATH: &str = "/shared/authentication/connect";
const ITEMS_PATH: &str = "/shared/commodity/items";
const ITEM_PATH: &str = "/shared/commodity/item";
const INVENTORY_PATH: &str = "/shared/commodity/inventory";
const STOCK_PATH: &str = "/shared/commodity/stock";
const TRADE_PATH: &str = "/shared/commodity/trade";
const QUERY_PATH: &str = "/shared/commodity/query";

/// Success `code` of the envelope (spec §1).
const OK_CODE: i64 = 200;
/// Low bits of a SKU id holding the variant hash (commodity ids are ≤ 2^32, spec §3.2,
/// so ids stay below 2^52).
const VARIANT_BITS: u32 = 20;
/// Largest variant hash (exclusive upper bound of `1..VARIANT_SPACE`).
const VARIANT_SPACE: u32 = (1 << VARIANT_BITS) - 1;
/// Race × option combinations expanded into SKUs; beyond this the product is not
/// importable (spec §7.1 recommendation).
const MAX_VARIANTS: usize = 50;
/// `order.request_no` is `char(19)` in acg-faka (`kernel/Install/Install.sql:385`,
/// unique index): longer values are refused or truncated by MySQL.
const REQUEST_NO_LEN: usize = 19;
/// Settlement currency when the connection does not configure one (spec §7.1).
const DEFAULT_CURRENCY: &str = "CNY";
/// Cover acg-faka substitutes for an empty image (spec §3.3, `Shop::getItem`).
const PLACEHOLDER_COVER: &str = "/favicon.ico";
/// Status of a manual-delivery purchase: it cannot be completed automatically.
const STATUS_MANUAL_CHECK: &str = "awaiting_manual_check";
/// Texts acg-faka puts into `secret` instead of cards (sold out after payment, manual
/// delivery, risk review — `Order.php:1069-1176`) plus the pending markers faka-bridge
/// recognises (`worker/index.js:398`).
const PENDING_MARKERS: [&str; 8] = [
    "抢走了商品",
    "正在发货中",
    "人工审核中",
    "待处理",
    "等待发货",
    "稍后查询",
    "pending",
    "processing",
];
/// Label of the race dimension in SKU spec values.
const RACE_LABEL: &str = "种类";
/// `secret` of a trade that was charged but found no cards left (`Order.php:1164-1176`):
/// permanent, needs a human (refund at the supplier).
const SOLD_OUT_AFTER_PAYMENT: &str = "抢走了商品";
/// Longest supplier text kept in logs and error messages.
const MSG_MAX_CHARS: usize = 300;

/// Supplier text shortened for logs and stored messages.
fn clip(s: &str) -> String {
    s.chars().take(MSG_MAX_CHARS).collect()
}

/// Failure of one call: transport / HTTP / protocol, or a business refusal (`code != 200`).
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

/// Neutral order error code of an acg-faka refusal (`msg` texts of `Order.php`,
/// `SharedValidation.php`, `Bill.php`; spec §7.1). Codes follow our procurement core
/// (UPS-11): stock / balance / availability refusals are permanent.
pub(crate) fn error_code(msg: &str) -> &'static str {
    let has = |needles: &[&str]| needles.iter().any(|n| msg.contains(n));
    if has(&["The request ID already exists"]) {
        RESULT_UNKNOWN
    } else if has(&["商户ID不存在", "密钥错误"]) {
        "unauthorized"
    } else if has(&["余额不足"]) {
        "insufficient_balance"
    } else if has(&["库存不足", "抢走", "已售罄"]) {
        "product_out_of_stock"
    } else if has(&["商品不存在", "未开放对接", "已停售", "暂未上架", "配置异常"])
    {
        "product_unavailable"
    } else if has(&["banned", "封禁"]) {
        "forbidden"
    } else if has(&["支付方式已停用", "支付方式"]) {
        "payment_failed"
    } else {
        "invalid_request"
    }
}

/// Manual-delivery content that cannot be told apart from a notice by itself.
fn unconfirmed(secret: &str) -> RemoteFulfillment {
    RemoteFulfillment {
        kind: "manual".into(),
        status: FULFILLMENT_UNCONFIRMED.into(),
        payload: secret.trim().to_owned(),
        delivery_data: None,
        delivered_at: None,
    }
}

/// True when a `secret` is one of acg-faka's notices rather than delivered content.
pub(crate) fn is_pending_notice(secret: &str) -> bool {
    let s = secret.trim();
    let lower = s.to_lowercase();
    s.is_empty() || PENDING_MARKERS.iter().any(|m| lower.contains(m))
}

/// Deterministic `request_no` of a purchase: `Z` + 18 hex chars of md5(our order
/// number) — stable across retries, 19 characters (acg-faka column width).
pub(crate) fn request_no(order_key: &str) -> String {
    let mut out = String::with_capacity(REQUEST_NO_LEN);
    out.push('Z');
    out.push_str(&md5_hex(order_key.as_bytes())[..REQUEST_NO_LEN - 1]);
    out
}

// ---------------------------------------------------------------------------
// Lenient JSON helpers
// ---------------------------------------------------------------------------

pub(crate) fn text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

pub(crate) fn int(v: &Value, key: &str) -> i64 {
    match v.get(key) {
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f.trunc() as i64))
            .unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0),
        Some(Value::Bool(b)) => i64::from(*b),
        _ => 0,
    }
}

pub(crate) fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(if *b { "1" } else { "0" }.to_owned()),
        _ => None,
    }
}

fn decimal(raw: &str) -> Option<Decimal> {
    raw.trim().parse::<Decimal>().ok()
}

/// Two-decimal amount text (`17` → `17.00`); empty when unparseable.
pub(crate) fn money(raw: &str) -> String {
    decimal(raw)
        .map(|d| format!("{:.2}", d.round_dp(2)))
        .unwrap_or_default()
}

/// `(key, value)` entries of a PHP array serialised as object or list.
fn entries(v: Option<&Value>) -> Vec<(String, Value)> {
    match v {
        Some(Value::Object(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        Some(Value::Array(a)) => a
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn zh(s: &str) -> JsonMap {
    let mut m = JsonMap::new();
    if !s.is_empty() {
        m.insert("zh-CN".into(), Value::String(s.to_owned()));
    }
    m
}

// ---------------------------------------------------------------------------
// Commodity config (INI `[category]` / `[category_factory]` / `[sku]`)
// ---------------------------------------------------------------------------

/// acg-faka INI (`app/Util/Ini.php::toArray`) as a JSON object: `[section]` lines,
/// `a.b.c=value` nested by `.`, exactly one `=` per line (bad lines are skipped).
pub(crate) fn parse_ini(raw: &str) -> Value {
    let mut root = Map::new();
    let mut section: Option<String> = None;
    for line in raw.split(['\r', '\n']) {
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].to_owned();
            root.entry(name.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            section = Some(name);
            continue;
        }
        let Some(name) = &section else { continue };
        let parts: Vec<&str> = line.split('=').collect();
        let [left, value] = parts.as_slice() else {
            continue;
        };
        let Some(Value::Object(mut node)) = root.get(name).cloned() else {
            continue;
        };
        insert_path(&mut node, &left.split('.').collect::<Vec<_>>(), value);
        root.insert(name.clone(), Value::Object(node));
    }
    Value::Object(root)
}

fn insert_path(node: &mut Map<String, Value>, path: &[&str], value: &str) {
    match path {
        [] => {}
        [last] => {
            node.insert((*last).to_owned(), Value::String(value.to_owned()));
        }
        [head, rest @ ..] => {
            let child = node
                .entry((*head).to_owned())
                .or_insert_with(|| Value::Object(Map::new()));
            if !child.is_object() {
                *child = Value::Object(Map::new());
            }
            if let Value::Object(m) = child {
                insert_path(m, rest, value);
            }
        }
    }
}

/// Pricing structure of a commodity.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PriceConfig {
    /// `[category]` race names in supplier order (empty = no races).
    pub races: Vec<String>,
    /// `[category_factory]`: our unit buy price per race.
    pub race_factory: Vec<(String, String)>,
    /// `[sku]`: spec name → options with their premium.
    pub specs: Vec<(String, Vec<(String, String)>)>,
}

impl PriceConfig {
    /// Reads the config of `item` (object) or `items` / `inventory` (INI text).
    pub(crate) fn of(config: Option<&Value>) -> Self {
        let parsed;
        let config = match config {
            Some(Value::String(s)) => {
                parsed = parse_ini(s);
                Some(&parsed)
            }
            other => other,
        };
        let get = |k: &str| config.and_then(|c| c.get(k));
        Self {
            races: entries(get("category"))
                .into_iter()
                .map(|(k, _)| k)
                .filter(|k| !k.trim().is_empty())
                .collect(),
            race_factory: entries(get("category_factory"))
                .into_iter()
                .filter_map(|(k, v)| scalar(&v).map(|v| (k, v)))
                .collect(),
            specs: entries(get("sku"))
                .into_iter()
                .map(|(spec, options)| {
                    let opts = entries(Some(&options))
                        .into_iter()
                        .filter_map(|(o, p)| scalar(&p).map(|p| (o, p)))
                        .collect::<Vec<_>>();
                    (spec, opts)
                })
                .filter(|(_, opts)| !opts.is_empty())
                .collect(),
        }
    }

    fn factory_of(&self, race: &str) -> Option<&str> {
        self.race_factory
            .iter()
            .find(|(r, _)| r == race)
            .map(|(_, p)| p.as_str())
    }
}

/// One orderable combination of a commodity.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Variant {
    /// Race name (empty without races).
    pub race: String,
    /// `(spec, option)` pairs sent as `sku[spec]=option`.
    pub options: Vec<(String, String)>,
    /// Unit buy price (race / commodity buy price + option premiums).
    pub price: Option<Decimal>,
}

impl Variant {
    fn key(&self) -> String {
        let mut k = self.race.clone();
        for (s, o) in &self.options {
            k.push('\u{1f}');
            k.push_str(s);
            k.push('=');
            k.push_str(o);
        }
        k
    }

    /// Variant part of the SKU id: 0 for the single default variant, else a 20-bit
    /// FNV-1a hash of race + options (≥ 1).
    fn hash(&self) -> i64 {
        let key = self.key();
        if key.is_empty() {
            return 0;
        }
        let mut h: u32 = 0x811c_9dc5;
        for b in key.bytes() {
            h ^= u32::from(b);
            h = h.wrapping_mul(0x0100_0193);
        }
        i64::from(h % VARIANT_SPACE + 1)
    }

    fn label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.race.is_empty() {
            parts.push(self.race.clone());
        }
        parts.extend(self.options.iter().map(|(_, o)| o.clone()));
        if parts.is_empty() {
            DEFAULT_SKU_CODE.to_owned()
        } else {
            parts.join(" / ")
        }
    }
}

/// SKU id of a variant of a commodity.
pub(crate) fn sku_id(commodity_id: Id, variant: &Variant) -> Id {
    (commodity_id << VARIANT_BITS) | variant.hash()
}

/// Commodity id of a SKU id.
pub(crate) fn commodity_of(sku_id: Id) -> Id {
    sku_id >> VARIANT_BITS
}

/// Every race × option combination with its unit buy price; `None` when there are
/// more than [`MAX_VARIANTS`]. Races without a buy price (the supplier could not
/// price them for us) are skipped.
pub(crate) fn variants(config: &PriceConfig, factory_price: &str) -> Option<Vec<Variant>> {
    let mut bases: Vec<(String, Option<Decimal>)> = if config.races.is_empty() {
        vec![(String::new(), decimal(factory_price))]
    } else {
        config
            .races
            .iter()
            .filter_map(|r| config.factory_of(r).map(|p| (r.clone(), decimal(p))))
            .collect()
    };
    let combos: usize = config
        .specs
        .iter()
        .map(|(_, o)| o.len())
        .product::<usize>()
        .saturating_mul(bases.len());
    if combos > MAX_VARIANTS {
        return None;
    }
    let mut out: Vec<Variant> = bases
        .drain(..)
        .map(|(race, price)| Variant {
            race,
            options: Vec::new(),
            price,
        })
        .collect();
    for (spec, opts) in &config.specs {
        out = out
            .into_iter()
            .flat_map(|v| {
                opts.iter().map(move |(opt, premium)| {
                    let mut next = v.clone();
                    next.options.push((spec.clone(), opt.clone()));
                    next.price = match (next.price, decimal(premium)) {
                        (Some(p), Some(extra)) => Some(p + extra),
                        _ => None,
                    };
                    next
                })
            })
            .collect();
    }
    Some(out)
}

/// acg-faka input widgets → our `manual_form_schema` (keys lower-cased).
///
/// ACG-06 (found live against acg-faka 3.7.9): the supplier checks each value with
/// `preg_match("/{regex}/")` inside `trade` — after our buyer has paid — so a pattern
/// our validator understands (same unanchored search semantics) is copied and checked
/// at checkout; PCRE-only syntax (look-around, back-references…) stays supplier-side.
pub(crate) fn form_schema(widget: Option<&Value>) -> JsonMap {
    let list = match widget {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::String(s)) => {
            // 3.5.9 stored the JSON url-encoded (`%5B%7B…`), spec §6.
            let raw = if s.trim_start().starts_with("%5B") {
                percent_decode(s)
            } else {
                s.clone()
            };
            serde_json::from_str::<Vec<Value>>(&raw).unwrap_or_default()
        }
        _ => Vec::new(),
    };
    let mut seen = HashSet::new();
    let mut fields = Vec::new();
    for w in &list {
        let key = text(w, "name").to_lowercase();
        let kind = text(w, "type");
        if kind == "custom"
            || key.is_empty()
            || key.len() > 64
            || !key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            || !seen.insert(key.clone())
        {
            continue;
        }
        let options: Vec<String> = text(w, "dict")
            .split(',')
            .filter_map(|item| {
                let v = item.split_once('=').map_or(item, |(_, v)| v).trim();
                (!v.is_empty()).then(|| v.to_owned())
            })
            .collect();
        let kind = match kind.as_str() {
            "number" | "textarea" => kind,
            "select" | "radio" | "checkbox" if !options.is_empty() => kind,
            _ => "text".to_owned(),
        };
        let mut field = json!({
            "key": key,
            "type": kind,
            "required": !text(w, "regex").is_empty(),
            "label": zh(&text(w, "cn")),
        });
        if let Some(m) = field.as_object_mut() {
            let placeholder = text(w, "placeholder");
            if !placeholder.is_empty() {
                m.insert("placeholder".into(), Value::Object(zh(&placeholder)));
            }
            if matches!(
                m.get("type").and_then(Value::as_str),
                Some("select" | "radio" | "checkbox")
            ) {
                m.insert("options".into(), json!(options));
            }
            if let Some(regex) = portable_regex(&text(w, "regex")) {
                m.insert("regex".into(), Value::String(regex));
                // LQA-I2: keep the supplier's message ("账号必须是5-10位数字").
                let error = text(w, "error");
                if !error.is_empty() {
                    m.insert("error_message".into(), Value::Object(zh(&error)));
                }
            }
        }
        fields.push(field);
    }
    let mut schema = JsonMap::new();
    schema.insert("fields".into(), Value::Array(fields));
    schema
}

/// An acg-faka widget pattern (`preg_match("/{p}/")`, no flags) that our checkout
/// validator runs with the same meaning; `None` when empty or not portable. A pattern
/// starting with `/` would be read as a `/…/flags` literal by our validator.
fn portable_regex(raw: &str) -> Option<String> {
    let p = raw.trim();
    if p.is_empty() || p.starts_with('/') || p.len() > MSG_MAX_CHARS {
        return None;
    }
    let probe = json!({"fields": [{"key": "probe", "type": "text", "regex": p}]});
    let probe = probe.as_object()?;
    zs_domain::catalog::manual_form::normalize_schema(probe)
        .is_ok()
        .then(|| p.to_owned())
}

/// Widget names of a commodity keyed by our lower-cased field key.
pub(crate) fn widget_names(widget: Option<&Value>) -> Vec<(String, String)> {
    let list = match widget {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::String(s)) => serde_json::from_str::<Vec<Value>>(s).unwrap_or_default(),
        _ => Vec::new(),
    };
    list.iter()
        .map(|w| text(w, "name"))
        .filter(|n| !n.is_empty())
        .map(|n| (n.to_lowercase(), n))
        .collect()
}

fn percent_decode(raw: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        char::from(b)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    }
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let pair = bytes
            .get(i + 1)
            .zip(bytes.get(i + 2))
            .and_then(|(h, l)| Some(hex(*h)? << 4 | hex(*l)?));
        match (bytes[i], pair) {
            (b'+', _) => out.push(b' '),
            (b'%', Some(b)) => {
                out.push(b);
                i += 2;
            }
            (b, _) => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Form value of a manual form answer (lists joined by `,`).
pub(crate) fn form_value(v: &Value) -> Option<String> {
    match v {
        Value::Array(items) => Some(
            items
                .iter()
                .filter_map(scalar)
                .collect::<Vec<_>>()
                .join(","),
        ),
        other => scalar(other),
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// The `acg-faka` system.
#[derive(Debug, Clone)]
pub struct AcgFakaAdapter {
    ctx: HttpConnector,
}

impl AcgFakaAdapter {
    pub fn new(ctx: HttpConnector) -> Self {
        Self { ctx }
    }

    /// Only categories: `valuation` does not lock a price (so no [`Capability::Quote`]),
    /// a repeated `request_no` is refused instead of answered (so no
    /// [`Capability::Idempotency`]), no change feed, no push, one item per trade, cards
    /// in clear.
    fn capabilities() -> Capabilities {
        Capabilities::of(&[Capability::Categories])
    }
}

/// Connection form: site, 商户 ID (`app_id` = the supplier user id), 对接密钥
/// (`app_key`), settlement currency (the protocol does not tell it).
pub(crate) fn lizhipay_fields(id_label: [&str; 3], key_label: [&str; 3]) -> Vec<ConfigField> {
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
            label: text3(id_label[0], id_label[1], id_label[2]),
            kind: FieldKind::Text,
            required: true,
            placeholder: text3(
                "供货站个人中心显示的用户 ID",
                "供貨站個人中心顯示的用戶 ID",
                "User id shown in the supplier's personal center",
            ),
            options: Vec::new(),
        },
        ConfigField {
            key: "api_secret",
            label: text3(key_label[0], key_label[1], key_label[2]),
            kind: FieldKind::Secret,
            required: true,
            placeholder: text3(
                "供货站个人中心显示的密钥",
                "供貨站個人中心顯示的密鑰",
                "Key shown in the supplier's personal center",
            ),
            options: Vec::new(),
        },
        ConfigField {
            key: "currency",
            label: text3("供货站结算货币", "供貨站結算貨幣", "Supplier currency"),
            kind: FieldKind::Text,
            required: false,
            placeholder: text3("CNY", "CNY", "CNY"),
            options: Vec::new(),
        },
    ]
}

/// Settlement currency configured on the connection (default CNY).
pub(crate) fn currency_of(endpoint: &Endpoint) -> String {
    endpoint
        .extra
        .get("currency")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_uppercase())
        .filter(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase()))
        .unwrap_or_else(|| DEFAULT_CURRENCY.to_owned())
}

impl SupplierAdapter for AcgFakaAdapter {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: PROTOCOL_ID,
            name: text3("异次元发卡", "異次元發卡", "ACG-Faka"),
            description: text3(
                "对接异次元发卡（acg-faka）站点的「店铺共享」接口：在供货站注册会员后填写用户 ID 与对接密钥。全量目录同步（种类/规格展开为 SKU），余额下单同步取卡；无推送、无锁价，请求超时的订单会标记为待人工核对，绝不重复下单。",
                "對接異次元發卡（acg-faka）站點的「店鋪共享」介面：在供貨站註冊會員後填寫用戶 ID 與對接密鑰。全量目錄同步（種類/規格展開為 SKU），餘額下單同步取卡；無推送、無鎖價，請求逾時的訂單會標記為待人工核對，絕不重複下單。",
                "Buys from acg-faka sites through their shared-store API: register as a member at the supplier and enter your user id and API key. Full catalog sync (races / specs become SKUs), balance-paid orders with synchronous cards; no push events or price locks, timed-out orders are flagged for a manual check and never bought twice.",
            ),
            fields: lizhipay_fields(
                [
                    "商户 ID（app_id）",
                    "商戶 ID（app_id）",
                    "Merchant ID (app_id)",
                ],
                [
                    "对接密钥（app_key）",
                    "對接密鑰（app_key）",
                    "API key (app_key)",
                ],
            ),
            capabilities: Self::capabilities(),
            supports_connection_code: false,
            inbound_path: "",
        }
    }

    fn open(&self, endpoint: &Endpoint) -> zs_domain::Result<Arc<dyn UpstreamClient>> {
        Ok(Arc::new(AcgClient {
            ctx: self.ctx.clone(),
            base_url: endpoint.base_url.trim().trim_end_matches('/').to_owned(),
            app_id: endpoint.api_key.trim().to_owned(),
            app_key: endpoint.api_secret.clone(),
            currency: currency_of(endpoint),
            tree: Mutex::new(None),
        }))
    }

    /// acg-faka never calls its buyers back.
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

/// A commodity read with `item`, ready to map.
struct Detail {
    product: RemoteProduct,
    variants: Vec<(Id, Variant)>,
    widget: Option<Value>,
    code: String,
    manual: bool,
}

/// Client bound to one connection (the catalog tree is fetched once per client).
struct AcgClient {
    ctx: HttpConnector,
    base_url: String,
    app_id: String,
    app_key: String,
    currency: String,
    tree: Mutex<Option<Arc<Vec<Value>>>>,
}

impl AcgClient {
    /// Signed form POST; `data` of `code == 200`, [`Fail::Refused`] with `msg` else.
    async fn post(&self, path: &str, mut fields: Form) -> Result<Value, Fail> {
        let url = check_url(self.ctx.policy, &format!("{}{path}", self.base_url))
            .map_err(UpstreamError::Forbidden)?;
        fields.push(("app_id".into(), self.app_id.clone()));
        let sign = php_form::signature(&fields, &self.app_key, ArrayFields::Signed);
        fields.push(("sign".into(), sign));
        let res = self
            .ctx
            .http
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(php_form::encode(&fields))
            .send()
            .await
            .map_err(|e| transport(&e))?;
        let status = res.status().as_u16();
        let raw = read_limited(res, MAX_RESPONSE_BYTES)
            .await
            .map_err(body_lost)?;
        if status != 200 {
            tracing::warn!(path, status, "acg-faka request error");
            return Err(Fail::Upstream(UpstreamError::Http {
                status,
                code: String::new(),
                message: String::from_utf8_lossy(&raw).chars().take(200).collect(),
            }));
        }
        let parsed: Value = serde_json::from_slice(&raw).map_err(|_| unreadable(&raw))?;
        if int(&parsed, "code") == OK_CODE {
            return Ok(parsed.get("data").cloned().unwrap_or(Value::Null));
        }
        let msg = clip(&text(&parsed, "msg"));
        tracing::info!(path, msg, "acg-faka refused");
        Err(Fail::Refused(if msg.is_empty() {
            "unknown error".into()
        } else {
            msg
        }))
    }

    /// The category → commodity tree of `items`.
    async fn tree(&self) -> Result<Arc<Vec<Value>>, Fail> {
        if let Some(t) = self
            .tree
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            return Ok(t);
        }
        let data = self.post(ITEMS_PATH, Vec::new()).await?;
        let tree = Arc::new(data.as_array().cloned().unwrap_or_default());
        *self.tree.lock().unwrap_or_else(PoisonError::into_inner) = Some(tree.clone());
        Ok(tree)
    }

    /// Commodity rows of the tree in catalog order.
    async fn rows(&self) -> Result<Vec<Value>, Fail> {
        Ok(self
            .tree()
            .await?
            .iter()
            .flat_map(|c| entries(c.get("children")).into_iter().map(|(_, v)| v))
            .filter(|r| int(r, "id") > 0 && !text(r, "code").is_empty())
            .collect())
    }

    async fn row(&self, id: Id) -> Result<Option<Value>, Fail> {
        Ok(self.rows().await?.into_iter().find(|r| int(r, "id") == id))
    }

    /// `item` of a code (≤ 3.1.1 answers a one-commodity tree, spec §3.3).
    async fn item(&self, code: &str) -> Result<Value, Fail> {
        let data = self
            .post(
                ITEM_PATH,
                form([("code", code.to_owned()), ("sharedCode", code.to_owned())]),
            )
            .await?;
        if data.is_object() {
            return Ok(data);
        }
        entries(Some(&data))
            .into_iter()
            .flat_map(|(_, c)| entries(c.get("children")))
            .map(|(_, v)| v)
            .find(|v| text(v, "code") == code)
            .ok_or_else(|| Fail::Refused("商品不存在".into()))
    }

    /// Buy prices of a commodity: `item` itself (3.6.5+), else `inventory`.
    async fn prices(&self, code: &str, item: &Value) -> Result<(PriceConfig, String), Fail> {
        let mut config = PriceConfig::of(item.get("config"));
        if item.get("factory_price").is_some() {
            return Ok((config, text(item, "factory_price")));
        }
        let inv = self
            .post(
                INVENTORY_PATH,
                form([("sharedCode", code.to_owned()), ("race", String::new())]),
            )
            .await?;
        let priced = PriceConfig::of(inv.get("config"));
        config.race_factory = priced.race_factory;
        Ok((config, text(&inv, "factory_price")))
    }

    /// Live stock of one variant; the commodity stock when the endpoint is missing.
    async fn variant_stock(&self, code: &str, v: &Variant, fallback: i64) -> Result<i64, Fail> {
        let mut fields = form([("code", code.to_owned()), ("race", v.race.clone())]);
        for (s, o) in &v.options {
            fields.push((format!("sku[{s}]"), o.clone()));
        }
        match self.post(STOCK_PATH, fields).await {
            Ok(d) => Ok(int(&d, "stock").max(0)),
            // ≤ 3.1.1 has no stock endpoint (HTML 404 / 405).
            Err(Fail::Upstream(UpstreamError::Http {
                status: 404 | 405, ..
            })) => Ok(fallback),
            Err(Fail::Refused(msg)) => {
                tracing::warn!(code, msg, "acg-faka stock refused");
                Ok(fallback)
            }
            Err(e) => Err(e),
        }
    }

    /// Reads and maps one commodity of the tree; `with_stock` asks per-variant stock.
    async fn detail(&self, row: &Value, with_stock: bool) -> Result<Detail, Fail> {
        let code = text(row, "code");
        let id = int(row, "id");
        let item = self.item(&code).await?;
        let (config, factory) = self.prices(&code, &item).await?;
        let manual = int(&item, "delivery_way") != 0;
        let row_stock = int(&item, "stock").max(int(row, "stock")).max(0);
        let mut product = stub(row);
        product.is_active = int(&item, "status") == 1 || item.get("status").is_none();
        product.fulfillment_type = if manual { "manual" } else { "auto" }.to_owned();
        product.content = zh(&text(&item, "description"));
        product.images = cover(&text(&item, "cover"));
        product.tags = match item.get("tags") {
            Some(Value::Array(a)) => a.iter().filter_map(scalar).collect(),
            Some(Value::String(s)) => s
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        };
        let widget = item.get("widget").cloned();
        product.manual_form_schema = Some(form_schema(widget.as_ref()));
        let Some(list) = variants(&config, &factory) else {
            tracing::warn!(
                code,
                "acg-faka commodity has too many variants, not importable"
            );
            product.is_active = false;
            return Ok(Detail {
                product,
                variants: Vec::new(),
                widget,
                code,
                manual,
            });
        };
        let single = list.len() == 1;
        let mut seen = HashSet::new();
        let mut out = Vec::with_capacity(list.len());
        for v in list {
            let sku = sku_id(id, &v);
            if !seen.insert(sku) {
                tracing::warn!(
                    code,
                    variant = v.label(),
                    "acg-faka variant hash collision, skipped"
                );
                continue;
            }
            let stock = if with_stock && !single && !manual {
                self.variant_stock(&code, &v, row_stock).await?
            } else {
                row_stock
            };
            let mut spec = JsonMap::new();
            if !v.race.is_empty() {
                spec.insert(RACE_LABEL.into(), Value::String(v.race.clone()));
            }
            for (s, o) in &v.options {
                spec.insert(s.clone(), Value::String(o.clone()));
            }
            product.skus.push(RemoteSku {
                id: sku,
                sku_code: v.label(),
                spec_values: spec,
                price_amount: v
                    .price
                    .filter(|p| *p > Decimal::ZERO)
                    .map(|p| format!("{:.2}", p.round_dp(2)))
                    .unwrap_or_default(),
                stock_status: StockPolicy::UPSTREAM.status(stock).as_str().to_owned(),
                stock_quantity: stock,
                is_active: true,
                ..RemoteSku::default()
            });
            out.push((sku, v));
        }
        product.price_amount = out
            .iter()
            .filter_map(|(_, v)| v.price)
            .filter(|p| *p > Decimal::ZERO)
            .min()
            .map(|p| format!("{:.2}", p.round_dp(2)))
            .unwrap_or_default();
        Ok(Detail {
            product,
            variants: out,
            widget,
            code,
            manual,
        })
    }

    /// Product of a tree row; refusals (delisted, misconfigured) become an inactive stub
    /// so a listing never loses a product (UPS-06).
    async fn listed(&self, row: &Value) -> Result<RemoteProduct, UpstreamError> {
        match self.detail(row, true).await {
            Ok(d) => Ok(d.product),
            Err(Fail::Refused(msg)) => {
                tracing::warn!(code = text(row, "code"), msg, "acg-faka item refused");
                Ok(stub(row))
            }
            Err(Fail::Upstream(e)) => Err(e),
        }
    }
}

/// Minimal product of a tree row (inactive until detailed).
fn stub(row: &Value) -> RemoteProduct {
    RemoteProduct {
        id: int(row, "id"),
        title: zh(&text(row, "name")),
        images: cover(&text(row, "cover")),
        category_id: int(row, "category_id"),
        fulfillment_type: if int(row, "delivery_way") == 0 {
            "auto"
        } else {
            "manual"
        }
        .to_owned(),
        currency: String::new(),
        is_active: false,
        ..RemoteProduct::default()
    }
}

pub(crate) fn cover(raw: &str) -> Vec<String> {
    if raw.is_empty() || raw == PLACEHOLDER_COVER {
        Vec::new()
    } else {
        vec![raw.to_owned()]
    }
}

pub(crate) fn not_found(msg: &str) -> UpstreamError {
    if msg.contains("商品不存在") {
        UpstreamError::ProductDeleted
    } else {
        UpstreamError::ProductUnavailable
    }
}

#[async_trait]
impl UpstreamClient for AcgClient {
    fn capabilities(&self) -> Capabilities {
        AcgFakaAdapter::capabilities()
    }

    async fn handshake(&self) -> Result<HandshakeInfo, UpstreamError> {
        let d = self.post(CONNECT_PATH, Vec::new()).await?;
        Ok(HandshakeInfo {
            protocol: PROTOCOL_ID.to_owned(),
            version: String::new(),
            site_name: text(&d, "shopName"),
            site_url: self.base_url.clone(),
            currency: self.currency.clone(),
            features: AcgFakaAdapter::capabilities()
                .names()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            limits: JsonMap::new(),
            user_id: self.app_id.parse().unwrap_or(0),
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
        let tree = self.tree().await?;
        Ok(CategoryList {
            supported: true,
            categories: tree
                .iter()
                .filter(|c| int(c, "id") > 0)
                .map(|c| RemoteCategory {
                    id: int(c, "id"),
                    parent_id: int(c, "pid").max(0),
                    slug: format!("acg-{}", int(c, "id")),
                    name: zh(&text(c, "name")),
                    icon: text(c, "icon"),
                    sort_order: int(c, "sort"),
                })
                .collect(),
        })
    }

    /// The whole tree paged locally; every product on the page is read with `item`
    /// for our buy prices. Only active, docked products are listed (no deletions can
    /// be inferred from absence).
    async fn list_products(&self, q: &ProductQuery) -> Result<ProductPage, UpstreamError> {
        let rows = self.rows().await?;
        let size = usize::try_from(q.page_size)
            .ok()
            .filter(|s| *s > 0)
            .unwrap_or(20);
        let page = usize::try_from(q.page).ok().filter(|p| *p > 0).unwrap_or(1);
        let mut items = Vec::new();
        for row in rows.iter().skip((page - 1).saturating_mul(size)).take(size) {
            items.push(self.listed(row).await?);
        }
        Ok(ProductPage {
            total: i64::try_from(rows.len()).unwrap_or(i64::MAX),
            items,
            includes_inactive: false,
            next_cursor: String::new(),
            has_more: false,
        })
    }

    async fn get_product(&self, id: Id) -> Result<RemoteProduct, UpstreamError> {
        let Some(row) = self.row(id).await? else {
            // Missing from the docked catalog: delisted, undocked or deleted.
            return Err(UpstreamError::ProductUnavailable);
        };
        match self.detail(&row, true).await {
            Ok(d) => Ok(d.product),
            Err(Fail::Refused(msg)) => Err(not_found(&msg)),
            Err(Fail::Upstream(e)) => Err(e),
        }
    }

    /// `trade` of one variant, paid from our supplier balance; the purchase number is a
    /// deterministic `request_no`, so a retry after a lost answer is refused by the
    /// supplier instead of buying twice ([`RESULT_UNKNOWN`], manual check).
    async fn place_order(&self, req: &PlaceOrder) -> Result<CreateOrderResponse, UpstreamError> {
        let refused = |code: &str, message: String| CreateOrderResponse {
            ok: false,
            error_code: code.to_owned(),
            error_message: message,
            ..CreateOrderResponse::default()
        };
        let [line] = req.lines.as_slice() else {
            return Err(UpstreamError::Protocol(
                "acg-faka orders take exactly one item".into(),
            ));
        };
        // Pre-checks are read-only: their failures never bought anything.
        let Some(row) = self
            .row(commodity_of(line.sku_id))
            .await
            .map_err(|f| UpstreamError::from(f).not_executed())?
        else {
            return Ok(refused(
                "product_unavailable",
                "commodity is no longer docked at the supplier".into(),
            ));
        };
        let detail = match self.detail(&row, false).await {
            Ok(d) => d,
            Err(Fail::Refused(msg)) => return Ok(refused(error_code(&msg), msg)),
            Err(Fail::Upstream(e)) => return Err(e.not_executed()),
        };
        let Some((_, variant)) = detail.variants.iter().find(|(id, _)| *id == line.sku_id) else {
            return Ok(refused(
                "sku_unavailable",
                "variant no longer offered by the supplier".into(),
            ));
        };
        let key = if req.downstream_order_no.is_empty() {
            &req.idempotency_key
        } else {
            &req.downstream_order_no
        };
        let request_no = request_no(key);
        let mut fields = form([
            ("shared_code", detail.code.clone()),
            ("contact", request_no.clone()),
            ("num", line.quantity.to_string()),
            ("card_id", "0".into()),
            ("device", "0".into()),
            ("password", String::new()),
            ("race", variant.race.clone()),
            ("request_no", request_no.clone()),
        ]);
        for (spec, option) in &variant.options {
            fields.push((format!("sku[{spec}]"), option.clone()));
        }
        let names = widget_names(detail.widget.as_ref());
        for (key, value) in line.manual_form_data.iter().flatten() {
            let Some((_, name)) = names.iter().find(|(k, _)| k == &key.to_lowercase()) else {
                continue;
            };
            if let Some(v) = form_value(value) {
                fields.push((name.clone(), v));
            }
        }
        match self.post(TRADE_PATH, fields).await {
            Ok(d) => {
                let trade_no = text(&d, "tradeNo");
                if trade_no.is_empty() {
                    // Paid but unidentifiable: never retry blindly.
                    return Ok(refused(
                        RESULT_UNKNOWN,
                        format!(
                            "trade answered without tradeNo (request_no {request_no}); check the order at the supplier"
                        ),
                    ));
                }
                let secret = text(&d, "secret").replace("\r\n", "\n");
                let secret = secret.trim();
                // ACG-04: only real cards of an automatic product are a delivery;
                // notices fall back to `query` polling.
                let (fulfillment, review) = if detail.manual {
                    // Baseline: later `query` content that differs from this notice is
                    // the seller's manual delivery (ACG-04).
                    (Some(unconfirmed(secret)), None)
                } else if secret.contains(SOLD_OUT_AFTER_PAYMENT) {
                    (
                        None,
                        Some(format!(
                            "trade {trade_no} was charged but the supplier had no cards left ({}); ask the supplier for a refund",
                            clip(secret)
                        )),
                    )
                } else if is_pending_notice(secret) {
                    (None, None)
                } else {
                    (
                        Some(RemoteFulfillment {
                            kind: "auto".into(),
                            status: "delivered".into(),
                            payload: secret.to_owned(),
                            delivery_data: None,
                            delivered_at: None,
                        }),
                        None,
                    )
                };
                Ok(CreateOrderResponse {
                    ok: true,
                    // Numeric supplier id only for automatic delivery: `get_order`
                    // never auto-completes manual deliveries (id 0).
                    order_id: if detail.manual {
                        0
                    } else {
                        trade_no.parse().unwrap_or(0)
                    },
                    order_no: trade_no,
                    status: "paid".into(),
                    amount: money(&text(&d, "amount")),
                    currency: self.currency.clone(),
                    fulfillment,
                    review,
                    ..CreateOrderResponse::default()
                })
            }
            Err(Fail::Refused(msg)) => {
                let code = error_code(&msg);
                let message = if code == RESULT_UNKNOWN {
                    format!(
                        "{msg}: request_no {request_no} was already used at the supplier, a previous attempt may have been charged; check it manually"
                    )
                } else {
                    msg
                };
                Ok(refused(code, message))
            }
            // Lost answer: retried later with the same request_no (deduplicated).
            Err(Fail::Upstream(e)) => Err(e),
        }
    }

    fn addressable(&self, order: &OrderRef) -> bool {
        !order.no.is_empty()
    }

    async fn get_order(&self, order: &OrderRef) -> Result<RemoteOrder, UpstreamError> {
        let d = self
            .post(QUERY_PATH, form([("tradeNo", order.no.clone())]))
            .await
            .map_err(|f| match f {
                Fail::Refused(msg) => UpstreamError::Protocol(msg),
                Fail::Upstream(e) => e,
            })?;
        let secret = text(&d, "secret").replace("\r\n", "\n");
        let paid = int(&d, "status") == 1;
        let status = if !paid {
            "pending"
        } else if order.id == 0 {
            STATUS_MANUAL_CHECK
        } else if is_pending_notice(&secret) {
            "processing"
        } else {
            "delivered"
        };
        let fulfillment = match status {
            "delivered" => Some(RemoteFulfillment {
                kind: "auto".into(),
                status: "delivered".into(),
                payload: secret.trim().to_owned(),
                delivery_data: None,
                delivered_at: None,
            }),
            // Manual delivery: the content is shown for the core to compare with the
            // trade-time baseline; known notices are never candidates.
            STATUS_MANUAL_CHECK if !is_pending_notice(&secret) => Some(unconfirmed(secret.trim())),
            _ => None,
        };
        Ok(RemoteOrder {
            order_id: order.id,
            order_no: order.no.clone(),
            status: status.to_owned(),
            amount: String::new(),
            refunded_amount: String::new(),
            currency: self.currency.clone(),
            fulfillment,
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

    fn cfg(raw: &str) -> PriceConfig {
        PriceConfig::of(Some(&Value::String(raw.into())))
    }

    #[test]
    fn ini_config_of_inventory() {
        let c = cfg(
            "[category]\n月卡=10\n季卡=28.00\n[category_factory]\n月卡=8.5\n季卡=25\n[sku]\n区服.亚服=0\n区服.美服=1.50\n[wholesale]\n10=9\nbad line\n",
        );
        assert_eq!(c.races, vec!["月卡", "季卡"]);
        assert_eq!(c.factory_of("月卡"), Some("8.5"));
        assert_eq!(
            c.specs,
            vec![(
                "区服".to_owned(),
                vec![
                    ("亚服".to_owned(), "0".to_owned()),
                    ("美服".to_owned(), "1.50".to_owned())
                ]
            )]
        );
    }

    #[test]
    fn object_config_of_item_with_numeric_races() {
        // PHP turns races "0","1" into a list when json-encoding.
        let c = PriceConfig::of(Some(&json!({
            "category": ["10", "20"],
            "category_factory": {"0": "9.00", "1": 18},
        })));
        assert_eq!(c.races, vec!["0", "1"]);
        assert_eq!(c.factory_of("1"), Some("18"));
    }

    #[test]
    fn variants_prices_and_stable_sku_ids() {
        let c = cfg(
            "[category]\n月卡=10\n季卡=28\n天卡=1\n[category_factory]\n月卡=8.5\n季卡=25\n[sku]\n区服.亚服=0\n区服.美服=1.50\n",
        );
        let vs = variants(&c, "0").unwrap();
        // 天卡 has no buy price for us: skipped; 2 races × 2 options
        assert_eq!(vs.len(), 4);
        assert_eq!(vs[1].race, "月卡");
        assert_eq!(vs[1].options, vec![("区服".to_owned(), "美服".to_owned())]);
        assert_eq!(vs[1].price, Some(Decimal::new(1000, 2)));
        assert_eq!(vs[1].label(), "月卡 / 美服");
        let ids: HashSet<Id> = vs.iter().map(|v| sku_id(12, v)).collect();
        assert_eq!(ids.len(), 4);
        for v in &vs {
            let id = sku_id(12, v);
            assert_eq!(commodity_of(id), 12);
            assert_ne!(id & i64::from(VARIANT_SPACE), 0);
        }
        // the id depends on names only, not on their order in the config
        let reordered = cfg(
            "[category]\n季卡=28\n月卡=10\n[category_factory]\n季卡=25\n月卡=8.5\n[sku]\n区服.美服=1.50\n区服.亚服=0\n",
        );
        let again: HashSet<Id> = variants(&reordered, "0")
            .unwrap()
            .iter()
            .map(|v| sku_id(12, v))
            .collect();
        assert_eq!(ids, again);
        // no race / spec: one default variant priced with factory_price, id = commodity << 20
        let plain = variants(&PriceConfig::default(), "5").unwrap();
        assert_eq!(plain.len(), 1);
        assert_eq!(sku_id(13, &plain[0]), 13 << 20);
        assert_eq!(plain[0].label(), DEFAULT_SKU_CODE);
        // more than 50 combinations are refused
        let many = (0..51).map(|i| format!("r{i}=1\n")).collect::<String>();
        let fac = (0..51).map(|i| format!("r{i}=1\n")).collect::<String>();
        assert!(
            variants(
                &cfg(&format!("[category]\n{many}[category_factory]\n{fac}")),
                "0"
            )
            .is_none()
        );
    }

    #[test]
    fn error_codes_follow_the_core_classes() {
        use zs_domain::integration::procurement::is_retryable_error_code as retryable;
        for (msg, code) in [
            ("密钥错误", "unauthorized"),
            ("商户ID不存在", "unauthorized"),
            ("余额不足", "insufficient_balance"),
            ("库存不足", "product_out_of_stock"),
            ("当前商品已停售", "product_unavailable"),
            ("该商品未开放对接", "product_unavailable"),
            ("The request ID already exists", RESULT_UNKNOWN),
            ("账号格式错误", "invalid_request"),
        ] {
            assert_eq!(error_code(msg), code, "{msg}");
            assert!(!retryable(error_code(msg)), "{msg} must not be retried");
        }
    }

    #[test]
    fn pending_notices_are_not_deliveries() {
        assert!(is_pending_notice(
            "很抱歉，有人在你付款之前抢走了商品，请联系客服。"
        ));
        assert!(is_pending_notice(
            "正在发货中，请耐心等待，如有疑问，请联系客服。"
        ));
        assert!(is_pending_notice(
            "订单正在人工审核中，通过后会立即发货，请耐心等待。"
        ));
        assert!(is_pending_notice("  "));
        assert!(!is_pending_notice("CARD-1\nCARD-2"));
    }

    // ACG-02: request_no fits acg-faka's char(19) unique column and is stable.
    #[test]
    fn acg02_request_no_fits_the_column() {
        let a = request_no("DJ123456789");
        assert_eq!(a.len(), 19);
        assert!(a.starts_with('Z'));
        assert_eq!(a, request_no("DJ123456789"));
        assert_ne!(a, request_no("DJ123456780"));
    }

    // ACG-05: widgets (also the 3.5.9 url-encoded form) become our schema.
    #[test]
    fn widgets_become_a_manual_form_schema() {
        let w = json!([
            {"cn": "游戏账号", "name": "Account", "type": "text", "regex": "^\\w+$", "placeholder": "账号", "dict": ""},
            {"cn": "区服", "name": "server", "type": "select", "dict": "亚服=asia,美服=us"},
            {"cn": "空选项", "name": "empty", "type": "radio", "dict": ""},
            {"cn": "自定义", "name": "c1", "type": "custom"},
            {"cn": "坏名", "name": "bad-name", "type": "text"},
        ]);
        let s = form_schema(Some(&w));
        assert_eq!(
            Value::Object(s),
            json!({"fields": [
                {"key": "account", "type": "text", "required": true, "label": {"zh-CN": "游戏账号"}, "placeholder": {"zh-CN": "账号"}, "regex": "^\\w+$"},
                {"key": "server", "type": "select", "required": false, "label": {"zh-CN": "区服"}, "options": ["asia", "us"]},
                {"key": "empty", "type": "text", "required": false, "label": {"zh-CN": "空选项"}},
            ]})
        );
        // 3.5.9 url-encoded widget JSON is decoded (spec §6)
        let enc = Value::String(
            "%5B%7B%22cn%22%3A%22QQ%22%2C%22name%22%3A%22qq%22%2C%22type%22%3A%22number%22%7D%5D"
                .into(),
        );
        let s = form_schema(Some(&enc));
        assert_eq!(s["fields"][0]["key"], "qq");
        assert_eq!(s["fields"][0]["type"], "number");
    }

    // ACG-06: portable widget patterns reach our checkout validator; PCRE-only or
    // `/`-delimited ones are left to the supplier.
    #[test]
    fn acg06_widget_regex_is_copied_when_portable() {
        let w = json!([
            {"cn": "游戏账号", "name": "account", "type": "text", "regex": "^[0-9]{6,12}$", "error": "账号格式不正确"},
            {"cn": "前瞻", "name": "look", "type": "text", "regex": "^(?=a)a$"},
            {"cn": "斜杠", "name": "slash", "type": "text", "regex": "/x/"},
            {"cn": "无", "name": "free", "type": "text", "regex": ""},
        ]);
        let s = form_schema(Some(&w));
        let f = s["fields"].as_array().unwrap();
        assert_eq!(f[0]["regex"], "^[0-9]{6,12}$");
        assert_eq!(f[0]["required"], true);
        // LQA-I2: the supplier message survives normalization for the storefront.
        assert_eq!(f[0]["error_message"], json!({"zh-CN": "账号格式不正确"}));
        let normalized = zs_domain::catalog::manual_form::normalize_schema(&s).unwrap();
        assert_eq!(
            normalized["fields"][0]["error_message"],
            json!({"zh-CN": "账号格式不正确"})
        );
        assert!(f[1].get("regex").is_none(), "{}", f[1]);
        assert_eq!(f[1]["required"], true);
        assert!(f[2].get("regex").is_none(), "{}", f[2]);
        assert!(f[3].get("regex").is_none(), "{}", f[3]);
        // the copied pattern is enforced at checkout like acg-faka's preg_match
        let schema = zs_domain::catalog::manual_form::normalize_schema(&s).unwrap();
        let ok = json!({"account": "12345678", "look": "a", "slash": "x"});
        assert!(
            zs_domain::catalog::manual_form::validate_and_normalize(
                &schema,
                ok.as_object().unwrap()
            )
            .is_ok()
        );
        let bad = json!({"account": "abc", "look": "a", "slash": "x"});
        assert!(
            zs_domain::catalog::manual_form::validate_and_normalize(
                &schema,
                bad.as_object().unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn money_texts() {
        assert_eq!(money("17"), "17.00");
        assert_eq!(money("8.5"), "8.50");
        assert_eq!(money("x"), "");
    }
}
