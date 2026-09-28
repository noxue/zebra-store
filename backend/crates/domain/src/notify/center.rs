//! Notification center rules: events, the dispatch payload, template rendering,
//! de-duplication keys and alert payloads (port of
//! `modules/notification/application/{dedupe.go,format/*}`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::settings::schema::notification::{
    LocalizedTemplate, NotificationCenterSetting, SceneTemplate, Scenes, Templates,
    normalize_locale,
};
use crate::settings::schema::storefront::DashboardAlert;

/// Template variables / event data.
pub type Vars = Map<String, Value>;

/// Event types (original `constants.NotificationEvent*`).
pub mod events {
    pub const WALLET_RECHARGE_SUCCESS: &str = "wallet_recharge_success";
    pub const ORDER_PAID_SUCCESS: &str = "order_paid_success";
    pub const MANUAL_FULFILLMENT_PENDING: &str = "manual_fulfillment_pending";
    pub const EXCEPTION_ALERT: &str = "exception_alert";
    /// Periodic inventory / payment alert check (never rendered itself).
    pub const EXCEPTION_ALERT_CHECK: &str = "exception_alert_check";
}

/// Delivery channels (original `constants.NotificationChannel*`).
pub mod channels {
    pub const EMAIL: &str = "email";
    pub const TELEGRAM: &str = "telegram";
    pub const FEISHU: &str = "feishu";
    /// Every channel accepted by the test-send endpoint.
    pub const ALL: [&str; 3] = [EMAIL, TELEGRAM, FEISHU];
}

/// Alert type keys (original `constants.NotificationAlertType*`).
pub mod alert_types {
    pub const OUT_OF_STOCK_PRODUCTS: &str = "out_of_stock_products";
    pub const LOW_STOCK_PRODUCTS: &str = "low_stock_products";
    pub const PENDING_PAYMENT_ORDERS: &str = "pending_payment_orders";
    pub const PAYMENTS_FAILED: &str = "payments_failed";
}

/// Business types (original `constants.NotificationBizType*`).
pub mod biz_types {
    pub const ORDER: &str = "order";
    pub const WALLET_RECHARGE: &str = "wallet_recharge";
    pub const DASHBOARD_ALERT: &str = "dashboard_alert";
    pub const PAYMENT_CALLBACK: &str = "payment_callback";
    pub const PROCUREMENT: &str = "procurement";
    pub const RECONCILIATION: &str = "reconciliation";
}

/// Default dedupe window when the setting is not positive (original `300`).
pub const DEFAULT_DEDUPE_TTL_SECONDS: i64 = 300;
/// Retry budget of `notification:dispatch` jobs (original `Enqueue(..., 5)`).
pub const DISPATCH_MAX_RETRY: i32 = 5;
/// Fallback title when a template renders empty.
pub const FALLBACK_TITLE: &str = "Notification";
/// Fallback title of test sends.
pub const TEST_FALLBACK_TITLE: &str = "Notification Test";

/// Payload of the `notification:dispatch` job (original `NotificationDispatchPayload`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DispatchPayload {
    pub event_type: String,
    pub biz_type: String,
    pub biz_id: i64,
    pub locale: String,
    pub force: bool,
    pub data: Vars,
}

/// A business event other modules ask the notification center to deliver.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NotifyEvent {
    pub event_type: String,
    pub biz_type: String,
    pub biz_id: i64,
    pub locale: String,
    /// Skip de-duplication.
    pub force: bool,
    pub data: Vars,
}

impl NotifyEvent {
    /// Normalizes the event into a job payload; `None` for unsupported events.
    pub fn into_payload(self) -> Option<DispatchPayload> {
        let event_type = normalize_event(&self.event_type);
        if !is_supported_event(&event_type) {
            return None;
        }
        Some(DispatchPayload {
            event_type,
            biz_type: self.biz_type.trim().to_owned(),
            biz_id: self.biz_id,
            locale: self.locale.trim().to_owned(),
            force: self.force,
            data: self.data,
        })
    }
}

/// Lower-cased, trimmed event type.
pub fn normalize_event(event: &str) -> String {
    event.trim().to_lowercase()
}

/// Events accepted by the notification center.
pub fn is_supported_event(event: &str) -> bool {
    matches!(
        normalize_event(event).as_str(),
        events::WALLET_RECHARGE_SUCCESS
            | events::ORDER_PAID_SUCCESS
            | events::MANUAL_FULFILLMENT_PENDING
            | events::EXCEPTION_ALERT
            | events::EXCEPTION_ALERT_CHECK
    )
}

/// Whether the scene of `event` is switched on (the alert check follows `exception_alert`).
pub fn scene_enabled(scenes: &Scenes, event: &str) -> bool {
    match normalize_event(event).as_str() {
        events::WALLET_RECHARGE_SUCCESS => scenes.wallet_recharge_success,
        events::ORDER_PAID_SUCCESS => scenes.order_paid_success,
        events::MANUAL_FULFILLMENT_PENDING => scenes.manual_fulfillment_pending,
        events::EXCEPTION_ALERT | events::EXCEPTION_ALERT_CHECK => scenes.exception_alert,
        _ => false,
    }
}

