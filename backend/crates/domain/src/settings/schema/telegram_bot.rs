//! `telegram_bot_config` and `telegram_bot_runtime_status`
//! (port of `schema/messaging/telegram_bot.go`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zs_shared::i18n::LOCALES;

use super::value::{Obj, as_obj, read_bool, read_int, read_string, read_string_list};

/// `{locale: text}`.
pub type Text = BTreeMap<String, String>;

const HELP_ITEMS_MAX: usize = 12;
const MENU_ITEMS_MAX: usize = 20;
const MENU_ACTION_TYPES: [&str; 4] = ["builtin", "url", "web_app", "command"];

/// Built-in menu keys in display order (must match the bot).
pub const BUILTIN_MENU_KEYS: [&str; 7] = [
    "shop_home",
    "my_orders",
    "my_wallet",
    "affiliate",
    "gift_card",
    "switch_language",
    "contact_support",
];

fn text3(zh_cn: &str, zh_tw: &str, en_us: &str) -> Text {
    [("zh-CN", zh_cn), ("zh-TW", zh_tw), ("en-US", en_us)]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn builtin_label(key: &str) -> Text {
    match key {
        "shop_home" => text3("🛍️ 开始购物", "🛍️ 開始購物", "🛍️ Shop Now"),
        "my_orders" => text3("📦 我的订单", "📦 我的訂單", "📦 My Orders"),
        "my_wallet" => text3("💰 我的钱包", "💰 我的錢包", "💰 My Wallet"),
        "affiliate" => text3("📣 推广返利", "📣 推廣返利", "📣 Affiliate"),
        "gift_card" => text3("🎁 礼品卡兑换", "🎁 禮品卡兌換", "🎁 Redeem Gift Card"),
        "switch_language" => text3("🌐 切换语言", "🌐 切換語言", "🌐 Language"),
        _ => text3("❓ 帮助中心", "❓ 幫助中心", "❓ Help"),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BotBasic {
    pub display_name: String,
    pub description: Text,
    pub support_url: String,
    pub cover_url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BotWelcome {
    pub enabled: bool,
    pub message: Text,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BotHelpItem {
    pub key: String,
    pub enabled: bool,
    pub order: i64,
    pub summary: Text,
    pub title: Text,
    pub content: Text,
    pub show_support_link: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BotHelp {
    pub enabled: bool,
    pub title: Text,
    pub intro: Text,
    pub center_hint: Text,
    pub support_hint: Text,
    pub items: Vec<BotHelpItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BotMenuAction {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BotMenuItem {
    pub key: String,
    pub enabled: bool,
    pub order: i64,
    pub label: Text,
    pub action: BotMenuAction,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BotMenu {
    pub items: Vec<BotMenuItem>,
}

/// Telegram bot configuration (whole-object PUT body and stored shape).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TelegramBotSetting {
    pub enabled: bool,
    pub default_locale: String,
    pub config_version: i64,
    pub basic: BotBasic,
    pub welcome: BotWelcome,
    pub help: BotHelp,
    pub menu: BotMenu,
}

fn help_item(key: &str, order: i64, summary: Text, content: Text, support: bool) -> BotHelpItem {
    BotHelpItem {
        key: key.into(),
        enabled: true,
        order,
        title: summary.clone(),
        summary,
        content,
        show_support_link: support,
    }
}

fn builtin_menu_item(key: &str, order: i64) -> BotMenuItem {
    BotMenuItem {
        key: key.into(),
        enabled: true,
        order,
        label: builtin_label(key),
        action: BotMenuAction {
            kind: "builtin".into(),
            value: String::new(),
        },
    }
}

impl TelegramBotSetting {
    /// The original default configuration (help center + 7 built-in menu items).
    pub fn defaults() -> Self {
        Self {
            enabled: false,
            default_locale: "zh-CN".into(),
            config_version: 0,
            basic: BotBasic::default(),
            welcome: BotWelcome::default(),
            help: BotHelp {
                enabled: true,
                title: text3("❓ 帮助中心", "❓ 幫助中心", "❓ Help Center"),
                intro: text3(
                    "这里整理了下单、订单、钱包和客服入口，先点最接近你问题的按钮。",
                    "這裡整理了下單、訂單、錢包與客服入口，先點最接近你問題的按鈕。",
                    "Quick answers for shopping, orders, wallet, and support are listed here. Start with the closest topic.",
                ),
                center_hint: text3(
                    "如果还是没解决，再进入客服入口即可。",
                    "如果還是沒解決，再進入客服入口即可。",
                    "If the issue remains, open the support topic from below.",
                ),
                support_hint: text3(
                    "当前还没有配置客服链接，你可以先查看上面的常见问题，或稍后再试。",
                    "目前還沒有配置客服連結，你可以先查看上面的常見問題，或稍後再試。",
                    "The support link is not configured yet. You can review the common topics above or try again later.",
                ),
                items: vec![
                    help_item(
                        "shop",
                        1,
                        text3("🛍️ 怎么下单", "🛍️ 怎麼下單", "🛍️ How to buy"),
                        text3(
                            "先点“开始购物”，进入分类后选择商品与规格，再确认数量并完成支付。支付成功后，订单会自动进入处理流程。",
                            "先點「開始購物」，進入分類後選擇商品與規格，再確認數量並完成付款。付款成功後，訂單會自動進入處理流程。",
                            "Tap \"Shop Now\", choose a category, pick the product and spec, confirm quantity, then finish payment. Your order enters processing right after payment succeeds.",
                        ),
                        false,
                    ),
                    help_item(
                        "orders",
                        2,
                        text3("📦 订单问题", "📦 訂單問題", "📦 Order issues"),
                        text3(
                            "在“我的订单”里可以查看状态、支付结果与发货内容。若支付完成但暂时没发货，先刷新订单状态；仍有问题再联系人工客服。",
                            "在「我的訂單」裡可以查看狀態、付款結果與發貨內容。若付款完成但暫時沒發貨，先刷新訂單狀態；仍有問題再聯繫人工客服。",
                            "Use \"My Orders\" to review status, payment result, and delivery content. If payment is done but delivery is pending, refresh the order status first and contact support if it still looks wrong.",
                        ),
                        false,
                    ),
                    help_item(
                        "wallet",
                        3,
                        text3("💰 钱包充值", "💰 錢包儲值", "💰 Wallet help"),
                        text3(
                            "打开“我的钱包”可以查看余额、充值记录并发起充值。充值成功后，余额会更新，可直接用于支付订单。",
                            "打開「我的錢包」可以查看餘額、儲值記錄並發起儲值。儲值成功後，餘額會更新，可直接用於支付訂單。",
                            "Open \"My Wallet\" to view balance, recharge history, and create a recharge. Once the recharge succeeds, the balance updates and can be used for orders.",
                        ),
                        false,
                    ),
                    help_item(
                        "support",
                        4,
                        text3("💬 联系客服", "💬 聯繫客服", "💬 Contact support"),
                        text3(
                            "如果上面的自助说明仍然无法解决问题，请通过下方客服入口联系人工，并尽量附上订单号、商品名和问题截图。",
                            "如果上面的自助說明仍然無法解決問題，請透過下方客服入口聯繫人工，並盡量附上訂單號、商品名與問題截圖。",
                            "If the self-service guides above do not solve the issue, contact support using the link below and include your order number, product name, and screenshots when possible.",
                        ),
                        true,
                    ),
                ],
            },
            menu: BotMenu {
                items: BUILTIN_MENU_KEYS
                    .iter()
                    .zip(1..)
                    .map(|(k, i)| builtin_menu_item(k, i))
                    .collect(),
            },
        }
    }

    /// Normalizes texts, help items and menu items (as done on every save).
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.default_locale = self.default_locale.trim().to_owned();
        self.basic.display_name = self.basic.display_name.trim().to_owned();
        self.basic.support_url = self.basic.support_url.trim().to_owned();
        self.basic.cover_url = self.basic.cover_url.trim().to_owned();
        self.basic.description = normalize_text(&self.basic.description);
        self.welcome.message = normalize_text(&self.welcome.message);
        let h = &mut self.help;
        h.title = normalize_text(&h.title);
        h.intro = normalize_text(&h.intro);
        h.center_hint = normalize_text(&h.center_hint);
        h.support_hint = normalize_text(&h.support_hint);
        h.items.truncate(HELP_ITEMS_MAX);
        for item in &mut h.items {
            item.key = item.key.trim().to_owned();
            item.summary = normalize_text(&item.summary);
            item.title = normalize_text(&item.title);
            item.content = normalize_text(&item.content);
        }
        self.menu.items = normalize_menu_items(std::mem::take(&mut self.menu.items));
        self
    }

    /// Decodes stored JSON over `fallback`, migrating the legacy flat format.
    pub fn decode(raw: Option<&Value>, fallback: Self) -> Self {
        let Some(o) = raw.and_then(Value::as_object) else {
            return fallback;
        };
        if o.contains_key("bot_display_name") {
            return migrate_legacy(o, fallback);
        }
        let mut next = fallback;
        next.enabled = read_bool(o, "enabled", next.enabled);
        next.default_locale = read_string(o, "default_locale", &next.default_locale);
        next.config_version = read_int(o, "config_version", next.config_version);
        if let Some(b) = as_obj(o.get("basic")) {
            next.basic.display_name = read_string(b, "display_name", &next.basic.display_name);
            next.basic.description = read_text(b, "description", &next.basic.description);
            next.basic.support_url = read_string(b, "support_url", &next.basic.support_url);
            next.basic.cover_url = read_string(b, "cover_url", &next.basic.cover_url);
        }
        if let Some(w) = as_obj(o.get("welcome")) {
            next.welcome.enabled = read_bool(w, "enabled", next.welcome.enabled);
            next.welcome.message = read_text(w, "message", &next.welcome.message);
        }
        if let Some(h) = as_obj(o.get("help")) {
            let help = &mut next.help;
            help.enabled = read_bool(h, "enabled", help.enabled);
            help.title = read_text(h, "title", &help.title);
            help.intro = read_text(h, "intro", &help.intro);
            help.center_hint = read_text(h, "center_hint", &help.center_hint);
            help.support_hint = read_text(h, "support_hint", &help.support_hint);
            if let Some(items) = h.get("items").and_then(Value::as_array) {
                help.items = items
                    .iter()
                    .filter_map(Value::as_object)
                    .map(|m| BotHelpItem {
                        key: read_string(m, "key", ""),
                        enabled: read_bool(m, "enabled", true),
                        order: read_int(m, "order", 0),
                        summary: read_text(m, "summary", &Text::new()),
                        title: read_text(m, "title", &Text::new()),
                        content: read_text(m, "content", &Text::new()),
                        show_support_link: read_bool(m, "show_support_link", false),
                    })
                    .collect();
            }
        }
        if let Some(m) = as_obj(o.get("menu")) {
            next.menu.items = m
                .get("items")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_object)
                        .map(|m| {
                            let action = as_obj(m.get("action"));
                            BotMenuItem {
                                key: read_string(m, "key", ""),
                                enabled: read_bool(m, "enabled", true),
                                order: read_int(m, "order", 0),
                                label: read_text(m, "label", &Text::new()),
                                action: BotMenuAction {
                                    kind: action
                                        .map(|a| read_string(a, "type", "builtin"))
                                        .unwrap_or_default(),
                                    value: action
                                        .map(|a| read_string(a, "value", ""))
                                        .unwrap_or_default(),
                                },
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        next
    }

    /// Stored / admin JSON shape.
    pub fn encode(&self) -> Value {
        json!(self)
    }
}

fn migrate_legacy(o: &Obj, fallback: TelegramBotSetting) -> TelegramBotSetting {
    let mut next = fallback;
    let locale = read_string(o, "default_locale", "zh-CN");
    next.default_locale.clone_from(&locale);
    next.basic.display_name = read_string(o, "bot_display_name", "");
    let description = read_string(o, "bot_description", "");
    if !description.is_empty() {
        next.basic.description = Text::from([(locale.clone(), description)]);
    }
    next.basic.support_url = read_string(o, "support_link", "");
    next.basic.cover_url = read_string(o, "welcome_cover_url", "");
    let welcome = read_string(o, "welcome_message", "");
    if !welcome.is_empty() {
        next.welcome.enabled = true;
        next.welcome.message = Text::from([(locale, welcome)]);
    }
    next
}

fn read_text(o: &Obj, key: &str, fallback: &Text) -> Text {
    let Some(m) = as_obj(o.get(key)) else {
        return fallback.clone();
    };
    let out: Text = m
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.trim().to_owned())))
        .collect();
    if out.is_empty() {
        fallback.clone()
    } else {
        out
    }
}

/// Ensures every supported locale key exists and trims values.
pub fn normalize_text(t: &Text) -> Text {
    let mut out: Text = LOCALES
        .iter()
        .map(|l| ((*l).to_owned(), String::new()))
        .collect();
    for (k, v) in t {
        out.insert(k.clone(), v.trim().to_owned());
    }
    out
}

/// Appends missing built-in menu keys (keeping existing items untouched).
pub fn ensure_builtin_menu(mut items: Vec<BotMenuItem>) -> Vec<BotMenuItem> {
    let mut max_order = items.iter().map(|i| i.order).max().unwrap_or(0).max(0);
    for key in BUILTIN_MENU_KEYS {
        if items.iter().any(|i| i.key == key) {
            continue;
        }
        max_order += 1;
        items.push(builtin_menu_item(key, max_order));
    }
    items
}

fn normalize_menu_items(mut items: Vec<BotMenuItem>) -> Vec<BotMenuItem> {
    items.truncate(MENU_ITEMS_MAX);
    for item in &mut items {
        item.key = item.key.trim().to_owned();
        item.label = normalize_text(&item.label);
        item.action.kind = item.action.kind.trim().to_owned();
        item.action.value = item.action.value.trim().to_owned();
        if !MENU_ACTION_TYPES.contains(&item.action.kind.as_str()) {
            item.action.kind = "builtin".into();
        }
    }
    ensure_builtin_menu(items)
}

/// Bot heartbeat status.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TelegramBotRuntimeStatus {
    pub connected: bool,
    pub last_seen_at: String,
    pub bot_version: String,
    pub webhook_status: String,
    pub machine_code: String,
    pub license_status: String,
    pub license_expires_at: String,
    pub warnings: Vec<String>,
    pub config_version: i64,
    pub last_config_sync_at: String,
}

impl TelegramBotRuntimeStatus {
    pub fn decode(raw: Option<&Value>) -> Self {
        let mut n = Self::default();
        let Some(o) = raw.and_then(Value::as_object) else {
            return n;
        };
        n.connected = read_bool(o, "connected", false);
        n.last_seen_at = read_string(o, "last_seen_at", "");
        n.bot_version = read_string(o, "bot_version", "");
        n.webhook_status = read_string(o, "webhook_status", "");
        n.machine_code = read_string(o, "machine_code", "");
        n.license_status = read_string(o, "license_status", "");
        n.license_expires_at = read_string(o, "license_expires_at", "");
        n.warnings = read_string_list(o, "warnings", &[]);
        n.config_version = read_int(o, "config_version", 0);
        n.last_config_sync_at = read_string(o, "last_config_sync_at", "");
        n
    }

    pub fn encode(&self) -> Value {
        json!(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_every_builtin_menu() {
        let d = TelegramBotSetting::defaults();
        let keys: Vec<&str> = d.menu.items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, BUILTIN_MENU_KEYS);
        assert_eq!(d.help.items.len(), 4);
    }

    #[test]
    fn normalize_backfills_builtin_keys_and_fixes_actions() {
        let s = TelegramBotSetting {
            menu: BotMenu {
                items: vec![BotMenuItem {
                    key: " my_orders ".into(),
                    order: 9,
                    action: BotMenuAction {
                        kind: "evil".into(),
                        value: String::new(),
                    },
                    ..Default::default()
                }],
            },
            ..Default::default()
        }
        .normalized();
        assert_eq!(s.menu.items.len(), 7);
        assert_eq!(s.menu.items[0].action.kind, "builtin");
        assert_eq!(s.menu.items[1].key, "shop_home");
        assert_eq!(s.menu.items[1].order, 10);
        assert_eq!(s.basic.description.len(), 3);
    }

    #[test]
    fn legacy_flat_config_migrates() {
        let s = TelegramBotSetting::decode(
            Some(
                &json!({"bot_display_name": "Shop", "welcome_message": "hi", "default_locale": "en-US"}),
            ),
            TelegramBotSetting::defaults(),
        );
        assert_eq!(s.basic.display_name, "Shop");
        assert!(s.welcome.enabled);
        assert_eq!(
            s.welcome.message.get("en-US").map(String::as_str),
            Some("hi")
        );
    }
}
