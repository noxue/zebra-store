//! Order status e-mails (`buildOrderStatusContentFromTemplate`, NTF-01, NTF-10, DLV-02,
//! DLV-09): scene selection, `{{variable}}` rendering, attachment decision and plain-text
//! delivery instructions.

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use zs_shared::money::Amount;

use super::model::{Order, OrderStatus, PAYLOAD_MAX_EMAIL_LINES};
use crate::Result;
use crate::settings::schema::order_email::{OrderEmailTemplateSetting, SceneTemplates};

/// A plain-text email, optionally with one text attachment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OrderMail {
    pub to: String,
    pub subject: String,
    pub body: String,
    /// `(file name, content)` of a `text/plain` attachment.
    pub attachment: Option<(String, String)>,
    /// Brand overrides (reseller white-label mails, NTF-02).
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
}

/// Sends order e-mails (SMTP with attachments).
#[async_trait]
pub trait OrderMailer: Send + Sync {
    async fn send(&self, mail: &OrderMail) -> Result<()>;
}

/// Localized status label (`order.status.*`).
pub fn status_label(status: OrderStatus, locale: &str) -> &'static str {
    use OrderStatus::*;
    let en = locale == "en-US";
    let tw = locale == "zh-TW";
    match status {
        PendingPayment => {
            if en {
                "Pending Payment"
            } else {
                "待支付"
            }
        }
        Paid => {
            if en {
                "Paid"
            } else {
                "已支付"
            }
        }
        Fulfilling => {
            if en {
                "Processing"
            } else if tw {
                "處理中"
            } else {
                "处理中"
            }
        }
        PartiallyDelivered => {
            if en {
                "Partially delivered"
            } else {
                "部分交付"
            }
        }
        Delivered => {
            if en {
                "Delivered"
            } else {
                "已交付"
            }
        }
        Completed => {
            if en {
                "Completed"
            } else {
                "已完成"
            }
        }
        Canceled => {
            if en {
                "Canceled"
            } else {
                "已取消"
            }
        }
        Refunded => {
            if en {
                "Refunded"
            } else {
                "已退款"
            }
        }
        PartiallyRefunded => {
            if en {
                "Partially refunded"
            } else {
                "部分退款"
            }
        }
    }
}

/// Supported mail locale (`zh-CN` default).
pub fn mail_locale(raw: &str) -> &'static str {
    match raw.trim() {
        "zh-TW" | "zh-tw" => "zh-TW",
        "en-US" | "en-us" | "en" => "en-US",
        _ => "zh-CN",
    }
}

static VAR: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"\{\{\s*([a-zA-Z0-9_]+)\s*\}\}").ok());

/// Replaces `{{name}}` with the (trimmed) variable; unknown names render empty.
pub fn render(template: &str, vars: &BTreeMap<&str, String>) -> String {
    let template = template.trim();
    let Some(re) = VAR.as_ref() else {
        return template.to_owned();
    };
    re.replace_all(template, |caps: &regex::Captures<'_>| {
        vars.get(caps.get(1).map_or("", |m| m.as_str()))
            .map(|v| v.trim().to_owned())
            .unwrap_or_default()
    })
    .into_owned()
}

/// Everything a status e-mail shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatusMailInput {
    pub order_no: String,
    pub status: Option<OrderStatus>,
    pub amount: Amount,
    pub refund_amount: Amount,
    pub refund_reason: String,
    pub currency: String,
    pub site_name: String,
    pub site_url: String,
    pub is_guest: bool,
    /// Delivered content (inline when short).
    pub fulfillment: String,
    pub instructions: String,
}