/// Template of `event` in `locale` (unknown events use the exception alert template).
pub fn template_for<'a>(
    templates: &'a Templates,
    event: &str,
    locale: &str,
) -> &'a LocalizedTemplate {
    let scene: &SceneTemplate = match normalize_event(event).as_str() {
        events::WALLET_RECHARGE_SUCCESS => &templates.wallet_recharge_success,
        events::ORDER_PAID_SUCCESS => &templates.order_paid_success,
        events::MANUAL_FULFILLMENT_PENDING => &templates.manual_fulfillment_pending,
        _ => &templates.exception_alert,
    };
    scene.resolve(locale)
}

/// Go `fmt.Sprintf("%v")`-like rendering of a variable (trimmed; `null` is empty).
pub fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.trim().to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn is_var_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Replaces `{{ name }}` placeholders; unknown names render empty (original `RenderTemplate`).
pub fn render_template(template: &str, vars: &Vars) -> String {
    let template = template.trim();
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let replaced = after.find("}}").and_then(|end| {
            let name = after[..end].trim();
            is_var_name(name).then(|| (vars.get(name).map(value_text).unwrap_or_default(), end))
        });
        match replaced {
            Some((text, end)) => {
                out.push_str(&text);
                rest = &after[end + 2..];
            }
            None => {
                out.push('{');
                rest = &rest[start + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `locale`, else `fallback`, normalized to a supported locale.
pub fn resolve_locale(locale: &str, fallback: &str) -> String {
    let locale = locale.trim();
    normalize_locale(if locale.is_empty() { fallback } else { locale })
}

/// `title\n\nbody` (either part may be empty).
pub fn compose_plain_text(title: &str, body: &str) -> String {
    let (title, body) = (title.trim(), body.trim());
    match (title.is_empty(), body.is_empty()) {
        (true, _) => body.to_owned(),
        (false, true) => title.to_owned(),
        (false, false) => format!("{title}\n\n{body}"),
    }
}

/// Event data plus `event_type`, `biz_type`, `biz_id` and `occurred_at`.
pub fn template_variables(payload: &DispatchPayload, now: DateTime<Utc>) -> Vars {
    let mut vars = payload.data.clone();
    vars.insert(
        "event_type".into(),
        normalize_event(&payload.event_type).into(),
    );
    vars.insert("biz_type".into(), payload.biz_type.trim().into());
    vars.insert("biz_id".into(), payload.biz_id.to_string().into());
    vars.insert(
        "occurred_at".into(),
        now.format("%Y-%m-%d %H:%M:%S").to_string().into(),
    );
    vars
}

/// Rendered title/body of an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub title: String,
    pub body: String,
}

/// Renders `template`, falling back to the title for an empty body and to
/// `fallback_title` for an empty title.
pub fn render(template: &LocalizedTemplate, vars: &Vars, fallback_title: &str) -> Rendered {
    let mut title = render_template(&template.title, vars);
    let mut body = render_template(&template.body, vars);
    if body.trim().is_empty() {
        body.clone_from(&title);
    }
    if title.trim().is_empty() {
        title = fallback_title.to_owned();
    }
    Rendered { title, body }
}

/// Key used to suppress duplicate deliveries (event, business id and data, ignoring `occurred_at`).
pub fn dedupe_key(payload: &DispatchPayload) -> String {
    let mut signature = format!(
        "{}|{}|{}|",
        normalize_event(&payload.event_type),
        payload.biz_type.trim().to_lowercase(),
        payload.biz_id
    );
    let mut keys: Vec<&String> = payload.data.keys().collect();
    keys.sort();
    for key in keys {
        if key == "occurred_at" {
            continue;
        }
        let value = payload.data.get(key).map(value_text).unwrap_or_default();
        signature.push_str(&format!("{key}={value};"));
    }
    format!(
        "notification:dedupe:{}",
        zs_shared::crypto::sha256_hex(signature.as_bytes())
    )
}

// ---------------------------------------------------------------------------
// localisation helpers
// ---------------------------------------------------------------------------

/// Picks the text of `locale` (zh-CN default).
pub fn localized(locale: &str, zh_cn: &str, zh_tw: &str, en_us: &str) -> String {
    match normalize_locale(locale).as_str() {
        "zh-TW" => zh_tw,
        "en-US" => en_us,
        _ => zh_cn,
    }
    .to_owned()
}

/// Display label of an alert type.
pub fn alert_type_label(locale: &str, alert_type: &str) -> String {
    let (cn, tw, en) = match alert_type {
        alert_types::OUT_OF_STOCK_PRODUCTS => ("售罄商品", "售罄商品", "Out of Stock"),
        alert_types::LOW_STOCK_PRODUCTS => ("低库存商品", "低庫存商品", "Low Stock"),
        alert_types::PENDING_PAYMENT_ORDERS => ("待支付订单", "待支付訂單", "Pending Payment"),
        alert_types::PAYMENTS_FAILED => ("支付失败", "支付失敗", "Payment Failed"),
        other => return other.to_owned(),
    };
    match locale.trim().to_lowercase().as_str() {
        "zh-tw" => tw,
        "en-us" | "en" => en,
        _ => cn,
    }
    .to_owned()
}

const INVENTORY_ALERT_TYPES: [&str; 2] = [
    alert_types::OUT_OF_STOCK_PRODUCTS,
    alert_types::LOW_STOCK_PRODUCTS,
];
const PAYMENT_ORDER_ALERT_TYPES: [&str; 2] = [
    alert_types::PENDING_PAYMENT_ORDERS,
    alert_types::PAYMENTS_FAILED,
];

/// Maps a key or any localized label back to the inventory alert key (NTF-04).
pub fn normalize_inventory_alert_type_key(value: &str) -> Option<&'static str> {
    let normalized = value.trim().to_lowercase();
    INVENTORY_ALERT_TYPES.into_iter().find(|key| {
        normalized == *key
            || zs_shared::i18n::LOCALES
                .iter()
                .any(|l| alert_type_label(l, key).trim().to_lowercase() == normalized)
    })
}

/// Inventory alert key of a payload: `alert_type_key` first, then the (possibly
/// localized, legacy) `alert_type` (NTF-04).
pub fn resolve_inventory_alert_type_key(data: &Vars) -> Option<&'static str> {
    let read = |k: &str| data.get(k).map(value_text).unwrap_or_default();
    normalize_inventory_alert_type_key(&read("alert_type_key"))
        .or_else(|| normalize_inventory_alert_type_key(&read("alert_type")))
}

/// Payment/order alert key of a payload (`alert_type_key`, then `alert_type`).
pub fn resolve_payment_order_alert_type_key(data: &Vars) -> Option<&'static str> {
    let read = |k: &str| {
        data.get(k)
            .map(value_text)
            .unwrap_or_default()
            .to_lowercase()
    };
    ["alert_type_key", "alert_type"].into_iter().find_map(|k| {
        PAYMENT_ORDER_ALERT_TYPES
            .into_iter()
            .find(|t| *t == read(k))
    })
}

