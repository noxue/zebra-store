//! [`ConnectionRepo`] on `site_connections`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use std::collections::HashMap;

use zs_domain::integration::connection::{
    ConnectionRepo, ConnectionState, ConnectionStatus, NewConnection, SiteConnection, SyncMode,
    WebhookStatus,
};
use zs_domain::integration::pricing::RoundingMode;
use zs_domain::{Id, Result};
use zs_shared::page::{Page, PageRequest};

use crate::db::entity::{integration_connection_states as states, site_connections};
use crate::db::repo::support::{DbResultExt, from_json, to_json};

/// SeaORM implementation of [`ConnectionRepo`].
#[derive(Debug, Clone)]
pub struct SeaConnectionRepo {
    db: DatabaseConnection,
}

impl SeaConnectionRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

pub(crate) fn to_domain(m: site_connections::Model) -> SiteConnection {
    SiteConnection {
        id: m.id,
        name: m.name,
        base_url: m.base_url,
        api_key: m.api_key,
        api_secret: m.api_secret,
        protocol: m.protocol,
        callback_url: m.callback_url,
        status: ConnectionStatus::from_stored(&m.status),
        last_ping_at: m.last_ping_at,
        last_ping_ok: m.last_ping_ok,
        retry_max: m.retry_max,
        retry_intervals: m.retry_intervals,
        exchange_rate: m.exchange_rate.normalize(),
        price_markup_percent: m.price_markup_percent.normalize(),
        price_rounding_mode: RoundingMode::from_stored(&m.price_rounding_mode),
        auto_sync_price: m.auto_sync_price,
        created_at: m.created_at,
        updated_at: m.updated_at,
        state: ConnectionState::default(),
    }
}

fn state_of(m: states::Model) -> ConnectionState {
    let mut s = ConnectionState {
        features: from_json(m.features),
        capabilities: from_json(m.capabilities),
        supplier_currency: m.supplier_currency,
        sync_mode: if m.sync_mode == "incremental" {
            SyncMode::Incremental
        } else {
            SyncMode::Full
        },
        last_change_seq: 0,
        webhook_status: WebhookStatus::from_stored(&m.webhook_status),
        last_handshake_at: m.last_handshake_at,
        handshake_error: m.handshake_error,
        change_cursor: String::new(),
        negotiated: m.negotiated,
        extra: from_json(m.extra),
    };
    s.set_cursor(&m.change_cursor);
    s
}

/// Attaches the stored adapter state to connections.
pub(crate) async fn with_states<C: ConnectionTrait>(
    conn: &C,
    mut items: Vec<SiteConnection>,
) -> Result<Vec<SiteConnection>> {
    if items.is_empty() {
        return Ok(items);
    }
    let ids: Vec<Id> = items.iter().map(|c| c.id).collect();
    let mut by_id: HashMap<Id, ConnectionState> = states::Entity::find()
        .filter(states::Column::ConnectionId.is_in(ids))
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(|m| (m.connection_id, state_of(m)))
        .collect();
    for c in &mut items {
        if let Some(s) = by_id.remove(&c.id) {
            c.state = s;
        }
    }
    Ok(items)
}

async fn one_with_state<C: ConnectionTrait>(
    conn: &C,
    row: Option<site_connections::Model>,
) -> Result<Option<SiteConnection>> {
    match row {
        Some(m) => Ok(with_states(conn, vec![to_domain(m)]).await?.pop()),
        None => Ok(None),
    }
}

/// Live connection by id on any connection handle.
pub(crate) async fn get_in<C: ConnectionTrait>(conn: &C, id: Id) -> Result<Option<SiteConnection>> {
    let row = site_connections::Entity::find_by_id(id)
        .filter(site_connections::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .dom()?;
    one_with_state(conn, row).await
}

/// Live connections by id (for list relations).
pub(crate) async fn many_in<C: ConnectionTrait>(
    conn: &C,
    ids: &[Id],
) -> Result<Vec<SiteConnection>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let items = site_connections::Entity::find()
        .filter(site_connections::Column::Id.is_in(ids.to_vec()))
        .filter(site_connections::Column::DeletedAt.is_null())
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(to_domain)
        .collect();
    // Embedded in admin views (purchase orders, mappings, reconciliation): adapter
    // configuration may hold `secret` fields and is never needed there.
    let mut out = with_states(conn, items).await?;
    for c in &mut out {
        c.state.extra.clear();
    }
    Ok(out)
}

#[async_trait]
impl ConnectionRepo for SeaConnectionRepo {
    async fn get(&self, id: Id) -> Result<Option<SiteConnection>> {
        get_in(&self.db, id).await
    }

