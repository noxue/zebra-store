//! `order_email_template_config` (port of `schema/messaging/order_email_template.go`).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::value::{Obj, as_obj, read_string};
use crate::Error;

const INVALID: &str = "order email template config invalid";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EmailTemplate {
    pub subject: String,
    pub body: String,
}

/// One scene in the three supported locales.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SceneTemplates {
    #[serde(rename = "zh-CN")]
    pub zh_cn: EmailTemplate,
    #[serde(rename = "zh-TW")]
    pub zh_tw: EmailTemplate,
    #[serde(rename = "en-US")]
    pub en_us: EmailTemplate,
}

impl SceneTemplates {
    fn locales_mut(&mut self) -> [(&'static str, &mut EmailTemplate); 3] {
        [
            ("zh-CN", &mut self.zh_cn),
            ("zh-TW", &mut self.zh_tw),
            ("en-US", &mut self.en_us),
        ]
    }

    /// Template for `locale` (unknown locales use zh-CN).
    pub fn resolve(&self, locale: &str) -> &EmailTemplate {
        match locale {
            "zh-TW" => &self.zh_tw,
            "en-US" => &self.en_us,
            _ => &self.zh_cn,
        }
    }
}

/// Localized plain text (`{zh-CN, zh-TW, en-US}`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Tip {
    #[serde(rename = "zh-CN")]
    pub zh_cn: String,
    #[serde(rename = "zh-TW")]
    pub zh_tw: String,
    #[serde(rename = "en-US")]
    pub en_us: String,
}

