//! [`CardSecretRepo`] backed by `card_secrets` and `card_secret_batches`.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use zs_domain::catalog::card_secret::{
    BulkTarget, CardSecret, CardSecretRepo, NewSecretBatch, SecretBatch, SecretFilter, SecretStats,
    SecretStatus, StockCount, keys,
};
use zs_domain::{Error, Id, Result};
use zs_shared::page::{Page, PageRequest};

use super::sql::{CHUNK, ilike_sql};
use crate::db::entity::{card_secret_batches, card_secrets};
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`CardSecretRepo`].
#[derive(Debug, Clone)]
pub struct SeaCardSecretRepo {
    db: DatabaseConnection,
}

impl SeaCardSecretRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

pub(crate) fn secret_to_domain(m: card_secrets::Model) -> CardSecret {
    CardSecret {
        id: m.id,
        product_id: m.product_id,
        sku_id: m.sku_id,
        batch_id: m.batch_id,
        secret: m.secret,
        status: m.status,
        order_id: m.order_id,
        reserved_at: m.reserved_at,
        used_at: m.used_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
        batch: None,
    }
}

fn batch_to_domain(m: card_secret_batches::Model) -> SecretBatch {
    SecretBatch {
        id: m.id,
        product_id: m.product_id,
        sku_id: m.sku_id,
        batch_no: m.batch_no,
        source: m.source,
        total_count: m.total_count,
        note: m.note,
        created_by: m.created_by,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn filter_condition(filter: &SecretFilter) -> Condition {
    let mut cond = Condition::all().add(card_secrets::Column::DeletedAt.is_null());
    if filter.product_id > 0 {
        cond = cond.add(card_secrets::Column::ProductId.eq(filter.product_id));
    }
    if filter.sku_id > 0 {
        cond = cond.add(card_secrets::Column::SkuId.eq(filter.sku_id));
    }
    if !filter.status.is_empty() {
        cond = cond.add(card_secrets::Column::Status.eq(filter.status.clone()));
    }
    if filter.batch_id > 0 {
        cond = cond.add(card_secrets::Column::BatchId.eq(filter.batch_id));
    }
    if !filter.secret.is_empty() {
        cond = cond.add(ilike_sql("card_secrets.secret", &filter.secret));
    }
    if !filter.batch_no.is_empty() {
        let batches = Query::select()
            .column(card_secret_batches::Column::Id)
            .from(card_secret_batches::Entity)
            .and_where(Expr::col(card_secret_batches::Column::DeletedAt).is_null())
            .and_where(ilike_sql("card_secret_batches.batch_no", &filter.batch_no))
            .to_owned();
        cond = cond.add(card_secrets::Column::BatchId.in_subquery(batches));
    }
    cond
}

async fn attach_batches<C: ConnectionTrait>(
    conn: &C,
    rows: Vec<card_secrets::Model>,
) -> Result<Vec<CardSecret>> {
    let ids: Vec<Id> = rows.iter().filter_map(|r| r.batch_id).collect();
    let batches: HashMap<Id, SecretBatch> = if ids.is_empty() {
        HashMap::new()
    } else {
        card_secret_batches::Entity::find()
            .filter(card_secret_batches::Column::Id.is_in(ids))
            .filter(card_secret_batches::Column::DeletedAt.is_null())
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
            let mut s = secret_to_domain(r);
            s.batch = s.batch_id.and_then(|id| batches.get(&id).cloned());
            s
        })
        .collect())
}

async fn count_by_status<C: ConnectionTrait>(
    conn: &C,
    cond: Condition,
) -> Result<Vec<(String, i64)>> {
    card_secrets::Entity::find()
        .select_only()
        .column(card_secrets::Column::Status)
        .column_as(card_secrets::Column::Id.count(), "total")
        .filter(cond)
        .group_by(card_secrets::Column::Status)
        .into_tuple::<(String, i64)>()
        .all(conn)
        .await
        .dom()
}

