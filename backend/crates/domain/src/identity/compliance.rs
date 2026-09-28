//! Compliance acknowledgement gate for payment/finance features.

use serde::Serialize;
use serde_json::Value;

use crate::{Error, Id};

/// Settings key holding the acknowledgement (original `complianceSettingKey`).
pub const SETTING_KEY: &str = "compliance.acknowledgement.v1";
/// Version of the acknowledged text.
pub const VERSION: &str = "v1";

const SEGMENT_1: &str = "我已阅读并理解上述合规声明提醒";
const SEGMENT_2: &str = "知悉相关法律风险";
const SEGMENT_3: &str = "并确认自行承担部署运营和收费行为产生的法律责任";
/// Segment 3 is split here to re-insert the enumeration comma of the full text.
const SEGMENT_3_SPLIT: &str = "并确认自行承担部署";
/// The full acknowledged sentence.
pub const FULL_TEXT: &str = "我已阅读并理解上述合规声明提醒，知悉相关法律风险，并确认自行承担部署、运营和收费行为产生的法律责任";

/// Response message of gated routes when a super admin has not acknowledged yet.
pub const MSG_REQUIRED: &str = "compliance_required";
/// Response message of gated routes for other administrators.
pub const MSG_REQUIRED_BY_SUPER_ADMIN: &str = "compliance_required_by_super_admin";
/// Only super administrators may acknowledge.
pub const KEY_SUPER_ADMIN_REQUIRED: &str = "compliance.error.super_admin_required";
/// The typed segments do not match.
pub const KEY_TEXT_MISMATCH: &str = "compliance.error.text_mismatch";

/// Checks the three typed segments and the reconstructed full sentence.
pub fn validate_segments(s1: &str, s2: &str, s3: &str) -> Result<(), Error> {
    if s1 != SEGMENT_1 || s2 != SEGMENT_2 || s3 != SEGMENT_3 {
        return Err(Error::bad_request(KEY_TEXT_MISMATCH));
    }
    let Some(rest) = s3.strip_prefix(SEGMENT_3_SPLIT) else {
        return Err(Error::bad_request(KEY_TEXT_MISMATCH));
    };
    let full = format!("{s1}，{s2}，{SEGMENT_3_SPLIT}、{rest}");
    if full != FULL_TEXT {
        return Err(Error::bad_request(KEY_TEXT_MISMATCH));
    }
    Ok(())
}

/// `/admin/compliance/status` response.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ComplianceStatus {
    pub acknowledged: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub acknowledged_at: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub acknowledged_by_admin_id: Id,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub acknowledged_by_username: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub version: String,
}

fn is_zero(v: &Id) -> bool {
    *v == 0
}

impl ComplianceStatus {
    /// Reads the stored setting value.
    pub fn from_value(value: Option<&Value>) -> Self {
        let Some(v) = value else {
            return Self::default();
        };
        let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        Self {
            acknowledged: v
                .get("acknowledged")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            acknowledged_at: text("acknowledged_at"),
            acknowledged_by_admin_id: v
                .get("acknowledged_by_admin_id")
                .and_then(Value::as_f64)
                .map_or(0, |f| f as Id),
            acknowledged_by_username: text("acknowledged_by_username"),
            version: text("version"),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn exact_segments_only() {
        assert!(validate_segments(SEGMENT_1, SEGMENT_2, SEGMENT_3).is_ok());
        // ADM-05: one character missing is rejected.
        let short = &SEGMENT_3[..SEGMENT_3.len() - "任".len()];
        assert_eq!(
            validate_segments(SEGMENT_1, SEGMENT_2, short)
                .unwrap_err()
                .key(),
            KEY_TEXT_MISMATCH
        );
        assert!(validate_segments(SEGMENT_2, SEGMENT_1, SEGMENT_3).is_err());
    }

    #[test]
    fn status_from_value() {
        let s = ComplianceStatus::from_value(Some(&json!({
            "acknowledged": true,
            "acknowledged_at": "2026-01-01T00:00:00Z",
            "acknowledged_by_admin_id": 1,
            "acknowledged_by_username": "admin",
            "version": "v1"
        })));
        assert!(s.acknowledged);
        assert_eq!(s.acknowledged_by_admin_id, 1);
        assert_eq!(
            serde_json::to_value(ComplianceStatus::default()).unwrap(),
            json!({"acknowledged": false})
        );
    }
}
