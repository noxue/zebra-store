//! Admin user management (original `identity/user/transport/http/admin`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use zs_domain::dashboard::users::{
    AdminUserDetail, AdminUserItem, AdminUserRepo, LoginProviders, UserCouponUsage, UserListFilter,
    UserPatch, apply_patch, assemble_coupon_usages, decode_scope_ids, keys, parse_status,
};
use zs_domain::identity::email;
use zs_domain::identity::user::User;
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::identity::password;

/// `PUT /admin/users/:id` request (raw values).
#[derive(Debug, Clone, Default)]
pub struct AdminUserUpdate {
    pub nickname: Option<String>,
    pub locale: Option<String>,
    pub status: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub admin_note: Option<String>,
    pub email_verified: Option<bool>,
}

/// User list, detail, edit, batch status, OAuth unbind and coupon usages.
#[derive(Clone)]
pub struct AdminUserService {
    repo: Arc<dyn AdminUserRepo>,
    providers: Arc<dyn LoginProviders>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for AdminUserService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AdminUserService")
    }
}

fn fetch_failed(e: Error) -> Error {
    e.or_internal(keys::USER_FETCH_FAILED)
}

fn update_failed(e: Error) -> Error {
    e.or_internal(keys::USER_UPDATE_FAILED)
}

