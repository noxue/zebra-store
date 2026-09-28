//! Wiring of the identity group.

use std::sync::Arc;

use zs_app::identity::admin_2fa::AdminTwoFactorService;
use zs_app::identity::admin_accounts::AdminAccountService;
use zs_app::identity::admin_auth::AdminAuthService;
use zs_app::identity::audit::AuditService;
use zs_app::identity::authz::AuthzService;
use zs_app::identity::captcha::CaptchaService;
use zs_app::identity::challenge::ChallengeStore;
use zs_app::identity::compliance::ComplianceService;
use zs_app::identity::mail_brand::MailBrandResolver;
use zs_app::identity::oauth::{OAuthDeps, OAuthService};
use zs_app::identity::rate_limit::{RateLimiter, RateRule};
use zs_app::identity::totp::{AdminAccounts, OnEnable, TotpService, UserAccounts};
use zs_app::identity::user_account::{UserAccountDeps, UserAccountService};
use zs_app::identity::user_auth::UserAuthService;
use zs_app::identity::verify_code::VerifyCodeService;
use zs_app::identity::{IdentityServices, LoginLimits};
use zs_domain::identity::mailer::MailBrands;
use zs_domain::identity::password::PasswordPolicy;
use zs_domain::identity::user::UserRepo;
use zs_domain::identity::verify_code::Policy;
use zs_domain::settings::schema::login::{GoogleAuthSetting, TelegramAuthSetting};

use super::WireCtx;
use crate::db::repo::dashboard::providers::SettingsLoginProviders;
use crate::db::repo::dashboard::users::SeaAdminUserRepo;
use crate::db::repo::identity::admin::SeaAdminRepo;
use crate::db::repo::identity::audit::SeaAuthzAuditRepo;
use crate::db::repo::identity::login_log::SeaUserLoginLogRepo;
use crate::db::repo::identity::member_level::SeaDefaultMemberLevel;
use crate::db::repo::identity::oauth::SeaExternalIdentityRepo;
use crate::db::repo::identity::rules::SeaRuleRepo;
use crate::db::repo::identity::user::SeaUserRepo;
use crate::db::repo::identity::verify_code::SeaVerifyCodeRepo;
use crate::db::repo::reseller::SeaResellerStore;
use crate::identity::captcha_image::PngCaptchaRenderer;
use crate::identity::mail::SmtpMailer;
use crate::identity::oauth::{HttpJwtVerifier, HttpOidcTokenClient};
use crate::identity::turnstile::TurnstileClient;
use crate::queue::JobRegistry;

/// Issuer used when `app.totp_issuer` is empty (admins).
const DEFAULT_ADMIN_ISSUER: &str = "Zebra-Store";
/// Issuer used when `app.totp_issuer` is empty (users).
const DEFAULT_USER_ISSUER: &str = "Zebra-Store-User";

/// Password policy from configuration.
pub fn password_policy(ctx: &WireCtx) -> PasswordPolicy {
    let pp = ctx.cfg.security.password_policy;
    PasswordPolicy {
        min_length: pp.min_length,
        require_upper: pp.require_upper,
        require_lower: pp.require_lower,
        require_number: pp.require_number,
        require_special: pp.require_special,
    }
}

fn issuer(ctx: &WireCtx, default: &str) -> String {
    match ctx.cfg.app.totp_issuer.trim() {
        "" => default.to_owned(),
        s => s.to_owned(),
    }
}

