//! [`SiteConfigRepo`]: `reseller_site_configs` (one live row per reseller).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait,
};
use zs_domain::reseller::SiteConfig;
use zs_domain::reseller::ports::{SiteConfigFilter, SiteConfigRepo};
use zs_domain::reseller::site::SiteConfigDraft;
use zs_domain::{Id, Result};
use zs_shared::page::{Page, PageRequest};

use super::{SeaResellerStore, keyword_profile_ids, load_profiles, site_model};
use crate::db::entity::reseller_site_configs as sites;
use crate::db::repo::catalog::sql::ilike_sql;
use crate::db::repo::support::DbResultExt;

#[async_trait]
impl SiteConfigRepo for SeaResellerStore {
    async fn site_config(&self, reseller_id: Id) -> Result<Option<SiteConfig>> {
        let Some(row) = sites::Entity::find()
            .filter(sites::Column::ResellerId.eq(reseller_id))
            .filter(sites::Column::DeletedAt.is_null())
            .order_by_desc(sites::Column::Id)
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let profile = load_profiles(&self.db, &[reseller_id])
            .await?
            .remove(&reseller_id);
        Ok(Some(site_model(row, profile)))
    }

    async fn upsert_site_config(
        &self,
        reseller_id: Id,
        d: &SiteConfigDraft,
        now: DateTime<Utc>,
    ) -> Result<SiteConfig> {
        let txn = self.db.begin().await.dom()?;
        let existing = sites::Entity::find()
            .filter(sites::Column::ResellerId.eq(reseller_id))
            .order_by_asc(sites::Column::DeletedAt.is_not_null())
            .order_by_desc(sites::Column::Id)
            .lock_exclusive()
            .one(&txn)
            .await
            .dom()?;
        let mut model = sites::ActiveModel {
            reseller_id: Set(reseller_id),
            site_name: Set(d.site_name.clone()),
            logo: Set(d.logo.clone()),
            favicon: Set(d.favicon.clone()),
            announcement_json: Set(Some(d.announcement.clone())),
            support_json: Set(Some(d.support.clone())),
            seo_json: Set(Some(d.seo.clone())),
            footer_links_json: Set(Some(d.footer_links.clone())),
            nav_config_json: Set(Some(d.nav_config.clone())),
            theme_json: Set(Some(d.theme.clone())),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        };
        let row = match existing {
            Some(row) => {
                model.id = Set(row.id);
                model.update(&txn).await.dom()?
            }
            None => {
                model.created_at = Set(now);
                model.insert(&txn).await.dom()?
            }
        };
        txn.commit().await.dom()?;
        let profile = load_profiles(&self.db, &[reseller_id])
            .await?
            .remove(&reseller_id);
        Ok(site_model(row, profile))
    }

    async fn delete_site_config(&self, reseller_id: Id, now: DateTime<Utc>) -> Result<()> {
        sites::Entity::update_many()
            .col_expr(sites::Column::DeletedAt, Expr::value(now))
            .col_expr(sites::Column::UpdatedAt, Expr::value(now))
            .filter(sites::Column::ResellerId.eq(reseller_id))
            .filter(sites::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn list_site_configs(
        &self,
        f: &SiteConfigFilter,
        page: PageRequest,
    ) -> Result<Page<SiteConfig>> {
        let mut cond = Condition::all().add(sites::Column::DeletedAt.is_null());
        if let Some(rid) = f.reseller_id {
            cond = cond.add(sites::Column::ResellerId.eq(rid));
        }
        let keyword = f.keyword.trim();
        if !keyword.is_empty() {
            cond = cond.add(Condition::any().add(ilike_sql("site_name", keyword)).add(
                sites::Column::ResellerId.is_in(keyword_profile_ids(&self.db, keyword).await?),
            ));
        }
        if let Some(from) = f.created_from {
            cond = cond.add(sites::Column::CreatedAt.gte(from));
        }
        if let Some(to) = f.created_to {
            cond = cond.add(sites::Column::CreatedAt.lte(to));
        }
        let q = sites::Entity::find().filter(cond);
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(sites::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        let ids: Vec<Id> = rows.iter().map(|r| r.reseller_id).collect();
        let profiles = load_profiles(&self.db, &ids).await?;
        let items = rows
            .into_iter()
            .map(|r| {
                let p = profiles.get(&r.reseller_id).cloned();
                site_model(r, p)
            })
            .collect();
        Ok(Page { items, total })
    }
}
