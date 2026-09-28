//! Gift cards: generation, admin management, export and the redemption rules used by the wallet.

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::catalog::card_secret::{ExportFormat, write_csv};
use crate::{Error, Id, Result};

pub mod keys {
    pub const INVALID: &str = "error.gift_card_invalid";
    pub const NOT_FOUND: &str = "error.gift_card_not_found";
    pub const EXPIRED: &str = "error.gift_card_expired";
    pub const DISABLED: &str = "error.gift_card_disabled";
    pub const REDEEMED: &str = "error.gift_card_redeemed";
    pub const CREATE_FAILED: &str = "error.gift_card_create_failed";
    pub const FETCH_FAILED: &str = "error.gift_card_fetch_failed";
    pub const UPDATE_FAILED: &str = "error.gift_card_update_failed";
    pub const DELETE_FAILED: &str = "error.gift_card_delete_failed";
    pub const REDEEM_FAILED: &str = "error.gift_card_redeem_failed";
}

/// Largest batch accepted by `POST /admin/gift-cards/generate` (original limit).
pub const MAX_GENERATE_QUANTITY: i32 = 10_000;
/// Card code prefix (`GC` + yymmddHHMMSS + index + random hex).
pub const CODE_PREFIX: &str = "GC";
/// Batch number prefix (`GCB` + yyyymmddHHMMSS + random hex).
pub const BATCH_PREFIX: &str = "GCB";
/// Fallback site currency (`constants.SiteCurrencyDefault`).
pub const DEFAULT_CURRENCY: &str = "CNY";
/// Wallet transaction type of a redemption (`constants.WalletTxnTypeGiftCard`).
pub const WALLET_TXN_TYPE: &str = "gift_card_redeem";

/// Gift card status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiftCardStatus {
    Active,
    Redeemed,
    Disabled,
}

impl GiftCardStatus {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "active" => Some(Self::Active),
            "redeemed" => Some(Self::Redeemed),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Redeemed => "redeemed",
            Self::Disabled => "disabled",
        }
    }
}

/// A generation batch.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GiftCardBatch {
    pub id: Id,
    pub batch_no: String,
    pub name: String,
    pub amount: Amount,
    pub currency: String,
    pub quantity: i32,
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<Id>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A gift card (admin JSON shape, batch preloaded).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GiftCard {
    pub id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<Id>,
    pub name: String,
    pub code: String,
    pub amount: Amount,
    pub currency: String,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub redeemed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redeemed_user_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet_txn_id: Option<Id>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<GiftCardBatch>,
}

/// Validated generation request.
#[derive(Debug, Clone, PartialEq)]
pub struct NewGiftCards {
    pub batch_no: String,
    pub name: String,
    pub amount: Amount,
    pub currency: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: Option<Id>,
    pub codes: Vec<String>,
}

/// Validates a generation request (name, 1..=10000 cards, positive amount).
pub fn validate_generate(name: &str, quantity: i32, amount: Amount) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || quantity <= 0 || quantity > MAX_GENERATE_QUANTITY || !amount.is_positive()
    {
        return Err(Error::bad_request(keys::INVALID));
    }
    Ok(name.to_owned())
}

/// `GC` + `yymmddHHMMSS` + index (mod 10000, 4 digits) + random hex, upper-cased.
pub fn card_code(now: DateTime<Utc>, index: usize, random_hex: &str) -> String {
    format!(
        "{CODE_PREFIX}{}{:04}{}",
        now.format("%y%m%d%H%M%S"),
        index % 10_000,
        random_hex
    )
    .to_uppercase()
}

/// `GCB` + `yyyymmddHHMMSS` + random hex, upper-cased.
pub fn batch_no(now: DateTime<Utc>, random_hex: &str) -> String {
    format!("{BATCH_PREFIX}{}{}", now.format("%Y%m%d%H%M%S"), random_hex).to_uppercase()
}

/// Admin update.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GiftCardUpdate {
    pub name: Option<String>,
    pub status: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub clear_expires_at: bool,
}

/// Applies an admin update: redeemed cards cannot change status; expiry cannot be in the past.
pub fn apply_update(
    card: &mut GiftCard,
    update: &GiftCardUpdate,
    now: DateTime<Utc>,
) -> Result<()> {
    let invalid = || Error::bad_request(keys::INVALID);
    if let Some(name) = &update.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(invalid());
        }
        card.name = name.to_owned();
    }
    if let Some(status) = &update.status {
        match GiftCardStatus::parse(status) {
            Some(s @ (GiftCardStatus::Active | GiftCardStatus::Disabled)) => {
                if card.status == GiftCardStatus::Redeemed.as_str() {
                    return Err(invalid());
                }
                card.status = s.as_str().to_owned();
            }
            _ => return Err(invalid()),
        }
    }
    if update.clear_expires_at {
        card.expires_at = None;
    } else if let Some(at) = update.expires_at {
        if at < now {
            return Err(invalid());
        }
        card.expires_at = Some(at);
    }
    Ok(())
}

