//! [`RuleRepo`] backed by `casbin_rule`.

use async_trait::async_trait;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use zs_domain::Result;
use zs_domain::authz::{Rule, RuleRepo};

use crate::db::entity::casbin_rule;
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`RuleRepo`].
#[derive(Debug, Clone)]
pub struct SeaRuleRepo {
    db: DatabaseConnection,
}

impl SeaRuleRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    fn matching(rule: &Rule) -> sea_orm::Select<casbin_rule::Entity> {
        casbin_rule::Entity::find()
            .filter(casbin_rule::Column::Ptype.eq(&rule.ptype))
            .filter(casbin_rule::Column::V0.eq(&rule.v0))
            .filter(casbin_rule::Column::V1.eq(&rule.v1))
            .filter(casbin_rule::Column::V2.eq(&rule.v2))
    }
}

#[async_trait]
impl RuleRepo for SeaRuleRepo {
    async fn load(&self) -> Result<Vec<Rule>> {
        let rows = casbin_rule::Entity::find().all(&self.db).await.dom()?;
        Ok(rows
            .into_iter()
            .map(|r| Rule {
                ptype: r.ptype,
                v0: r.v0,
                v1: r.v1,
                v2: r.v2,
            })
            .collect())
    }

    async fn add(&self, rule: &Rule) -> Result<bool> {
        if Self::matching(rule).one(&self.db).await.dom()?.is_some() {
            return Ok(false);
        }
        casbin_rule::ActiveModel {
            ptype: Set(rule.ptype.clone()),
            v0: Set(rule.v0.clone()),
            v1: Set(rule.v1.clone()),
            v2: Set(rule.v2.clone()),
            v3: Set(String::new()),
            v4: Set(String::new()),
            v5: Set(String::new()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(true)
    }

    async fn remove(&self, rule: &Rule) -> Result<bool> {
        let res = casbin_rule::Entity::delete_many()
            .filter(casbin_rule::Column::Ptype.eq(&rule.ptype))
            .filter(casbin_rule::Column::V0.eq(&rule.v0))
            .filter(casbin_rule::Column::V1.eq(&rule.v1))
            .filter(casbin_rule::Column::V2.eq(&rule.v2))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(res.rows_affected > 0)
    }

    async fn remove_by_v0(&self, ptype: &str, v0: &str) -> Result<u64> {
        let res = casbin_rule::Entity::delete_many()
            .filter(casbin_rule::Column::Ptype.eq(ptype))
            .filter(casbin_rule::Column::V0.eq(v0))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(res.rows_affected)
    }

    async fn remove_by_v1(&self, ptype: &str, v1: &str) -> Result<u64> {
        let res = casbin_rule::Entity::delete_many()
            .filter(casbin_rule::Column::Ptype.eq(ptype))
            .filter(casbin_rule::Column::V1.eq(v1))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(res.rows_affected)
    }
}
