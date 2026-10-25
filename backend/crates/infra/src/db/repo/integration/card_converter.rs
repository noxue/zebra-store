//! SeaORM storage for converter profiles and product mappings.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
use zs_domain::integration::card_converter::{
    Binding, BindingDependencies, Converter, ConverterEvent, ConverterRepo, ConverterSource,
    NewConverterEvent,
};
use zs_domain::{Error, Id, Result};

use crate::db::entity::extra::{
    card_converter_bindings as bindings, card_converter_events as events,
    card_converters as converters,
};
use crate::db::entity::{
    card_secret_batches, card_secrets, procurement_orders, product_mappings, product_skus,
    products, settings, site_connections,
};
use crate::db::repo::support::{DbResultExt, from_json, to_json};

#[derive(Debug, Clone)]
pub struct SeaCardConverterRepo {
    db: DatabaseConnection,
}

impl SeaCardConverterRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    async fn site_snapshot(&self) -> Result<serde_json::Value> {
        let config = settings::Entity::find_by_id(zs_domain::settings::keys::SITE_CONFIG)
            .one(&self.db)
            .await
            .dom()?
            .and_then(|row| row.value_json)
            .unwrap_or_default();
        let brand = &config["brand"];
        Ok(serde_json::json!({
            "domain": brand["site_url"].as_str().unwrap_or_default(),
            "name": brand["site_name"].as_str().unwrap_or_default(),
        }))
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
        extra_template: m.extra_template_json.unwrap_or(serde_json::Value::Null),
        issues: Vec::new(),
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

#[async_trait]
impl ConverterRepo for SeaCardConverterRepo {
    async fn binding_dependencies(&self, binding: &Binding) -> Result<BindingDependencies> {
        let saved_snapshot = bindings::Entity::find()
            .filter(bindings::Column::ProductId.eq(binding.product_id))
            .filter(bindings::Column::SkuId.eq(binding.sku_id))
            .one(&self.db)
            .await
            .dom()?
            .and_then(|row| row.fields_json);
        let current_site = self.site_snapshot().await?;
        let site_changed = saved_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.get("site"))
            .is_some_and(|saved| saved != &current_site);
        let product = products::Entity::find_by_id(binding.product_id)
            .filter(products::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?;
        let skus = product_skus::Entity::find()
            .filter(product_skus::Column::ProductId.eq(binding.product_id))
            .filter(product_skus::Column::DeletedAt.is_null())
            .filter(product_skus::Column::IsActive.eq(true))
            .all(&self.db)
            .await
            .dom()?;
        let sku_available = binding.sku_id == 0 || skus.iter().any(|sku| sku.id == binding.sku_id);
        let sku_changed = skus.iter().any(|sku| {
            (binding.sku_id == 0 || sku.id == binding.sku_id) && sku.updated_at > binding.updated_at
        });
        let converter_enabled = converters::Entity::find_by_id(binding.converter_id)
            .one(&self.db)
            .await
            .dom()?
            .is_some_and(|converter| converter.enabled);
        let mappings = product_mappings::Entity::find()
            .filter(product_mappings::Column::LocalProductId.eq(binding.product_id))
            .filter(product_mappings::Column::DeletedAt.is_null())
            .filter(product_mappings::Column::IsActive.eq(true))
            .all(&self.db)
            .await
            .dom()?;
        let mut sources = Vec::new();
        let mut source_changed = false;
        for mapping in mappings {
            source_changed |= mapping.updated_at > binding.updated_at;
            if let Some(row) = site_connections::Entity::find_by_id(mapping.connection_id)
                .filter(site_connections::Column::DeletedAt.is_null())
                .filter(site_connections::Column::Status.eq("active"))
                .one(&self.db)
                .await
                .dom()?
            {
                source_changed |= row.updated_at > binding.updated_at;
                sources.push(ConverterSource {
                    id: row.id,
                    name: row.name,
                    protocol: row.protocol,
                });
            }
        }
        Ok(BindingDependencies {
            product_changed: product
                .as_ref()
                .is_some_and(|row| row.updated_at > binding.updated_at),
            product_exists: product.is_some(),
            sku_available,
            sku_changed,
            converter_enabled,
            sources,
            source_changed,
            site_changed,
        })
    }
    async fn source_for_order(&self, order_id: Id) -> Result<Option<ConverterSource>> {
        let Some(procurement) = procurement_orders::Entity::find()
            .filter(procurement_orders::Column::LocalOrderId.eq(order_id))
            .filter(procurement_orders::Column::DeletedAt.is_null())
            .order_by_desc(procurement_orders::Column::Id)
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let connection = site_connections::Entity::find_by_id(procurement.connection_id)
            .one(&self.db)
            .await
            .dom()?;
        Ok(connection.map(|row| ConverterSource {
            id: row.id,
            name: row.name,
            protocol: row.protocol,
        }))
    }

    async fn local_card_notes(&self, order_id: Id) -> Result<Vec<(String, String, bool)>> {
        let cards = card_secrets::Entity::find()
            .filter(card_secrets::Column::OrderId.eq(order_id))
            .filter(card_secrets::Column::Status.eq("used"))
            .filter(card_secrets::Column::DeletedAt.is_null())
            .order_by_asc(card_secrets::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        let batch_ids = cards
            .iter()
            .filter_map(|card| card.batch_id)
            .collect::<Vec<_>>();
        let batches = if batch_ids.is_empty() {
            Vec::new()
        } else {
            card_secret_batches::Entity::find()
                .filter(card_secret_batches::Column::Id.is_in(batch_ids))
                .all(&self.db)
                .await
                .dom()?
        };
        let notes = batches
            .into_iter()
            .map(|batch| {
                (
                    batch.id,
                    (
                        batch.note,
                        zs_domain::catalog::card_secret::BatchSource::is_preconverted(
                            &batch.source,
                        ),
                    ),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        Ok(cards
            .into_iter()
            .map(|card| {
                let (note, preconverted) = card
                    .batch_id
                    .and_then(|id| notes.get(&id))
                    .cloned()
                    .unwrap_or_default();
                (card.secret, note, preconverted)
            })
            .collect())
    }

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
            .filter(products::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .ok_or_else(|| Error::not_found("error.product_not_found"))?;
        if b.sku_id > 0
            && product_skus::Entity::find_by_id(b.sku_id)
                .filter(product_skus::Column::DeletedAt.is_null())
                .one(&self.db)
                .await
                .dom()?
                .is_none_or(|sku| sku.product_id != b.product_id || !sku.is_active)
        {
            return Err(Error::bad_request("error.card_converter_binding_invalid"));
        }
        let now = Utc::now();
        let extra_template_json = Some(b.extra_template.clone());
        let fields_json = Some(serde_json::json!({"site": self.site_snapshot().await?}));
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
