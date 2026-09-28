//! [`UserRepo`] backed by `users`.

use async_trait::async_trait;
use sea_orm::sea_query::Expr;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use zs_domain::identity::user::{NewUser, STATUS_ACTIVE, User, UserRepo};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;

use crate::db::entity::users;
use crate::db::repo::support::{DbResultExt, now};

/// SeaORM implementation of [`UserRepo`].
#[derive(Debug, Clone)]
pub struct SeaUserRepo {
    db: DatabaseConnection,
}

impl SeaUserRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

/// Maps a `users` row to the domain model.
pub fn to_domain(m: users::Model) -> User {
    User {
        id: m.id,
        email: m.email,
        password_hash: m.password_hash,
        password_setup_required: m.password_setup_required,
        display_name: m.display_name,
        locale: m.locale,
        status: m.status,
        member_level_id: m.member_level_id,
        total_recharged: Amount::new(m.total_recharged),
        total_spent: Amount::new(m.total_spent),
        admin_note: m.admin_note,
        token_version: m.token_version,
        token_invalid_before: m.token_invalid_before,
        totp_secret: m.totp_secret,
        totp_enabled_at: m.totp_enabled_at,
        totp_pending_secret: m.totp_pending_secret,
        totp_pending_expires_at: m.totp_pending_expires_at,
        recovery_codes: m.recovery_codes,
        email_verified_at: m.email_verified_at,
        last_login_at: m.last_login_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

/// Insert model of a new active user (shared with transactional creators).
pub fn new_user_model(u: &NewUser) -> users::ActiveModel {
    let now = now();
    users::ActiveModel {
        email: Set(u.email.trim().to_lowercase()),
        password_hash: Set(u.password_hash.clone()),
        password_setup_required: Set(u.password_setup_required),
        display_name: Set(u.display_name.clone()),
        locale: Set(u.locale.clone()),
        status: Set(STATUS_ACTIVE.into()),
        member_level_id: Set(u.member_level_id),
        total_recharged: Set(Amount::ZERO.decimal()),
        total_spent: Set(Amount::ZERO.decimal()),
        admin_note: Set(String::new()),
        token_version: Set(0),
        totp_secret: Set(String::new()),
        totp_pending_secret: Set(String::new()),
        recovery_codes: Set(String::new()),
        email_verified_at: Set(u.email_verified_at),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
}

#[async_trait]
impl UserRepo for SeaUserRepo {
    async fn get(&self, id: Id) -> Result<Option<User>> {
        let row = users::Entity::find_by_id(id)
            .filter(users::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_domain))
    }

    async fn get_by_email(&self, email: &str) -> Result<Option<User>> {
        let row = users::Entity::find()
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::Email.eq(email.trim().to_lowercase()))
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_domain))
    }

    async fn create(&self, u: &NewUser) -> Result<User> {
        Ok(to_domain(new_user_model(u).insert(&self.db).await.dom()?))
    }

    async fn save(&self, u: &User) -> Result<()> {
        users::ActiveModel {
            id: Set(u.id),
            email: Set(u.email.clone()),
            password_hash: Set(u.password_hash.clone()),
            password_setup_required: Set(u.password_setup_required),
            display_name: Set(u.display_name.clone()),
            locale: Set(u.locale.clone()),
            status: Set(u.status.clone()),
            admin_note: Set(u.admin_note.clone()),
            token_version: Set(u.token_version),
            token_invalid_before: Set(u.token_invalid_before),
            totp_secret: Set(u.totp_secret.clone()),
            totp_enabled_at: Set(u.totp_enabled_at),
            totp_pending_secret: Set(u.totp_pending_secret.clone()),
            totp_pending_expires_at: Set(u.totp_pending_expires_at),
            recovery_codes: Set(u.recovery_codes.clone()),
            email_verified_at: Set(u.email_verified_at),
            last_login_at: Set(u.last_login_at),
            updated_at: Set(now()),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn replace_recovery_codes(&self, id: Id, expected: &str, codes: &str) -> Result<bool> {
        let res = users::Entity::update_many()
            .col_expr(users::Column::RecoveryCodes, Expr::value(codes))
            .col_expr(users::Column::UpdatedAt, Expr::value(now()))
            .filter(users::Column::Id.eq(id))
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::RecoveryCodes.eq(expected))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(res.rows_affected > 0)
    }
}