    async fn find_by_key(&self, api_key: &str) -> Result<Option<SiteConnection>> {
        if api_key.is_empty() {
            return Ok(None);
        }
        let row = site_connections::Entity::find()
            .filter(site_connections::Column::ApiKey.eq(api_key))
            .filter(site_connections::Column::DeletedAt.is_null())
            .order_by_asc(site_connections::Column::Id)
            .one(&self.db)
            .await
            .dom()?;
        one_with_state(&self.db, row).await
    }

    async fn create(
        &self,
        c: &NewConnection,
        encrypted_secret: &str,
        now: DateTime<Utc>,
    ) -> Result<SiteConnection> {
        let row = site_connections::ActiveModel {
            name: Set(c.name.clone()),
            base_url: Set(c.base_url.clone()),
            api_key: Set(c.api_key.clone()),
            api_secret: Set(encrypted_secret.to_owned()),
            protocol: Set(c.protocol.clone()),
            callback_url: Set(c.callback_url.clone()),
            status: Set(ConnectionStatus::Pending.as_str().to_owned()),
            last_ping_at: Set(None),
            last_ping_ok: Set(false),
            retry_max: Set(c.retry_max),
            retry_intervals: Set(c.retry_intervals.clone()),
            exchange_rate: Set(c.pricing.exchange_rate),
            price_markup_percent: Set(c.pricing.markup_percent),
            price_rounding_mode: Set(c.pricing.rounding.as_str().to_owned()),
            auto_sync_price: Set(c.auto_sync_price),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(to_domain(row))
    }

    async fn save(&self, c: &SiteConnection, now: DateTime<Utc>) -> Result<()> {
        site_connections::ActiveModel {
            id: Set(c.id),
            name: Set(c.name.clone()),
            base_url: Set(c.base_url.clone()),
            api_key: Set(c.api_key.clone()),
            api_secret: Set(c.api_secret.clone()),
            protocol: Set(c.protocol.clone()),
            callback_url: Set(c.callback_url.clone()),
            status: Set(c.status.as_str().to_owned()),
            last_ping_at: Set(c.last_ping_at),
            last_ping_ok: Set(c.last_ping_ok),
            retry_max: Set(c.retry_max),
            retry_intervals: Set(c.retry_intervals.clone()),
            exchange_rate: Set(c.exchange_rate),
            price_markup_percent: Set(c.price_markup_percent),
            price_rounding_mode: Set(c.price_rounding_mode.as_str().to_owned()),
            auto_sync_price: Set(c.auto_sync_price),
            updated_at: Set(now),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn delete(&self, id: Id, at: DateTime<Utc>) -> Result<()> {
        site_connections::Entity::update_many()
            .col_expr(site_connections::Column::DeletedAt, Expr::value(at))
            .filter(site_connections::Column::Id.eq(id))
            .filter(site_connections::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn list(&self, page: PageRequest, status: &str) -> Result<Page<SiteConnection>> {
        let mut q =
            site_connections::Entity::find().filter(site_connections::Column::DeletedAt.is_null());
        if !status.is_empty() {
            q = q.filter(site_connections::Column::Status.eq(status));
        }
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(site_connections::Column::CreatedAt)
            .order_by_desc(site_connections::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: with_states(&self.db, rows.into_iter().map(to_domain).collect()).await?,
            total,
        })
    }

    async fn save_state(&self, id: Id, s: &ConnectionState, now: DateTime<Utc>) -> Result<()> {
        let existing = states::Entity::find()
            .filter(states::Column::ConnectionId.eq(id))
            .one(&self.db)
            .await
            .dom()?;
        let sync_mode = match s.sync_mode {
            SyncMode::Incremental => "incremental",
            SyncMode::Full => "full",
        };
        let mut row = states::ActiveModel {
            connection_id: Set(id),
            features: Set(to_json(&s.features)?),
            capabilities: Set(to_json(&s.capabilities)?),
            negotiated: Set(s.negotiated),
            supplier_currency: Set(s.supplier_currency.chars().take(16).collect()),
            sync_mode: Set(sync_mode.to_owned()),
            change_cursor: Set(s.change_cursor.clone()),
            webhook_status: Set(s.webhook_status.as_str().to_owned()),
            extra: Set(to_json(&s.extra)?),
            last_handshake_at: Set(s.last_handshake_at),
            handshake_error: Set(s.handshake_error.chars().take(1000).collect()),
            updated_at: Set(now),
            ..Default::default()
        };
        match existing {
            Some(m) => {
                row.id = Set(m.id);
                row.update(&self.db).await.dom()?;
            }
            None => {
                row.created_at = Set(now);
                row.insert(&self.db).await.dom()?;
            }
        }
        Ok(())
    }
}