/// One inventory anomaly (original `InventoryAlertRow`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InventoryAlertRow {
    pub product_id: i64,
    pub sku_id: i64,
    /// Localized product title JSON.
    pub product_title: Value,
    pub sku_code: String,
    /// Localized SKU spec values JSON.
    pub sku_spec_values: Value,
    pub fulfillment_type: String,
    pub alert_type: String,
    pub available_stock: i64,
}

/// Counts inside the payment alert window (original `PaymentOrderAlertCountsRow`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaymentOrderAlertCounts {
    pub pending_payment_orders: i64,
    pub payments_failed: i64,
}

/// First non-empty text of a localized JSON value (`locale`, then zh-CN, then any).
pub fn localized_json_text(value: &Value, locale: &str) -> String {
    match value {
        Value::Object(m) => [locale, zs_shared::i18n::DEFAULT_LOCALE]
            .iter()
            .filter_map(|l| m.get(*l))
            .chain(m.values())
            .map(value_text)
            .find(|t| !t.is_empty())
            .unwrap_or_default(),
        other => value_text(other),
    }
}

fn format_interval(locale: &str, seconds: i64) -> String {
    if seconds > 0 && seconds % 3600 == 0 {
        let h = seconds / 3600;
        localized(
            locale,
            &format!("{h} 小时"),
            &format!("{h} 小時"),
            &format!("{h} hours"),
        )
    } else if seconds > 0 && seconds % 60 == 0 {
        let m = seconds / 60;
        localized(
            locale,
            &format!("{m} 分钟"),
            &format!("{m} 分鐘"),
            &format!("{m} minutes"),
        )
    } else {
        localized(
            locale,
            &format!("{seconds} 秒"),
            &format!("{seconds} 秒"),
            &format!("{seconds} seconds"),
        )
    }
}

fn fulfillment_label(locale: &str, fulfillment_type: &str) -> String {
    match normalize_fulfillment_type(fulfillment_type) {
        "auto" => localized(locale, "自动交付", "自動交付", "Auto"),
        "upstream" => localized(locale, "上游交付", "上游交付", "Upstream"),
        _ => localized(locale, "人工交付", "人工交付", "Manual"),
    }
}

/// `auto`, `upstream`, anything else (including blank) is `manual`.
pub fn normalize_fulfillment_type(value: &str) -> &'static str {
    match value.trim().to_lowercase().as_str() {
        "auto" => "auto",
        "upstream" => "upstream",
        _ => "manual",
    }
}

fn inventory_threshold(alert: &DashboardAlert, alert_type: &str) -> i64 {
    match alert_type {
        alert_types::OUT_OF_STOCK_PRODUCTS => alert.out_of_stock_products_threshold,
        alert_types::LOW_STOCK_PRODUCTS => alert.low_stock_threshold,
        alert_types::PENDING_PAYMENT_ORDERS => alert.pending_payment_orders_threshold,
        alert_types::PAYMENTS_FAILED => alert.payments_failed_threshold,
        _ => 0,
    }
}

fn sku_summary(row: &InventoryAlertRow, locale: &str) -> String {
    let spec = localized_json_text(&row.sku_spec_values, locale);
    if !spec.is_empty() {
        return spec;
    }
    let code = row.sku_code.trim();
    if code.is_empty() || code.eq_ignore_ascii_case(crate::catalog::product::DEFAULT_SKU_CODE) {
        String::new()
    } else {
        code.to_owned()
    }
}