/// Bulk status target: only `active` / `disabled`.
pub fn parse_bulk_status(raw: &str) -> Result<GiftCardStatus> {
    match GiftCardStatus::parse(raw) {
        Some(s @ (GiftCardStatus::Active | GiftCardStatus::Disabled)) => Ok(s),
        _ => Err(Error::bad_request(keys::INVALID)),
    }
}

pub fn is_expired(card: &GiftCard, now: DateTime<Utc>) -> bool {
    card.expires_at.is_some_and(|e| e < now)
}

/// Redemption checks in the original order: status, expiry, positive amount.
pub fn check_redeemable(card: &GiftCard, now: DateTime<Utc>) -> Result<()> {
    match GiftCardStatus::parse(&card.status) {
        Some(GiftCardStatus::Redeemed) => return Err(Error::bad_request(keys::REDEEMED)),
        Some(GiftCardStatus::Disabled) => return Err(Error::bad_request(keys::DISABLED)),
        Some(GiftCardStatus::Active) => {}
        None => return Err(Error::bad_request(keys::INVALID)),
    }
    if is_expired(card, now) {
        return Err(Error::bad_request(keys::EXPIRED));
    }
    if !card.amount.is_positive() {
        return Err(Error::bad_request(keys::INVALID));
    }
    Ok(())
}

/// Normalises a code typed by a user (codes are stored upper-case).
pub fn normalize_code(raw: &str) -> String {
    raw.trim().to_uppercase()
}

/// Wallet remark of a redemption (original `礼品卡兑换：<code>`).
pub fn redeem_remark(code: &str) -> String {
    format!("礼品卡兑换：{code}")
}

/// Wallet reference of a redemption (`gift_card:<id>`).
pub fn redeem_reference(card_id: Id) -> String {
    format!("gift_card:{card_id}")
}

fn fmt_time(t: Option<DateTime<Utc>>) -> String {
    t.map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// Builds the export file.
pub fn build_export(cards: &[GiftCard], format: ExportFormat) -> String {
    match format {
        ExportFormat::Txt => cards
            .iter()
            .map(|c| c.code.trim())
            .collect::<Vec<_>>()
            .join("\n"),
        ExportFormat::Csv => {
            let mut rows = vec![
                [
                    "id",
                    "batch_no",
                    "name",
                    "code",
                    "amount",
                    "currency",
                    "status",
                    "redeemed_user_id",
                    "redeemed_at",
                    "expires_at",
                    "created_at",
                ]
                .map(str::to_owned)
                .to_vec(),
            ];
            for c in cards {
                rows.push(vec![
                    c.id.to_string(),
                    c.batch
                        .as_ref()
                        .map(|b| b.batch_no.clone())
                        .unwrap_or_default(),
                    c.name.clone(),
                    c.code.clone(),
                    c.amount.to_string(),
                    c.currency.clone(),
                    c.status.clone(),
                    c.redeemed_user_id
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                    fmt_time(c.redeemed_at),
                    fmt_time(c.expires_at),
                    fmt_time(Some(c.created_at)),
                ]);
            }
            write_csv(&rows)
        }
    }
}

/// Admin list filter (`status=expired` = active but past expiry).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GiftCardFilter {
    /// Upper-cased substring.
    pub code: String,
    pub status: String,
    /// Upper-cased substring.
    pub batch_no: String,
    pub redeemed_user_id: Id,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
    pub redeemed_from: Option<DateTime<Utc>>,
    pub redeemed_to: Option<DateTime<Utc>>,
    pub expires_from: Option<DateTime<Utc>>,
    pub expires_to: Option<DateTime<Utc>>,
}

/// Redeemer summary shown in the admin list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RedeemedUser {
    pub id: Id,
    pub email: String,
    pub display_name: String,
}

/// Persistence port for gift cards. Soft-deleted rows are never returned.
#[async_trait]
pub trait GiftCardRepo: Send + Sync {
    /// Inserts the batch and its cards in one transaction (chunked inserts).
    async fn create_batch(&self, cards: &NewGiftCards, now: DateTime<Utc>)
    -> Result<GiftCardBatch>;
    async fn get(&self, id: Id) -> Result<Option<GiftCard>>;
    async fn list(
        &self,
        filter: &GiftCardFilter,
        page: PageRequest,
        now: DateTime<Utc>,
    ) -> Result<Page<GiftCard>>;
    async fn list_by_ids(&self, ids: &[Id]) -> Result<Vec<GiftCard>>;
    async fn save(&self, card: &GiftCard) -> Result<()>;
    async fn delete(&self, id: Id, now: DateTime<Utc>) -> Result<()>;
    /// Updates status of non-redeemed cards; returns rows affected.
    async fn update_status(
        &self,
        ids: &[Id],
        status: GiftCardStatus,
        now: DateTime<Utc>,
    ) -> Result<u64>;
    async fn redeemed_users(&self, user_ids: &[Id]) -> Result<Vec<RedeemedUser>>;
}

