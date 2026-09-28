//! [`VerifyCodeRepo`] backed by `email_verify_codes`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use zs_domain::identity::verify_code::{NewVerifyCode, Purpose, VerifyCode, VerifyCodeRepo};
use zs_domain::{Id, Result};

use crate::db::entity::email_verify_codes as codes;
use crate::db::repo::support::{DbResultExt, now};

/// SeaORM implementation of [`VerifyCodeRepo`].
#[derive(Debug, Clone)]
pub struct SeaVerifyCodeRepo {
    db: DatabaseConnection,
}

impl SeaVerifyCodeRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: codes::Model) -> VerifyCode {
    VerifyCode {
        id: m.id,
        email: m.email,
        user_id: m.user_id,
        purpose: m.purpose,
        code: m.code,
        expires_at: m.expires_at,
        verified_at: m.verified_at,
        attempt_count: m.attempt_count,
        sent_at: m.sent_at,
    }
}

#[async_trait]
impl VerifyCodeRepo for SeaVerifyCodeRepo {
    async fn latest(&self, email: &str, purpose: Purpose) -> Result<Option<VerifyCode>> {
        Ok(codes::Entity::find()
            .filter(codes::Column::Email.eq(email))
            .filter(codes::Column::Purpose.eq(purpose.as_str()))
            .filter(codes::Column::DeletedAt.is_null())
            .order_by_desc(codes::Column::SentAt)
            .order_by_desc(codes::Column::Id)
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn create(&self, c: &NewVerifyCode) -> Result<()> {
        codes::ActiveModel {
            email: Set(c.email.clone()),
            user_id: Set(c.user_id),
            purpose: Set(c.purpose.as_str().to_owned()),
            code: Set(c.code.clone()),
            expires_at: Set(c.expires_at),
            verified_at: Set(None),
            attempt_count: Set(0),
            sent_at: Set(c.sent_at),
            created_at: Set(now()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn mark_verified(&self, id: Id, at: DateTime<Utc>) -> Result<bool> {
        let res = codes::Entity::update_many()
            .col_expr(codes::Column::VerifiedAt, Expr::value(at))
            .filter(codes::Column::Id.eq(id))
            .filter(codes::Column::DeletedAt.is_null())
            .filter(codes::Column::VerifiedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(res.rows_affected > 0)
    }

    async fn increment_attempt(&self, id: Id) -> Result<()> {
        codes::Entity::update_many()
            .col_expr(
                codes::Column::AttemptCount,
                Expr::col(codes::Column::AttemptCount).add(1),
            )
            .filter(codes::Column::Id.eq(id))
            .filter(codes::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}