fn inventory_summary(locale: &str, rows: &[&InventoryAlertRow]) -> String {
    let mut sorted = rows.to_vec();
    sorted.sort_by(|a, b| {
        (a.product_id, a.sku_id, &a.alert_type).cmp(&(b.product_id, b.sku_id, &b.alert_type))
    });
    sorted
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let mut title = localized_json_text(&row.product_title, locale);
            if title.is_empty() {
                title = localized(locale, "未命名商品", "未命名商品", "Unnamed item");
            }
            let mut line = format!("{}. {title}", idx + 1);
            let sku = sku_summary(row, locale);
            if !sku.is_empty() {
                line.push_str(&format!(" / {sku}"));
            }
            line.push_str(&format!(
                " [{}]",
                fulfillment_label(locale, &row.fulfillment_type)
            ));
            let status = if row.alert_type == alert_types::OUT_OF_STOCK_PRODUCTS {
                localized(locale, "缺货", "缺貨", "Out of stock")
            } else {
                localized(locale, "低库存", "低庫存", "Low stock")
            };
            let n = row.available_stock;
            line.push_str(&localized(
                locale,
                &format!(" 剩余 {n}（{status}）"),
                &format!(" 剩餘 {n}（{status}）"),
                &format!(" | Remaining {n} ({status})"),
            ));
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn alert_payload(base: &DispatchPayload, data: Vars) -> DispatchPayload {
    DispatchPayload {
        event_type: events::EXCEPTION_ALERT.into(),
        biz_type: biz_types::DASHBOARD_ALERT.into(),
        data,
        ..base.clone()
    }
}

fn put(data: &mut Vars, key: &str, value: impl Into<Value>) {
    data.insert(key.to_owned(), value.into());
}

/// Groups inventory anomalies (minus ignored products) into at most one
/// out-of-stock and one low-stock alert payload.
pub fn build_inventory_alert_payloads(
    setting: &NotificationCenterSetting,
    alert: &DashboardAlert,
    base: &DispatchPayload,
    rows: &[InventoryAlertRow],
) -> Vec<DispatchPayload> {
    let locale = resolve_locale(&base.locale, &setting.default_locale);
    let rows: Vec<&InventoryAlertRow> = rows
        .iter()
        .filter(|r| !setting.ignored_product_ids.contains(&r.product_id))
        .collect();
    let mut out = Vec::new();
    for alert_type in INVENTORY_ALERT_TYPES {
        let group: Vec<&InventoryAlertRow> = rows
            .iter()
            .copied()
            .filter(|r| r.alert_type.trim().eq_ignore_ascii_case(alert_type))
            .collect();
        if group.is_empty() {
            continue;
        }
        let mut products: Vec<i64> = group
            .iter()
            .map(|r| r.product_id)
            .filter(|id| *id != 0)
            .collect();
        products.sort_unstable();
        products.dedup();
        let product_count = i64::try_from(products.len()).unwrap_or(i64::MAX);
        let threshold = inventory_threshold(alert, alert_type);
        if alert_type == alert_types::OUT_OF_STOCK_PRODUCTS && product_count < threshold {
            continue;
        }
        let items = group.len();
        let label = alert_type_label(&locale, alert_type);
        let mut data = base.data.clone();
        put(&mut data, "alert_type", label.clone());
        put(&mut data, "alert_type_label", label);
        put(&mut data, "alert_type_key", alert_type);
        let level = if alert_type == alert_types::OUT_OF_STOCK_PRODUCTS {
            "error"
        } else {
            "warning"
        };
        put(&mut data, "alert_level", level);
        put(&mut data, "alert_value", items.to_string());
        put(&mut data, "alert_threshold", threshold.to_string());
        put(&mut data, "affected_items_count", items.to_string());
        put(
            &mut data,
            "affected_product_count",
            product_count.to_string(),
        );
        put(
            &mut data,
            "affected_items_summary",
            inventory_summary(&locale, &group),
        );
        put(&mut data, "inventory_alert_scope", "inventory");
        let interval = format_interval(&locale, setting.inventory_alert_interval_seconds);
        let message = if alert_type == alert_types::OUT_OF_STOCK_PRODUCTS {
            localized(
                &locale,
                &format!(
                    "检测到 {items} 个 SKU 缺货（涉及 {product_count} 个商品）；本类告警最短 {interval} 发送一次。"
                ),
                &format!(
                    "偵測到 {items} 個 SKU 缺貨（涉及 {product_count} 個商品）；本類告警最短 {interval} 發送一次。"
                ),
                &format!(
                    "Detected {items} out-of-stock SKUs across {product_count} products; this alert is sent at most once every {interval}."
                ),
            )
        } else {
            localized(
                &locale,
                &format!(
                    "检测到 {items} 个 SKU 低库存（涉及 {product_count} 个商品）；本类告警最短 {interval} 发送一次。"
                ),
                &format!(
                    "偵測到 {items} 個 SKU 低庫存（涉及 {product_count} 個商品）；本類告警最短 {interval} 發送一次。"
                ),
                &format!(
                    "Detected {items} low-stock SKUs across {product_count} products; this alert is sent at most once every {interval}."
                ),
            )
        };
        put(&mut data, "message", message);
        out.push(alert_payload(base, data));
    }
    out
}

/// Pending-payment and failed-payment alerts whose count reached the threshold (NTF-08).
pub fn build_payment_order_alert_payloads(
    setting: &NotificationCenterSetting,
    alert: &DashboardAlert,
    base: &DispatchPayload,
    counts: PaymentOrderAlertCounts,
) -> Vec<DispatchPayload> {
    let locale = resolve_locale(&base.locale, &setting.default_locale);
    let interval = format_interval(&locale, setting.payment_order_alert_interval_seconds);
    [
        (alert_types::PENDING_PAYMENT_ORDERS, counts.pending_payment_orders),
        (alert_types::PAYMENTS_FAILED, counts.payments_failed),
    ]
    .into_iter()
    .filter_map(|(alert_type, value)| {
        let threshold = inventory_threshold(alert, alert_type);
        if value < threshold {
            return None;
        }
        let label = alert_type_label(&locale, alert_type);
        let mut data = base.data.clone();
        put(&mut data, "alert_type", label.clone());
        put(&mut data, "alert_type_label", label);
        put(&mut data, "alert_type_key", alert_type);
        put(&mut data, "alert_level", "warning");
        put(&mut data, "alert_value", value.to_string());
        put(&mut data, "alert_threshold", threshold.to_string());
        let message = if alert_type == alert_types::PENDING_PAYMENT_ORDERS {
            localized(
                &locale,
                &format!("当前统计周期内待支付订单 {value} 笔，已达到告警阈值 {threshold} 笔；本类告警最短 {interval} 发送一次。"),
                &format!("目前統計週期內待支付訂單 {value} 筆，已達到告警門檻 {threshold} 筆；本類告警最短 {interval} 發送一次。"),
                &format!("Pending payment orders reached {value} in the current alert window, meeting the threshold of {threshold}; this alert is sent at most once every {interval}."),
            )
        } else {
            localized(
                &locale,
                &format!("当前统计周期内支付失败 {value} 笔，已达到告警阈值 {threshold} 笔；本类告警最短 {interval} 发送一次。"),
                &format!("目前統計週期內支付失敗 {value} 筆，已達到告警門檻 {threshold} 筆；本類告警最短 {interval} 發送一次。"),
                &format!("Payment failures reached {value} in the current alert window, meeting the threshold of {threshold}; this alert is sent at most once every {interval}."),
            )
        };
        put(&mut data, "message", message);
        Some(alert_payload(base, data))
    })
    .collect()
}

/// Payload of the periodic alert check job (original `NewNotificationInventoryAlertCheckTask`).
pub fn alert_check_payload() -> DispatchPayload {
    let mut data = Vars::new();
    data.insert("message".into(), "scheduled_inventory_alert_check".into());
    DispatchPayload {
        event_type: events::EXCEPTION_ALERT_CHECK.into(),
        biz_type: biz_types::DASHBOARD_ALERT.into(),
        ..DispatchPayload::default()
    }
    .with_data(data)
}

impl DispatchPayload {
    #[must_use]
    pub fn with_data(mut self, data: Vars) -> Self {
        self.data = data;
        self
    }
}

// ---------------------------------------------------------------------------
// order helpers for event producers (NTF-07, NTF-11)
// ---------------------------------------------------------------------------

/// One order item as seen by notification formatting.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrderItemLine {
    /// Localized title JSON.
    pub title: Value,
    /// SKU snapshot JSON (`spec_values`, `sku_code`).
    pub sku_snapshot: Value,
    pub quantity: i64,
    pub fulfillment_type: String,
}