/// Result of a redemption.
#[derive(Debug, Clone, PartialEq)]
pub struct Redemption {
    pub card: GiftCard,
    /// Wallet transaction id created by the credit.
    pub wallet_txn_id: Option<Id>,
}

/// Redemption port for the wallet group.
///
/// Implementations must, in one transaction: lock the card by normalised code, run
/// [`check_redeemable`], credit the wallet (`WALLET_TXN_TYPE`, [`redeem_reference`],
/// [`redeem_remark`]) and flip the card with a conditional update
/// (`WHERE status='active'`, rows affected must be 1) so a card is never redeemed twice.
/// `zs_infra::db::repo::marketing::gift_card::{lock_for_redeem, mark_redeemed}` provide
/// the card half for use inside the wallet's own transaction.
#[async_trait]
pub trait GiftCardRedeemer: Send + Sync {
    async fn redeem(&self, user_id: Id, code: &str) -> Result<Redemption>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn card(status: &str) -> GiftCard {
        GiftCard {
            id: 1,
            batch_id: Some(2),
            name: "N".into(),
            code: "GC1".into(),
            amount: Amount::from(10),
            currency: "CNY".into(),
            status: status.into(),
            expires_at: None,
            redeemed_at: None,
            redeemed_user_id: None,
            wallet_txn_id: None,
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
            batch: None,
        }
    }

    #[test]
    fn generation_validation_and_codes() {
        assert_eq!(validate_generate(" A ", 1, Amount::from(1)).unwrap(), "A");
        for (name, qty, amount) in [
            ("", 1, 1),
            ("A", 0, 1),
            ("A", MAX_GENERATE_QUANTITY + 1, 1),
            ("A", 1, 0),
        ] {
            assert!(validate_generate(name, qty, Amount::from(amount)).is_err());
        }
        let now = Utc.with_ymd_and_hms(2026, 7, 21, 10, 5, 9).unwrap();
        assert_eq!(
            card_code(now, 12_345, "ab12cd34ef"),
            "GC2607211005092345AB12CD34EF"
        );
        assert_eq!(batch_no(now, "0a1b2c3d"), "GCB202607211005090A1B2C3D");
    }

    #[test]
    fn redeem_checks() {
        let now = Utc::now();
        assert!(check_redeemable(&card("active"), now).is_ok());
        assert_eq!(
            check_redeemable(&card("redeemed"), now).unwrap_err().key(),
            keys::REDEEMED
        );
        assert_eq!(
            check_redeemable(&card("disabled"), now).unwrap_err().key(),
            keys::DISABLED
        );
        let mut expired = card("active");
        expired.expires_at = Some(now - chrono::Duration::seconds(1));
        assert_eq!(
            check_redeemable(&expired, now).unwrap_err().key(),
            keys::EXPIRED
        );
        let mut zero = card("active");
        zero.amount = Amount::ZERO;
        assert_eq!(
            check_redeemable(&zero, now).unwrap_err().key(),
            keys::INVALID
        );
        assert_eq!(normalize_code(" gc1a "), "GC1A");
    }

    #[test]
    fn admin_update_rules() {
        let now = Utc::now();
        let mut c = card("redeemed");
        let disable = GiftCardUpdate {
            status: Some("disabled".into()),
            ..GiftCardUpdate::default()
        };
        assert!(apply_update(&mut c, &disable, now).is_err());
        let mut c = card("active");
        apply_update(&mut c, &disable, now).unwrap();
        assert_eq!(c.status, "disabled");
        let past = GiftCardUpdate {
            expires_at: Some(now - chrono::Duration::days(1)),
            ..GiftCardUpdate::default()
        };
        assert!(apply_update(&mut c, &past, now).is_err());
        let redeem = GiftCardUpdate {
            status: Some("redeemed".into()),
            ..GiftCardUpdate::default()
        };
        assert!(apply_update(&mut c, &redeem, now).is_err());
        c.expires_at = Some(now);
        let clear = GiftCardUpdate {
            clear_expires_at: true,
            ..GiftCardUpdate::default()
        };
        apply_update(&mut c, &clear, now).unwrap();
        assert!(c.expires_at.is_none());
        assert!(parse_bulk_status("redeemed").is_err());
        assert_eq!(parse_bulk_status("Active").unwrap(), GiftCardStatus::Active);
    }

    #[test]
    fn export_csv() {
        let csv = build_export(&[card("active")], ExportFormat::Csv);
        assert!(csv.starts_with("id,batch_no,name,code,amount,currency,status,"));
        assert!(csv.contains("1,,N,GC1,10.00,CNY,active,,,,"));
        assert_eq!(
            build_export(&[card("active"), card("active")], ExportFormat::Txt),
            "GC1\nGC1"
        );
    }
}