impl Tip {
    fn locales_mut(&mut self) -> [(&'static str, &mut String); 3] {
        [
            ("zh-CN", &mut self.zh_cn),
            ("zh-TW", &mut self.zh_tw),
            ("en-US", &mut self.en_us),
        ]
    }

    pub fn resolve(&self, locale: &str) -> &str {
        match locale {
            "zh-TW" => &self.zh_tw,
            "en-US" => &self.en_us,
            _ => &self.zh_cn,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OrderEmailScenes {
    pub default: SceneTemplates,
    pub paid: SceneTemplates,
    pub delivered: SceneTemplates,
    pub delivered_with_content: SceneTemplates,
    pub refunded: SceneTemplates,
    pub partially_refunded: SceneTemplates,
}

impl OrderEmailScenes {
    fn all_mut(&mut self) -> [(&'static str, &mut SceneTemplates); 6] {
        [
            ("default", &mut self.default),
            ("paid", &mut self.paid),
            ("delivered", &mut self.delivered),
            ("delivered_with_content", &mut self.delivered_with_content),
            ("refunded", &mut self.refunded),
            ("partially_refunded", &mut self.partially_refunded),
        ]
    }
}

/// Order e-mail templates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OrderEmailTemplateSetting {
    pub templates: OrderEmailScenes,
    pub guest_tip: Tip,
    pub fulfillment_attachment_tip: Tip,
}

fn t(subject: &str, body: &str) -> EmailTemplate {
    EmailTemplate {
        subject: subject.into(),
        body: body.into(),
    }
}

fn scene(zh_cn: &str, zh_tw: &str, en_us: &str) -> SceneTemplates {
    SceneTemplates {
        zh_cn: t("订单状态更新：{{status}}", zh_cn),
        zh_tw: t("訂單狀態更新：{{status}}", zh_tw),
        en_us: t("Order status updated: {{status}}", en_us),
    }
}

impl OrderEmailTemplateSetting {
    /// The original default templates (migrated from the old i18n strings).
    pub fn defaults() -> Self {
        Self {
            templates: OrderEmailScenes {
                default: scene(
                    "订单号：{{order_no}}\n状态：{{status}}\n金额：{{amount}} {{currency}}\n\n感谢您的购买。\n\n{{site_name}} 的网址：{{site_url}}",
                    "訂單號：{{order_no}}\n狀態：{{status}}\n金額：{{amount}} {{currency}}\n\n感謝您的購買。\n\n{{site_name}} 的網址：{{site_url}}",
                    "Order No: {{order_no}}\nStatus: {{status}}\nAmount: {{amount}} {{currency}}\n\nThank you for your purchase.\n\n{{site_name}}'s Site URL: {{site_url}}",
                ),
                paid: scene(
                    "订单号：{{order_no}}\n状态：{{status}}\n金额：{{amount}} {{currency}}\n\n我们已收到您的付款，将尽快完成交付。\n\n{{site_name}} 的网址：{{site_url}}",
                    "訂單號：{{order_no}}\n狀態：{{status}}\n金額：{{amount}} {{currency}}\n\n已收到付款，將盡快完成交付。\n\n{{site_name}} 的網址：{{site_url}}",
                    "Order No: {{order_no}}\nStatus: {{status}}\nAmount: {{amount}} {{currency}}\n\nWe have received your payment and will deliver soon.\n\n{{site_name}}'s Site URL: {{site_url}}",
                ),
                delivered: scene(
                    "订单号：{{order_no}}\n状态：{{status}}\n金额：{{amount}} {{currency}}\n\n交付已完成，感谢您的购买。\n\n{{site_name}} 的网址：{{site_url}}",
                    "訂單號：{{order_no}}\n狀態：{{status}}\n金額：{{amount}} {{currency}}\n\n交付已完成，感謝您的購買。\n\n{{site_name}} 的網址：{{site_url}}",
                    "Order No: {{order_no}}\nStatus: {{status}}\nAmount: {{amount}} {{currency}}\n\nDelivery completed. Thank you for your purchase.\n\n{{site_name}}'s Site URL: {{site_url}}",
                ),
                delivered_with_content: scene(
                    "订单号：{{order_no}}\n状态：{{status}}\n金额：{{amount}} {{currency}}\n\n交付内容：\n{{fulfillment_info}}\n\n使用说明：\n{{instructions}}\n\n感谢您的购买。\n\n{{site_name}} 的网址：{{site_url}}",
                    "訂單號：{{order_no}}\n狀態：{{status}}\n金額：{{amount}} {{currency}}\n\n交付內容：\n{{fulfillment_info}}\n\n使用說明：\n{{instructions}}\n\n感謝您的購買。\n\n{{site_name}} 的網址：{{site_url}}",
                    "Order No: {{order_no}}\nStatus: {{status}}\nAmount: {{amount}} {{currency}}\n\nDelivery content:\n{{fulfillment_info}}\n\nUsage instructions:\n{{instructions}}\n\nThank you for your purchase.\n\n{{site_name}}'s Site URL: {{site_url}}",
                ),
                refunded: scene(
                    "订单号：{{order_no}}\n状态：{{status}}\n退款金额：{{refund_amount}} {{currency}}\n退款原因：{{refund_reason}}\n\n订单已退款，如有疑问请联系管理员。\n\n{{site_name}} 的网址：{{site_url}}",
                    "訂單號：{{order_no}}\n狀態：{{status}}\n退款金額：{{refund_amount}} {{currency}}\n退款原因：{{refund_reason}}\n\n訂單已退款，如有疑問請聯絡管理員。\n\n{{site_name}} 的網址：{{site_url}}",
                    "Order No: {{order_no}}\nStatus: {{status}}\nRefund Amount: {{refund_amount}} {{currency}}\nReason for refund: {{refund_reason}}\n\nThe order has been refunded. Please contact admin if needed.\n\n{{site_name}}'s Site URL: {{site_url}}",
                ),
                partially_refunded: scene(
                    "订单号：{{order_no}}\n状态：{{status}}\n退款金额：{{refund_amount}} {{currency}}\n退款原因：{{refund_reason}}\n\n订单已部分退款，如有疑问请联系管理员。\n\n{{site_name}} 的网址：{{site_url}}",
                    "訂單號：{{order_no}}\n狀態：{{status}}\n退款金額：{{refund_amount}} {{currency}}\n退款原因：{{refund_reason}}\n\n訂單已部分退款，如有疑問請聯絡管理員。\n\n{{site_name}} 的網址：{{site_url}}",
                    "Order No: {{order_no}}\nStatus: {{status}}\nRefund Amount: {{refund_amount}} {{currency}}\nReason for refund: {{refund_reason}}\n\nThe order has been partially refunded. Please contact admin if needed.\n\n{{site_name}}'s Site URL: {{site_url}}",
                ),
            },
            guest_tip: Tip {
                zh_cn: "游客订单可使用下单邮箱与订单密码在网站查询订单详情。".into(),
                zh_tw: "遊客訂單可使用下單信箱與訂單密碼在網站查詢訂單詳情。".into(),
                en_us: "Guest orders can be queried on the site using the checkout email and order password.".into(),
            },
            fulfillment_attachment_tip: Tip {
                zh_cn: "交付内容较多，已作为附件发送，请查看邮件附件获取完整交付内容。".into(),
                zh_tw: "交付內容較多，已作為附件發送，請查看郵件附件獲取完整交付內容。".into(),
                en_us: "The delivery content is included as an attachment. Please check the email attachment for the full content.".into(),
            },
        }
    }

    #[must_use]
    pub fn normalized(mut self) -> Self {
        for (_, scene) in self.templates.all_mut() {
            for (_, tpl) in scene.locales_mut() {
                tpl.subject = tpl.subject.trim().to_owned();
                tpl.body = tpl.body.trim().to_owned();
            }
        }
        for tip in [&mut self.guest_tip, &mut self.fulfillment_attachment_tip] {
            for (_, s) in tip.locales_mut() {
                *s = s.trim().to_owned();
            }
        }
        self
    }

    /// Every scene/locale needs a subject and a body.
    pub fn validate(&self) -> crate::Result<()> {
        let mut copy = self.clone();
        let ok = copy.templates.all_mut().into_iter().all(|(_, scene)| {
            scene
                .locales_mut()
                .into_iter()
                .all(|(_, t)| !t.subject.is_empty() && !t.body.is_empty())
        });
        if ok {
            Ok(())
        } else {
            Err(Error::bad_request(INVALID))
        }
    }

    pub fn decode(raw: Option<&Value>, fallback: Self) -> Self {
        let mut next = fallback;
        let Some(o) = raw.and_then(Value::as_object) else {
            return next;
        };
        if let Some(tm) = as_obj(o.get("templates")) {
            for (key, scene) in next.templates.all_mut() {
                if let Some(sm) = as_obj(tm.get(key)) {
                    for (locale, tpl) in scene.locales_mut() {
                        if let Some(lm) = as_obj(sm.get(locale)) {
                            tpl.subject = read_string(lm, "subject", &tpl.subject);
                            tpl.body = read_string(lm, "body", &tpl.body);
                        }
                    }
                }
            }
        }
        for (key, tip) in [
            ("guest_tip", &mut next.guest_tip),
            (
                "fulfillment_attachment_tip",
                &mut next.fulfillment_attachment_tip,
            ),
        ] {
            if let Some(m) = as_obj(o.get(key)) {
                decode_tip(m, tip);
            }
        }
        next
    }

    pub fn encode(&self) -> Value {
        json!(self.clone().normalized())
    }

    pub fn apply_patch(&self, patch: OrderEmailTemplatePatch) -> crate::Result<Self> {
        let mut next = self.clone();
        if let Some(tp) = patch.templates {
            let patches = [
                tp.default,
                tp.paid,
                tp.delivered,
                tp.delivered_with_content,
                tp.refunded,
                tp.partially_refunded,
            ];
            for ((_, scene), p) in next.templates.all_mut().into_iter().zip(patches) {
                let Some(p) = p else { continue };
                for ((_, tpl), lp) in scene
                    .locales_mut()
                    .into_iter()
                    .zip([p.zh_cn, p.zh_tw, p.en_us])
                {
                    if let Some(lp) = lp {
                        if let Some(s) = lp.subject {
                            tpl.subject = s.trim().to_owned();
                        }
                        if let Some(b) = lp.body {
                            tpl.body = b.trim().to_owned();
                        }
                    }
                }
            }
        }
        for (tip, p) in [
            (&mut next.guest_tip, patch.guest_tip),
            (
                &mut next.fulfillment_attachment_tip,
                patch.fulfillment_attachment_tip,
            ),
        ] {
            if let Some(p) = p {
                for ((_, s), v) in tip
                    .locales_mut()
                    .into_iter()
                    .zip([p.zh_cn, p.zh_tw, p.en_us])
                {
                    if let Some(v) = v {
                        *s = v.trim().to_owned();
                    }
                }
            }
        }
        let next = next.normalized();
        next.validate()?;
        Ok(next)
    }
}

fn decode_tip(m: &Obj, tip: &mut Tip) {
    for (locale, s) in tip.locales_mut() {
        *s = read_string(m, locale, s);
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EmailTemplatePatch {
    pub subject: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SceneTemplatesPatch {
    #[serde(rename = "zh-CN")]
    pub zh_cn: Option<EmailTemplatePatch>,
    #[serde(rename = "zh-TW")]
    pub zh_tw: Option<EmailTemplatePatch>,
    #[serde(rename = "en-US")]
    pub en_us: Option<EmailTemplatePatch>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OrderEmailScenesPatch {
    pub default: Option<SceneTemplatesPatch>,
    pub paid: Option<SceneTemplatesPatch>,
    pub delivered: Option<SceneTemplatesPatch>,
    pub delivered_with_content: Option<SceneTemplatesPatch>,
    pub refunded: Option<SceneTemplatesPatch>,
    pub partially_refunded: Option<SceneTemplatesPatch>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TipPatch {
    #[serde(rename = "zh-CN")]
    pub zh_cn: Option<String>,
    #[serde(rename = "zh-TW")]
    pub zh_tw: Option<String>,
    #[serde(rename = "en-US")]
    pub en_us: Option<String>,
}

/// Partial update of the order e-mail templates.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OrderEmailTemplatePatch {
    pub templates: Option<OrderEmailScenesPatch>,
    pub guest_tip: Option<TipPatch>,
    pub fulfillment_attachment_tip: Option<TipPatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_have_no_canceled_scene() {
        let d = OrderEmailTemplateSetting::defaults();
        assert!(d.validate().is_ok());
        let v = d.encode();
        assert!(v["templates"].get("canceled").is_none());
        assert_eq!(
            v["templates"]["paid"]["en-US"]["subject"],
            "Order status updated: {{status}}"
        );
    }

    #[test]
    fn empty_subject_is_rejected() {
        let err = OrderEmailTemplateSetting::defaults()
            .apply_patch(OrderEmailTemplatePatch {
                templates: Some(OrderEmailScenesPatch {
                    paid: Some(SceneTemplatesPatch {
                        zh_tw: Some(EmailTemplatePatch {
                            subject: Some("  ".into()),
                            body: None,
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap_err();
        assert_eq!(err.key(), INVALID);
    }

    #[test]
    fn patch_updates_single_locale() {
        let s = OrderEmailTemplateSetting::defaults()
            .apply_patch(OrderEmailTemplatePatch {
                guest_tip: Some(TipPatch {
                    en_us: Some(" Hi ".into()),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.guest_tip.en_us, "Hi");
        assert_eq!(
            s.guest_tip.zh_cn,
            OrderEmailTemplateSetting::defaults().guest_tip.zh_cn
        );
    }
}
