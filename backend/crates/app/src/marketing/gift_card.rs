//! Gift card admin use cases (redemption is exposed to the wallet group through
//! `zs_domain::marketing::gift_card::GiftCardRedeemer`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use zs_domain::catalog::card_secret::ExportFormat;
use zs_domain::catalog::card_secret::normalize_ids;
use zs_domain::marketing::gift_card::{
    DEFAULT_CURRENCY, GiftCard, GiftCardBatch, GiftCardFilter, GiftCardRepo, GiftCardUpdate,
    NewGiftCards, RedeemedUser, apply_update, batch_no, build_export, card_code, keys,
    parse_bulk_status, validate_generate,
};
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

/// Random hex characters in a card code (original `randomHex(5)` = 10 hex chars).
const CODE_RANDOM_HEX_LEN: usize = 10;
/// Random hex characters in a batch number (original `randomHex(4)` = 8 hex chars).
const BATCH_RANDOM_HEX_LEN: usize = 8;

/// A listed gift card with admin extras.
#[derive(Debug, Clone)]
pub struct GiftCardRow {
    pub card: GiftCard,
    pub is_expired: bool,
    pub redeemed_user: Option<RedeemedUser>,
}

/// Gift card service.
#[derive(Clone)]
pub struct GiftCardService {
    repo: Arc<dyn GiftCardRepo>,
    settings: Arc<dyn SettingsStore>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for GiftCardService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GiftCardService")
    }
}

fn random_hex(len: usize) -> String {
    let mut out = String::with_capacity(len);
    while out.len() < len {
        out.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    out.truncate(len);
    out
}

impl GiftCardService {
    pub fn new(
        repo: Arc<dyn GiftCardRepo>,
        settings: Arc<dyn SettingsStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            settings,
            clock,
        }
    }

    /// Site currency (`site_config.currency`, three letters, default CNY).
    async fn site_currency(&self) -> String {
        let value = self
            .settings
            .get(setting_keys::SITE_CONFIG)
            .await
            .ok()
            .flatten();
        value
            .as_ref()
            .and_then(|v| v.get("currency"))
            .and_then(serde_json::Value::as_str)
            .map(|c| c.trim().to_uppercase())
            .filter(|c| c.len() == 3 && c.chars().all(|ch| ch.is_ascii_uppercase()))
            .unwrap_or_else(|| DEFAULT_CURRENCY.to_owned())
    }

    pub async fn generate(
        &self,
        name: &str,
        quantity: i32,
        amount: Amount,
        expires_at: Option<DateTime<Utc>>,
        created_by: Option<Id>,
    ) -> Result<(GiftCardBatch, usize)> {
        let name = validate_generate(name, quantity, amount)?;
        let now = self.clock.now();
        let count = usize::try_from(quantity).unwrap_or(0);
        let mut seen = HashSet::with_capacity(count);
        let mut codes = Vec::with_capacity(count);
        for i in 0..count {
            let code = card_code(now, i, &random_hex(CODE_RANDOM_HEX_LEN));
            if seen.insert(code.clone()) {
                codes.push(code);
            }
        }
        let batch = self
            .repo
            .create_batch(
                &NewGiftCards {
                    batch_no: batch_no(now, &random_hex(BATCH_RANDOM_HEX_LEN)),
                    name,
                    amount,
                    currency: self.site_currency().await,
                    expires_at,
                    created_by,
                    codes,
                },
                now,
            )
            .await
            .map_err(|e| e.or_internal(keys::CREATE_FAILED))?;
        Ok((batch, count))
    }

    pub async fn list(
        &self,
        filter: GiftCardFilter,
        page: PageRequest,
    ) -> Result<Page<GiftCardRow>> {
        let fetch = |e: Error| e.or_internal(keys::FETCH_FAILED);
        let now = self.clock.now();
        let filter = GiftCardFilter {
            code: filter.code.trim().to_uppercase(),
            status: filter.status.trim().to_lowercase(),
            batch_no: filter.batch_no.trim().to_uppercase(),
            ..filter
        };
        let page = self.repo.list(&filter, page, now).await.map_err(fetch)?;
        let user_ids: Vec<Id> = page
            .items
            .iter()
            .filter_map(|c| c.redeemed_user_id)
            .filter(|id| *id > 0)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let users: HashMap<Id, RedeemedUser> = if user_ids.is_empty() {
            HashMap::new()
        } else {
            self.repo
                .redeemed_users(&user_ids)
                .await
                .map_err(fetch)?
                .into_iter()
                .map(|u| (u.id, u))
                .collect()
        };
        Ok(page.map(|card| GiftCardRow {
            is_expired: card.expires_at.is_some_and(|e| e < now),
            redeemed_user: card.redeemed_user_id.and_then(|id| users.get(&id).cloned()),
            card,
        }))
    }

    pub async fn update(&self, id: Id, update: GiftCardUpdate) -> Result<GiftCard> {
        if id <= 0 {
            return Err(Error::bad_request(keys::INVALID));
        }
        let mut card = self
            .repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(keys::NOT_FOUND))?;
        let now = self.clock.now();
        apply_update(&mut card, &update, now)?;
        card.updated_at = now;
        self.repo
            .save(&card)
            .await
            .map_err(|e| e.or_internal(keys::UPDATE_FAILED))?;
        Ok(card)
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        if id <= 0 {
            return Err(Error::bad_request(keys::INVALID));
        }
        let card = self
            .repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(keys::NOT_FOUND))?;
        if card.status == "redeemed" {
            return Err(Error::bad_request(keys::INVALID));
        }
        self.repo
            .delete(id, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::DELETE_FAILED))
    }

    /// Updates status of non-redeemed cards.
    pub async fn batch_status(&self, ids: &[Id], status: &str) -> Result<u64> {
        let ids = normalize_ids(ids);
        if ids.is_empty() {
            return Err(Error::bad_request(keys::INVALID));
        }
        let status = parse_bulk_status(status)?;
        self.repo
            .update_status(&ids, status, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::UPDATE_FAILED))
    }

    pub async fn export(
        &self,
        ids: &[Id],
        format: &str,
    ) -> Result<(String, &'static str, &'static str)> {
        let ids = normalize_ids(ids);
        if ids.is_empty() {
            return Err(Error::bad_request(keys::INVALID));
        }
        let format =
            ExportFormat::parse(format).ok_or_else(|| Error::bad_request(keys::INVALID))?;
        let cards = self
            .repo
            .list_by_ids(&ids)
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))?;
        if cards.is_empty() {
            return Err(Error::not_found(keys::NOT_FOUND));
        }
        Ok((
            build_export(&cards, format),
            format.content_type(),
            format.extension(),
        ))
    }
}
