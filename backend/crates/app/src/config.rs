//! Application configuration model (mirrors the original `config.yml`).
//!
//! Loading from files/environment happens in `zs-server`; this module only
//! defines the shape and defaults so services can depend on it.

use serde::Deserialize;

/// Root configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub app: AppConfig,
    pub server: ServerConfig,
    pub log: LogConfig,
    pub database: DatabaseConfig,
    pub jwt: JwtConfig,
    pub user_jwt: UserJwtConfig,
    pub bootstrap: BootstrapConfig,
    pub telegram_auth: TelegramAuthConfig,
    pub google_auth: GoogleAuthConfig,
    pub redis: RedisConfig,
    pub queue: QueueConfig,
    pub upload: UploadConfig,
    pub cors: CorsConfig,
    pub security: SecurityConfig,
    pub email: EmailConfig,
    pub order: OrderConfig,
    pub reseller: ResellerConfig,
    pub web: WebConfig,
    pub integration: IntegrationConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Source of the AES-256 key protecting sensitive columns.
    pub secret_key: String,
    pub totp_issuer: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            secret_key: String::new(),
            totp_issuer: "Zebra-Store".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// `debug` or `release`.
    pub mode: String,
    pub trusted_proxies: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            port: 8081,
            mode: "debug".into(),
            trusted_proxies: vec!["127.0.0.1/32".into(), "::1/128".into()],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    pub dir: String,
    pub level: String,
    pub json: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            dir: String::new(),
            level: "info".into(),
            json: false,
        }
    }
}

/// Database settings. The scheme of `url` selects the driver:
/// `sqlite://…`, `mysql://…` or `postgres://…`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_seconds: u64,
    pub idle_timeout_seconds: u64,
    /// Log every SQL statement (debug aid).
    pub sql_log: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://data/zebra.db?mode=rwc".into(),
            max_connections: 10,
            min_connections: 1,
            connect_timeout_seconds: 10,
            idle_timeout_seconds: 600,
            sql_log: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct JwtConfig {
    pub secret: String,
    pub expire_hours: i64,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            expire_hours: 24,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UserJwtConfig {
    pub secret: String,
    pub expire_hours: i64,
    pub remember_me_expire_hours: i64,
}

impl Default for UserJwtConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            expire_hours: 24,
            remember_me_expire_hours: 168,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct BootstrapConfig {
    pub default_admin_username: String,
    pub default_admin_password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TelegramAuthConfig {
    pub enabled: bool,
    pub bot_username: String,
    pub bot_token: String,
    pub client_secret: String,
    pub oidc_redirect_uri: String,
    pub mini_app_url: String,
    pub login_expire_seconds: i64,
    pub replay_ttl_seconds: i64,
}

impl Default for TelegramAuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bot_username: String::new(),
            bot_token: String::new(),
            client_secret: String::new(),
            oidc_redirect_uri: String::new(),
            mini_app_url: String::new(),
            login_expire_seconds: 300,
            replay_ttl_seconds: 300,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct GoogleAuthConfig {
    pub enabled: bool,
    pub client_id: String,
}

/// Optional Redis used for shared cache / rate limiting. Disabled by default.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RedisConfig {
    pub enabled: bool,
    pub url: String,
    pub prefix: String,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: "redis://127.0.0.1:6379/0".into(),
            prefix: "zs".into(),
        }
    }
}

