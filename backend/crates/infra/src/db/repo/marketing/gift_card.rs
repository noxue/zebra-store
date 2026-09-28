//! [`GiftCardRepo`] backed by `gift_cards` / `gift_card_batches`, plus the card half of
//! a redemption for use inside the wallet group's transaction.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use zs_domain::marketing::gift_card::{
    GiftCard, GiftCardBatch, GiftCardFilter, GiftCardRepo, GiftCardStatus, NewGiftCards,
    RedeemedUser, check_redeemable, keys, normalize_code,
};
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::db::entity::{gift_card_batches, gift_cards, users};
use crate::db::repo::catalog::sql::CHUNK;
use crate::db::repo::support::DbResultExt;

/// `status=expired` list filter (active but past expiry).
const LIST_STATUS_EXPIRED: &str = "expired";

/// SeaORM implementation of [`GiftCardRepo`].
#[derive(Debug, Clone)]
pub struct SeaGiftCardRepo {
    db: DatabaseConnection,
}

impl SeaGiftCardRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn batch_to_domain(m: gift_card_batches::Model) -> GiftCardBatch {
    GiftCardBatch {
        id: m.id,
        batch_no: m.batch_no,
        name: m.name,
        amount: Amount::new(m.amount),
        currency: m.currency,
        quantity: m.quantity,
        expires_at: m.expires_at,
        created_by: m.created_by,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn card_to_domain(m: gift_cards::Model) -> GiftCard {
    GiftCard {
        id: m.id,
        batch_id: m.batch_id,
        name: m.name,
        code: m.code,
        amount: Amount::new(m.amount),
        currency: m.currency,
        status: m.status,
        expires_at: m.expires_at,
        redeemed_at: m.redeemed_at,
        redeemed_user_id: m.redeemed_user_id,
        wallet_txn_id: m.wallet_txn_id,
        created_at: m.created_at,
        updated_at: m.updated_at,
        batch: None,
    }
}

async fn attach_batches<C: ConnectionTrait>(
    conn: &C,
    rows: Vec<gift_cards::Model>,
) -> Result<Vec<GiftCard>> {
    let ids: Vec<Id> = rows.iter().filter_map(|r| r.batch_id).collect();
    let batches: HashMap<Id, GiftCardBatch> = if ids.is_empty() {
        HashMap::new()
    } else {
        gift_card_batches::Entity::find()
            .filter(gift_card_batches::Column::Id.is_in(ids))
            .filter(gift_card_batches::Column::DeletedAt.is_null())
            .all(conn)
            .await
            .dom()?
            .into_iter()
            .map(|b| (b.id, batch_to_domain(b)))
            .collect()
    };
    Ok(rows
        .into_iter()
        .map(|r| {
            let mut c = card_to_domain(r);
            c.batch = c.batch_id.and_then(|id| batches.get(&id).cloned());
            c
        })
        .collect())
}

fn list_condition(f: &GiftCardFilter, now: DateTime<Utc>) -> Condition {
    let mut cond = Condition::all().add(gift_cards::Column::DeletedAt.is_null());
    if !f.code.is_empty() {
        cond = cond.add(gift_cards::Column::Code.contains(f.code.clone()));
    }
    match f.status.as_str() {
        "" => {}
        LIST_STATUS_EXPIRED => {
            cond = cond
                .add(gift_cards::Column::Status.eq(GiftCardStatus::Active.as_str()))
                .add(gift_cards::Column::ExpiresAt.is_not_null())
                .add(gift_cards::Column::ExpiresAt.lt(now));
        }
        "active" => {
            cond = cond
                .add(gift_cards::Column::Status.eq(GiftCardStatus::Active.as_str()))
                .add(
                    Condition::any()
                        .add(gift_cards::Column::ExpiresAt.is_null())
                        .add(gift_cards::Column::ExpiresAt.gte(now)),
                );
        }
        other => cond = cond.add(gift_cards::Column::Status.eq(other)),
    }
    if !f.batch_no.is_empty() {
        let batches = Query::select()
            .column(gift_card_batches::Column::Id)
            .from(gift_card_batches::Entity)
            .and_where(Expr::col(gift_card_batches::Column::DeletedAt).is_null())
            .and_where(
                Expr::col(gift_card_batches::Column::BatchNo).like(format!("%{}%", f.batch_no)),
            )
            .to_owned();
        cond = cond.add(gift_cards::Column::BatchId.in_subquery(batches));
    }
    if f.redeemed_user_id > 0 {
        cond = cond.add(gift_cards::Column::RedeemedUserId.eq(f.redeemed_user_id));
    }
    let ranges = [
        (gift_cards::Column::CreatedAt, f.created_from, f.created_to),
        (
            gift_cards::Column::RedeemedAt,
            f.redeemed_from,
            f.redeemed_to,
        ),
        (gift_cards::Column::ExpiresAt, f.expires_from, f.expires_to),
    ];
    for (col, from, to) in ranges {
        if let Some(from) = from {
            cond = cond.add(col.gte(from));
        }
        if let Some(to) = to {
            cond = cond.add(col.lte(to));
        }
    }
    cond
}

#[async_trait]
impl GiftCardRepo for SeaGiftCardRepo {
    async fn create_batch(&self, new: &NewGiftCards, now: DateTime<Utc>) -> Result<GiftCardBatch> {
        let txn = self.db.begin().await.dom()?;
        let batch = gift_card_batches::ActiveModel {
            batch_no: Set(new.batch_no.clone()),
            name: Set(new.name.clone()),
            amount: Set(new.amount.decimal()),
            currency: Set(new.currency.clone()),
            quantity: Set(i32::try_from(new.codes.len()).unwrap_or(i32::MAX)),
            expires_at: Set(new.expires_at),
            created_by: Set(new.created_by),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .dom()?;
        for chunk in new.codes.chunks(CHUNK) {
            let rows = chunk.iter().map(|code| gift_cards::ActiveModel {
                batch_id: Set(Some(batch.id)),
                name: Set(new.name.clone()),
                code: Set(code.clone()),
                amount: Set(new.amount.decimal()),
                currency: Set(new.currency.clone()),
                status: Set(GiftCardStatus::Active.as_str().to_owned()),
                expires_at: Set(new.expires_at),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            });
            gift_cards::Entity::insert_many(rows)
                .exec(&txn)
                .await
                .dom()?;
        }
        txn.commit().await.dom()?;
        Ok(batch_to_domain(batch))
    }

    async fn get(&self, id: Id) -> Result<Option<GiftCard>> {
        let row = gift_cards::Entity::find_by_id(id)
            .filter(gift_cards::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?;
        Ok(match row {
            Some(r) => attach_batches(&self.db, vec![r]).await?.pop(),
            None => None,
        })
    }

    async fn list(
        &self,
        filter: &GiftCardFilter,
        page: PageRequest,
        now: DateTime<Utc>,
    ) -> Result<Page<GiftCard>> {
        let q = gift_cards::Entity::find().filter(list_condition(filter, now));
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(gift_cards::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: attach_batches(&self.db, rows).await?,
            total,
        })
    }

    async fn list_by_ids(&self, ids: &[Id]) -> Result<Vec<GiftCard>> {
        let mut rows = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(CHUNK) {
            rows.extend(
                gift_cards::Entity::find()
                    .filter(gift_cards::Column::Id.is_in(chunk.to_vec()))
                    .filter(gift_cards::Column::DeletedAt.is_null())
                    .all(&self.db)
                    .await
                    .dom()?,
            );
        }
        rows.sort_by_key(|r| r.id);
        attach_batches(&self.db, rows).await
    }

    async fn save(&self, card: &GiftCard) -> Result<()> {
        gift_cards::ActiveModel {
            id: Set(card.id),
            name: Set(card.name.clone()),
            status: Set(card.status.clone()),
            expires_at: Set(card.expires_at),
            updated_at: Set(card.updated_at),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn delete(&self, id: Id, now: DateTime<Utc>) -> Result<()> {
        gift_cards::Entity::update_many()
            .col_expr(gift_cards::Column::DeletedAt, Expr::value(now))
            .filter(gift_cards::Column::Id.eq(id))
            .filter(gift_cards::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn update_status(
        &self,
        ids: &[Id],
        status: GiftCardStatus,
        now: DateTime<Utc>,
    ) -> Result<u64> {
        let txn = self.db.begin().await.dom()?;
        let mut affected = 0;
        for chunk in ids.chunks(CHUNK) {
            affected += gift_cards::Entity::update_many()
                .col_expr(gift_cards::Column::Status, Expr::value(status.as_str()))
                .col_expr(gift_cards::Column::UpdatedAt, Expr::value(now))
                .filter(gift_cards::Column::Id.is_in(chunk.to_vec()))
                .filter(gift_cards::Column::DeletedAt.is_null())
                .filter(gift_cards::Column::Status.ne(GiftCardStatus::Redeemed.as_str()))
                .exec(&txn)
                .await
                .dom()?
                .rows_affected;
        }
        txn.commit().await.dom()?;
        Ok(affected)
    }

    async fn redeemed_users(&self, user_ids: &[Id]) -> Result<Vec<RedeemedUser>> {
        let rows: Vec<(Id, String, String)> = users::Entity::find()
            .select_only()
            .column(users::Column::Id)
            .column(users::Column::Email)
            .column(users::Column::DisplayName)
            .filter(users::Column::Id.is_in(user_ids.to_vec()))
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows
            .into_iter()
            .map(|(id, email, display_name)| RedeemedUser {
                id,
                email,
                display_name,
            })
            .collect())
    }
}

/// Locks an active gift card by (normalised) code inside the caller's transaction and
/// validates it (`check_redeemable`).
pub async fn lock_for_redeem<C: ConnectionTrait>(
    conn: &C,
    code: &str,
    now: DateTime<Utc>,
) -> Result<GiftCard> {
    let code = normalize_code(code);
    if code.is_empty() {
        return Err(Error::bad_request(keys::INVALID));
    }
    let row = gift_cards::Entity::find()
        .filter(gift_cards::Column::Code.eq(code))
        .filter(gift_cards::Column::DeletedAt.is_null())
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
        .ok_or_else(|| Error::not_found(keys::NOT_FOUND))?;
    let card = card_to_domain(row);
    check_redeemable(&card, now)?;
    Ok(card)
}

/// Flips a locked card to `redeemed` with a conditional update (exactly one row, never twice).
pub async fn mark_redeemed<C: ConnectionTrait>(
    conn: &C,
    card_id: Id,
    user_id: Id,
    wallet_txn_id: Option<Id>,
    now: DateTime<Utc>,
) -> Result<()> {
    let affected = gift_cards::Entity::update_many()
        .col_expr(
            gift_cards::Column::Status,
            Expr::value(GiftCardStatus::Redeemed.as_str()),
        )
        .col_expr(gift_cards::Column::RedeemedUserId, Expr::value(user_id))
        .col_expr(gift_cards::Column::RedeemedAt, Expr::value(now))
        .col_expr(gift_cards::Column::WalletTxnId, Expr::value(wallet_txn_id))
        .col_expr(gift_cards::Column::UpdatedAt, Expr::value(now))
        .filter(gift_cards::Column::Id.eq(card_id))
        .filter(gift_cards::Column::Status.eq(GiftCardStatus::Active.as_str()))
        .filter(gift_cards::Column::DeletedAt.is_null())
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if affected != 1 {
        return Err(Error::bad_request(keys::REDEEMED));
    }
    Ok(())
}