/// `[<child no>]\n<payload>` blocks of the delivered children (`buildOrderFulfillmentEmailPayload`).
pub fn fulfillment_text(order: &Order) -> String {
    if let Some(f) = order.fulfillment.as_ref()
        && !f.payload.trim().is_empty()
    {
        return f.payload.trim().to_owned();
    }
    order
        .children
        .iter()
        .filter_map(|c| {
            let payload = c.fulfillment.as_ref()?.payload.trim();
            (!payload.is_empty()).then(|| format!("[{}]\n{payload}", c.order_no.trim()))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Whether the delivered content goes into an attachment (more than 20 lines, NTF-10).
pub fn should_attach(payload: &str) -> bool {
    !payload.is_empty() && payload.split('\n').count() > PAYLOAD_MAX_EMAIL_LINES
}

/// Strips HTML to plain text: block tags become line breaks, entities are decoded and
/// blank lines collapsed (DLV-02).
pub fn html_to_text(html: &str) -> String {
    static BLOCK: LazyLock<Option<Regex>> = LazyLock::new(|| {
        Regex::new(r"(?i)<\s*(br|/p|/div|/li|/h[1-6]|/tr|/blockquote|/pre)\s*/?>").ok()
    });
    static TAG: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").ok());
    let mut text = html.to_owned();
    if let Some(re) = BLOCK.as_ref() {
        text = re.replace_all(&text, "\n").into_owned();
    }
    if let Some(re) = TAG.as_ref() {
        text = re.replace_all(&text, "").into_owned();
    }
    let text = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let mut out: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() && out.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        out.push(line);
    }
    out.join("\n").trim().to_owned()
}

/// Delivery instructions of the order's items for `locale`, de-duplicated plain text.
pub fn instructions_text(order: &Order, locale: &str) -> String {
    let mut seen = HashSet::new();
    let mut parts = Vec::new();
    for item in order.all_items() {
        let text = [locale, "zh-CN", "en-US", "zh-TW"]
            .iter()
            .filter_map(|k| item.instructions.get(*k))
            .filter_map(Value::as_str)
            .map(str::trim)
            .find(|s| !s.is_empty())
            .map(html_to_text)
            .unwrap_or_default();
        if !text.is_empty() && seen.insert(text.clone()) {
            parts.push(text);
        }
    }
    parts.join("\n\n")
}

/// Renders subject and body from the templates.
pub fn status_mail(
    input: &StatusMailInput,
    locale: &str,
    templates: &OrderEmailTemplateSetting,
    attached: bool,
) -> (String, String) {
    let locale = mail_locale(locale);
    let scenes = &templates.templates;
    let scene: &SceneTemplates = match input.status {
        Some(OrderStatus::Paid) => &scenes.paid,
        Some(OrderStatus::Delivered | OrderStatus::Completed) => {
            if input.fulfillment.trim().is_empty() {
                &scenes.delivered
            } else {
                &scenes.delivered_with_content
            }
        }
        Some(OrderStatus::Refunded) => &scenes.refunded,
        Some(OrderStatus::PartiallyRefunded) => &scenes.partially_refunded,
        _ => &scenes.default,
    };
    let template = scene.resolve(locale);
    let refund = matches!(
        input.status,
        Some(OrderStatus::Refunded | OrderStatus::PartiallyRefunded)
    );
    let label = input
        .status
        .map(|s| status_label(s, locale))
        .unwrap_or_default();
    let vars: BTreeMap<&str, String> = [
        ("order_no", input.order_no.clone()),
        ("status", label.to_owned()),
        ("amount", input.amount.to_string()),
        (
            "refund_amount",
            if refund {
                input.refund_amount.to_string()
            } else {
                String::new()
            },
        ),
        (
            "refund_reason",
            if refund {
                input.refund_reason.clone()
            } else {
                String::new()
            },
        ),
        ("currency", input.currency.trim().to_owned()),
        ("site_name", input.site_name.trim().to_owned()),
        ("site_url", input.site_url.trim().to_owned()),
        ("fulfillment_info", input.fulfillment.trim().to_owned()),
        ("instructions", input.instructions.trim().to_owned()),
    ]
    .into_iter()
    .collect();
    let subject = render(&template.subject, &vars);
    let mut body = render(&template.body, &vars);
    if !input.instructions.trim().is_empty() && !template.body.contains("{{instructions}}") {
        body = format!(
            "{}\n\n{}",
            body.trim_end_matches('\n'),
            input.instructions.trim()
        );
    }
    if attached {
        let tip = templates.fulfillment_attachment_tip.resolve(locale).trim();
        if !tip.is_empty() {
            body = format!("{body}\n\n{tip}");
        }
    }
    if input.is_guest {
        let tip = templates.guest_tip.resolve(locale).trim();
        if !tip.is_empty() {
            body = format!("{body}\n\n{tip}");
        }
    }
    (subject, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(status: OrderStatus) -> StatusMailInput {
        StatusMailInput {
            order_no: "DJ1".into(),
            status: Some(status),
            amount: Amount::from(10),
            currency: "CNY".into(),
            site_name: "Zebra".into(),
            site_url: "https://z".into(),
            ..StatusMailInput::default()
        }
    }

    /// DLV-09: completed mail carries the delivered content.
    #[test]
    fn dlv_09_completed_mail_has_content() {
        let mut i = input(OrderStatus::Completed);
        i.fulfillment = "AUTO-CODE-001".into();
        let (subject, body) =
            status_mail(&i, "zh-CN", &OrderEmailTemplateSetting::defaults(), false);
        assert!(subject.contains("已完成"));
        assert!(body.contains("交付内容"));
        assert!(body.contains("AUTO-CODE-001"));
    }

    /// NTF-10: over 20 lines goes to an attachment with a tip.
    #[test]
    fn ntf_10_attachment_threshold() {
        let twenty = (0..20)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let twenty_one = format!("{twenty}\n20");
        assert!(!should_attach(&twenty));
        assert!(should_attach(&twenty_one));
        let i = input(OrderStatus::Completed);
        let (_, body) = status_mail(&i, "en-US", &OrderEmailTemplateSetting::defaults(), true);
        assert!(body.contains("attachment"));
    }

    #[test]
    fn guest_tip_and_refund_vars() {
        let mut i = input(OrderStatus::PartiallyRefunded);
        i.is_guest = true;
        i.refund_amount = Amount::from(3);
        i.refund_reason = "oops".into();
        let (subject, body) =
            status_mail(&i, "zh-CN", &OrderEmailTemplateSetting::defaults(), false);
        assert_eq!(subject, "订单状态更新：部分退款");
        assert!(body.contains("退款金额：3.00 CNY"));
        assert!(body.contains("oops"));
        assert!(body.contains("游客订单"));
    }

    /// DLV-02: instructions are plain text.
    #[test]
    fn dlv_02_html_is_stripped() {
        assert_eq!(
            html_to_text("<p>Step 1</p><p>Step&nbsp;2<img src=x onerror=alert(1)></p>"),
            "Step 1\nStep 2"
        );
    }

    #[test]
    fn render_unknown_vars_empty() {
        let vars: BTreeMap<&str, String> = [("a", " x ".to_owned())].into_iter().collect();
        assert_eq!(render("{{ a }}-{{b}}", &vars), "x-");
    }
}
