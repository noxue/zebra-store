//! Crypto wallet details shown next to QR payments (`presenter.ExtractCryptoWalletInfo`).

use serde_json::{Map, Value};

use super::types::provider;

/// On-chain payment details extracted from the creation payload.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CryptoWalletInfo {
    pub address: String,
    pub chain_amount: String,
    pub chain: String,
    pub token_id: String,
}

fn read(payload: &Map<String, Value>, path: &[&str]) -> String {
    let mut current: Option<&Value> = None;
    for (i, key) in path.iter().enumerate() {
        let next = if i == 0 {
            payload.get(*key)
        } else {
            current
                .and_then(|v| v.as_object())
                .and_then(|m| m.get(*key))
        };
        match next {
            Some(v) => current = Some(v),
            None => return String::new(),
        }
    }
    match current {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => {
                let f = n.as_f64().unwrap_or_default();
                let s = format!("{f:.8}");
                s.trim_end_matches('0').trim_end_matches('.').to_owned()
            }
        },
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn first(values: [String; 2]) -> String {
    values
        .into_iter()
        .find(|v| !v.is_empty())
        .unwrap_or_default()
}

/// Only QR payments of BEpusdt, epusdt and DujiaoPay expose wallet details.
pub fn extract_crypto_wallet_info(
    provider_type: &str,
    interaction_mode: &str,
    payload: &Map<String, Value>,
) -> CryptoWalletInfo {
    if !interaction_mode.trim().eq_ignore_ascii_case("qr") {
        return CryptoWalletInfo::default();
    }
    match provider_type.trim().to_ascii_lowercase().as_str() {
        provider::BEPUSDT => CryptoWalletInfo {
            address: read(payload, &["data", "token"]),
            chain_amount: read(payload, &["data", "actual_amount"]),
            chain: read(payload, &["data", "chain"]),
            token_id: read(payload, &["data", "token_id"]),
        },
        provider::EPUSDT => CryptoWalletInfo {
            address: read(payload, &["data", "receive_address"]),
            chain_amount: read(payload, &["data", "actual_amount"]),
            ..CryptoWalletInfo::default()
        },
        provider::DUJIAOPAY => CryptoWalletInfo {
            address: first([
                read(payload, &["pay_address"]),
                read(payload, &["data", "pay_address"]),
            ]),
            chain_amount: first([
                read(payload, &["payable_amount"]),
                read(payload, &["data", "payable_amount"]),
            ]),
            chain: first([read(payload, &["chain"]), read(payload, &["data", "chain"])]),
            token_id: first([
                read(payload, &["token_id"]),
                read(payload, &["data", "token_id"]),
            ]),
        },
        _ => CryptoWalletInfo::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_bepusdt_and_epusdt_details() {
        let payload = json!({"data": {"token": "TAddr", "actual_amount": 1.4600000, "chain": "tron", "token_id": "tron-usdt"}});
        let payload = payload.as_object().cloned().unwrap_or_default();
        let info = extract_crypto_wallet_info("bepusdt", "qr", &payload);
        assert_eq!(info.address, "TAddr");
        assert_eq!(info.chain_amount, "1.46");
        assert_eq!(info.token_id, "tron-usdt");
        assert_eq!(
            extract_crypto_wallet_info("bepusdt", "redirect", &payload),
            CryptoWalletInfo::default()
        );
        let djp = json!({"data": {"pay_address": "0xabc"}, "payable_amount": "9.90"});
        let djp = djp.as_object().cloned().unwrap_or_default();
        let info = extract_crypto_wallet_info("dujiaopay", "QR", &djp);
        assert_eq!(info.address, "0xabc");
        assert_eq!(info.chain_amount, "9.90");
    }
}