/// Builds the identity services.
pub fn build(ctx: &WireCtx) -> IdentityServices {
    let db = &ctx.db;
    let admin_repo = Arc::new(SeaAdminRepo::new(db.clone()));
    let user_repo: Arc<dyn UserRepo> = Arc::new(SeaUserRepo::new(db.clone()));
    let policy = password_policy(ctx);
    let audit = AuditService::new(
        Arc::new(SeaAuthzAuditRepo::new(db.clone())),
        Arc::new(SeaUserLoginLogRepo::new(db.clone())),
        admin_repo.clone(),
    );
    let admin_auth = AdminAuthService::new(
        admin_repo.clone(),
        admin_repo.clone(),
        ctx.clock.clone(),
        &ctx.cfg.jwt.secret,
        ctx.cfg.jwt.expire_hours,
        policy,
    );
    let authz = AuthzService::new(Arc::new(SeaRuleRepo::new(db.clone())));
    let user_auth = UserAuthService::new(
        user_repo.clone(),
        ctx.clock.clone(),
        &ctx.cfg.user_jwt.secret,
        ctx.cfg.user_jwt.expire_hours,
        ctx.cfg.user_jwt.remember_me_expire_hours,
    );
    let admin_totp = TotpService::new(
        Arc::new(AdminAccounts(admin_repo.clone())),
        ctx.cipher.clone(),
        ctx.clock.clone(),
        &issuer(ctx, DEFAULT_ADMIN_ISSUER),
        OnEnable::KeepSessions,
    );
    let user_totp = TotpService::new(
        Arc::new(UserAccounts(user_repo.clone())),
        ctx.cipher.clone(),
        ctx.clock.clone(),
        &issuer(ctx, DEFAULT_USER_ISSUER),
        OnEnable::RevokeSessions,
    );
    let admin_2fa = AdminTwoFactorService::new(
        admin_auth.clone(),
        admin_totp,
        ChallengeStore::new(ctx.clock.clone()),
        audit.clone(),
    );
    let mailer = Arc::new(SmtpMailer::new(
        ctx.settings.clone(),
        ctx.cfg.email.clone(),
        ctx.cipher.clone(),
    ));
    let vc = ctx.cfg.email.verify_code;
    let verify_codes = VerifyCodeService::new(
        Arc::new(SeaVerifyCodeRepo::new(db.clone())),
        mailer,
        ctx.settings.clone(),
        Policy {
            expire_minutes: vc.expire_minutes,
            send_interval_seconds: vc.send_interval_seconds,
            max_attempts: vc.max_attempts,
            length: vc.length,
        },
        ctx.clock.clone(),
    )
    .with_brands(mail_brands(ctx));
    let captcha = CaptchaService::new(
        ctx.settings.clone(),
        Arc::new(PngCaptchaRenderer),
        Arc::new(TurnstileClient::new()),
        ctx.clock.clone(),
    );
    let oauth = oauth_service(ctx, user_repo.clone(), user_auth.clone(), audit.clone());
    let users = UserAccountService::new(UserAccountDeps {
        users: user_repo,
        auth: user_auth.clone(),
        totp: user_totp,
        challenges: ChallengeStore::new(ctx.clock.clone()),
        codes: verify_codes.clone(),
        captcha: captcha.clone(),
        audit: audit.clone(),
        settings: ctx.settings.clone(),
        levels: Arc::new(SeaDefaultMemberLevel::new(db.clone())),
        policy,
        clock: ctx.clock.clone(),
    });
    let admin_accounts = AdminAccountService::new(
        admin_repo,
        authz.clone(),
        audit.clone(),
        ctx.clock.clone(),
        policy,
    );
    let rl = ctx.cfg.security.login_rate_limit;
    let rule = RateRule {
        window_seconds: i64::try_from(rl.window_seconds).unwrap_or(i64::MAX),
        max_requests: i64::from(rl.max_attempts),
        block_seconds: i64::try_from(rl.block_seconds).unwrap_or(i64::MAX),
    };
    IdentityServices {
        admin_auth,
        admin_2fa,
        admin_accounts,
        authz,
        user_auth,
        users,
        verify_codes,
        captcha,
        compliance: ComplianceService::new(ctx.settings.clone(), ctx.clock.clone()),
        audit,
        login_limits: LoginLimits {
            user: RateLimiter::new(rule, ctx.clock.clone()),
            admin: RateLimiter::new(rule, ctx.clock.clone()),
        },
        oauth,
    }
}

/// `config.yml` defaults of the login settings (used until they are saved).
fn login_defaults(ctx: &WireCtx) -> (TelegramAuthSetting, GoogleAuthSetting) {
    let t = &ctx.cfg.telegram_auth;
    (
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
    )
}

/// Telegram / Google login service.
fn oauth_service(
    ctx: &WireCtx,
    users: Arc<dyn UserRepo>,
    auth: UserAuthService,
    audit: AuditService,
) -> OAuthService {
    let db = &ctx.db;
    let (telegram_default, google_default) = login_defaults(ctx);
    OAuthService::new(OAuthDeps {
        users,
        identities: Arc::new(SeaExternalIdentityRepo::new(db.clone())),
        directory: Arc::new(SeaAdminUserRepo::new(db.clone())),
        providers: Arc::new(SettingsLoginProviders::new(
            ctx.settings.clone(),
            telegram_default.clone(),
            google_default.clone(),
        )),
        levels: Arc::new(SeaDefaultMemberLevel::new(db.clone())),
        settings: ctx.settings.clone(),
        telegram_default,
        google_default,
        auth,
        audit,
        jwt: Arc::new(HttpJwtVerifier::new(ctx.clock.clone())),
        tokens: Arc::new(HttpOidcTokenClient::new()),
        clock: ctx.clock.clone(),
    })
}

/// Registers the identity job handlers.
pub fn jobs(_ctx: &WireCtx, _services: &zs_app::Services, _registry: &mut JobRegistry) {}

/// Start-up tasks: built-in roles, the bootstrap administrator and the
/// compliance flag.
pub async fn bootstrap(ctx: &WireCtx, services: &zs_app::Services) -> zs_domain::Result<()> {
    let identity = &services.identity;
    identity.authz.bootstrap_builtin_roles().await?;
    identity
        .admin_auth
        .bootstrap(
            &ctx.cfg.bootstrap.default_admin_username,
            &ctx.cfg.bootstrap.default_admin_password,
            ctx.cfg.server.mode.eq_ignore_ascii_case("release"),
        )
        .await?;
    identity.compliance.load().await?;
    Ok(())
}

/// Email brand resolver (main `site_config.brand` / reseller site config, NTF-02).
pub fn mail_brands(ctx: &WireCtx) -> Arc<dyn MailBrands> {
    Arc::new(MailBrandResolver::new(
        ctx.settings.clone(),
        Arc::new(SeaResellerStore::new(ctx.db.clone())),
    ))
}
