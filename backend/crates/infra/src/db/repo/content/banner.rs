//! [`BannerRepo`] and [`MediaRepo`] backed by `banners` and `media`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde_json::Value;
use zs_domain::content::banner::{Banner, BannerQuery, BannerRepo};
use zs_domain::content::media::{Media, MediaQuery, MediaRepo, NewMedia};
use zs_domain::{Id, Result};
use zs_shared::page::Page;

use super::search::localized_like;
use crate::db::entity::{banners, media};
use crate::db::repo::support::{DbResultExt, now};

/// SeaORM implementation of [`BannerRepo`].
#[derive(Debug, Clone)]
pub struct SeaBannerRepo {
    db: DatabaseConnection,
}

impl SeaBannerRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_banner(m: banners::Model) -> Banner {
    Banner {
        id: m.id,
        name: m.name,
        position: m.position,
        title: m.title_json.unwrap_or(Value::Null),
        subtitle: m.subtitle_json.unwrap_or(Value::Null),
        image: m.image,
        mobile_image: m.mobile_image,
        link_type: m.link_type,
        link_value: m.link_value,
        open_in_new_tab: m.open_in_new_tab,
        is_active: m.is_active,
        start_at: m.start_at,
        end_at: m.end_at,
        sort_order: m.sort_order,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn opt_json(v: &Value) -> Option<Value> {
    (!v.is_null()).then(|| v.clone())
}

fn banner_alive() -> Condition {
    Condition::all().add(banners::Column::DeletedAt.is_null())
}

#[async_trait]
impl BannerRepo for SeaBannerRepo {
    async fn list(&self, query: &BannerQuery) -> Result<Page<Banner>> {
        let mut q = banners::Entity::find().filter(banner_alive());
        if !query.position.is_empty() {
            q = q.filter(banners::Column::Position.eq(query.position.clone()));
        }
        if let Some(active) = query.is_active {
            q = q.filter(banners::Column::IsActive.eq(active));
        }
        if !query.search.is_empty() {
            q = q.filter(localized_like(
                self.db.get_database_backend(),
                &["name"],
                &["title_json"],
                &query.search,
            ));
        }
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(banners::Column::SortOrder)
            .order_by_desc(banners::Column::CreatedAt)
            .order_by_desc(banners::Column::Id)
            .offset(query.page.offset())
            .limit(query.page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: rows.into_iter().map(to_banner).collect(),
            total,
        })
    }

    async fn list_valid(
        &self,
        position: &str,
        limit: u64,
        now: DateTime<Utc>,
    ) -> Result<Vec<Banner>> {
        let mut q = banners::Entity::find()
            .filter(banner_alive())
            .filter(banners::Column::IsActive.eq(true))
            .filter(
                Condition::any()
                    .add(banners::Column::StartAt.is_null())
                    .add(banners::Column::StartAt.lte(now)),
            )
            .filter(
                Condition::any()
                    .add(banners::Column::EndAt.is_null())
                    .add(banners::Column::EndAt.gte(now)),
            );
        if !position.is_empty() {
            q = q.filter(banners::Column::Position.eq(position));
        }
        if limit > 0 {
            q = q.limit(limit);
        }
        let rows = q
            .order_by_desc(banners::Column::SortOrder)
            .order_by_desc(banners::Column::CreatedAt)
            .order_by_desc(banners::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows.into_iter().map(to_banner).collect())
    }

    async fn get(&self, id: Id) -> Result<Option<Banner>> {
        let row = banners::Entity::find_by_id(id)
            .filter(banner_alive())
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_banner))
    }

    async fn create(&self, b: &Banner) -> Result<Banner> {
        let model = banners::ActiveModel {
            name: Set(b.name.clone()),
            position: Set(b.position.clone()),
            title_json: Set(opt_json(&b.title)),
            subtitle_json: Set(opt_json(&b.subtitle)),
            image: Set(b.image.clone()),
            mobile_image: Set(b.mobile_image.clone()),
            link_type: Set(b.link_type.clone()),
            link_value: Set(b.link_value.clone()),
            open_in_new_tab: Set(b.open_in_new_tab),
            is_active: Set(b.is_active),
            start_at: Set(b.start_at),
            end_at: Set(b.end_at),
            sort_order: Set(b.sort_order),
            created_at: Set(b.created_at),
            updated_at: Set(b.updated_at),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(to_banner(model))
    }

    async fn update(&self, b: &Banner) -> Result<()> {
        banners::ActiveModel {
            id: Set(b.id),
            name: Set(b.name.clone()),
            position: Set(b.position.clone()),
            title_json: Set(opt_json(&b.title)),
            subtitle_json: Set(opt_json(&b.subtitle)),
            image: Set(b.image.clone()),
            mobile_image: Set(b.mobile_image.clone()),
            link_type: Set(b.link_type.clone()),
            link_value: Set(b.link_value.clone()),
            open_in_new_tab: Set(b.open_in_new_tab),
            is_active: Set(b.is_active),
            start_at: Set(b.start_at),
            end_at: Set(b.end_at),
            sort_order: Set(b.sort_order),
            updated_at: Set(b.updated_at),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn delete(&self, id: Id) -> Result<()> {
        banners::Entity::update_many()
            .col_expr(banners::Column::DeletedAt, Expr::value(now()))
            .filter(banners::Column::Id.eq(id))
            .filter(banner_alive())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}

/// SeaORM implementation of [`MediaRepo`].
#[derive(Debug, Clone)]
pub struct SeaMediaRepo {
    db: DatabaseConnection,
}

impl SeaMediaRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_media(m: media::Model) -> Media {
    Media {
        id: m.id,
        name: m.name,
        filename: m.filename,
        path: m.path,
        mime_type: m.mime_type,
        size: m.size,
        scene: m.scene,
        width: m.width,
        height: m.height,
        created_at: m.created_at,
    }
}

fn media_alive() -> Condition {
    Condition::all().add(media::Column::DeletedAt.is_null())
}

#[async_trait]
impl MediaRepo for SeaMediaRepo {
    async fn list(&self, query: &MediaQuery) -> Result<Page<Media>> {
        let mut q = media::Entity::find().filter(media_alive());
        if !query.scene.is_empty() {
            q = q.filter(media::Column::Scene.eq(query.scene.clone()));
        }
        if !query.search.trim().is_empty() {
            q = q.filter(localized_like(
                self.db.get_database_backend(),
                &["name", "filename"],
                &[],
                &query.search,
            ));
        }
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(media::Column::CreatedAt)
            .order_by_desc(media::Column::Id)
            .offset(query.page.offset())
            .limit(query.page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: rows.into_iter().map(to_media).collect(),
            total,
        })
    }

    async fn get(&self, id: Id) -> Result<Option<Media>> {
        let row = media::Entity::find_by_id(id)
            .filter(media_alive())
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_media))
    }

    async fn get_by_path(&self, path: &str) -> Result<Option<Media>> {
        let row = media::Entity::find()
            .filter(media_alive())
            .filter(media::Column::Path.eq(path))
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_media))
    }

    async fn create(&self, m: &NewMedia) -> Result<Media> {
        let model = media::ActiveModel {
            name: Set(m.name.clone()),
            filename: Set(m.filename.clone()),
            path: Set(m.path.clone()),
            mime_type: Set(m.mime_type.clone()),
            size: Set(m.size),
            scene: Set(m.scene.clone()),
            width: Set(m.width),
            height: Set(m.height),
            created_at: Set(now()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(to_media(model))
    }

    async fn rename(&self, id: Id, name: &str) -> Result<()> {
        media::Entity::update_many()
            .col_expr(media::Column::Name, Expr::value(name))
            .filter(media::Column::Id.eq(id))
            .filter(media_alive())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn delete(&self, id: Id) -> Result<()> {
        media::Entity::update_many()
            .col_expr(media::Column::DeletedAt, Expr::value(now()))
            .filter(media::Column::Id.eq(id))
            .filter(media_alive())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}
