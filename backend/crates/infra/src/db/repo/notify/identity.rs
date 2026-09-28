//! [`ChannelIdentityRepo`] over `users`, `user_oauth_identities`,
//! `member_levels` and `registration_config` (consumer-side port).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use zs_domain::notify::channel::{ChannelIdentityRepo, ChannelUser, NewChannelUser, OAuthIdentity};
use zs_domain::settings::schema::value::parse_bool;
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Id, Result};

use crate::db::entity::{member_levels, user_oauth_identities, users};
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`ChannelIdentityRepo`].
#[derive(Clone)]
pub struct SeaChannelIdentityRepo {
    db: DatabaseConnection,
    settings: Arc<dyn SettingsStore>,
}

impl std::fmt::Debug for SeaChannelIdentityRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SeaChannelIdentityRepo")
    }
}

impl SeaChannelIdentityRepo {
    pub fn new(db: DatabaseConnection, settings: Arc<dyn SettingsStore>) -> Self {
        Self { db, settings }
    }

    async fn default_level_id(&self) -> Result<Id> {
        Ok(member_levels::Entity::find()
            .filter(member_levels::Column::IsDefault.eq(true))
            .filter(member_levels::Column::IsActive.eq(true))
            .filter(member_levels::Column::DeletedAt.is_null())
            .order_by_asc(member_levels::Column::SortOrder)
            .order_by_asc(member_levels::Column::Id)
            .one(&self.db)
            .await
            .dom()?
            .map_or(0, |l| l.id))
    }
}

fn user_to_domain(m: users::Model) -> ChannelUser {
    ChannelUser {
        id: m.id,
        email: m.email,
        display_name: m.display_name,
        status: m.status,
        locale: m.locale,
        email_verified: m.email_verified_at.is_some(),
        password_setup_required: m.password_setup_required,
        member_level_id: m.member_level_id,
    }
}

fn identity_to_domain(m: user_oauth_identities::Model) -> OAuthIdentity {
    OAuthIdentity {
        id: m.id,
        user_id: m.user_id,
        provider: m.provider,
        provider_user_id: m.provider_user_id,
        username: m.username,
        avatar_url: m.avatar_url,
        auth_at: m.auth_at,
    }
}

fn alive_users() -> sea_orm::Select<users::Entity> {
    users::Entity::find().filter(users::Column::DeletedAt.is_null())
}

#[async_trait]
impl ChannelIdentityRepo for SeaChannelIdentityRepo {
    async fn user_by_id(&self, id: Id) -> Result<Option<ChannelUser>> {
        let row = alive_users()
            .filter(users::Column::Id.eq(id))
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(user_to_domain))
    }

    async fn user_by_email(&self, email: &str) -> Result<Option<ChannelUser>> {
        let row = alive_users()
            .filter(users::Column::Email.eq(email.trim().to_lowercase()))
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(user_to_domain))
    }

    async fn create_user(&self, u: &NewChannelUser, now: DateTime<Utc>) -> Result<ChannelUser> {
        // a551e8f8: Telegram users get the default member level like any registration.
        let level = self.default_level_id().await?;
        let row = users::ActiveModel {
            email: Set(u.email.trim().to_lowercase()),
            password_hash: Set(u.password_hash.clone()),
            password_setup_required: Set(true),
            display_name: Set(u.display_name.clone()),
            locale: Set(zs_shared::i18n::DEFAULT_LOCALE.into()),
            status: Set("active".into()),
            member_level_id: Set(level),
            total_recharged: Set(Decimal::ZERO),
            total_spent: Set(Decimal::ZERO),
            admin_note: Set(String::new()),
            token_version: Set(0),
            totp_secret: Set(String::new()),
            totp_pending_secret: Set(String::new()),
            recovery_codes: Set(String::new()),
            last_login_at: Set(Some(now)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(user_to_domain(row))
    }

    async fn identity_by_provider_user(
        &self,
        provider: &str,
        provider_user_id: &str,
    ) -> Result<Option<OAuthIdentity>> {
        let row = user_oauth_identities::Entity::find()
            .filter(user_oauth_identities::Column::Provider.eq(provider))
            .filter(user_oauth_identities::Column::ProviderUserId.eq(provider_user_id))
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(identity_to_domain))
    }

    async fn identity_by_user(&self, user_id: Id, provider: &str) -> Result<Option<OAuthIdentity>> {
        let row = user_oauth_identities::Entity::find()
            .filter(user_oauth_identities::Column::UserId.eq(user_id))
            .filter(user_oauth_identities::Column::Provider.eq(provider))
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(identity_to_domain))
    }

    async fn create_identity(
        &self,
        i: &OAuthIdentity,
        now: DateTime<Utc>,
    ) -> Result<OAuthIdentity> {
        let row = user_oauth_identities::ActiveModel {
            user_id: Set(i.user_id),
            provider: Set(i.provider.clone()),
            provider_user_id: Set(i.provider_user_id.clone()),
            username: Set(i.username.clone()),
            avatar_url: Set(i.avatar_url.clone()),
            auth_at: Set(i.auth_at),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(identity_to_domain(row))
    }

    async fn update_identity(&self, i: &OAuthIdentity, now: DateTime<Utc>) -> Result<()> {
        use user_oauth_identities::Column as C;
        user_oauth_identities::Entity::update_many()
            .col_expr(C::UserId, Expr::value(i.user_id))
            .col_expr(C::Username, Expr::value(i.username.clone()))
            .col_expr(C::AvatarUrl, Expr::value(i.avatar_url.clone()))
            .col_expr(C::AuthAt, Expr::value(i.auth_at))
            .col_expr(C::UpdatedAt, Expr::value(now))
            .filter(C::Id.eq(i.id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn registration_enabled(&self) -> Result<bool> {
        let raw = self.settings.get(setting_keys::REGISTRATION_CONFIG).await?;
        Ok(raw
            .as_ref()
            .and_then(|v| v.get("registration_enabled"))
            .is_none_or(|v| parse_bool(Some(v))))
    }
}
