//! Conservative admin-only explanations from stored, verified gateway facts.
//! A closed trade alone says nothing about which party prevented payment.

use serde::Serialize;

use super::{GatewayError, Payment, PaymentStatus};

const MERCHANT_MESSAGES: &[&str] = &[
    "商户被封控",
    "商户封控",
    "商户被冻结",
    "商户已冻结",
    "商户被停用",
    "商户已停用",
    "商户状态异常",
    "商户无此交易权限",
    "商户未开通",
    "商户收款受限",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosisKind {
    Paid,
    MerchantRestricted,
    MerchantConfiguration,
    UserCanceled,
    ExpiredUnpaid,
    ClosedUnknown,
    FailedUnknown,
    AwaitingPayment,
    Superseded,
    GatewayError,
    Unknown,
}

impl DiagnosisKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paid => "paid",
            Self::MerchantRestricted => "merchant_restricted",
            Self::MerchantConfiguration => "merchant_configuration",
            Self::UserCanceled => "user_canceled",
            Self::ExpiredUnpaid => "expired_unpaid",
            Self::ClosedUnknown => "closed_unknown",
            Self::FailedUnknown => "failed_unknown",
            Self::AwaitingPayment => "awaiting_payment",
            Self::Superseded => "superseded",
            Self::GatewayError => "gateway_error",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PaymentDiagnosis {
    pub kind: DiagnosisKind,
    /// Only a bounded gateway error code, never the raw payload or provider message (PAY-45).
    pub gateway_code: String,
}

pub fn diagnose(payment: &Payment) -> PaymentDiagnosis {
    let payload = &payment.provider_payload;
    let text = |key: &str| {
        payload
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
    };
    let gateway_code = if payment.provider_type == "huifu" {
        ["bank_code", "resp_code"]
            .into_iter()
            .map(text)
            .find(|s| {
                !s.is_empty()
                    && s.len() <= 64
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
            .unwrap_or_default()
            .to_owned()
    } else {
        String::new()
    };
    let messages = ["bank_message", "bank_desc", "resp_desc", "sub_resp_desc"]
        .map(text)
        .join(" ");
    let huifu = payment.provider_type == "huifu";
    let kind = if payment.status == PaymentStatus::Success {
        DiagnosisKind::Paid
    } else if huifu && MERCHANT_MESSAGES.iter().any(|s| messages.contains(s)) {
        DiagnosisKind::MerchantRestricted
    } else if huifu
        && [
            "用户取消支付",
            "用户取消付款",
            "买家取消支付",
            "买家取消付款",
        ]
        .iter()
        .any(|s| messages.contains(s))
    {
        DiagnosisKind::UserCanceled
    } else if huifu
        && ["支付超时", "付款超时", "超时未支付", "超时未付款"]
            .iter()
            .any(|s| messages.contains(s))
    {
        DiagnosisKind::ExpiredUnpaid
    } else if huifu && (text("bank_code") == "TRADE_CLOSED" || messages.contains("交易已关闭"))
    {
        DiagnosisKind::ClosedUnknown
    } else if matches!(
        text("creation_failure_kind"),
        "merchant_restricted" | "merchant_configuration" | "gateway_error"
    ) {
        match text("creation_failure_kind") {
            "merchant_restricted" => DiagnosisKind::MerchantRestricted,
            "merchant_configuration" => DiagnosisKind::MerchantConfiguration,
            _ => DiagnosisKind::GatewayError,
        }
    } else if payment.superseded_at.is_some() {
        DiagnosisKind::Superseded
    } else {
        match payment.status {
            PaymentStatus::Expired => DiagnosisKind::ExpiredUnpaid,
            PaymentStatus::Failed => DiagnosisKind::FailedUnknown,
            PaymentStatus::Initiated | PaymentStatus::Pending => DiagnosisKind::AwaitingPayment,
            _ => DiagnosisKind::Unknown,
        }
    };
    PaymentDiagnosis { kind, gateway_code }
}

/// Only a safe category is retained from a gateway exception; no config, URL or message.
pub fn creation_failure(error: &GatewayError) -> DiagnosisKind {
    match error {
        GatewayError::ConfigInvalid(_)
        | GatewayError::ProviderPermissionMissing(_)
        | GatewayError::AuthFailed(_)
        | GatewayError::UnsupportedChannel(_)
        | GatewayError::ProviderNotFound => DiagnosisKind::MerchantConfiguration,
        GatewayError::ResponseInvalid(message)
            if MERCHANT_MESSAGES.iter().any(|s| message.contains(s)) =>
        {
            DiagnosisKind::MerchantRestricted
        }
        _ => DiagnosisKind::GatewayError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payment(status: PaymentStatus, payload: serde_json::Value) -> Payment {
        let mut p = super::super::model::tests::sample();
        p.provider_type = "huifu".into();
        p.status = status;
        p.provider_payload = payload.as_object().cloned().unwrap_or_default();
        p
    }

    #[test]
    fn closed_trade_does_not_identify_merchant_or_buyer() {
        // PAY-21/PAY-45: production DJ20261016153048637285; expose safe facts only.
        let p = payment(
            PaymentStatus::Failed,
            json!({"bank_code":"TRADE_CLOSED", "bank_message":"交易已关闭", "resp_desc":"交易失败，请重试", "secret":"hidden"}),
        );
        let d = diagnose(&p);
        assert_eq!(d.kind.as_str(), "closed_unknown");
        assert_eq!(d.gateway_code, "TRADE_CLOSED");
        let admin = serde_json::to_value(super::super::model::admin_payment(
            &p,
            "",
            &Default::default(),
        ))
        .unwrap_or_default();
        assert_eq!(admin["provider_payload"], json!({}));
        assert_eq!(admin["diagnosis"]["kind"], "closed_unknown");
        assert!(!admin.to_string().contains("hidden"));
    }

    #[test]
    fn distinguishes_explicit_reasons_and_unknown_failures() {
        for (message, expected) in [
            ("商户被封控，禁止交易", "merchant_restricted"),
            ("商户状态异常", "merchant_restricted"),
            ("用户取消支付", "user_canceled"),
            ("支付超时", "expired_unpaid"),
            ("风控拒绝", "failed_unknown"),
            ("交易失败，请重试", "failed_unknown"),
        ] {
            assert_eq!(
                diagnose(&payment(
                    PaymentStatus::Failed,
                    json!({"bank_message":message})
                ))
                .kind
                .as_str(),
                expected
            );
        }
        assert_eq!(
            diagnose(&payment(PaymentStatus::Pending, json!({})))
                .kind
                .as_str(),
            "awaiting_payment"
        );
        assert_eq!(
            diagnose(&payment(PaymentStatus::Expired, json!({})))
                .kind
                .as_str(),
            "expired_unpaid"
        );
        assert_eq!(
            diagnose(&payment(
                PaymentStatus::Success,
                json!({"bank_message":"商户被封控"})
            ))
            .kind
            .as_str(),
            "paid"
        );
    }

    #[test]
    fn never_exports_arbitrary_codes_or_applies_huifu_rules_to_other_providers() {
        let mut p = payment(
            PaymentStatus::Failed,
            json!({"bank_code":"https://secret.example/key", "resp_code":"90000000", "bank_message":"商户被封控"}),
        );
        assert_eq!(diagnose(&p).gateway_code, "90000000");
        p.provider_type = "epay".into();
        assert_eq!(diagnose(&p).kind.as_str(), "failed_unknown");
        assert!(diagnose(&p).gateway_code.is_empty());
    }
    #[test]
    fn creation_errors_retain_safe_category_without_claiming_nonpayment() {
        for (error, expected) in [
            (
                GatewayError::config("private config"),
                "merchant_configuration",
            ),
            (
                GatewayError::ProviderPermissionMissing("private message".into()),
                "merchant_configuration",
            ),
            (GatewayError::response("商户被封控"), "merchant_restricted"),
            (GatewayError::request("gateway timeout"), "gateway_error"),
            (
                GatewayError::signature("invalid signature"),
                "gateway_error",
            ),
        ] {
            assert_eq!(creation_failure(&error).as_str(), expected);
            assert_eq!(
                diagnose(&payment(
                    PaymentStatus::Failed,
                    json!({"creation_failure_kind": creation_failure(&error).as_str()})
                ))
                .kind
                .as_str(),
                expected
            );
        }
    }
}
