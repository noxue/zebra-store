//! Wiring of the `notify` group.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use zs_app::identity::rate_limit::RateLimiter;
use zs_app::identity::verify_code::VerifyCodeService;
use zs_app::notify::bot::BotNotifyService;
use zs_app::notify::broadcast::{BroadcastJob, BroadcastService, SEND_INTERVAL};
use zs_app::notify::center::{CenterDeps, NotificationService, QueueNotifier};
use zs_app::notify::channel::ChannelService;
use zs_app::notify::clients::ChannelClientService;
use zs_app::notify::guard::MemoryGuard;
use zs_app::notify::{CHANNEL_RATE_RULE, NotifyServices};
use zs_domain::Result;
use zs_domain::identity::verify_code::{Policy, Purpose};
use zs_domain::notify::center::DispatchPayload;
use zs_domain::notify::channel::{BindCodeVerifier, BotNotifyPayload};
use zs_domain::notify::ports::{CallbackPoster, FeishuSender, TelegramSender};
use zs_domain::queue::{JobHandler, kinds};

use super::WireCtx;
use crate::db::repo::identity::verify_code::SeaVerifyCodeRepo;
use crate::db::repo::notify::alerts::SeaAlertSource;
use crate::db::repo::notify::broadcast::SeaBroadcastRepo;
use crate::db::repo::notify::clients::SeaChannelClientRepo;
use crate::db::repo::notify::identity::SeaChannelIdentityRepo;
use crate::db::repo::notify::log::SeaNotificationLogRepo;
use crate::db::repo::notify::lookup::SeaChannelLookup;
use crate::identity::mail::SmtpMailer;
use crate::notify::email::NotifyMailer;
use crate::notify::feishu::{FEISHU_API_BASE, FeishuClient};
use crate::notify::safe_http::{AddressPolicy, SafeHttp};
use crate::notify::telegram::{BotApi, TELEGRAM_API_BASE};
use crate::queue::JobRegistry;

/// How often the inventory / payment alert check runs (original: every minute).
const ALERT_CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// Outbound endpoints and policies (overridable in tests to hit local mocks).
#[derive(Debug, Clone)]
pub struct Adapters {
    pub telegram_base: String,
    pub feishu_base: String,
    /// Address policy of bot callbacks and other peer-configured URLs.
    pub callback_policy: AddressPolicy,
    /// Pause between broadcast messages.
    pub broadcast_interval: Duration,
}

impl Default for Adapters {
    fn default() -> Self {
        Self {
            telegram_base: TELEGRAM_API_BASE.into(),
            feishu_base: FEISHU_API_BASE.into(),
            callback_policy: AddressPolicy::PublicOnly,
            broadcast_interval: SEND_INTERVAL,
        }
    }
}

/// `telegram_bind` code check backed by the identity verification codes.
struct BindCodes(VerifyCodeService);

#[async_trait]
impl BindCodeVerifier for BindCodes {
    async fn verify(&self, email: &str, code: &str) -> Result<()> {
        self.0.verify(email, Purpose::TelegramBind, code).await
    }
}

/// Builds the `notify` services with the production adapters.
pub fn build(ctx: &WireCtx) -> NotifyServices {
    build_with(ctx, &Adapters::default())
}

