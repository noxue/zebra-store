//! [`Mailer`] over SMTP (lettre). Reads `smtp_config` on every send, falling back
//! to the `email` section of the configuration file; when SMTP is disabled the
//! message is only logged.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};
use zs_app::config::EmailConfig;
use zs_domain::identity::email;
use zs_domain::identity::mailer::{self, Email, Mailer};
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, Result};
use zs_shared::crypto::Cipher;

/// SMTP connection/command timeout.
const SMTP_TIMEOUT: Duration = Duration::from_secs(15);

/// Effective SMTP settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmtpSettings {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub from_name: String,
    pub use_tls: bool,
    pub use_ssl: bool,
}

impl Default for SmtpSettings {
    fn default() -> Self {
        Self::from_config(&EmailConfig::default())
    }
}

impl SmtpSettings {
    pub fn from_config(c: &EmailConfig) -> Self {
        Self {
            enabled: c.enabled,
            host: c.host.clone(),
            port: c.port,
            username: c.username.clone(),
            password: c.password.clone(),
            from: c.from.clone(),
            from_name: c.from_name.clone(),
            use_tls: c.use_tls,
            use_ssl: c.use_ssl,
        }
    }
}

/// True for SMTP errors meaning "this recipient does not exist" (original
/// `isEmailRecipientRejected`).
pub fn is_recipient_rejected(message: &str) -> bool {
    let m = message.trim().to_lowercase();
    const DIRECT: [&str; 9] = [
        "no such recipient",
        "no such user",
        "recipient not found",
        "recipient address rejected",
        "invalid recipient",
        "user unknown",
        "unknown user",
        "unknown mailbox",
        "mailbox unavailable",
    ];
    if DIRECT.iter().any(|k| m.contains(k)) {
        return true;
    }
    m.contains("550")
        && ["recipient", "user", "mailbox", "address", "rcpt"]
            .iter()
            .any(|h| m.contains(h))
}

/// SMTP transport shared by every sender (NTF-03): implicit TLS, STARTTLS or plain;
/// credentials only when a username is set; `AUTH LOGIN` preferred over `PLAIN` like the
/// original `authenticateSMTPClient` (Office365 only offers LOGIN).
pub(crate) fn smtp_transport(
    host: &str,
    port: u16,
    use_ssl: bool,
    use_tls: bool,
    username: &str,
    password: &str,
    timeout: Duration,
) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let host = host.trim();
    let builder = if use_ssl {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host).map_err(Error::internal)?
    } else if use_tls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host).map_err(Error::internal)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
    };
    let mut builder = builder.port(port).timeout(Some(timeout));
    if !username.is_empty() {
        builder = builder
            .credentials(Credentials::new(username.to_owned(), password.to_owned()))
            .authentication(vec![Mechanism::Login, Mechanism::Plain]);
    }
    Ok(builder.build())
}

/// RFC 5322 `Message-ID` whose domain is the sender's (`<16 random bytes hex@from-domain>`,
/// original `writeStandardHeaders`).
pub(crate) fn message_id(from: &str) -> String {
    let domain = from
        .trim()
        .rsplit_once('@')
        .map(|(_, d)| d.trim_end_matches('>').trim())
        .filter(|d| !d.is_empty())
        .unwrap_or("localhost");
    let bytes: [u8; 16] = rand::random();
    format!("<{}@{domain}>", hex::encode(bytes))
}

/// SMTP mailer.
#[derive(Clone)]
pub struct SmtpMailer {
    settings: Arc<dyn SettingsStore>,
    fallback: EmailConfig,
    cipher: Cipher,
    timeout: Duration,
}

impl std::fmt::Debug for SmtpMailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SmtpMailer")
    }
}

impl SmtpMailer {
    pub fn new(settings: Arc<dyn SettingsStore>, fallback: EmailConfig, cipher: Cipher) -> Self {
        Self {
            settings,
            fallback,
            cipher,
            timeout: SMTP_TIMEOUT,
        }
    }

