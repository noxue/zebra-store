//! `notification_center_config` (port of `schema/messaging/notification.go`).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::value::{
    Obj, as_obj, is_email_address, read_bool, read_id_list, read_int, read_string, read_string_list,
};
use crate::Error;

const INVALID: &str = "notification config invalid";
const LOCALES: [&str; 3] = ["zh-CN", "zh-TW", "en-US"];
const FEISHU_ID_TYPES: [&str; 5] = ["chat_id", "open_id", "user_id", "union_id", "email"];
const INTERVAL_MIN: i64 = 60;
const INTERVAL_MAX: i64 = 604_800;
const INVENTORY_INTERVAL_DEFAULT: i64 = 1800;
const PAYMENT_ALERT_INTERVAL_DEFAULT: i64 = 1800;
const PAYMENT_ALERT_CHECK_DEFAULT: i64 = 86400;
const DEDUPE_DEFAULT: i64 = 300;

/// A basic channel (email / telegram).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Channel {
    pub enabled: bool,
    pub recipients: Vec<String>,
}

/// Feishu self-built app bot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FeishuChannel {
    pub enabled: bool,
    pub app_id: String,
    pub app_secret: String,
    pub receive_id_type: String,
    pub recipients: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Channels {
    pub email: Channel,
    pub telegram: Channel,
    pub feishu: FeishuChannel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Scenes {
    pub wallet_recharge_success: bool,
    pub order_paid_success: bool,
    pub manual_fulfillment_pending: bool,
    pub exception_alert: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LocalizedTemplate {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SceneTemplate {
    #[serde(rename = "zh-CN")]
    pub zh_cn: LocalizedTemplate,
    #[serde(rename = "zh-TW")]
    pub zh_tw: LocalizedTemplate,
    #[serde(rename = "en-US")]
    pub en_us: LocalizedTemplate,
}

impl SceneTemplate {
    fn locales_mut(&mut self) -> [&mut LocalizedTemplate; 3] {
        [&mut self.zh_cn, &mut self.zh_tw, &mut self.en_us]
    }

    /// Template for `locale` (unknown locales use zh-CN).
    pub fn resolve(&self, locale: &str) -> &LocalizedTemplate {
        match locale.trim() {
            "zh-TW" => &self.zh_tw,
            "en-US" => &self.en_us,
            _ => &self.zh_cn,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Templates {
    pub wallet_recharge_success: SceneTemplate,
    pub order_paid_success: SceneTemplate,
    pub manual_fulfillment_pending: SceneTemplate,
    pub exception_alert: SceneTemplate,
}

impl Templates {
    fn scenes_mut(&mut self) -> [(&'static str, &mut SceneTemplate); 4] {
        [
            ("wallet_recharge_success", &mut self.wallet_recharge_success),
            ("order_paid_success", &mut self.order_paid_success),
            (
                "manual_fulfillment_pending",
                &mut self.manual_fulfillment_pending,
            ),
            ("exception_alert", &mut self.exception_alert),
        ]
    }
}

/// Notification center settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotificationCenterSetting {
    pub default_locale: String,
    pub channels: Channels,
    pub scenes: Scenes,
    pub templates: Templates,
    pub dedupe_ttl_seconds: i64,
    pub inventory_alert_interval_seconds: i64,
    pub payment_order_alert_interval_seconds: i64,
    pub payment_order_alert_check_interval_seconds: i64,
    pub ignored_product_ids: Vec<i64>,
}

fn tpl(title: &str, body: &str) -> LocalizedTemplate {
    LocalizedTemplate {
        title: title.into(),
        body: body.into(),
    }
}

impl Default for NotificationCenterSetting {
    fn default() -> Self {
        Self {
            default_locale: "zh-CN".into(),
            channels: Channels {
                feishu: FeishuChannel {
                    receive_id_type: "chat_id".into(),
                    ..FeishuChannel::default()
                },
                ..Channels::default()
            },
            scenes: Scenes {
                wallet_recharge_success: true,
                order_paid_success: true,
                manual_fulfillment_pending: true,
                exception_alert: true,
            },
            templates: Templates {
                wallet_recharge_success: SceneTemplate {
                    zh_cn: tpl("用户充值成功通知", "用户：{{customer_label}}\n邮箱：{{customer_email}}\n充值单号：{{recharge_no}}\n充值金额：{{amount}} {{currency}}\n支付渠道：{{payment_channel}}"),
                    zh_tw: tpl("用戶儲值成功通知", "用戶：{{customer_label}}\n郵箱：{{customer_email}}\n儲值單號：{{recharge_no}}\n儲值金額：{{amount}} {{currency}}\n支付渠道：{{payment_channel}}"),
                    en_us: tpl("Wallet Recharge Succeeded", "Customer: {{customer_label}}\nEmail: {{customer_email}}\nRecharge No: {{recharge_no}}\nAmount: {{amount}} {{currency}}\nChannel: {{payment_channel}}"),
                },
                order_paid_success: SceneTemplate {
                    zh_cn: tpl("订单支付成功通知", "购买人：{{customer_label}}\n邮箱：{{customer_email}}\n订单号：{{order_no}}\n订单金额：{{amount}} {{currency}}\n支付渠道：{{payment_channel}}\n商品明细：\n{{items_summary}}\n交付摘要：{{delivery_summary}}"),
                    zh_tw: tpl("訂單支付成功通知", "購買人：{{customer_label}}\n郵箱：{{customer_email}}\n訂單號：{{order_no}}\n訂單金額：{{amount}} {{currency}}\n支付渠道：{{payment_channel}}\n商品明細：\n{{items_summary}}\n交付摘要：{{delivery_summary}}"),
                    en_us: tpl("Order Payment Succeeded", "Customer: {{customer_label}}\nEmail: {{customer_email}}\nOrder No: {{order_no}}\nAmount: {{amount}} {{currency}}\nChannel: {{payment_channel}}\nItems:\n{{items_summary}}\nDelivery Summary: {{delivery_summary}}"),
                },
                manual_fulfillment_pending: SceneTemplate {
                    zh_cn: tpl("待人工交付订单提醒", "购买人：{{customer_label}}\n邮箱：{{customer_email}}\n订单号：{{order_no}}\n订单状态：{{order_status}}\n待处理商品：\n{{fulfillment_items_summary}}\n交付摘要：{{delivery_summary}}"),
                    zh_tw: tpl("待人工交付訂單提醒", "購買人：{{customer_label}}\n郵箱：{{customer_email}}\n訂單號：{{order_no}}\n訂單狀態：{{order_status}}\n待處理商品：\n{{fulfillment_items_summary}}\n交付摘要：{{delivery_summary}}"),
                    en_us: tpl("Manual Fulfillment Required", "Customer: {{customer_label}}\nEmail: {{customer_email}}\nOrder No: {{order_no}}\nOrder Status: {{order_status}}\nPending Items:\n{{fulfillment_items_summary}}\nDelivery Summary: {{delivery_summary}}"),
                },
                exception_alert: SceneTemplate {
                    zh_cn: tpl("系统异常告警", "告警类型：{{alert_type}}\n告警级别：{{alert_level}}\n当前值：{{alert_value}}\n阈值：{{alert_threshold}}\n详情：{{message}}\n{{affected_items_summary}}"),
                    zh_tw: tpl("系統異常告警", "告警類型：{{alert_type}}\n告警級別：{{alert_level}}\n當前值：{{alert_value}}\n閾值：{{alert_threshold}}\n詳情：{{message}}\n{{affected_items_summary}}"),
                    en_us: tpl("System Exception Alert", "Type: {{alert_type}}\nLevel: {{alert_level}}\nCurrent: {{alert_value}}\nThreshold: {{alert_threshold}}\nDetails: {{message}}\n{{affected_items_summary}}"),
                },
            },
            dedupe_ttl_seconds: DEDUPE_DEFAULT,
            inventory_alert_interval_seconds: INVENTORY_INTERVAL_DEFAULT,
            payment_order_alert_interval_seconds: PAYMENT_ALERT_INTERVAL_DEFAULT,
            payment_order_alert_check_interval_seconds: PAYMENT_ALERT_CHECK_DEFAULT,
            ignored_product_ids: Vec::new(),
        }
        .normalized()
    }
}

/// A Telegram chat id (`-?\d{5,20}`).
pub fn is_telegram_chat_id(v: &str) -> bool {
    let digits = v.strip_prefix('-').unwrap_or(v);
    (5..=20).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit())
}

fn dedupe_list(items: &[String], lower: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let mut t = item.trim().to_owned();
        if t.is_empty() {
            continue;
        }
        if lower {
            t = t.to_lowercase();
        }
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

fn in_range_or(v: i64, min: i64, max: i64, default: i64) -> i64 {
    if (min..=max).contains(&v) { v } else { default }
}

/// Normalizes a locale to one of the supported ones (default zh-CN).
pub fn normalize_locale(locale: &str) -> String {
    let t = locale.trim();
    if LOCALES.contains(&t) { t } else { "zh-CN" }.to_owned()
}

// --- patches ---------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChannelPatch {
    pub enabled: Option<bool>,
    pub recipients: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FeishuPatch {
    pub enabled: Option<bool>,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub receive_id_type: Option<String>,
    pub recipients: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChannelsPatch {
    pub email: Option<ChannelPatch>,
    pub telegram: Option<ChannelPatch>,
    pub feishu: Option<FeishuPatch>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScenesPatch {
    pub wallet_recharge_success: Option<bool>,
    pub order_paid_success: Option<bool>,
    pub manual_fulfillment_pending: Option<bool>,
    pub exception_alert: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LocalizedTemplatePatch {
    pub title: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SceneTemplatePatch {
    #[serde(rename = "zh-CN")]
    pub zh_cn: Option<LocalizedTemplatePatch>,
    #[serde(rename = "zh-TW")]
    pub zh_tw: Option<LocalizedTemplatePatch>,
    #[serde(rename = "en-US")]
    pub en_us: Option<LocalizedTemplatePatch>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TemplatesPatch {
    pub wallet_recharge_success: Option<SceneTemplatePatch>,
    pub order_paid_success: Option<SceneTemplatePatch>,
    pub manual_fulfillment_pending: Option<SceneTemplatePatch>,
    pub exception_alert: Option<SceneTemplatePatch>,
}

/// Partial update of [`NotificationCenterSetting`]; an empty Feishu secret keeps the stored one.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NotificationCenterPatch {
    pub default_locale: Option<String>,
    pub channels: Option<ChannelsPatch>,
    pub scenes: Option<ScenesPatch>,
    pub templates: Option<TemplatesPatch>,
    pub dedupe_ttl_seconds: Option<i64>,
    pub inventory_alert_interval_seconds: Option<i64>,
    pub payment_order_alert_interval_seconds: Option<i64>,
    pub payment_order_alert_check_interval_seconds: Option<i64>,
    pub ignored_product_ids: Option<Vec<i64>>,
}

fn apply_scene_patch(target: &mut SceneTemplate, patch: SceneTemplatePatch) {
    for (t, p) in [
        (&mut target.zh_cn, patch.zh_cn),
        (&mut target.zh_tw, patch.zh_tw),
        (&mut target.en_us, patch.en_us),
    ] {
        if let Some(p) = p {
            if let Some(v) = p.title {
                t.title = v.trim().to_owned();
            }
            if let Some(v) = p.body {
                t.body = v.trim().to_owned();
            }
        }
    }
}

fn decode_scene(raw: &Obj, fallback: &mut SceneTemplate) {
    for (locale, t) in LOCALES.iter().zip(fallback.locales_mut()) {
        if let Some(m) = as_obj(raw.get(*locale)) {
            t.title = read_string(m, "title", &t.title);
            t.body = read_string(m, "body", &t.body);
        }
    }
}

impl NotificationCenterSetting {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.default_locale = normalize_locale(&self.default_locale);
        self.channels.email.recipients = dedupe_list(&self.channels.email.recipients, true);
        self.channels.telegram.recipients = dedupe_list(&self.channels.telegram.recipients, false)
            .into_iter()
            .filter(|r| is_telegram_chat_id(r))
            .collect();
        let f = &mut self.channels.feishu;
        f.app_id = f.app_id.trim().to_owned();
        f.app_secret = f.app_secret.trim().to_owned();
        f.receive_id_type = f.receive_id_type.trim().to_lowercase();
        if f.receive_id_type.is_empty() {
            f.receive_id_type = "chat_id".into();
        }
        f.recipients = dedupe_list(&f.recipients, f.receive_id_type == "email");
        self.dedupe_ttl_seconds = in_range_or(self.dedupe_ttl_seconds, 30, 86400, DEDUPE_DEFAULT);
        self.inventory_alert_interval_seconds = in_range_or(
            self.inventory_alert_interval_seconds,
            INTERVAL_MIN,
            INTERVAL_MAX,
            INVENTORY_INTERVAL_DEFAULT,
        );
        self.payment_order_alert_interval_seconds = in_range_or(
            self.payment_order_alert_interval_seconds,
            INTERVAL_MIN,
            INTERVAL_MAX,
            PAYMENT_ALERT_INTERVAL_DEFAULT,
        );
        self.payment_order_alert_check_interval_seconds = in_range_or(
            self.payment_order_alert_check_interval_seconds,
            INTERVAL_MIN,
            INTERVAL_MAX,
            PAYMENT_ALERT_CHECK_DEFAULT,
        );
        let mut ids: Vec<i64> = Vec::new();
        for id in &self.ignored_product_ids {
            if *id > 0 && !ids.contains(id) {
                ids.push(*id);
            }
        }
        self.ignored_product_ids = ids;
        for (_, scene) in self.templates.scenes_mut() {
            for t in scene.locales_mut() {
                t.title = t.title.trim().to_owned();
                t.body = t.body.trim().to_owned();
            }
        }
        self
    }

    pub fn validate(&self) -> crate::Result<()> {
        let n = self.clone().normalized();
        let invalid = |d: &str| Error::bad_request(format!("{INVALID}: {d}"));
        let c = &n.channels;
        if c.email.enabled && c.email.recipients.is_empty() {
            return Err(invalid("邮件渠道已启用但未配置收件邮箱"));
        }
        if c.telegram.enabled && c.telegram.recipients.is_empty() {
            return Err(invalid("Telegram 渠道已启用但未配置接收人ID"));
        }
        if c.feishu.enabled && c.feishu.recipients.is_empty() {
            return Err(invalid("飞书渠道已启用但未配置接收人ID"));
        }
        if c.email.enabled && !c.email.recipients.iter().all(|r| is_email_address(r)) {
            return Err(invalid("邮件收件人格式不合法"));
        }
        if c.feishu.enabled {
            if c.feishu.app_id.is_empty() {
                return Err(invalid("飞书 App ID 不能为空"));
            }
            if c.feishu.app_secret.is_empty() {
                return Err(invalid("飞书 App Secret 不能为空"));
            }
            if !FEISHU_ID_TYPES.contains(&c.feishu.receive_id_type.as_str()) {
                return Err(invalid("飞书接收人ID类型不合法"));
            }
            if c.feishu.receive_id_type == "email"
                && !c.feishu.recipients.iter().all(|r| is_email_address(r))
            {
                return Err(invalid("飞书接收邮箱格式不合法"));
            }
        }
        Ok(())
    }

    pub fn decode(raw: Option<&Value>, fallback: Self) -> Self {
        let mut next = fallback;
        let Some(o) = raw.and_then(Value::as_object) else {
            return next;
        };
        let legacy_enabled = read_bool(o, "enabled", true);
        next.default_locale = read_string(o, "default_locale", &next.default_locale);
        next.dedupe_ttl_seconds = read_int(o, "dedupe_ttl_seconds", next.dedupe_ttl_seconds);
        next.inventory_alert_interval_seconds = read_int(
            o,
            "inventory_alert_interval_seconds",
            next.inventory_alert_interval_seconds,
        );
        let legacy_payment = read_int(
            o,
            "payment_failed_alert_interval_seconds",
            next.payment_order_alert_interval_seconds,
        );
        next.payment_order_alert_interval_seconds =
            read_int(o, "payment_order_alert_interval_seconds", legacy_payment);
        next.payment_order_alert_check_interval_seconds = read_int(
            o,
            "payment_order_alert_check_interval_seconds",
            next.payment_order_alert_check_interval_seconds,
        );
        next.ignored_product_ids =
            read_id_list(o, "ignored_product_ids", &next.ignored_product_ids);
        if let Some(ch) = as_obj(o.get("channels")) {
            for (key, c) in [
                ("email", &mut next.channels.email),
                ("telegram", &mut next.channels.telegram),
            ] {
                if let Some(m) = as_obj(ch.get(key)) {
                    c.enabled = read_bool(m, "enabled", c.enabled);
                    c.recipients = read_string_list(m, "recipients", &c.recipients);
                }
            }
            if let Some(m) = as_obj(ch.get("feishu")) {
                let f = &mut next.channels.feishu;
                f.enabled = read_bool(m, "enabled", f.enabled);
                f.app_id = read_string(m, "app_id", &f.app_id);
                f.app_secret = read_string(m, "app_secret", &f.app_secret);
                f.receive_id_type = read_string(m, "receive_id_type", &f.receive_id_type);
                f.recipients = read_string_list(m, "recipients", &f.recipients);
            }
        }
        if let Some(m) = as_obj(o.get("scenes")) {
            let s = &mut next.scenes;
            s.wallet_recharge_success =
                read_bool(m, "wallet_recharge_success", s.wallet_recharge_success);
            s.order_paid_success = read_bool(m, "order_paid_success", s.order_paid_success);
            s.manual_fulfillment_pending = read_bool(
                m,
                "manual_fulfillment_pending",
                s.manual_fulfillment_pending,
            );
            s.exception_alert = read_bool(m, "exception_alert", s.exception_alert);
        }
        if let Some(m) = as_obj(o.get("templates")) {
            for (key, scene) in next.templates.scenes_mut() {
                if let Some(sm) = as_obj(m.get(key)) {
                    decode_scene(sm, scene);
                }
            }
        }
        if !legacy_enabled {
            next.channels.email.enabled = false;
            next.channels.telegram.enabled = false;
            next.channels.feishu.enabled = false;
        }
        next
    }

    pub fn encode(&self) -> Value {
        json!(self.clone().normalized())
    }

    /// Admin view without the Feishu app secret.
    pub fn masked(&self) -> Value {
        let n = self.clone().normalized();
        let mut v = json!(n);
        v["channels"]["feishu"]["app_secret"] = json!("");
        v["channels"]["feishu"]["has_app_secret"] = json!(!n.channels.feishu.app_secret.is_empty());
        v
    }

    pub fn apply_patch(&self, patch: NotificationCenterPatch) -> crate::Result<Self> {
        let mut next = self.clone();
        if let Some(v) = patch.default_locale {
            next.default_locale = v.trim().to_owned();
        }
        if let Some(v) = patch.dedupe_ttl_seconds {
            next.dedupe_ttl_seconds = v;
        }
        if let Some(v) = patch.inventory_alert_interval_seconds {
            next.inventory_alert_interval_seconds = v;
        }
        if let Some(v) = patch.payment_order_alert_interval_seconds {
            next.payment_order_alert_interval_seconds = v;
        }
        if let Some(v) = patch.payment_order_alert_check_interval_seconds {
            next.payment_order_alert_check_interval_seconds = v;
        }
        if let Some(v) = patch.ignored_product_ids {
            next.ignored_product_ids = v;
        }
        if let Some(ch) = patch.channels {
            for (target, p) in [
                (&mut next.channels.email, ch.email),
                (&mut next.channels.telegram, ch.telegram),
            ] {
                if let Some(p) = p {
                    if let Some(v) = p.enabled {
                        target.enabled = v;
                    }
                    if let Some(v) = p.recipients {
                        target.recipients = v;
                    }
                }
            }
            if let Some(p) = ch.feishu {
                let f = &mut next.channels.feishu;
                if let Some(v) = p.enabled {
                    f.enabled = v;
                }
                if let Some(v) = p.app_id {
                    f.app_id = v.trim().to_owned();
                }
                if let Some(v) = p.app_secret.filter(|v| !v.trim().is_empty()) {
                    f.app_secret = v.trim().to_owned();
                }
                if let Some(v) = p.receive_id_type {
                    f.receive_id_type = v.trim().to_lowercase();
                }
                if let Some(v) = p.recipients {
                    f.recipients = v;
                }
            }
        }
        if let Some(s) = patch.scenes {
            let sc = &mut next.scenes;
            for (t, p) in [
                (&mut sc.wallet_recharge_success, s.wallet_recharge_success),
                (&mut sc.order_paid_success, s.order_paid_success),
                (
                    &mut sc.manual_fulfillment_pending,
                    s.manual_fulfillment_pending,
                ),
                (&mut sc.exception_alert, s.exception_alert),
            ] {
                if let Some(v) = p {
                    *t = v;
                }
            }
        }
        if let Some(t) = patch.templates {
            let tp = &mut next.templates;
            for (target, p) in [
                (&mut tp.wallet_recharge_success, t.wallet_recharge_success),
                (&mut tp.order_paid_success, t.order_paid_success),
                (
                    &mut tp.manual_fulfillment_pending,
                    t.manual_fulfillment_pending,
                ),
                (&mut tp.exception_alert, t.exception_alert),
            ] {
                if let Some(p) = p {
                    apply_scene_patch(target, p);
                }
            }
        }
        let next = next.normalized();
        next.validate()?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_validation() {
        let d = NotificationCenterSetting::default();
        assert_eq!(d.channels.feishu.receive_id_type, "chat_id");
        assert!(d.validate().is_ok());
        let err = d
            .apply_patch(NotificationCenterPatch {
                channels: Some(ChannelsPatch {
                    email: Some(ChannelPatch {
                        enabled: Some(true),
                        recipients: Some(vec![]),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap_err();
        assert_eq!(
            err.key(),
            "notification config invalid: 邮件渠道已启用但未配置收件邮箱"
        );
    }

    #[test]
    fn feishu_secret_kept_and_masked() {
        let s = NotificationCenterSetting::default()
            .apply_patch(NotificationCenterPatch {
                channels: Some(ChannelsPatch {
                    feishu: Some(FeishuPatch {
                        enabled: Some(true),
                        app_id: Some("cli".into()),
                        app_secret: Some("sec".into()),
                        receive_id_type: Some(" EMAIL ".into()),
                        recipients: Some(vec!["A@x.com".into(), "a@x.com".into()]),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.channels.feishu.recipients, vec!["a@x.com"]);
        let kept = s
            .apply_patch(NotificationCenterPatch {
                channels: Some(ChannelsPatch {
                    feishu: Some(FeishuPatch {
                        app_secret: Some(String::new()),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(kept.channels.feishu.app_secret, "sec");
        let m = kept.masked();
        assert_eq!(m["channels"]["feishu"]["app_secret"], "");
        assert_eq!(m["channels"]["feishu"]["has_app_secret"], true);
    }

    #[test]
    fn decode_legacy_fields() {
        let s = NotificationCenterSetting::decode(
            Some(&json!({
                "enabled": false,
                "payment_failed_alert_interval_seconds": 120,
                "channels": {"telegram": {"enabled": true, "recipients": ["123456", "12", 5]}},
                "ignored_product_ids": [3, 0, 3],
            })),
            NotificationCenterSetting::default(),
        )
        .normalized();
        assert!(!s.channels.telegram.enabled);
        assert_eq!(s.channels.telegram.recipients, vec!["123456"]);
        assert_eq!(s.payment_order_alert_interval_seconds, 120);
        assert_eq!(s.ignored_product_ids, vec![3]);
    }
}
