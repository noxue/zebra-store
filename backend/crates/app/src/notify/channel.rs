//! Channel API use cases that do not depend on orders: Telegram identity
//! resolution/provisioning/binding (original `userauth/telegram_channel.go`)
//! and the bot configuration / heartbeat (original `channelbot`).

use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use rand::Rng;
use serde_json::{Value, json};
use zs_domain::identity::email;
use zs_domain::notify::channel::{
    BindCodeVerifier, ChannelIdentityRepo, ChannelLookup, ChannelUser, NewChannelUser,
    OAuthIdentity, PROVIDER_TELEGRAM, PaymentMethod, TelegramIdentityInput,
    allowed_channel_intersection, identity_keys, placeholder_email, telegram_display_name,
};
use zs_domain::settings::schema::site::WalletSetting;
use zs_domain::settings::schema::telegram_bot::{
    TelegramBotRuntimeStatus, TelegramBotSetting, ensure_builtin_menu,
};
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;

use super::clients::ChannelClientService;

/// Channels listed by default (original `ListChannels(page 1, size 50)`).
const DEFAULT_CHANNEL_LIMIT: u64 = 50;
/// Channels considered for product / recharge filtering (original page size 200).
const FILTER_CHANNEL_LIMIT: u64 = 200;
/// Provider types never offered as an online payment method.
const INTERNAL_PROVIDERS: [&str; 2] = ["balance", "wallet"];

/// Digits of the random suffix of a Telegram user's initial password seed.
const PASSWORD_SEED_DIGITS: usize = 16;

/// A resolved identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub user: ChannelUser,
    pub identity: OAuthIdentity,
    /// Whether provisioning created the user.
    pub created: bool,
    /// Placeholder user the identity was moved away from (bind only; 0 = none).
    pub previous_user_id: Id,
}

/// Heartbeat reported by the bot.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct Heartbeat {
    pub bot_version: String,
    pub webhook_status: String,
    pub machine_code: String,
    pub license_status: String,
    pub license_expires_at: String,
    pub warnings: Vec<String>,
}

/// Channel identity and bot runtime service (cheap to clone).
#[derive(Clone)]
pub struct ChannelService {
    repo: Arc<dyn ChannelIdentityRepo>,
    codes: Arc<dyn BindCodeVerifier>,
    settings: Arc<dyn SettingsStore>,
    clients: ChannelClientService,
    lookup: Arc<dyn ChannelLookup>,
    clock: Arc<dyn Clock>,
}

/// Payment methods offered to the bot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentMethods {
    pub items: Vec<PaymentMethod>,
    pub wallet_only_payment: bool,
}

impl std::fmt::Debug for ChannelService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChannelService")
    }
}

struct Verified {
    provider_user_id: String,
    username: String,
    avatar_url: String,
    first_name: String,
    last_name: String,
    auth_at: DateTime<Utc>,
}

fn apply(v: &Verified, identity: &mut OAuthIdentity) {
    if identity.provider.is_empty() {
        identity.provider = PROVIDER_TELEGRAM.into();
    }
    if identity.provider_user_id.is_empty() {
        identity.provider_user_id.clone_from(&v.provider_user_id);
    }
    identity.username.clone_from(&v.username);
    identity.avatar_url.clone_from(&v.avatar_url);
    identity.auth_at = Some(v.auth_at);
}

fn random_digits(n: usize) -> String {
    let mut rng = rand::rng();
    (0..n)
        .map(|_| char::from(b'0' + rng.random_range(0..10u8)))
        .collect()
}