impl AdminUserService {
    pub fn new(
        repo: Arc<dyn AdminUserRepo>,
        providers: Arc<dyn LoginProviders>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            providers,
            clock,
        }
    }

    pub async fn list(
        &self,
        filter: &UserListFilter,
        page: PageRequest,
    ) -> Result<Page<AdminUserItem>> {
        let users = self.repo.list(filter, page).await.map_err(fetch_failed)?;
        let ids: Vec<Id> = users.items.iter().map(|u| u.id).collect();
        let balances = self.repo.balances(&ids).await.map_err(fetch_failed)?;
        Ok(users.map(|user| AdminUserItem {
            wallet_balance: balances.get(&user.id).copied().unwrap_or(Amount::ZERO),
            user,
        }))
    }

    async fn user(&self, id: Id) -> Result<User> {
        self.repo
            .get(id)
            .await
            .map_err(fetch_failed)?
            .ok_or_else(|| Error::not_found(keys::USER_NOT_FOUND))
    }

    pub async fn detail(&self, id: Id) -> Result<AdminUserDetail> {
        let user = self.user(id).await?;
        let balances = self.repo.balances(&[id]).await.map_err(fetch_failed)?;
        let oauth_identities = self.repo.identities(id).await.map_err(fetch_failed)?;
        Ok(AdminUserDetail {
            wallet_balance: balances.get(&id).copied().unwrap_or(Amount::ZERO),
            oauth_identities,
            user,
        })
    }

    /// Applies an admin edit; nothing to change is `error.bad_request`.
    pub async fn update(&self, id: Id, req: AdminUserUpdate) -> Result<User> {
        let mut user = self.user(id).await?;
        let mut patch = UserPatch {
            nickname: req.nickname,
            locale: req.locale,
            status: req.status,
            admin_note: req.admin_note,
            email_verified: req.email_verified,
            ..UserPatch::default()
        };
        if let Some(raw) = req.email {
            let normalized = email::normalize(&raw)?;
            let existing = self
                .repo
                .get_by_email(&normalized)
                .await
                .map_err(update_failed)?;
            if existing.is_some_and(|u| u.id != user.id) {
                return Err(Error::bad_request(keys::EMAIL_EXISTS));
            }
            patch.email = Some(normalized);
        }
        if let Some(pw) = req
            .password
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            patch.password_hash = Some(password::hash(pw).await.map_err(update_failed)?);
        }
        let outcome = apply_patch(&mut user, patch, self.clock.now());
        if !outcome.updated {
            return Err(Error::invalid());
        }
        self.repo
            .save_admin_edit(&user)
            .await
            .map_err(update_failed)?;
        user.updated_at = self.clock.now();
        Ok(user)
    }

    /// Sets the status of several users; returns how many ids were requested.
    pub async fn batch_status(&self, ids: &[Id], status: &str) -> Result<usize> {
        if ids.is_empty() {
            return Err(Error::invalid());
        }
        let status = parse_status(status).ok_or_else(Error::invalid)?;
        self.repo
            .batch_status(ids, status, self.clock.now())
            .await
            .map_err(update_failed)?;
        Ok(ids.len())
    }

    /// Removes the `provider` identity when the account keeps a login method.
    pub async fn unbind(&self, id: Id, provider: &str) -> Result<()> {
        let usable = self.providers.usable().await.map_err(update_failed)?;
        self.repo
            .unbind(id, provider, usable)
            .await
            .map_err(update_failed)?
            .into_result(provider)
    }

    pub async fn coupon_usages(
        &self,
        user_id: Id,
        page: PageRequest,
    ) -> Result<Page<UserCouponUsage>> {
        let usages = self
            .repo
            .coupon_usages(user_id, page)
            .await
            .map_err(fetch_failed)?;
        let coupon_ids: Vec<Id> = usages
            .items
            .iter()
            .map(|u| u.coupon_id)
            .filter(|id| *id != 0)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let coupons: HashMap<_, _> = if coupon_ids.is_empty() {
            HashMap::new()
        } else {
            self.repo
                .coupons(&coupon_ids)
                .await
                .map_err(fetch_failed)?
                .into_iter()
                .map(|c| (c.id, c))
                .collect()
        };
        let product_ids: Vec<Id> = coupons
            .values()
            .filter_map(|c| decode_scope_ids(&c.scope_ref_ids))
            .flatten()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let products: HashMap<_, _> = if product_ids.is_empty() {
            HashMap::new()
        } else {
            self.repo
                .products(&product_ids)
                .await
                .map_err(fetch_failed)?
                .into_iter()
                .map(|p| (p.id, p))
                .collect()
        };
        Ok(Page {
            items: assemble_coupon_usages(usages.items, &coupons, &products),
            total: usages.total,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use std::sync::Mutex;
    use zs_domain::dashboard::users::{
        CouponBrief, CouponUsage, OAuthIdentity, ScopeProduct, UnbindOutcome, UsableProviders,
    };
    use zs_shared::clock::FixedClock;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn user(id: Id, email: &str) -> User {
        User {
            id,
            email: email.into(),
            password_hash: String::new(),
            password_setup_required: false,
            display_name: String::new(),
            locale: "zh-CN".into(),
            status: "active".into(),
            member_level_id: 0,
            total_recharged: Amount::ZERO,
            total_spent: Amount::ZERO,
            admin_note: String::new(),
            token_version: 0,
            token_invalid_before: None,
            totp_secret: String::new(),
            totp_enabled_at: None,
            totp_pending_secret: String::new(),
            totp_pending_expires_at: None,
            recovery_codes: String::new(),
            email_verified_at: None,
            last_login_at: None,
            created_at: at("2026-09-01T00:00:00Z"),
            updated_at: at("2026-09-01T00:00:00Z"),
        }
    }

    #[derive(Default)]
    struct Repo {
        users: Mutex<Vec<User>>,
        batches: Mutex<Vec<(Vec<Id>, String)>>,
        usable: Mutex<Option<UsableProviders>>,
    }

    #[async_trait]
    impl AdminUserRepo for Repo {
        async fn list(&self, _: &UserListFilter, _: PageRequest) -> Result<Page<User>> {
            let items = self.users.lock().unwrap().clone();
            Ok(Page {
                total: items.len() as u64,
                items,
            })
        }
        async fn get(&self, id: Id) -> Result<Option<User>> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.id == id)
                .cloned())
        }
        async fn get_by_email(&self, email: &str) -> Result<Option<User>> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.email == email)
                .cloned())
        }
        async fn save_admin_edit(&self, user: &User) -> Result<()> {
            let mut users = self.users.lock().unwrap();
            if let Some(u) = users.iter_mut().find(|u| u.id == user.id) {
                *u = user.clone();
            }
            Ok(())
        }
        async fn batch_status(&self, ids: &[Id], status: &str, _: DateTime<Utc>) -> Result<()> {
            self.batches
                .lock()
                .unwrap()
                .push((ids.to_vec(), status.to_owned()));
            Ok(())
        }
        async fn balances(&self, _: &[Id]) -> Result<HashMap<Id, Amount>> {
            Ok(HashMap::from([(1, Amount::from_cents(123))]))
        }
        async fn identities(&self, _: Id) -> Result<Vec<OAuthIdentity>> {
            Ok(Vec::new())
        }
        async fn unbind(&self, _: Id, _: &str, usable: UsableProviders) -> Result<UnbindOutcome> {
            *self.usable.lock().unwrap() = Some(usable);
            Ok(UnbindOutcome::Locked)
        }
        async fn coupon_usages(&self, _: Id, _: PageRequest) -> Result<Page<CouponUsage>> {
            Ok(Page {
                items: Vec::new(),
                total: 0,
            })
        }
        async fn coupons(&self, _: &[Id]) -> Result<Vec<CouponBrief>> {
            Ok(Vec::new())
        }
        async fn products(&self, _: &[Id]) -> Result<Vec<ScopeProduct>> {
            Ok(Vec::new())
        }
    }

    struct Providers;

    #[async_trait]
    impl LoginProviders for Providers {
        async fn usable(&self) -> Result<UsableProviders> {
            Ok(UsableProviders {
                google: true,
                telegram: false,
            })
        }
    }

    fn service(repo: Arc<Repo>) -> AdminUserService {
        AdminUserService::new(
            repo,
            Arc::new(Providers),
            Arc::new(FixedClock(at("2026-09-24T00:00:00Z"))),
        )
    }

    #[tokio::test]
    async fn update_validates_email_and_revokes_on_disable() {
        let repo = Arc::new(Repo::default());
        repo.users
            .lock()
            .unwrap()
            .extend([user(1, "a@x.com"), user(2, "b@x.com")]);
        let svc = service(repo.clone());
        let taken = AdminUserUpdate {
            email: Some("B@X.com".into()),
            ..AdminUserUpdate::default()
        };
        assert_eq!(
            svc.update(1, taken).await.unwrap_err().key(),
            keys::EMAIL_EXISTS
        );
        let bad = AdminUserUpdate {
            email: Some("nope".into()),
            ..AdminUserUpdate::default()
        };
        assert_eq!(
            svc.update(1, bad).await.unwrap_err().key(),
            "error.email_invalid"
        );
        assert_eq!(
            svc.update(1, AdminUserUpdate::default())
                .await
                .unwrap_err()
                .key(),
            "error.bad_request"
        );
        assert_eq!(
            svc.update(9, AdminUserUpdate::default())
                .await
                .unwrap_err()
                .key(),
            keys::USER_NOT_FOUND
        );
        let disable = AdminUserUpdate {
            status: Some("disabled".into()),
            ..AdminUserUpdate::default()
        };
        let u = svc.update(1, disable).await.unwrap();
        assert_eq!(u.token_version, 1);
        assert_eq!(repo.users.lock().unwrap()[0].token_version, 1);
    }

    #[tokio::test]
    async fn batch_status_validation() {
        let repo = Arc::new(Repo::default());
        let svc = service(repo.clone());
        assert!(svc.batch_status(&[], "active").await.is_err());
        assert!(svc.batch_status(&[1], "banned").await.is_err());
        assert_eq!(svc.batch_status(&[1, 2], " DISABLED ").await.unwrap(), 2);
        assert_eq!(
            repo.batches.lock().unwrap()[0],
            (vec![1, 2], "disabled".to_owned())
        );
    }

    #[tokio::test]
    async fn unbind_passes_usable_providers_and_maps_errors() {
        let repo = Arc::new(Repo::default());
        let svc = service(repo.clone());
        let err = svc.unbind(1, "google").await.unwrap_err();
        assert_eq!(err.key(), keys::GOOGLE_UNBIND_LOCKED);
        assert_eq!(
            *repo.usable.lock().unwrap(),
            Some(UsableProviders {
                google: true,
                telegram: false
            })
        );
    }

    #[tokio::test]
    async fn list_defaults_missing_balances_to_zero() {
        let repo = Arc::new(Repo::default());
        repo.users
            .lock()
            .unwrap()
            .extend([user(1, "a@x.com"), user(2, "b@x.com")]);
        let page = service(repo)
            .list(&UserListFilter::default(), PageRequest::default())
            .await
            .unwrap();
        assert_eq!(page.items[0].wallet_balance, Amount::from_cents(123));
        assert_eq!(page.items[1].wallet_balance, Amount::ZERO);
    }
}