/// Item counts per fulfillment type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemCounts {
    pub total: usize,
    pub auto: usize,
    pub manual: usize,
    pub upstream: usize,
}

/// True when at least one item needs manual fulfillment (blank counts as manual; NTF-07).
pub fn has_manual_fulfillment_items(items: &[OrderItemLine]) -> bool {
    items
        .iter()
        .any(|i| normalize_fulfillment_type(&i.fulfillment_type) == "manual")
}

/// Items of a parent order, falling back to its children's items when the parent has none (NTF-11).
pub fn effective_order_items(
    parent: &[OrderItemLine],
    children: &[Vec<OrderItemLine>],
) -> Vec<OrderItemLine> {
    if parent.is_empty() {
        children.iter().flatten().cloned().collect()
    } else {
        parent.to_vec()
    }
}

/// `(items_summary, fulfillment_items_summary, counts)` (original `BuildOrderItemSummaries`).
pub fn build_order_item_summaries(
    items: &[OrderItemLine],
    locale: &str,
) -> (String, String, ItemCounts) {
    let mut counts = ItemCounts {
        total: items.len(),
        ..ItemCounts::default()
    };
    if items.is_empty() {
        let empty = localized(locale, "暂无商品明细", "暫無商品明細", "No item details");
        return (empty.clone(), empty, counts);
    }
    let mut all = Vec::with_capacity(items.len());
    let mut pending = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        let mut title = localized_json_text(&item.title, locale);
        if title.is_empty() {
            title = localized(locale, "未命名商品", "未命名商品", "Unnamed item");
        }
        let mut line = format!("{}. {title}", idx + 1);
        let spec = item
            .sku_snapshot
            .get("spec_values")
            .map(|v| localized_json_text(v, locale))
            .filter(|s| !s.is_empty())
            .or_else(|| {
                item.sku_snapshot
                    .get("sku_code")
                    .map(value_text)
                    .filter(|s| !s.is_empty())
            });
        if let Some(spec) = spec {
            line.push_str(&format!(" / {spec}"));
        }
        line.push_str(&format!(" x{}", item.quantity));
        line.push_str(&format!(
            " [{}]",
            fulfillment_label(locale, &item.fulfillment_type)
        ));
        match normalize_fulfillment_type(&item.fulfillment_type) {
            "auto" => counts.auto += 1,
            "upstream" => {
                counts.upstream += 1;
                pending.push(line.clone());
            }
            _ => {
                counts.manual += 1;
                pending.push(line.clone());
            }
        }
        all.push(line);
    }
    if pending.is_empty() {
        pending.push(localized(
            locale,
            "无需人工跟进",
            "無需人工跟進",
            "No manual follow-up required",
        ));
    }
    (all.join("\n"), pending.join("\n"), counts)
}