/// Database-backed job queue settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct QueueConfig {
    pub concurrency: usize,
    pub poll_interval_ms: u64,
    /// Upstream stock sync interval, e.g. `5m`.
    pub upstream_sync_interval: String,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            concurrency: 8,
            poll_interval_ms: 500,
            upstream_sync_interval: "5m".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UploadConfig {
    pub dir: String,
    pub max_size: u64,
    pub allowed_types: Vec<String>,
    pub allowed_extensions: Vec<String>,
    pub max_width: u32,
    pub max_height: u32,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            dir: "uploads".into(),
            max_size: 10 * 1024 * 1024,
            allowed_types: ["image/jpeg", "image/png", "image/gif", "image/webp"]
                .map(String::from)
                .to_vec(),
            allowed_extensions: [".jpg", ".jpeg", ".png", ".gif", ".webp"]
                .map(String::from)
                .to_vec(),
            max_width: 4096,
            max_height: 4096,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
    pub allow_credentials: bool,
    pub max_age: u64,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec!["*".into()],
            allow_credentials: true,
            max_age: 600,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct SecurityConfig {
    pub login_rate_limit: LoginRateLimit,
    pub password_policy: PasswordPolicy,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct LoginRateLimit {
    pub window_seconds: u64,
    pub max_attempts: u32,
    pub block_seconds: u64,
}

impl Default for LoginRateLimit {
    fn default() -> Self {
        Self {
            window_seconds: 300,
            max_attempts: 5,
            block_seconds: 900,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct PasswordPolicy {
    pub min_length: usize,
    pub require_upper: bool,
    pub require_lower: bool,
    pub require_number: bool,
    pub require_special: bool,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_length: 8,
            require_upper: true,
            require_lower: true,
            require_number: true,
            require_special: false,
        }
    }
}

/// Bootstrap SMTP defaults (runtime settings in `smtp_config` override these).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub from_name: String,
    pub use_tls: bool,
    pub use_ssl: bool,
    pub verify_code: VerifyCodeConfig,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            port: 465,
            username: String::new(),
            password: String::new(),
            from: String::new(),
            from_name: String::new(),
            use_tls: false,
            use_ssl: true,
            verify_code: VerifyCodeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct VerifyCodeConfig {
    pub expire_minutes: i64,
    pub send_interval_seconds: i64,
    pub max_attempts: i32,
    pub length: usize,
}

impl Default for VerifyCodeConfig {
    fn default() -> Self {
        Self {
            expire_minutes: 10,
            send_interval_seconds: 60,
            max_attempts: 5,
            length: 6,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct OrderConfig {
    pub payment_expire_minutes: i64,
    pub max_refund_days: i64,
}

impl Default for OrderConfig {
    fn default() -> Self {
        Self {
            payment_expire_minutes: 15,
            max_refund_days: 30,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ResellerConfig {
    pub enabled: bool,
    pub main_hosts: Vec<String>,
    pub trusted_forwarded_host: bool,
    pub subdomain_base: String,
    pub self_apply_enabled: bool,
    pub settlement_confirm_days: i64,
}

impl Default for ResellerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            main_hosts: vec!["localhost".into(), "127.0.0.1".into(), "::1".into()],
            trusted_forwarded_host: false,
            subdomain_base: String::new(),
            self_apply_enabled: true,
            settlement_confirm_days: 7,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    /// Directory holding built storefront assets to serve at `/` (optional).
    pub storefront_dir: String,
    /// Directory holding built admin assets (optional).
    pub admin_dir: String,
    pub admin_path: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            storefront_dir: String::new(),
            admin_dir: String::new(),
            admin_path: "/admin".into(),
        }
    }
}

/// Site-to-site integration (supplier connections, callbacks, pushed events).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct IntegrationConfig {
    /// Lets supplier calls, downstream callbacks and pushed events reach private /
    /// loopback / LAN addresses. **Only for trusted LAN or testing setups**: it enables
    /// SSRF to private networks. Redirects stay refused either way. Default `false`
    /// (env `ZS__INTEGRATION__ALLOW_PRIVATE_ADDRESSES`).
    pub allow_private_addresses: bool,
    /// Serves the acg-faka (异次元发卡) "共享店铺" provider protocol on `/shared/*`.
    /// Default `true`: the endpoints are inert until a user with an admin-approved
    /// credential issues a compat key in the personal center (env
    /// `ZS__INTEGRATION__ACG_FAKA_COMPAT`). See docs/protocol/third-party/provider-compat.md.
    pub acg_faka_compat: bool,
    /// Serves the mcy-shop OpenApi plugin provider protocol on `/plugin/open-api/*`
    /// (acg-faka "萌次元(V4.0)" stores, gmshop-edge). Default `true`, same opt-in rules
    /// (env `ZS__INTEGRATION__MCY_COMPAT`).
    pub mcy_compat: bool,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            allow_private_addresses: false,
            acg_faka_compat: true,
            mcy_compat: true,
        }
    }
}

/// Errors detected by [`Config::validate`].
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} must be set to a strong random value")]
    MissingSecret(&'static str),
    #[error("{0} and {1} must differ")]
    DuplicateSecrets(&'static str, &'static str),
    #[error("unsupported database url scheme: {0}")]
    DatabaseScheme(String),
    #[error("server.trusted_proxies entry {0:?} must be an IP or CIDR narrower than /0")]
    TrustedProxy(String),
}

/// A `server.trusted_proxies` entry: an IP or CIDR; `/0` (trust everyone, which makes
/// `X-Forwarded-For` spoofable) is refused (RISK-01).
fn valid_trusted_proxy(entry: &str) -> bool {
    let entry = entry.trim();
    let (ip, prefix) = match entry.split_once('/') {
        Some((ip, p)) => (ip.parse::<std::net::IpAddr>(), p.parse::<u8>().ok()),
        None => (entry.parse::<std::net::IpAddr>(), None),
    };
    let Ok(ip) = ip else {
        return false;
    };
    let max = if ip.is_ipv4() { 32 } else { 128 };
    match (entry.contains('/'), prefix) {
        (false, _) => true,
        (true, Some(p)) => p > 0 && p <= max,
        (true, None) => false,
    }
}

/// Placeholder values shipped in the example configs; refusing them forces operators to change them.
const PLACEHOLDER_SECRETS: [&str; 3] = [
    "your-secret-key-change-in-production-please",
    "user-secret-key-change-in-production-please",
    "change-me",
];

/// Prefix of every secret in `config.example.yml` (`change-me-…`), refused as a default (MISC-01).
const PLACEHOLDER_PREFIX: &str = "change-me";

/// Weak (short), placeholder or example secret (MISC-01).
fn is_weak_secret(value: &str) -> bool {
    let v = value.trim();
    v.len() < 16
        || PLACEHOLDER_SECRETS.contains(&v)
        || v.to_ascii_lowercase().starts_with(PLACEHOLDER_PREFIX)
}

impl Config {
    /// Validates secrets and the database URL (the original refuses to start on placeholders).
    pub fn validate(&self) -> Result<(), ConfigError> {
        let secrets = [
            ("app.secret_key", self.app.secret_key.as_str()),
            ("jwt.secret", self.jwt.secret.as_str()),
            ("user_jwt.secret", self.user_jwt.secret.as_str()),
        ];
        for (name, value) in secrets {
            if is_weak_secret(value) {
                return Err(ConfigError::MissingSecret(name));
            }
        }
        // MISC-01: any two secrets equal after trimming are refused, naming both.
        for (i, (a, va)) in secrets.iter().enumerate() {
            for (b, vb) in secrets.iter().skip(i + 1) {
                if va.trim() == vb.trim() {
                    return Err(ConfigError::DuplicateSecrets(a, b));
                }
            }
        }
        if let Some(bad) = self
            .server
            .trusted_proxies
            .iter()
            .find(|e| !valid_trusted_proxy(e))
        {
            return Err(ConfigError::TrustedProxy(bad.clone()));
        }
        let scheme = self.database.url.split(':').next().unwrap_or_default();
        if !matches!(scheme, "sqlite" | "mysql" | "postgres" | "postgresql") {
            return Err(ConfigError::DatabaseScheme(scheme.to_owned()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> Config {
        let mut c = Config::default();
        c.app.secret_key = "a".repeat(32);
        c.jwt.secret = "b".repeat(32);
        c.user_jwt.secret = "c".repeat(32);
        c
    }

    #[test]
    fn default_database_is_sqlite() {
        assert!(Config::default().database.url.starts_with("sqlite://"));
    }

    #[test]
    fn rejects_placeholder_and_duplicate_secrets() {
        assert!(valid().validate().is_ok());
        let mut c = valid();
        c.jwt.secret = "your-secret-key-change-in-production-please".into();
        assert!(matches!(
            c.validate(),
            Err(ConfigError::MissingSecret("jwt.secret"))
        ));
        let mut c = valid();
        c.user_jwt.secret = c.jwt.secret.clone();
        assert!(matches!(
            c.validate(),
            Err(ConfigError::DuplicateSecrets(
                "jwt.secret",
                "user_jwt.secret"
            ))
        ));
    }

    /// MISC-01: duplicates are detected after trimming and the error names both keys;
    /// the secrets shipped in `config.example.yml` are refused.
    #[test]
    fn misc_01_duplicate_and_example_secrets_refused() {
        let mut c = valid();
        c.app.secret_key = format!(" {} ", c.user_jwt.secret);
        let err = c.validate().err().map(|e| e.to_string());
        assert_eq!(
            err.as_deref(),
            Some("app.secret_key and user_jwt.secret must differ")
        );
        for example in [
            "change-me-app-secret-at-least-16-chars",
            "change-me-admin-jwt-secret-16+",
            "CHANGE-ME-user-jwt-secret-16+",
        ] {
            let mut c = valid();
            c.jwt.secret = example.into();
            assert!(
                matches!(c.validate(), Err(ConfigError::MissingSecret("jwt.secret"))),
                "{example}"
            );
        }
        let mut c = valid();
        c.app.secret_key = "short".into();
        assert!(matches!(
            c.validate(),
            Err(ConfigError::MissingSecret("app.secret_key"))
        ));
    }

    /// RISK-01: trusted proxies must be IPs/CIDRs; `/0` would trust every client's XFF.
    #[test]
    fn risk_01_trusted_proxies_are_validated() {
        let mut c = valid();
        c.server.trusted_proxies = vec!["127.0.0.1/32".into(), "::1".into(), "10.0.0.0/8".into()];
        assert!(c.validate().is_ok());
        for bad in [
            "0.0.0.0/0",
            "::/0",
            "10.0.0.0/33",
            "proxy.local",
            "1.2.3.4/x",
        ] {
            let mut c = valid();
            c.server.trusted_proxies = vec![bad.into()];
            assert!(
                matches!(c.validate(), Err(ConfigError::TrustedProxy(ref e)) if e == bad),
                "{bad}"
            );
        }
    }

    #[test]
    fn rejects_unknown_database_scheme() {
        let mut c = valid();
        c.database.url = "oracle://x".into();
        assert!(c.validate().is_err());
    }
}
