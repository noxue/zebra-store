//! Authentication and authorization use cases.

pub mod admin_2fa;
pub mod admin_accounts;
pub mod admin_auth;
pub mod audit;
pub mod authz;
pub mod captcha;
pub mod challenge;
pub mod compliance;
pub mod jwt;
pub mod mail_brand;
pub mod oauth;
pub mod password;
pub mod rate_limit;
pub mod totp;
pub mod user_account;
pub mod user_auth;
pub mod verify_code;

use chrono::{DateTime, SecondsFormat, Utc};

/// RFC 3339 timestamp with seconds precision (original `2006-01-02T15:04:05Z07:00`).
pub fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Login rate limiters (original `loginRule` / `adminLoginRule`).
#[derive(Debug, Clone)]
pub struct LoginLimits {
    /// `/auth/login` (key `email|ip`) and `/auth/login/verify-2fa` (key `ip`).
    pub user: rate_limit::RateLimiter,
    /// `/admin/login` and `/admin/login/verify-2fa` (key `ip`).
    pub admin: rate_limit::RateLimiter,
}

/// Services of the identity group.
#[derive(Debug, Clone)]
pub struct IdentityServices {
    pub admin_auth: admin_auth::AdminAuthService,
    pub admin_2fa: admin_2fa::AdminTwoFactorService,
    pub admin_accounts: admin_accounts::AdminAccountService,
    pub authz: authz::AuthzService,
    pub user_auth: user_auth::UserAuthService,
    pub users: user_account::UserAccountService,
    pub verify_codes: verify_code::VerifyCodeService,
    /// Reusable by every module: `captcha.verify(scene, &payload, ip)`.
    pub captcha: captcha::CaptchaService,
    pub compliance: compliance::ComplianceService,
    pub audit: audit::AuditService,
    pub login_limits: LoginLimits,
    /// Telegram / Google login and binding.
    pub oauth: oauth::OAuthService,
}