/// Delivery summary line.
pub fn delivery_summary(locale: &str, c: ItemCounts) -> String {
    let (t, a, m, u) = (c.total, c.auto, c.manual, c.upstream);
    localized(
        locale,
        &format!("共{t}项，自动交付{a}项，人工交付{m}项，上游交付{u}项"),
        &format!("共{t}項，自動交付{a}項，人工交付{m}項，上游交付{u}項"),
        &format!("Total {t} items, auto {a}, manual {m}, upstream {u}"),
    )
}

/// Payment channel label; wallet-only payments read `wallet/balance` (NTF-11).
pub fn payment_channel_label(
    provider_type: &str,
    channel_type: &str,
    wallet_paid_full: bool,
) -> String {
    let (p, c) = (provider_type.trim(), channel_type.trim());
    if p.is_empty() && c.is_empty() {
        return if wallet_paid_full {
            "wallet/balance".into()
        } else {
            String::new()
        };
    }
    match (p.is_empty(), c.is_empty()) {
        (false, false) => format!("{p}/{c}"),
        (false, true) => p.to_owned(),
        _ => c.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// test-send sample variables
// ---------------------------------------------------------------------------

/// Sample variables for a test send of `scene` (original `BuildTestVariables`).
pub fn test_variables(scene: &str, locale: &str) -> Vars {
    let l = resolve_locale(locale, "zh-CN");
    let customer = localized(
        &l,
        "张三 <zhangsan@example.com>",
        "張三 <zhangsan@example.com>",
        "Alex Zhang <zhangsan@example.com>",
    );
    let manual_line = localized(
        &l,
        "1. ChatGPT Plus 代充 / 周期: 1个月 x1 [人工交付]",
        "1. ChatGPT Plus 代充 / 週期: 1個月 x1 [人工交付]",
        "1. ChatGPT Plus Recharge / Cycle: 1 month x1 [Manual]",
    );
    let delivery = delivery_summary(
        &l,
        ItemCounts {
            total: 2,
            auto: 1,
            manual: 1,
            upstream: 0,
        },
    );
    let mut v = Vars::new();
    let mut set = |k: &str, val: String| {
        v.insert(k.to_owned(), Value::String(val));
    };
    match normalize_event(scene).as_str() {
        events::WALLET_RECHARGE_SUCCESS => {
            set("customer_label", customer);
            set("customer_email", "zhangsan@example.com".into());
            set("recharge_no", "RC202603230001".into());
            set("amount", "100.00".into());
            set("currency", "USD".into());
            set("payment_channel", "epay/alipay".into());
        }
        events::ORDER_PAID_SUCCESS => {
            set("customer_label", customer);
            set("customer_email", "zhangsan@example.com".into());
            set("order_no", "DJ202603230001".into());
            set("amount", "299.00".into());
            set("currency", "USD".into());
            set("payment_channel", "epay/alipay".into());
            let items = [
                localized(
                    &l,
                    "1. Netflix 年付 / 区域: HK x1 [自动交付]",
                    "1. Netflix 年付 / 區域: HK x1 [自動交付]",
                    "1. Netflix Annual / Region: HK x1 [Auto]",
                ),
                localized(
                    &l,
                    "2. ChatGPT Plus 代充 / 周期: 1个月 x1 [人工交付]",
                    "2. ChatGPT Plus 代充 / 週期: 1個月 x1 [人工交付]",
                    "2. ChatGPT Plus Recharge / Cycle: 1 month x1 [Manual]",
                ),
            ];
            set("items_summary", items.join("\n"));
            set("fulfillment_items_summary", manual_line);
            set("delivery_summary", delivery);
        }
        events::MANUAL_FULFILLMENT_PENDING => {
            set("customer_label", customer);
            set("customer_email", "zhangsan@example.com".into());
            set("order_no", "DJ202603230001".into());
            set("order_status", "paid".into());
            set("fulfillment_items_summary", manual_line);
            set("delivery_summary", delivery);
        }
        _ => {
            let label = alert_type_label(&l, alert_types::LOW_STOCK_PRODUCTS);
            set("alert_type", label.clone());
            set("alert_type_label", label);
            set("alert_level", "warning".into());
            set("alert_value", "2".into());
            set("alert_threshold", "5".into());
            set("affected_items_count", "2".into());
            set("affected_product_count", "2".into());
            let items = [
                localized(
                    &l,
                    "1. Netflix 年付 / 区域: HK [自动交付] 剩余 1（低库存）",
                    "1. Netflix 年付 / 區域: HK [自動交付] 剩餘 1（低庫存）",
                    "1. Netflix Annual / Region: HK [Auto] | Remaining 1 (Low stock)",
                ),
                localized(
                    &l,
                    "2. ChatGPT Plus 代充 / 周期: 1个月 [人工交付] 剩余 0（缺货）",
                    "2. ChatGPT Plus 代充 / 週期: 1個月 [人工交付] 剩餘 0（缺貨）",
                    "2. ChatGPT Plus Recharge / Cycle: 1 month [Manual] | Remaining 0 (Out of stock)",
                ),
            ];
            set("affected_items_summary", items.join("\n"));
            set(
                "message",
                localized(
                    &l,
                    "检测到 2 个低库存商品，涉及 2 个具体库存项；本类告警最短 30 分钟发送一次。",
                    "偵測到 2 個低庫存商品，涉及 2 個具體庫存項；本類告警最短 30 分鐘發送一次。",
                    "Detected 2 low-stock products across 2 inventory items; this alert is sent at most once every 30 minutes.",
                ),
            );
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn vars(v: Value) -> Vars {
        v.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn renders_known_and_unknown_placeholders() {
        let v = vars(json!({"order_no": " DJ1 ", "amount": 12.5, "n": null}));
        assert_eq!(
            render_template(
                "  No {{ order_no }} / {{amount}} / {{missing}} / {{n}}  ",
                &v
            ),
            "No DJ1 / 12.5 /  / "
        );
        // Not a variable name: kept verbatim.
        assert_eq!(render_template("{{ a-b }} {x}", &v), "{{ a-b }} {x}");
        assert_eq!(render_template("{{{order_no}}}", &v), "{DJ1}");
    }

    #[test]
    fn render_falls_back_to_title_and_default() {
        let t = LocalizedTemplate {
            title: "T {{x}}".into(),
            body: "  ".into(),
        };
        let r = render(&t, &vars(json!({"x": "1"})), FALLBACK_TITLE);
        assert_eq!(r.body, "T 1");
        let empty = LocalizedTemplate::default();
        assert_eq!(
            render(&empty, &Vars::new(), FALLBACK_TITLE).title,
            "Notification"
        );
    }

    #[test]
    fn compose_plain_text_joins_parts() {
        assert_eq!(compose_plain_text(" T ", " B "), "T\n\nB");
        assert_eq!(compose_plain_text("", "B"), "B");
        assert_eq!(compose_plain_text("T", ""), "T");
    }

    #[test]
    fn dedupe_key_ignores_occurred_at_and_order() {
        let a = DispatchPayload {
            event_type: "ORDER_PAID_SUCCESS".into(),
            biz_type: "order".into(),
            biz_id: 7,
            data: vars(json!({"b": 1, "a": "x", "occurred_at": "t1"})),
            ..DispatchPayload::default()
        };
        let mut b = a.clone();
        b.event_type = "order_paid_success".into();
        b.data = vars(json!({"a": "x", "occurred_at": "t2", "b": 1}));
        assert_eq!(dedupe_key(&a), dedupe_key(&b));
        b.biz_id = 8;
        assert_ne!(dedupe_key(&a), dedupe_key(&b));
        assert!(dedupe_key(&a).starts_with("notification:dedupe:"));
    }

    #[test]
    fn scene_switches_follow_event() {
        let s = Scenes {
            wallet_recharge_success: false,
            order_paid_success: true,
            manual_fulfillment_pending: true,
            exception_alert: false,
        };
        assert!(!scene_enabled(&s, "wallet_recharge_success"));
        assert!(scene_enabled(&s, " ORDER_PAID_SUCCESS "));
        assert!(!scene_enabled(&s, "exception_alert_check"));
        assert!(!scene_enabled(&s, "unknown"));
        assert!(
            NotifyEvent {
                event_type: "bogus".into(),
                ..NotifyEvent::default()
            }
            .into_payload()
            .is_none()
        );
    }

    // NTF-04 ②: a legacy payload carrying only the localized label still resolves.
    #[test]
    fn ntf04_localized_alert_type_resolves_to_key() {
        let legacy = vars(json!({"alert_type": "低庫存商品"}));
        assert_eq!(
            resolve_inventory_alert_type_key(&legacy),
            Some(alert_types::LOW_STOCK_PRODUCTS)
        );
        let en = vars(json!({"alert_type": "out of stock"}));
        assert_eq!(
            resolve_inventory_alert_type_key(&en),
            Some(alert_types::OUT_OF_STOCK_PRODUCTS)
        );
        let keyed = vars(json!({"alert_type": "whatever", "alert_type_key": "low_stock_products"}));
        assert_eq!(
            resolve_inventory_alert_type_key(&keyed),
            Some(alert_types::LOW_STOCK_PRODUCTS)
        );
        // NTF-04 ③: pending orders are not an inventory alert.
        let pending = vars(json!({"alert_type_key": "pending_payment_orders"}));
        assert_eq!(resolve_inventory_alert_type_key(&pending), None);
        assert_eq!(
            resolve_payment_order_alert_type_key(&pending),
            Some(alert_types::PENDING_PAYMENT_ORDERS)
        );
    }

    fn row(product: i64, sku: i64, kind: &str, stock: i64) -> InventoryAlertRow {
        InventoryAlertRow {
            product_id: product,
            sku_id: sku,
            product_title: json!({"zh-CN": format!("商品{product}"), "en-US": format!("P{product}")}),
            sku_code: "DEFAULT".into(),
            sku_spec_values: Value::Null,
            fulfillment_type: "manual".into(),
            alert_type: kind.into(),
            available_stock: stock,
        }
    }

    // NTF-04: payloads keep the machine key separate from the display label.
    #[test]
    fn ntf04_inventory_payloads_carry_key_and_label() {
        let setting = NotificationCenterSetting {
            ignored_product_ids: vec![3],
            default_locale: "en-US".into(),
            ..NotificationCenterSetting::default()
        };
        let alert = crate::settings::schema::storefront::DashboardSetting::default().alert;
        let rows = vec![
            row(1, 11, alert_types::LOW_STOCK_PRODUCTS, 2),
            row(2, 21, alert_types::OUT_OF_STOCK_PRODUCTS, 0),
            row(3, 31, alert_types::OUT_OF_STOCK_PRODUCTS, 0),
        ];
        let out =
            build_inventory_alert_payloads(&setting, &alert, &DispatchPayload::default(), &rows);
        assert_eq!(out.len(), 2);
        let oos = &out[0].data;
        assert_eq!(out[0].event_type, events::EXCEPTION_ALERT);
        assert_eq!(out[0].biz_type, biz_types::DASHBOARD_ALERT);
        assert_eq!(oos["alert_type_key"], "out_of_stock_products");
        assert_eq!(oos["alert_type"], "Out of Stock");
        assert_eq!(oos["alert_level"], "error");
        assert_eq!(oos["affected_items_count"], "1");
        assert_eq!(
            oos["affected_items_summary"],
            "1. P2 [Manual] | Remaining 0 (Out of stock)"
        );
        assert_eq!(
            oos["message"],
            "Detected 1 out-of-stock SKUs across 1 products; this alert is sent at most once every 30 minutes."
        );
        assert_eq!(out[1].data["alert_type_key"], "low_stock_products");
    }

    #[test]
    fn out_of_stock_below_product_threshold_is_skipped() {
        let setting = NotificationCenterSetting::default();
        let mut alert = crate::settings::schema::storefront::DashboardSetting::default().alert;
        alert.out_of_stock_products_threshold = 2;
        let rows = vec![row(1, 11, alert_types::OUT_OF_STOCK_PRODUCTS, 0)];
        assert!(
            build_inventory_alert_payloads(&setting, &alert, &DispatchPayload::default(), &rows)
                .is_empty()
        );
    }

    // NTF-08: payment alerts only fire at/above their thresholds.
    #[test]
    fn ntf08_payment_alerts_respect_thresholds() {
        let setting = NotificationCenterSetting::default();
        let alert = crate::settings::schema::storefront::DashboardSetting::default().alert;
        let counts = PaymentOrderAlertCounts {
            pending_payment_orders: 20,
            payments_failed: 9,
        };
        let out = build_payment_order_alert_payloads(
            &setting,
            &alert,
            &DispatchPayload::default(),
            counts,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data["alert_type_key"], "pending_payment_orders");
        assert_eq!(out[0].data["alert_value"], "20");
        assert_eq!(out[0].data["alert_threshold"], "20");
        assert_eq!(out[0].data["alert_type"], "待支付订单");
    }

    // NTF-07
    #[test]
    fn ntf07_manual_detection() {
        let item = |t: &str| OrderItemLine {
            fulfillment_type: t.into(),
            ..OrderItemLine::default()
        };
        assert!(!has_manual_fulfillment_items(&[]));
        assert!(!has_manual_fulfillment_items(&[item("upstream")]));
        assert!(!has_manual_fulfillment_items(&[item("auto")]));
        assert!(has_manual_fulfillment_items(&[item("manual")]));
        assert!(has_manual_fulfillment_items(&[item("  ")]));
        assert!(has_manual_fulfillment_items(&[
            item("upstream"),
            item("manual")
        ]));
    }

    // NTF-11
    #[test]
    fn ntf11_parent_without_items_aggregates_children() {
        let child = |title: &str, qty: i64, t: &str| OrderItemLine {
            title: json!({"zh-CN": title}),
            sku_snapshot: json!({"sku_code": "S1"}),
            quantity: qty,
            fulfillment_type: t.into(),
        };
        let items = effective_order_items(
            &[],
            &[
                vec![child("A", 1, "upstream")],
                vec![child("B", 2, "manual")],
            ],
        );
        let (all, pending, counts) = build_order_item_summaries(&items, "zh-CN");
        assert_eq!(all, "1. A / S1 x1 [上游交付]\n2. B / S1 x2 [人工交付]");
        assert_eq!(pending, all);
        assert_eq!(
            counts,
            ItemCounts {
                total: 2,
                auto: 0,
                manual: 1,
                upstream: 1
            }
        );
        assert_eq!(
            delivery_summary("en-US", counts),
            "Total 2 items, auto 0, manual 1, upstream 1"
        );
        assert_eq!(payment_channel_label("", "", true), "wallet/balance");
        assert_eq!(
            payment_channel_label("epay", "alipay", false),
            "epay/alipay"
        );
    }

    #[test]
    fn empty_items_render_placeholder() {
        let (all, pending, _) = build_order_item_summaries(&[], "en-US");
        assert_eq!(all, "No item details");
        assert_eq!(pending, "No item details");
    }

    #[test]
    fn test_variables_by_scene() {
        let v = test_variables("wallet_recharge_success", "en-US");
        assert_eq!(v["recharge_no"], "RC202603230001");
        let a = test_variables("exception_alert", "zh-TW");
        assert_eq!(a["alert_type"], "低庫存商品");
    }

    #[test]
    fn template_variables_add_meta() {
        let p = DispatchPayload {
            event_type: " Order_Paid_Success ".into(),
            biz_type: "order".into(),
            biz_id: 3,
            ..DispatchPayload::default()
        };
        let now = DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_default();
        let v = template_variables(&p, now);
        assert_eq!(v["event_type"], "order_paid_success");
        assert_eq!(v["biz_id"], "3");
        assert_eq!(v["occurred_at"], "2026-01-02 03:04:05");
    }
}