impl ChannelService {
    pub fn new(
        repo: Arc<dyn ChannelIdentityRepo>,
        codes: Arc<dyn BindCodeVerifier>,
        settings: Arc<dyn SettingsStore>,
        clients: ChannelClientService,
        lookup: Arc<dyn ChannelLookup>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            codes,
            settings,
            clients,
            lookup,
            clock,
        }
    }

    /// Member level of a bound Telegram user (0 when unbound or on error).
    pub async fn member_level_of(&self, channel_user_id: &str) -> Id {
        if channel_user_id.trim().is_empty() {
            return 0;
        }
        let input = TelegramIdentityInput {
            channel_user_id: channel_user_id.to_owned(),
            ..TelegramIdentityInput::default()
        };
        match self.resolve(&input).await {
            Ok(Some(r)) => r.user.member_level_id,
            _ => 0,
        }
    }

    /// Slug of a live product (channel product detail is addressed by id).
    pub async fn product_slug(&self, id: Id) -> Result<Option<String>> {
        self.lookup.product_slug(id).await
    }

    /// Active products per category.
    pub async fn category_product_counts(&self) -> Result<std::collections::HashMap<Id, i64>> {
        self.lookup.category_product_counts().await
    }

    /// Payment methods for top-ups (`context=recharge`), an order's products
    /// (`order_no` + channel user) or everything active.
    pub async fn payment_methods(
        &self,
        context: &str,
        order_no: &str,
        channel_user_id: &str,
    ) -> Result<PaymentMethods> {
        let raw = self.settings.get(setting_keys::WALLET_CONFIG).await?;
        let wallet = WalletSetting::decode(raw.as_ref());
        let channels = if context.trim() == "recharge" {
            let all = self
                .lookup
                .active_payment_channels(FILTER_CHANNEL_LIMIT)
                .await?;
            if wallet.recharge_channel_ids.is_empty() {
                all
            } else {
                all.into_iter()
                    .filter(|c| wallet.recharge_channel_ids.contains(&c.id))
                    .collect()
            }
        } else if !order_no.trim().is_empty() && !channel_user_id.trim().is_empty() {
            self.order_channels(order_no, channel_user_id, wallet.wallet_only_payment)
                .await?
        } else {
            self.lookup
                .active_payment_channels(DEFAULT_CHANNEL_LIMIT)
                .await?
        };
        Ok(PaymentMethods {
            items: channels
                .into_iter()
                .filter(|c| !INTERNAL_PROVIDERS.contains(&c.provider_type.as_str()))
                .collect(),
            wallet_only_payment: wallet.wallet_only_payment,
        })
    }

    async fn order_channels(
        &self,
        order_no: &str,
        channel_user_id: &str,
        wallet_only: bool,
    ) -> Result<Vec<PaymentMethod>> {
        let input = TelegramIdentityInput {
            channel_user_id: channel_user_id.to_owned(),
            ..TelegramIdentityInput::default()
        };
        let user_id = match self.provision_user_id(&input).await {
            Ok(id) => id,
            Err(error) => {
                tracing::warn!(%error, "channel payment channels: resolve user failed");
                return self
                    .lookup
                    .active_payment_channels(DEFAULT_CHANNEL_LIMIT)
                    .await;
            }
        };
        let Some(product_ids) = self.lookup.order_product_ids(user_id, order_no).await? else {
            return self
                .lookup
                .active_payment_channels(DEFAULT_CHANNEL_LIMIT)
                .await;
        };
        if wallet_only {
            return Ok(Vec::new());
        }
        let all = self
            .lookup
            .active_payment_channels(FILTER_CHANNEL_LIMIT)
            .await?;
        let raw = self
            .lookup
            .product_payment_channel_ids(&product_ids)
            .await?;
        Ok(match allowed_channel_intersection(&raw) {
            None => all,
            Some(allowed) => all
                .into_iter()
                .filter(|c| allowed.contains(&c.id))
                .collect(),
        })
    }

    fn verified(&self, input: &TelegramIdentityInput) -> Result<Verified> {
        let id = input.channel_user_id.trim();
        if id.is_empty() {
            return Err(Error::invalid());
        }
        Ok(Verified {
            provider_user_id: id.to_owned(),
            username: input.username.trim().to_owned(),
            avatar_url: input.avatar_url.trim().to_owned(),
            first_name: input.first_name.trim().to_owned(),
            last_name: input.last_name.trim().to_owned(),
            auth_at: self.clock.now(),
        })
    }

    async fn active_user(&self, id: Id) -> Result<ChannelUser> {
        let user = self
            .repo
            .user_by_id(id)
            .await?
            .ok_or_else(|| Error::not_found(identity_keys::USER_NOT_FOUND))?;
        if !user.is_active() {
            return Err(Error::unauthorized(identity_keys::USER_DISABLED));
        }
        Ok(user)
    }

    async fn resolve_verified(&self, v: &Verified) -> Result<Option<(ChannelUser, OAuthIdentity)>> {
        let Some(mut identity) = self
            .repo
            .identity_by_provider_user(PROVIDER_TELEGRAM, &v.provider_user_id)
            .await?
        else {
            return Ok(None);
        };
        let user = self.active_user(identity.user_id).await?;
        apply(v, &mut identity);
        self.repo
            .update_identity(&identity, self.clock.now())
            .await?;
        Ok(Some((user, identity)))
    }

    /// Finds the shop user bound to a Telegram id (`None` when unbound).
    pub async fn resolve(&self, input: &TelegramIdentityInput) -> Result<Option<Resolved>> {
        let v = self.verified(input)?;
        Ok(self
            .resolve_verified(&v)
            .await?
            .map(|(user, identity)| Resolved {
                user,
                identity,
                created: false,
                previous_user_id: 0,
            }))
    }

    async fn find_or_create_user(&self, v: &Verified) -> Result<ChannelUser> {
        let email = placeholder_email(&v.provider_user_id);
        if let Some(user) = self.repo.user_by_email(&email).await? {
            if !user.is_active() {
                return Err(Error::unauthorized(identity_keys::USER_DISABLED));
            }
            return Ok(user);
        }
        if !self.repo.registration_enabled().await? {
            return Err(Error::forbidden(identity_keys::REGISTRATION_DISABLED));
        }
        let seed = format!(
            "tg_{}_{}",
            v.provider_user_id,
            random_digits(PASSWORD_SEED_DIGITS)
        );
        let password_hash = crate::identity::password::hash(&seed).await?;
        self.repo
            .create_user(
                &NewChannelUser {
                    email,
                    password_hash,
                    display_name: telegram_display_name(
                        &v.provider_user_id,
                        &v.username,
                        &v.first_name,
                        &v.last_name,
                    ),
                },
                self.clock.now(),
            )
            .await
    }

    /// Resolves or creates the placeholder account of a Telegram id.
    pub async fn provision(&self, input: &TelegramIdentityInput) -> Result<Resolved> {
        let v = self.verified(input)?;
        if let Some((user, identity)) = self.resolve_verified(&v).await? {
            return Ok(Resolved {
                user,
                identity,
                created: false,
                previous_user_id: 0,
            });
        }
        let created = self
            .repo
            .user_by_email(&placeholder_email(&v.provider_user_id))
            .await?
            .is_none();
        let user = self.find_or_create_user(&v).await?;
        let now = self.clock.now();
        if let Some(mut identity) = self
            .repo
            .identity_by_user(user.id, PROVIDER_TELEGRAM)
            .await?
        {
            if identity.provider_user_id != v.provider_user_id {
                return Err(Error::bad_request(identity_keys::ALREADY_BOUND));
            }
            apply(&v, &mut identity);
            self.repo.update_identity(&identity, now).await?;
            return Ok(Resolved {
                user,
                identity,
                created,
                previous_user_id: 0,
            });
        }
        let mut identity = OAuthIdentity {
            id: 0,
            user_id: user.id,
            provider: PROVIDER_TELEGRAM.into(),
            provider_user_id: v.provider_user_id.clone(),
            username: String::new(),
            avatar_url: String::new(),
            auth_at: None,
        };
        apply(&v, &mut identity);
        match self.repo.create_identity(&identity, now).await {
            Ok(identity) => Ok(Resolved {
                user,
                identity,
                created,
                previous_user_id: 0,
            }),
            Err(error) => {
                // Lost a race on the unique key: return the winner's binding.
                let Some(existing) = self
                    .repo
                    .identity_by_provider_user(PROVIDER_TELEGRAM, &v.provider_user_id)
                    .await?
                else {
                    return Err(error);
                };
                let user = self.active_user(existing.user_id).await?;
                Ok(Resolved {
                    user,
                    identity: existing,
                    created: false,
                    previous_user_id: 0,
                })
            }
        }
    }

    /// Shop user id of a Telegram id, provisioning it when needed.
    pub async fn provision_user_id(&self, input: &TelegramIdentityInput) -> Result<Id> {
        Ok(self.provision(input).await?.user.id)
    }

    /// Binds a Telegram id to an existing account proven by an email code; a
    /// binding held by a Telegram placeholder account is moved over.
    pub async fn bind(
        &self,
        input: &TelegramIdentityInput,
        email_raw: &str,
        code: &str,
    ) -> Result<Resolved> {
        let v = self.verified(input)?;
        let email = email::normalize(email_raw)?;
        self.codes.verify(&email, code).await?;
        let target = self
            .repo
            .user_by_email(&email)
            .await?
            .ok_or_else(|| Error::not_found(identity_keys::USER_NOT_FOUND))?;
        if !target.is_active() {
            return Err(Error::unauthorized(identity_keys::USER_DISABLED));
        }
        let now = self.clock.now();
        if let Some(current) = self
            .repo
            .identity_by_user(target.id, PROVIDER_TELEGRAM)
            .await?
            && current.provider_user_id != v.provider_user_id
        {
            return Err(Error::bad_request(identity_keys::ALREADY_BOUND));
        }
        let occupied = self
            .repo
            .identity_by_provider_user(PROVIDER_TELEGRAM, &v.provider_user_id)
            .await?;
        match occupied {
            Some(mut identity) if identity.user_id == target.id => {
                apply(&v, &mut identity);
                self.repo.update_identity(&identity, now).await?;
                Ok(Resolved {
                    user: target,
                    identity,
                    created: false,
                    previous_user_id: 0,
                })
            }
            Some(mut identity) => {
                let previous = self.repo.user_by_id(identity.user_id).await?;
                if !previous
                    .as_ref()
                    .is_some_and(|p| email::is_placeholder(&p.email))
                {
                    return Err(Error::bad_request(identity_keys::BIND_CONFLICT));
                }
                let previous_user_id = identity.user_id;
                identity.user_id = target.id;
                apply(&v, &mut identity);
                self.repo.update_identity(&identity, now).await?;
                Ok(Resolved {
                    user: target,
                    identity,
                    created: false,
                    previous_user_id,
                })
            }
            None => {
                let mut identity = OAuthIdentity {
                    id: 0,
                    user_id: target.id,
                    provider: PROVIDER_TELEGRAM.into(),
                    provider_user_id: v.provider_user_id.clone(),
                    username: String::new(),
                    avatar_url: String::new(),
                    auth_at: None,
                };
                apply(&v, &mut identity);
                let identity = self.repo.create_identity(&identity, now).await?;
                Ok(Resolved {
                    user: target,
                    identity,
                    created: false,
                    previous_user_id: 0,
                })
            }
        }
    }

    async fn runtime_status(&self) -> TelegramBotRuntimeStatus {
        match self
            .settings
            .get(setting_keys::TELEGRAM_BOT_RUNTIME_STATUS)
            .await
        {
            Ok(raw) => TelegramBotRuntimeStatus::decode(raw.as_ref()),
            Err(error) => {
                tracing::warn!(%error, "channel telegram runtime status read failed");
                TelegramBotRuntimeStatus::default()
            }
        }
    }

    /// `GET /channel/telegram/config`: bot settings plus the client's decrypted bot token.
    pub async fn bot_config(&self, client_id: Id) -> Result<Value> {
        let raw = self.settings.get(setting_keys::TELEGRAM_BOT_CONFIG).await?;
        let mut cfg = TelegramBotSetting::decode(raw.as_ref(), TelegramBotSetting::defaults());
        cfg.menu.items = ensure_builtin_menu(std::mem::take(&mut cfg.menu.items));
        let bot_token = self
            .clients
            .bot_token_of(client_id)
            .await
            .unwrap_or_default();
        let mut config = cfg.encode();
        if let Value::Object(m) = &mut config {
            m.insert("bot_token".into(), bot_token.into());
        }
        let status = self.runtime_status().await;
        Ok(json!({"config": config, "config_version": status.config_version}))
    }

    /// `POST /channel/telegram/heartbeat`: records the bot's runtime status.
    pub async fn heartbeat(&self, hb: Heartbeat) -> Result<Value> {
        let current = self.runtime_status().await;
        let status = TelegramBotRuntimeStatus {
            connected: true,
            last_seen_at: self.clock.now().to_rfc3339_opts(SecondsFormat::Secs, true),
            bot_version: hb.bot_version,
            webhook_status: hb.webhook_status,
            machine_code: hb.machine_code,
            license_status: hb.license_status,
            license_expires_at: hb.license_expires_at,
            warnings: hb.warnings,
            config_version: current.config_version,
            last_config_sync_at: current.last_config_sync_at,
        };
        self.settings
            .set(setting_keys::TELEGRAM_BOT_RUNTIME_STATUS, &status.encode())
            .await?;
        Ok(json!({"config_version": status.config_version}))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use zs_shared::clock::FixedClock;

    use super::*;

    #[derive(Default)]
    struct Mem {
        users: Mutex<Vec<ChannelUser>>,
        identities: Mutex<Vec<OAuthIdentity>>,
        registration: Mutex<bool>,
    }

    #[async_trait]
    impl ChannelIdentityRepo for Mem {
        async fn user_by_id(&self, id: Id) -> Result<Option<ChannelUser>> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.id == id)
                .cloned())
        }
        async fn user_by_email(&self, email: &str) -> Result<Option<ChannelUser>> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.email == email)
                .cloned())
        }
        async fn create_user(
            &self,
            u: &NewChannelUser,
            _now: DateTime<Utc>,
        ) -> Result<ChannelUser> {
            let mut users = self.users.lock().unwrap();
            let user = ChannelUser {
                id: i64::try_from(users.len()).unwrap() + 1,
                email: u.email.clone(),
                display_name: u.display_name.clone(),
                status: "active".into(),
                locale: "zh-CN".into(),
                email_verified: false,
                password_setup_required: true,
                member_level_id: 1,
            };
            users.push(user.clone());
            Ok(user)
        }
        async fn identity_by_provider_user(
            &self,
            p: &str,
            id: &str,
        ) -> Result<Option<OAuthIdentity>> {
            Ok(self
                .identities
                .lock()
                .unwrap()
                .iter()
                .find(|i| i.provider == p && i.provider_user_id == id)
                .cloned())
        }
        async fn identity_by_user(&self, user_id: Id, p: &str) -> Result<Option<OAuthIdentity>> {
            Ok(self
                .identities
                .lock()
                .unwrap()
                .iter()
                .find(|i| i.provider == p && i.user_id == user_id)
                .cloned())
        }
        async fn create_identity(
            &self,
            i: &OAuthIdentity,
            _now: DateTime<Utc>,
        ) -> Result<OAuthIdentity> {
            let mut all = self.identities.lock().unwrap();
            let mut row = i.clone();
            row.id = i64::try_from(all.len()).unwrap() + 1;
            all.push(row.clone());
            Ok(row)
        }
        async fn update_identity(&self, i: &OAuthIdentity, _now: DateTime<Utc>) -> Result<()> {
            let mut all = self.identities.lock().unwrap();
            if let Some(slot) = all.iter_mut().find(|x| x.id == i.id) {
                *slot = i.clone();
            }
            Ok(())
        }
        async fn registration_enabled(&self) -> Result<bool> {
            Ok(*self.registration.lock().unwrap())
        }
    }

    struct Codes;
    #[async_trait]
    impl BindCodeVerifier for Codes {
        async fn verify(&self, _email: &str, code: &str) -> Result<()> {
            if code == "123456" {
                Ok(())
            } else {
                Err(Error::bad_request("error.verify_code_invalid"))
            }
        }
    }

    #[derive(Default)]
    struct Settings(Mutex<HashMap<String, Value>>);
    #[async_trait]
    impl SettingsStore for Settings {
        async fn get(&self, key: &str) -> Result<Option<Value>> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        async fn set(&self, key: &str, value: &Value) -> Result<()> {
            self.0.lock().unwrap().insert(key.into(), value.clone());
            Ok(())
        }
    }

    struct Lookup;
    #[async_trait]
    impl ChannelLookup for Lookup {
        async fn product_slug(&self, _id: Id) -> Result<Option<String>> {
            Ok(None)
        }
        async fn category_product_counts(&self) -> Result<HashMap<Id, i64>> {
            Ok(HashMap::new())
        }
        async fn active_payment_channels(&self, _limit: u64) -> Result<Vec<PaymentMethod>> {
            let m = |id: Id, provider: &str| PaymentMethod {
                id,
                name: format!("c{id}"),
                provider_type: provider.into(),
                channel_type: "alipay".into(),
                interaction_mode: "qr".into(),
                fee_rate: zs_shared::money::Amount::ZERO,
                fixed_fee: zs_shared::money::Amount::ZERO,
            };
            Ok(vec![m(1, "epay"), m(2, "wallet"), m(3, "epay")])
        }
        async fn order_product_ids(&self, _user: Id, order_no: &str) -> Result<Option<Vec<Id>>> {
            Ok((order_no == "DJ1").then(|| vec![7]))
        }
        async fn product_payment_channel_ids(&self, _ids: &[Id]) -> Result<Vec<String>> {
            Ok(vec!["[3]".into()])
        }
    }

    #[tokio::test]
    async fn payment_methods_by_context() {
        let (svc, _, settings) = service();
        let ids = |m: PaymentMethods| m.items.iter().map(|c| c.id).collect::<Vec<_>>();
        assert_eq!(
            ids(svc.payment_methods("", "", "").await.unwrap()),
            vec![1, 3],
            "wallet provider hidden"
        );
        assert_eq!(
            ids(svc.payment_methods("", "DJ1", "42").await.unwrap()),
            vec![3],
            "product restriction"
        );
        assert_eq!(
            ids(svc.payment_methods("", "NOPE", "42").await.unwrap()),
            vec![1, 3],
            "unknown order: all"
        );
        settings.0.lock().unwrap().insert(
            setting_keys::WALLET_CONFIG.into(),
            json!({"recharge_channel_ids": [1], "wallet_only_payment": true}),
        );
        let recharge = svc.payment_methods("recharge", "", "").await.unwrap();
        assert!(recharge.wallet_only_payment);
        assert_eq!(ids(recharge), vec![1]);
        assert!(
            svc.payment_methods("", "DJ1", "42")
                .await
                .unwrap()
                .items
                .is_empty(),
            "wallet only"
        );
    }

    fn service() -> (ChannelService, Arc<Mem>, Arc<Settings>) {
        let now = Utc::now();
        let repo = Arc::new(Mem::default());
        *repo.registration.lock().unwrap() = true;
        let settings = Arc::new(Settings::default());
        let (clients, _) = crate::notify::clients::tests::service(now);
        let svc = ChannelService::new(
            repo.clone(),
            Arc::new(Codes),
            settings.clone(),
            clients,
            Arc::new(Lookup),
            Arc::new(FixedClock(now)),
        );
        (svc, repo, settings)
    }

    fn tg(id: &str) -> TelegramIdentityInput {
        TelegramIdentityInput {
            channel_user_id: id.into(),
            username: "neo".into(),
            first_name: "Thomas".into(),
            last_name: "Anderson".into(),
            ..TelegramIdentityInput::default()
        }
    }

    #[tokio::test]
    async fn provision_creates_placeholder_once() {
        let (svc, repo, _) = service();
        assert!(svc.resolve(&tg("42")).await.unwrap().is_none());
        let first = svc.provision(&tg("42")).await.unwrap();
        assert!(first.created);
        assert_eq!(first.user.email, "telegram_42@login.local");
        assert_eq!(first.user.display_name, "Thomas Anderson");
        assert_eq!(first.identity.username, "neo");
        let again = svc.provision(&tg("42")).await.unwrap();
        assert!(!again.created);
        assert_eq!(again.user.id, first.user.id);
        assert_eq!(repo.users.lock().unwrap().len(), 1);
        assert_eq!(
            svc.resolve(&tg("42")).await.unwrap().unwrap().user.id,
            first.user.id
        );
        assert_eq!(
            svc.resolve(&tg(" ")).await.unwrap_err().key(),
            "error.bad_request"
        );
    }

    // 803f11a1: Telegram sign-up follows the registration switch.
    #[tokio::test]
    async fn provision_blocked_when_registration_disabled() {
        let (svc, repo, _) = service();
        *repo.registration.lock().unwrap() = false;
        let err = svc.provision(&tg("7")).await.unwrap_err();
        assert_eq!(err.key(), "error.registration_disabled");
    }

    #[tokio::test]
    async fn bind_moves_identity_from_placeholder_account() {
        let (svc, repo, _) = service();
        let placeholder = svc.provision(&tg("42")).await.unwrap();
        repo.users.lock().unwrap().push(ChannelUser {
            id: 50,
            email: "buyer@example.com".into(),
            display_name: "B".into(),
            status: "active".into(),
            locale: "en-US".into(),
            email_verified: true,
            password_setup_required: false,
            member_level_id: 0,
        });
        let err = svc
            .bind(&tg("42"), "buyer@example.com", "000000")
            .await
            .unwrap_err();
        assert_eq!(err.key(), "error.verify_code_invalid");
        let bound = svc
            .bind(&tg("42"), " Buyer@Example.com ", "123456")
            .await
            .unwrap();
        assert_eq!(bound.user.id, 50);
        assert_eq!(bound.previous_user_id, placeholder.user.id);
        assert_eq!(svc.resolve(&tg("42")).await.unwrap().unwrap().user.id, 50);
        // Another Telegram id cannot take over the bound account.
        let err = svc
            .bind(&tg("43"), "buyer@example.com", "123456")
            .await
            .unwrap_err();
        assert_eq!(err.key(), "error.telegram_already_bound");
        let err = svc
            .bind(&tg("42"), "nobody@example.com", "123456")
            .await
            .unwrap_err();
        assert_eq!(err.key(), "error.user_not_found");
    }

    #[tokio::test]
    async fn heartbeat_keeps_config_version() {
        let (svc, _, settings) = service();
        settings.0.lock().unwrap().insert(
            setting_keys::TELEGRAM_BOT_RUNTIME_STATUS.into(),
            json!({"config_version": 4, "last_config_sync_at": "x"}),
        );
        let out = svc
            .heartbeat(Heartbeat {
                bot_version: "1.2.3".into(),
                warnings: vec!["w".into()],
                ..Heartbeat::default()
            })
            .await
            .unwrap();
        assert_eq!(out, json!({"config_version": 4}));
        let stored = settings.0.lock().unwrap()[setting_keys::TELEGRAM_BOT_RUNTIME_STATUS].clone();
        assert_eq!(stored["connected"], true);
        assert_eq!(stored["bot_version"], "1.2.3");
        assert_eq!(stored["last_config_sync_at"], "x");
        assert_eq!(stored["warnings"], json!(["w"]));
    }
}
