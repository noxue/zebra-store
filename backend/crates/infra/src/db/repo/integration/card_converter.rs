//! SeaORM storage for converter profiles and product mappings.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
use zs_domain::integration::card_converter::{
    Binding, Converter, ConverterEvent, ConverterRepo, NewConverterEvent,
};
use zs_domain::{Error, Id, Result};

use crate::db::entity::extra::{
    card_converter_bindings as bindings, card_converter_events as events,
    card_converters as converters,
};
use crate::db::entity::{product_skus, products};
use crate::db::repo::support::{DbResultExt, from_json, to_json};

#[derive(Debug, Clone)]
pub struct SeaCardConverterRepo {
    db: DatabaseConnection,
}

impl SeaCardConverterRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn converter(m: converters::Model) -> Converter {
    let token_configured = !m.token_enc.is_empty();
    Converter {
        id: m.id,
        name: m.name,
        base_url: m.base_url,
        token_enc: m.token_enc,
        token_configured,
        enabled: m.enabled,
        types: from_json(m.types_json),
        health: m.health,
        consecutive_failures: m.consecutive_failures,
        last_checked_at: m.last_checked_at,
        last_error: m.last_error,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn binding(m: bindings::Model) -> Binding {
    Binding {
        id: m.id,
        product_id: m.product_id,
        sku_id: m.sku_id,
        converter_id: m.converter_id,
        type_id: m.type_id,
        fields: from_json(m.fields_json),
        extra_template: m.extra_template_json.unwrap_or(serde_json::Value::Null),
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

#[async_trait]
impl ConverterRepo for SeaCardConverterRepo {
    async fn list(&self) -> Result<Vec<Converter>> {
        Ok(converters::Entity::find()
            .order_by_asc(converters::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(converter)
            .collect())
    }

    async fn get(&self, id: Id) -> Result<Option<Converter>> {
        Ok(converters::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .dom()?
            .map(converter))
    }

    async fn save(&self, c: &Converter) -> Result<Converter> {
        let types_json = to_json(&c.types)?;
        let now = Utc::now();
        if c.id > 0 {
            let mut active: converters::ActiveModel = converters::Entity::find_by_id(c.id)
                .one(&self.db)
                .await
                .dom()?
                .ok_or_else(|| Error::not_found("error.card_converter_not_found"))?
                .into();
            active.name = Set(c.name.clone());
            active.base_url = Set(c.base_url.clone());
            active.token_enc = Set(c.token_enc.clone());
            active.enabled = Set(c.enabled);
            active.types_json = Set(types_json);
            active.updated_at = Set(now);
            return active.update(&self.db).await.dom().map(converter);
        }
        converters::ActiveModel {
            name: Set(c.name.clone()),
            base_url: Set(c.base_url.clone()),
            token_enc: Set(c.token_enc.clone()),
            enabled: Set(c.enabled),
            types_json: Set(types_json),
            health: Set("unknown".into()),
            consecutive_failures: Set(0),
            last_checked_at: Set(None),
            last_error: Set(String::new()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()
        .map(converter)
    }

    async fn delete(&self, id: Id) -> Result<()> {
        converters::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn bindings(&self, product_id: Option<Id>) -> Result<Vec<Binding>> {
        let mut query = bindings::Entity::find().order_by_asc(bindings::Column::ProductId);
        if let Some(product_id) = product_id.filter(|id| *id > 0) {
            query = query.filter(bindings::Column::ProductId.eq(product_id));
        }
        Ok(query
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(binding)
            .collect())
    }

    async fn get_binding(&self, product_id: Id, sku_id: Id) -> Result<Option<Binding>> {
        Ok(bindings::Entity::find()
            .filter(bindings::Column::ProductId.eq(product_id))
            .filter(bindings::Column::SkuId.eq(sku_id))
            .one(&self.db)
            .await
            .dom()?
            .map(binding))
    }

    async fn save_binding(&self, b: &Binding) -> Result<Binding> {
        let _product = products::Entity::find_by_id(b.product_id)
            .one(&self.db)
            .await
            .dom()?
            .ok_or_else(|| Error::not_found("error.product_not_found"))?;
        if b.sku_id > 0
            && product_skus::Entity::find_by_id(b.sku_id)
                .one(&self.db)
                .await
                .dom()?
                .is_none_or(|sku| sku.product_id != b.product_id || !sku.is_active)
        {
            return Err(Error::bad_request("error.card_converter_binding_invalid"));
        }
        let now = Utc::now();
        let fields_json = to_json(&b.fields)?;
        let extra_template_json = Some(b.extra_template.clone());
        let model = bindings::ActiveModel {
            product_id: Set(b.product_id),
            sku_id: Set(b.sku_id),
            converter_id: Set(b.converter_id),
            type_id: Set(b.type_id.clone()),
            fields_json: Set(fields_json),
            extra_template_json: Set(extra_template_json),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        bindings::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([bindings::Column::ProductId, bindings::Column::SkuId])
                    .update_columns([
                        bindings::Column::ConverterId,
                        bindings::Column::TypeId,
                        bindings::Column::FieldsJson,
                        bindings::Column::ExtraTemplateJson,
                        bindings::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await
            .dom()?;
        self.get_binding(b.product_id, b.sku_id)
            .await?
            .ok_or_else(|| Error::internal_msg("saved converter binding disappeared"))
    }

    async fn delete_binding(&self, product_id: Id, sku_id: Id) -> Result<()> {
        bindings::Entity::delete_many()
            .filter(bindings::Column::ProductId.eq(product_id))
            .filter(bindings::Column::SkuId.eq(sku_id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn health_result(
        &self,
        id: Id,
        ok: bool,
        at: DateTime<Utc>,
        safe_error: &str,
    ) -> Result<(Converter, bool)> {
        let model = converters::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .dom()?
            .ok_or_else(|| Error::not_found("error.card_converter_not_found"))?;
        let previous = model.health.clone();
        let mut active: converters::ActiveModel = model.into();
        let (health, failures, error) = if ok {
            ("healthy", 0, String::new())
        } else {
            let count = active.consecutive_failures.clone().unwrap() + 1;
            (
                if count >= 3 { "unhealthy" } else { "degraded" },
                count,
                safe_error.to_owned(),
            )
        };
        active.health = Set(health.into());
        active.consecutive_failures = Set(failures);
        active.last_checked_at = Set(Some(at));
        active.last_error = Set(error);
        active.updated_at = Set(at);
        let saved = active.update(&self.db).await.dom().map(converter)?;
        Ok((
            saved.clone(),
            (previous != saved.health)
                && (saved.health == "healthy" || saved.health == "unhealthy"),
        ))
    }

    async fn record_event(&self, event: &NewConverterEvent) -> Result<()> {
        events::ActiveModel {
            converter_id: Set(event.converter_id),
            event_type: Set(event.event_type.clone()),
            status: Set(event.status.clone()),
            error_code: Set(event.error_code.clone()),
            duration_ms: Set(event.duration_ms.max(0)),
            order_no: Set(event.order_no.chars().take(64).collect()),
            created_at: Set(event.created_at),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn events(&self, converter_id: Id, limit: u64) -> Result<Vec<ConverterEvent>> {
        Ok(events::Entity::find()
            .filter(events::Column::ConverterId.eq(converter_id))
            .order_by_desc(events::Column::Id)
            .limit(limit.clamp(1, 200))
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|event| ConverterEvent {
                id: event.id,
                converter_id: event.converter_id,
                event_type: event.event_type,
                status: event.status,
                error_code: event.error_code,
                duration_ms: event.duration_ms,
                order_no: event.order_no,
                created_at: event.created_at,
            })
            .collect())
    }
}