/// Builds the `notify` services with explicit adapters.
pub fn build_with(ctx: &WireCtx, adapters: &Adapters) -> NotifyServices {
    let db = &ctx.db;
    let upload_dir = PathBuf::from(&ctx.cfg.upload.dir);
    // Client construction only fails when the TLS backend cannot initialise,
    // which is a broken build, not a runtime condition.
    #[expect(
        clippy::expect_used,
        reason = "TLS backend initialisation is a start-up invariant"
    )]
    let telegram: Arc<dyn TelegramSender> =
        Arc::new(BotApi::new(&adapters.telegram_base, upload_dir).expect("telegram http client"));
    #[expect(
        clippy::expect_used,
        reason = "TLS backend initialisation is a start-up invariant"
    )]
    let feishu: Arc<dyn FeishuSender> =
        Arc::new(FeishuClient::new(&adapters.feishu_base).expect("feishu http client"));
    #[expect(
        clippy::expect_used,
        reason = "TLS backend initialisation is a start-up invariant"
    )]
    let poster: Arc<dyn CallbackPoster> =
        Arc::new(SafeHttp::new(adapters.callback_policy).expect("callback http client"));

    let smtp = SmtpMailer::new(
        ctx.settings.clone(),
        ctx.cfg.email.clone(),
        ctx.cipher.clone(),
    );
    let center = NotificationService::new(CenterDeps {
        settings: ctx.settings.clone(),
        email: Arc::new(NotifyMailer::new(smtp.clone())),
        telegram: telegram.clone(),
        feishu,
        logs: Arc::new(SeaNotificationLogRepo::new(db.clone())),
        guard: Arc::new(MemoryGuard::new(ctx.clock.clone())),
        alerts: Arc::new(SeaAlertSource::new(db.clone())),
        clock: ctx.clock.clone(),
        fallback_bot_token: ctx.cfg.telegram_auth.bot_token.clone(),
    });
    let clients = ChannelClientService::new(
        Arc::new(SeaChannelClientRepo::new(db.clone())),
        ctx.cipher.clone(),
        ctx.clock.clone(),
    );
    let vc = ctx.cfg.email.verify_code;
    let codes = VerifyCodeService::new(
        Arc::new(SeaVerifyCodeRepo::new(db.clone())),
        Arc::new(smtp),
        ctx.settings.clone(),
        Policy {
            expire_minutes: vc.expire_minutes,
            send_interval_seconds: vc.send_interval_seconds,
            max_attempts: vc.max_attempts,
            length: vc.length,
        },
        ctx.clock.clone(),
    )
    .with_brands(super::identity::mail_brands(ctx));
    let channel = ChannelService::new(
        Arc::new(SeaChannelIdentityRepo::new(
            db.clone(),
            ctx.settings.clone(),
        )),
        Arc::new(BindCodes(codes)),
        ctx.settings.clone(),
        clients.clone(),
        Arc::new(SeaChannelLookup::new(db.clone())),
        ctx.clock.clone(),
    );
    let broadcasts = BroadcastService::new(
        Arc::new(SeaBroadcastRepo::new(db.clone())),
        clients.clone(),
        telegram,
        ctx.queue.clone(),
        ctx.clock.clone(),
    )
    .with_interval(adapters.broadcast_interval);
    let bot = BotNotifyService::new(
        clients.clone(),
        poster,
        ctx.queue.clone(),
        ctx.clock.clone(),
    );
    NotifyServices {
        center,
        notifier: Arc::new(QueueNotifier::new(ctx.queue.clone())),
        clients,
        channel,
        broadcasts,
        bot,
        channel_limiter: RateLimiter::new(CHANNEL_RATE_RULE, ctx.clock.clone()),
    }
}

fn decode<T: serde::de::DeserializeOwned + Default>(payload: Value) -> T {
    if payload.is_null() {
        return T::default();
    }
    serde_json::from_value(payload).unwrap_or_else(|error| {
        tracing::warn!(%error, "job payload malformed; using defaults");
        T::default()
    })
}

struct DispatchJob(NotificationService);

#[async_trait]
impl JobHandler for DispatchJob {
    async fn handle(&self, payload: Value) -> Result<()> {
        let payload: DispatchPayload = decode(payload);
        self.0.dispatch(&payload).await
    }
}

struct AlertCheckJob(NotificationService);

#[async_trait]
impl JobHandler for AlertCheckJob {
    async fn handle(&self, _payload: Value) -> Result<()> {
        self.0.run_alert_check().await
    }
}

struct BroadcastJobHandler(BroadcastService);

#[async_trait]
impl JobHandler for BroadcastJobHandler {
    async fn handle(&self, payload: Value) -> Result<()> {
        let job: BroadcastJob = decode(payload);
        if job.broadcast_id <= 0 {
            return Ok(());
        }
        self.0.process(job.broadcast_id).await
    }
}

struct BotNotifyJob(BotNotifyService);

#[async_trait]
impl JobHandler for BotNotifyJob {
    async fn handle(&self, payload: Value) -> Result<()> {
        let payload: BotNotifyPayload = decode(payload);
        self.0.handle(&payload).await
    }
}

/// Registers the `notify` job handlers and periodic jobs.
pub fn jobs(_ctx: &WireCtx, services: &zs_app::Services, registry: &mut JobRegistry) {
    let n = &services.notify;
    registry
        .handle(
            kinds::NOTIFICATION_DISPATCH,
            Arc::new(DispatchJob(n.center.clone())),
        )
        .handle(
            kinds::ALERT_CHECK,
            Arc::new(AlertCheckJob(n.center.clone())),
        )
        .handle(
            kinds::TELEGRAM_BROADCAST,
            Arc::new(BroadcastJobHandler(n.broadcasts.clone())),
        )
        .handle(kinds::BOT_NOTIFY, Arc::new(BotNotifyJob(n.bot.clone())))
        .every(kinds::ALERT_CHECK, ALERT_CHECK_INTERVAL);
}
