//! Wiring of the `dashboard` group.

use std::sync::Arc;

use zs_app::dashboard::DashboardServices;
use zs_app::dashboard::report::ReportService;
use zs_app::dashboard::system::SystemService;
use zs_app::dashboard::users::AdminUserService;
use zs_domain::settings::schema::login::{GoogleAuthSetting, TelegramAuthSetting};

use super::WireCtx;
use crate::db::repo::dashboard::providers::SettingsLoginProviders;
use crate::db::repo::dashboard::report::SeaDashboardRepo;
use crate::db::repo::dashboard::users::SeaAdminUserRepo;
use crate::queue::JobRegistry;

/// Builds the `dashboard` services.
pub fn build(ctx: &WireCtx) -> DashboardServices {
    let t = &ctx.cfg.telegram_auth;
    let providers = SettingsLoginProviders::new(
        ctx.settings.clone(),
        TelegramAuthSetting {
            enabled: t.enabled,
            bot_username: t.bot_username.clone(),
            bot_token: t.bot_token.clone(),
            client_secret: t.client_secret.clone(),
            oidc_redirect_uri: t.oidc_redirect_uri.clone(),
            mini_app_url: t.mini_app_url.clone(),
            login_expire_seconds: t.login_expire_seconds,
            replay_ttl_seconds: t.replay_ttl_seconds,
        },
        GoogleAuthSetting {
            enabled: ctx.cfg.google_auth.enabled,
            client_id: ctx.cfg.google_auth.client_id.clone(),
        },
    );
    DashboardServices {
        reports: ReportService::new(
            Arc::new(SeaDashboardRepo::new(ctx.db.clone())),
            ctx.settings.clone(),
            ctx.clock.clone(),
        ),
        users: AdminUserService::new(
            Arc::new(SeaAdminUserRepo::new(ctx.db.clone())),
            Arc::new(providers),
            ctx.clock.clone(),
        ),
        system: SystemService::new(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        ),
    }
}

/// Registers the `dashboard` job handlers and periodic jobs.
pub fn jobs(_ctx: &WireCtx, _services: &zs_app::Services, _registry: &mut JobRegistry) {}