    /// Overrides the connection/command timeout (tests of the retry classification).
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Connection/command timeout of every send.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// `smtp_config` merged over the configuration file.
    pub async fn effective(&self) -> Result<SmtpSettings> {
        let base = serde_json::to_value(SmtpSettings::from_config(&self.fallback))?;
        let mut merged = base;
        if let Some(serde_json::Value::Object(stored)) =
            self.settings.get(setting_keys::SMTP_CONFIG).await?
            && let serde_json::Value::Object(m) = &mut merged
        {
            for (k, v) in stored {
                if !v.is_null() {
                    m.insert(k, v);
                }
            }
        }
        let mut s: SmtpSettings = serde_json::from_value(merged).unwrap_or_default();
        // Passwords may be stored encrypted with the application key.
        if let Ok(plain) = self.cipher.decrypt(&s.password) {
            s.password = plain;
        }
        Ok(s)
    }
}

fn build_message(s: &SmtpSettings, mail: &Email) -> Result<Message> {
    let from_addr = s
        .from
        .trim()
        .parse()
        .map_err(|_| mailer::not_configured())?;
    // NTF-02: a white-label mail carries the reseller's name, never the main shop's.
    let from_name = match mail.from_name.trim() {
        "" => s.from_name.trim(),
        brand => brand,
    };
    let from = Mailbox::new(
        (!from_name.is_empty()).then(|| from_name.to_owned()),
        from_addr,
    );
    let to: Mailbox = mail
        .to
        .parse()
        .map_err(|_| Error::bad_request(email::KEY_EMAIL_INVALID))?;
    let mut builder = Message::builder()
        .from(from)
        .to(to)
        .subject(mail.subject.clone())
        .date_now()
        .message_id(Some(message_id(&s.from)));
    if let Ok(reply) = mail.reply_to.trim().parse::<Mailbox>() {
        builder = builder.reply_to(reply);
    }
    builder
        .header(ContentType::TEXT_PLAIN)
        .body(mail.body.clone())
        .map_err(Error::internal)
}

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, mail: &Email) -> Result<()> {
        // Placeholder addresses of Telegram-only accounts never receive mail.
        if email::is_placeholder(&mail.to) {
            return Ok(());
        }
        let s = self.effective().await?;
        if !s.enabled {
            tracing::info!(to = %mail.to, subject = %mail.subject, "smtp disabled; email logged instead of sent");
            tracing::debug!(to = %mail.to, body = %mail.body, "unsent email body");
            return Ok(());
        }
        if s.host.trim().is_empty() || s.port == 0 || s.from.trim().is_empty() {
            return Err(mailer::not_configured());
        }
        let message = build_message(&s, mail)?;
        let transport = smtp_transport(
            &s.host,
            s.port,
            s.use_ssl,
            s.use_tls,
            &s.username,
            &s.password,
            self.timeout,
        )?;
        match transport.send(message).await {
            Ok(_) => Ok(()),
            Err(e) if is_recipient_rejected(&e.to_string()) => Err(mailer::recipient_rejected()),
            Err(e) => {
                tracing::warn!(error = %e, to = %mail.to, "smtp send failed");
                Err(Error::internal(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use serde_json::{Value, json};

    use super::*;

    #[derive(Default)]
    struct Mem(Mutex<HashMap<String, Value>>);

    #[async_trait]
    impl SettingsStore for Mem {
        async fn get(&self, key: &str) -> Result<Option<Value>> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        async fn set(&self, key: &str, value: &Value) -> Result<()> {
            self.0.lock().unwrap().insert(key.into(), value.clone());
            Ok(())
        }
    }

    #[test]
    fn detects_rejected_recipients() {
        assert!(is_recipient_rejected(
            "550 5.1.1 <x@y>: Recipient address rejected"
        ));
        assert!(is_recipient_rejected(
            "permanent error (550): mailbox unavailable"
        ));
        assert!(!is_recipient_rejected("535 authentication failed"));
    }

    #[tokio::test]
    async fn settings_override_config_and_disabled_logs_only() {
        let store = Arc::new(Mem::default());
        let cipher = Cipher::from_secret("k");
        let enc = cipher.encrypt("secret-pw").unwrap();
        store.0.lock().unwrap().insert(
            "smtp_config".into(),
            json!({"enabled": false, "host": "smtp.x.io", "port": 587, "password": enc}),
        );
        let m = SmtpMailer::new(store.clone(), EmailConfig::default(), cipher);
        let s = m.effective().await.unwrap();
        assert_eq!(
            (s.host.as_str(), s.port, s.password.as_str()),
            ("smtp.x.io", 587, "secret-pw")
        );
        let mail = Email {
            to: "a@b.io".into(),
            subject: "s".into(),
            body: "b".into(),
            ..Email::default()
        };
        m.send(&mail).await.unwrap();
        // Enabled but incomplete → not configured.
        store
            .0
            .lock()
            .unwrap()
            .insert("smtp_config".into(), json!({"enabled": true, "host": ""}));
        assert_eq!(
            m.send(&mail).await.unwrap_err().key(),
            mailer::KEY_NOT_CONFIGURED
        );
    }

    fn mailer_for(port: u16) -> SmtpMailer {
        let store = Arc::new(Mem::default());
        store.0.lock().unwrap().insert(
            "smtp_config".into(),
            json!({"enabled": true, "host": "127.0.0.1", "port": port, "username": "bot",
                "password": "pw", "from": "shop@mail.example", "from_name": "Main Shop",
                "use_tls": false, "use_ssl": false}),
        );
        SmtpMailer::new(store, EmailConfig::default(), Cipher::from_secret("k"))
    }

    fn mail() -> Email {
        Email {
            to: "buyer@example.com".into(),
            subject: "注册验证码".into(),
            body: "123456".into(),
            ..Email::default()
        }
    }

    /// NTF-03 ①: a server offering only `AUTH LOGIN` (Office365) accepts the login.
    #[tokio::test]
    async fn ntf_03_login_only_server() {
        let smtp = crate::testkit::MockSmtp::start("LOGIN").await;
        mailer_for(smtp.port).send(&mail()).await.unwrap();
        let got = smtp.messages();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].auth, "LOGIN");
        assert_eq!(got[0].credentials, ("bot".into(), "pw".into()));
    }

    /// NTF-03 ②③: with `AUTH PLAIN LOGIN` offered LOGIN is chosen; the message carries
    /// `Date` and a `Message-ID` on the sender's domain, and an RFC 2047 subject.
    #[tokio::test]
    async fn ntf_03_prefers_login_and_writes_standard_headers() {
        let smtp = crate::testkit::MockSmtp::start("PLAIN LOGIN").await;
        mailer_for(smtp.port).send(&mail()).await.unwrap();
        let got = &smtp.messages()[0];
        assert_eq!(got.auth, "LOGIN");
        assert!(
            got.header("Date").is_some_and(|d| !d.is_empty()),
            "{}",
            got.data
        );
        let id = got.header("Message-ID").unwrap_or_default();
        assert!(
            id.starts_with('<') && id.ends_with("@mail.example>"),
            "{id}"
        );
        let subject = got.header("Subject").unwrap_or_default();
        assert!(subject.starts_with("=?utf-8?"), "{subject}");
        assert_eq!(
            got.header("From").as_deref(),
            Some("\"Main Shop\" <shop@mail.example>")
        );
    }

    /// NTF-02: a branded mail overrides the From name and sets a sanitised Reply-To.
    #[tokio::test]
    async fn ntf_02_brand_headers() {
        let smtp = crate::testkit::MockSmtp::start("LOGIN").await;
        let branded = Email {
            from_name: "White Label".into(),
            reply_to: "help@shop.example".into(),
            ..mail()
        };
        mailer_for(smtp.port).send(&branded).await.unwrap();
        let got = &smtp.messages()[0];
        assert_eq!(
            got.header("From").as_deref(),
            Some("\"White Label\" <shop@mail.example>")
        );
        assert_eq!(got.header("Reply-To").as_deref(), Some("help@shop.example"));
    }

    /// NTF-05: a malformed recipient is a non-retryable 4xx-class error; an unreachable
    /// server is an internal (retryable) one.
    #[tokio::test]
    async fn ntf_05_error_classes() {
        let smtp = crate::testkit::MockSmtp::start("LOGIN").await;
        let bad = Email {
            to: "not-an-email".into(),
            ..mail()
        };
        let err = mailer_for(smtp.port).send(&bad).await.unwrap_err();
        assert_eq!(err.key(), email::KEY_EMAIL_INVALID);
        // nothing listens on the port any more
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let err = mailer_for(port)
            .with_timeout(Duration::from_millis(500))
            .send(&mail())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), zs_domain::ErrorKind::Internal);
    }
}
