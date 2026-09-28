//! [`AdminRepo`] and [`AdminLoginLogRepo`] backed by `admins` / `admin_login_logs`.

use async_trait::async_trait;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Set,
};
use zs_domain::identity::admin::{Admin, AdminLoginLog, AdminLoginLogRepo, AdminRepo, NewAdmin};
use zs_domain::{Id, Result};

use crate::db::entity::{admin_login_logs, admins};
use crate::db::repo::support::{DbResultExt, now};

/// SeaORM implementation of [`AdminRepo`] and [`AdminLoginLogRepo`].
#[derive(Debug, Clone)]
pub struct SeaAdminRepo {
    db: DatabaseConnection,
}

impl SeaAdminRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: admins::Model) -> Admin {
    Admin {
        id: m.id,
        username: m.username,
        password_hash: m.password_hash,
        token_version: m.token_version,
        token_invalid_before: m.token_invalid_before,
        is_super: m.is_super,
        last_login_at: m.last_login_at,
        totp_secret: m.totp_secret,
        totp_enabled_at: m.totp_enabled_at,
        totp_pending_secret: m.totp_pending_secret,
        totp_pending_expires_at: m.totp_pending_expires_at,
        recovery_codes: m.recovery_codes,
        created_at: m.created_at,
    }
}

fn alive() -> sea_orm::Condition {
    sea_orm::Condition::all().add(admins::Column::DeletedAt.is_null())
}

#[async_trait]
impl AdminRepo for SeaAdminRepo {
    async fn get(&self, id: Id) -> Result<Option<Admin>> {
        Ok(admins::Entity::find_by_id(id)
            .filter(alive())
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn get_by_username(&self, username: &str) -> Result<Option<Admin>> {
        let row = admins::Entity::find()
            .filter(alive())
            .filter(admins::Column::Username.eq(username))
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_domain))
    }

    async fn count(&self) -> Result<u64> {
        admins::Entity::find()
            .filter(alive())
            .count(&self.db)
            .await
            .dom()
    }

    async fn list(&self) -> Result<Vec<Admin>> {
        let rows = admins::Entity::find()
            .filter(alive())
            .order_by_asc(admins::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows.into_iter().map(to_domain).collect())
    }

    async fn create(&self, admin: &NewAdmin) -> Result<Admin> {
        let model = admins::ActiveModel {
            username: Set(admin.username.clone()),
            password_hash: Set(admin.password_hash.clone()),
            token_version: Set(0),
            is_super: Set(admin.is_super),
            totp_secret: Set(String::new()),
            totp_pending_secret: Set(String::new()),
            recovery_codes: Set(String::new()),
            created_at: Set(now()),
            ..Default::default()
        };
        Ok(to_domain(model.insert(&self.db).await.dom()?))
    }

    async fn save(&self, a: &Admin) -> Result<()> {
        admins::ActiveModel {
            id: Set(a.id),
            username: Set(a.username.clone()),
            password_hash: Set(a.password_hash.clone()),
            token_version: Set(a.token_version),
            token_invalid_before: Set(a.token_invalid_before),
            is_super: Set(a.is_super),
            last_login_at: Set(a.last_login_at),
            totp_secret: Set(a.totp_secret.clone()),
            totp_enabled_at: Set(a.totp_enabled_at),
            totp_pending_secret: Set(a.totp_pending_secret.clone()),
            totp_pending_expires_at: Set(a.totp_pending_expires_at),
            recovery_codes: Set(a.recovery_codes.clone()),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn replace_recovery_codes(&self, id: Id, expected: &str, codes: &str) -> Result<bool> {
        let res = admins::Entity::update_many()
            .col_expr(admins::Column::RecoveryCodes, Expr::value(codes))
            .filter(admins::Column::Id.eq(id))
            .filter(admins::Column::DeletedAt.is_null())
            .filter(admins::Column::RecoveryCodes.eq(expected))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(res.rows_affected > 0)
    }

    async fn delete(&self, id: Id) -> Result<()> {
        admins::Entity::update_many()
            .col_expr(admins::Column::DeletedAt, Expr::value(now()))
            .filter(admins::Column::Id.eq(id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}

#[async_trait]
impl AdminLoginLogRepo for SeaAdminRepo {
    async fn record(&self, log: AdminLoginLog) -> Result<()> {
        admin_login_logs::ActiveModel {
            admin_id: Set(log.admin_id),
            username: Set(log.username),
            event_type: Set(log.event_type),
            status: Set(log.status),
            fail_reason: Set(log.fail_reason),
            client_ip: Set(log.client_ip),
            user_agent: Set(log.user_agent),
            request_id: Set(log.request_id),
            operator_id: Set(log.operator_id),
            created_at: Set(now()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(())
    }
}
