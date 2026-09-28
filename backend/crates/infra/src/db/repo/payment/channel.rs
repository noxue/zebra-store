//! [`ChannelRepo`] backed by `payment_channels`.

use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
};
use zs_domain::payment::channel::{ChannelDraft, ChannelFilter, ChannelRepo, PaymentChannel};
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;

use crate::db::entity::payment_channels;
use crate::db::repo::support::{DbResultExt, from_json, now, to_json};

/// SeaORM implementation of [`ChannelRepo`].
#[derive(Debug, Clone)]
pub struct SeaChannelRepo {
    db: DatabaseConnection,
}

impl SeaChannelRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

pub(crate) fn to_domain(m: payment_channels::Model) -> PaymentChannel {
    PaymentChannel {
        id: m.id,
        name: m.name,
        icon: m.icon,
        provider_type: m.provider_type,
        channel_type: m.channel_type,
        interaction_mode: m.interaction_mode,
        fee_rate: Amount::new(m.fee_rate),
        fixed_fee: Amount::new(m.fixed_fee),
        min_amount: Amount::new(m.min_amount),
        max_amount: Amount::new(m.max_amount),
        hide_amount_out_range: m.hide_amount_out_range,
        payment_roles: from_json(m.payment_roles),
        member_levels: from_json(m.member_levels),
        payment_types: from_json(m.payment_types),
        config_json: from_json(m.config_json),
        is_active: m.is_active,
        sort_order: m.sort_order,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn alive() -> Condition {
    Condition::all().add(payment_channels::Column::DeletedAt.is_null())
}

fn fill(model: &mut payment_channels::ActiveModel, d: &ChannelDraft) -> Result<()> {
    model.name = Set(d.name.clone());
    model.icon = Set(d.icon.clone());
    model.provider_type = Set(d.provider_type.clone());
    model.channel_type = Set(d.channel_type.clone());
    model.interaction_mode = Set(d.interaction_mode.clone());
    model.fee_rate = Set(d.fee_rate.decimal());
    model.fixed_fee = Set(d.fixed_fee.decimal());
    model.min_amount = Set(d.min_amount.decimal());
    model.max_amount = Set(d.max_amount.decimal());
    model.hide_amount_out_range = Set(d.hide_amount_out_range);
    model.payment_roles = Set(to_json(&d.payment_roles)?);
    model.member_levels = Set(to_json(&d.member_levels)?);
    model.payment_types = Set(to_json(&d.payment_types)?);
    model.config_json = Set(to_json(&d.config_json)?);
    model.is_active = Set(d.is_active);
    model.sort_order = Set(d.sort_order);
    model.updated_at = Set(now());
    Ok(())
}

#[async_trait]
impl ChannelRepo for SeaChannelRepo {
    async fn list(&self, filter: &ChannelFilter) -> Result<(Vec<PaymentChannel>, u64)> {
        let mut cond = alive();
        if !filter.provider_type.is_empty() {
            cond =
                cond.add(payment_channels::Column::ProviderType.eq(filter.provider_type.clone()));
        }
        if !filter.channel_type.is_empty() {
            cond = cond.add(payment_channels::Column::ChannelType.eq(filter.channel_type.clone()));
        }
        if filter.active_only {
            cond = cond.add(payment_channels::Column::IsActive.eq(true));
        }
        let total = payment_channels::Entity::find()
            .filter(cond.clone())
            .count(&self.db)
            .await
            .dom()?;
        let mut q = payment_channels::Entity::find()
            .filter(cond)
            .order_by_desc(payment_channels::Column::SortOrder)
            .order_by_asc(payment_channels::Column::Id);
        if let Some(page) = filter.page {
            q = q.offset(page.offset()).limit(page.page_size);
        }
        let rows = q.all(&self.db).await.dom()?;
        Ok((rows.into_iter().map(to_domain).collect(), total))
    }

    async fn get(&self, id: Id) -> Result<Option<PaymentChannel>> {
        if id <= 0 {
            return Ok(None);
        }
        let row = payment_channels::Entity::find_by_id(id)
            .filter(alive())
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_domain))
    }

    async fn list_by_ids(&self, ids: &[Id]) -> Result<Vec<PaymentChannel>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = payment_channels::Entity::find()
            .filter(alive())
            .filter(payment_channels::Column::Id.is_in(ids.to_vec()))
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows.into_iter().map(to_domain).collect())
    }

    async fn create(&self, draft: &ChannelDraft) -> Result<PaymentChannel> {
        let mut model = payment_channels::ActiveModel {
            created_at: Set(now()),
            ..Default::default()
        };
        fill(&mut model, draft)?;
        Ok(to_domain(model.insert(&self.db).await.dom()?))
    }

    async fn update(&self, id: Id, draft: &ChannelDraft) -> Result<PaymentChannel> {
        let mut model = payment_channels::ActiveModel {
            id: Set(id),
            ..Default::default()
        };
        fill(&mut model, draft)?;
        model.update(&self.db).await.dom()?;
        self.get(id)
            .await?
            .ok_or_else(|| Error::not_found("error.payment_channel_not_found"))
    }

    async fn delete(&self, id: Id) -> Result<()> {
        let ts = now();
        payment_channels::Entity::update_many()
            .col_expr(
                payment_channels::Column::DeletedAt,
                sea_orm::sea_query::Expr::value(Some(ts)),
            )
            .col_expr(
                payment_channels::Column::UpdatedAt,
                sea_orm::sea_query::Expr::value(ts),
            )
            .filter(payment_channels::Column::Id.eq(id))
            .filter(payment_channels::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}