#[async_trait]
impl CardSecretRepo for SeaCardSecretRepo {
    async fn create_batch(
        &self,
        batch: &NewSecretBatch,
        now: DateTime<Utc>,
    ) -> Result<SecretBatch> {
        let txn = self.db.begin().await.dom()?;
        let created = card_secret_batches::ActiveModel {
            product_id: Set(batch.product_id),
            sku_id: Set(batch.sku_id),
            batch_no: Set(batch.batch_no.clone()),
            source: Set(batch.source.as_str().to_owned()),
            total_count: Set(i32::try_from(batch.secrets.len()).unwrap_or(i32::MAX)),
            note: Set(batch.note.clone()),
            created_by: Set(batch.created_by),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .map_err(|e| Error::internal(e).or_internal(keys::BATCH_CREATE_FAILED))?;
        // DLV-06: chunked inserts inside one transaction.
        for chunk in batch.secrets.chunks(CHUNK) {
            let rows = chunk.iter().map(|secret| card_secrets::ActiveModel {
                product_id: Set(batch.product_id),
                sku_id: Set(batch.sku_id),
                batch_id: Set(Some(created.id)),
                secret: Set(secret.clone()),
                status: Set(SecretStatus::Available.as_str().to_owned()),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            });
            card_secrets::Entity::insert_many(rows)
                .exec(&txn)
                .await
                .dom()?;
        }
        txn.commit().await.dom()?;
        Ok(batch_to_domain(created))
    }

    async fn list(&self, filter: &SecretFilter, page: PageRequest) -> Result<Page<CardSecret>> {
        let query = card_secrets::Entity::find().filter(filter_condition(filter));
        let total = query.clone().count(&self.db).await.dom()?;
        let rows = query
            .order_by_asc(card_secrets::Column::Id)
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

    async fn target_ids(&self, target: &BulkTarget) -> Result<Vec<Id>> {
        let cond = match target {
            BulkTarget::Ids(ids) => return Ok(ids.clone()),
            BulkTarget::Filter(filter) => filter_condition(filter),
            BulkTarget::Batch(batch_id) => Condition::all()
                .add(card_secrets::Column::DeletedAt.is_null())
                .add(card_secrets::Column::BatchId.eq(*batch_id)),
        };
        card_secrets::Entity::find()
            .select_only()
            .column(card_secrets::Column::Id)
            .filter(cond)
            .order_by_asc(card_secrets::Column::Id)
            .into_tuple()
            .all(&self.db)
            .await
            .dom()
    }

    async fn list_by_ids(&self, ids: &[Id]) -> Result<Vec<CardSecret>> {
        let mut out = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(CHUNK) {
            let rows = card_secrets::Entity::find()
                .filter(card_secrets::Column::Id.is_in(chunk.to_vec()))
                .filter(card_secrets::Column::DeletedAt.is_null())
                .all(&self.db)
                .await
                .dom()?;
            out.extend(rows.into_iter().map(secret_to_domain));
        }
        out.sort_by_key(|s| s.id);
        Ok(out)
    }

    async fn get(&self, id: Id) -> Result<Option<CardSecret>> {
        Ok(card_secrets::Entity::find_by_id(id)
            .filter(card_secrets::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(secret_to_domain))
    }

    async fn update(&self, id: Id, secret: &str, status: &str, now: DateTime<Utc>) -> Result<u64> {
        let mut q = card_secrets::Entity::update_many()
            .col_expr(card_secrets::Column::Secret, Expr::value(secret))
            .col_expr(card_secrets::Column::Status, Expr::value(status))
            .col_expr(card_secrets::Column::UpdatedAt, Expr::value(now))
            .filter(card_secrets::Column::Id.eq(id))
            .filter(card_secrets::Column::DeletedAt.is_null());
        if status != SecretStatus::Used.as_str() {
            q = q.filter(card_secrets::Column::Status.ne(SecretStatus::Used.as_str()));
        }
        Ok(q.exec(&self.db).await.dom()?.rows_affected)
    }

    async fn existing_secrets(
        &self,
        product_id: Id,
        sku_id: Id,
        secrets: &[String],
    ) -> Result<HashSet<String>> {
        let mut found = HashSet::new();
        for chunk in secrets.chunks(CHUNK) {
            let rows: Vec<String> = card_secrets::Entity::find()
                .select_only()
                .column(card_secrets::Column::Secret)
                .filter(card_secrets::Column::ProductId.eq(product_id))
                .filter(card_secrets::Column::SkuId.eq(sku_id))
                .filter(card_secrets::Column::DeletedAt.is_null())
                .filter(card_secrets::Column::Secret.is_in(chunk.to_vec()))
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            found.extend(rows);
        }
        Ok(found)
    }

    async fn update_status(
        &self,
        ids: &[Id],
        status: SecretStatus,
        now: DateTime<Utc>,
    ) -> Result<u64> {
        let txn = self.db.begin().await.dom()?;
        let mut affected = 0;
        for chunk in ids.chunks(CHUNK) {
            let mut q = card_secrets::Entity::update_many()
                .col_expr(card_secrets::Column::Status, Expr::value(status.as_str()))
                .col_expr(card_secrets::Column::UpdatedAt, Expr::value(now))
                .filter(card_secrets::Column::Id.is_in(chunk.to_vec()))
                .filter(card_secrets::Column::DeletedAt.is_null());
            if status != SecretStatus::Used {
                q = q.filter(card_secrets::Column::Status.ne(SecretStatus::Used.as_str()));
            }
            affected += q.exec(&txn).await.dom()?.rows_affected;
        }
        txn.commit().await.dom()?;
        Ok(affected)
    }

    async fn soft_delete(&self, ids: &[Id], now: DateTime<Utc>) -> Result<u64> {
        let txn = self.db.begin().await.dom()?;
        let mut affected = 0;
        for chunk in ids.chunks(CHUNK) {
            affected += card_secrets::Entity::update_many()
                .col_expr(card_secrets::Column::DeletedAt, Expr::value(now))
                .col_expr(card_secrets::Column::UpdatedAt, Expr::value(now))
                .filter(card_secrets::Column::Id.is_in(chunk.to_vec()))
                .filter(card_secrets::Column::DeletedAt.is_null())
                .exec(&txn)
                .await
                .dom()?
                .rows_affected;
        }
        txn.commit().await.dom()?;
        Ok(affected)
    }

    async fn stats(&self, product_id: Id, sku_id: Id) -> Result<SecretStats> {
        let mut cond = Condition::all()
            .add(card_secrets::Column::DeletedAt.is_null())
            .add(card_secrets::Column::ProductId.eq(product_id));
        if sku_id > 0 {
            cond = cond.add(card_secrets::Column::SkuId.eq(sku_id));
        }
        let mut stats = SecretStats::default();
        for (status, total) in count_by_status(&self.db, cond).await? {
            stats.total += total;
            match SecretStatus::parse(&status) {
                Some(SecretStatus::Available) => stats.available = total,
                Some(SecretStatus::Reserved) => stats.reserved = total,
                Some(SecretStatus::Used) => stats.used = total,
                None => {}
            }
        }
        Ok(stats)
    }

    async fn stock_counts(&self, product_ids: &[Id]) -> Result<Vec<StockCount>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<(Id, Id, String, i64)> = card_secrets::Entity::find()
            .select_only()
            .column(card_secrets::Column::ProductId)
            .column(card_secrets::Column::SkuId)
            .column(card_secrets::Column::Status)
            .column_as(card_secrets::Column::Id.count(), "total")
            .filter(card_secrets::Column::ProductId.is_in(product_ids.to_vec()))
            .filter(card_secrets::Column::DeletedAt.is_null())
            .group_by(card_secrets::Column::ProductId)
            .group_by(card_secrets::Column::SkuId)
            .group_by(card_secrets::Column::Status)
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows
            .into_iter()
            .map(|(product_id, sku_id, status, total)| StockCount {
                product_id,
                sku_id,
                status,
                total,
            })
            .collect())
    }

    async fn list_batches(
        &self,
        product_id: Id,
        sku_id: Id,
        page: PageRequest,
    ) -> Result<Page<SecretBatch>> {
        let mut q = card_secret_batches::Entity::find()
            .filter(card_secret_batches::Column::DeletedAt.is_null())
            .filter(card_secret_batches::Column::ProductId.eq(product_id));
        if sku_id > 0 {
            q = q.filter(card_secret_batches::Column::SkuId.eq(sku_id));
        }
        let total = q.clone().count(&self.db).await.dom()?;
        let items = q
            .order_by_desc(card_secret_batches::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(batch_to_domain)
            .collect();
        Ok(Page { items, total })
    }

    async fn batch_counts(&self, batch_ids: &[Id]) -> Result<Vec<(Id, String, i64)>> {
        let rows: Vec<(Option<Id>, String, i64)> = card_secrets::Entity::find()
            .select_only()
            .column(card_secrets::Column::BatchId)
            .column(card_secrets::Column::Status)
            .column_as(card_secrets::Column::Id.count(), "total")
            .filter(card_secrets::Column::BatchId.is_in(batch_ids.to_vec()))
            .filter(card_secrets::Column::DeletedAt.is_null())
            .group_by(card_secrets::Column::BatchId)
            .group_by(card_secrets::Column::Status)
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows
            .into_iter()
            .filter_map(|(batch, status, total)| batch.map(|b| (b, status, total)))
            .collect())
    }

    async fn take_available(
        &self,
        product_id: Id,
        sku_id: Id,
        batch_id: Id,
        limit: u64,
        delete: bool,
        now: DateTime<Utc>,
    ) -> Result<Vec<CardSecret>> {
        let txn = self.db.begin().await.dom()?;
        let mut q = card_secrets::Entity::find()
            .filter(card_secrets::Column::ProductId.eq(product_id))
            .filter(card_secrets::Column::Status.eq(SecretStatus::Available.as_str()))
            .filter(card_secrets::Column::DeletedAt.is_null());
        if sku_id > 0 {
            q = q.filter(card_secrets::Column::SkuId.eq(sku_id));
        }
        if batch_id > 0 {
            q = q.filter(card_secrets::Column::BatchId.eq(batch_id));
        }
        let rows = q
            .order_by_asc(card_secrets::Column::Id)
            .limit(limit)
            .lock_exclusive()
            .all(&txn)
            .await
            .dom()?;
        if rows.is_empty() {
            return Err(Error::not_found(keys::NOT_FOUND));
        }
        if (rows.len() as u64) < limit {
            return Err(Error::bad_request(keys::INSUFFICIENT));
        }
        let ids: Vec<Id> = rows.iter().map(|r| r.id).collect();
        // DLV-01: conditional flip; any row taken concurrently aborts the whole export.
        let mut update = card_secrets::Entity::update_many()
            .col_expr(card_secrets::Column::UpdatedAt, Expr::value(now))
            .filter(card_secrets::Column::Id.is_in(ids.clone()))
            .filter(card_secrets::Column::Status.eq(SecretStatus::Available.as_str()))
            .filter(card_secrets::Column::DeletedAt.is_null());
        update = if delete {
            update.col_expr(card_secrets::Column::DeletedAt, Expr::value(now))
        } else {
            update.col_expr(
                card_secrets::Column::Status,
                Expr::value(SecretStatus::Used.as_str()),
            )
        };
        let affected = update.exec(&txn).await.dom()?.rows_affected;
        if affected != ids.len() as u64 {
            return Err(Error::bad_request(keys::INSUFFICIENT));
        }
        txn.commit().await.dom()?;
        Ok(rows.into_iter().map(secret_to_domain).collect())
    }
}
