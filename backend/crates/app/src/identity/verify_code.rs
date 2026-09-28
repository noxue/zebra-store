//! Sending and checking email verification codes.

use std::sync::Arc;

use chrono::Duration;
use rand::Rng;
use serde_json::Value;
use zs_domain::identity::mailer::{self, BrandScope, Email, MailBrands, Mailer};
use zs_domain::identity::verify_code::{
    self, Check, NewVerifyCode, Policy, Purpose, VerifyCodeRepo,
};
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, Result};
use zs_shared::clock::Clock;

/// Verification code service (cheap to clone).
#[derive(Clone)]
pub struct VerifyCodeService {
    repo: Arc<dyn VerifyCodeRepo>,
    mailer: Arc<dyn Mailer>,
    settings: Arc<dyn SettingsStore>,
    fallback: Policy,
    clock: Arc<dyn Clock>,
    brands: Option<Arc<dyn MailBrands>>,
}

impl std::fmt::Debug for VerifyCodeService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerifyCodeService")
            .field("fallback", &self.fallback)
            .finish()
    }
}

/// Generates a numeric code of `length` digits.
pub fn random_code(length: usize) -> String {
    let mut rng = rand::rng();
    (0..length)
        .map(|_| char::from(b'0' + rng.random_range(0..10u8)))
        .collect()
}

impl VerifyCodeService {
    /// `fallback` is `email.verify_code` from the configuration file.
    pub fn new(
        repo: Arc<dyn VerifyCodeRepo>,
        mailer: Arc<dyn Mailer>,
        settings: Arc<dyn SettingsStore>,
        fallback: Policy,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            mailer,
            settings,
            fallback,
            clock,
            brands: None,
        }
    }

    /// Brands verification mails by storefront (NTF-02).
    #[must_use]
    pub fn with_brands(mut self, brands: Arc<dyn MailBrands>) -> Self {
        self.brands = Some(brands);
        self
    }

    pub fn mailer(&self) -> &Arc<dyn Mailer> {
        &self.mailer
    }

    /// Effective policy: `smtp_config.verify_code` overrides the configuration file.
    pub async fn policy(&self) -> Result<Policy> {
        let mut policy = self.fallback;
        let smtp = self.settings.get(setting_keys::SMTP_CONFIG).await?;
        if let Some(vc) = smtp.as_ref().and_then(|v| v.get("verify_code")) {
            let int = |k: &str| vc.get(k).and_then(Value::as_i64).filter(|n| *n > 0);
            if let Some(n) = int("expire_minutes") {
                policy.expire_minutes = n;
            }
            if let Some(n) = int("send_interval_seconds") {
                policy.send_interval_seconds = n;
            }
            if let Some(n) = int("max_attempts") {
                policy.max_attempts = i32::try_from(n).unwrap_or(i32::MAX);
            }
            if let Some(n) = int("length") {
                policy.length = usize::try_from(n).unwrap_or(0);
            }
        }
        Ok(policy.resolved())
    }

    /// Sends a new code to `email` (already normalised) with the main shop's brand.
    pub async fn send(&self, email: &str, purpose: Purpose, locale: &str) -> Result<()> {
        self.send_for(email, purpose, locale, &BrandScope::default())
            .await
    }

    /// Sends a new code branded for `scope` (the requesting storefront), respecting the
    /// resend interval. A failing brand lookup fails the send (no fallback to the main brand).
    pub async fn send_for(
        &self,
        email: &str,
        purpose: Purpose,
        locale: &str,
        scope: &BrandScope,
    ) -> Result<()> {
        let policy = self.policy().await?;
        let now = self.clock.now();
        if let Some(latest) = self.repo.latest(email, purpose).await?
            && now - latest.sent_at < Duration::seconds(policy.send_interval_seconds)
        {
            return Err(Error::too_many(verify_code::keys::TOO_FREQUENT));
        }
        let code = random_code(policy.length);
        let brand = match &self.brands {
            Some(brands) => brands.resolve(scope).await?,
            None => mailer::MailBrand::default(),
        };
        let (subject, body) = mailer::verify_code_content(&code, purpose, locale);
        let (subject, body) = mailer::apply_verify_code_brand(locale, subject, body, &brand);
        self.mailer
            .send(&Email {
                to: email.to_owned(),
                subject,
                body,
                from_name: brand.from_name,
                reply_to: brand.reply_to,
            })
            .await?;
        self.repo
            .create(&NewVerifyCode {
                email: email.to_owned(),
                user_id: None,
                purpose,
                code,
                expires_at: now + Duration::minutes(policy.expire_minutes),
                sent_at: now,
            })
            .await
    }

    /// Checks `code` for `email` + `purpose` and consumes it on success.
    pub async fn verify(&self, email: &str, purpose: Purpose, code: &str) -> Result<()> {
        let policy = self.policy().await?;
        let now = self.clock.now();
        let record = self.repo.latest(email, purpose).await?;
        match verify_code::check(record.as_ref(), code, now, policy.max_attempts)? {
            Check::Mismatch => {
                if let Some(r) = &record {
                    self.repo.increment_attempt(r.id).await?;
                }
                Err(Error::bad_request(verify_code::keys::INVALID))
            }
            Check::Valid => {
                let Some(r) = record else {
                    return Err(Error::bad_request(verify_code::keys::INVALID));
                };
                // Conditional update: a code can only be consumed once even under races.
                if self.repo.mark_verified(r.id, now).await? {
                    Ok(())
                } else {
                    Err(Error::bad_request(verify_code::keys::INVALID))
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use zs_domain::Id;
    use zs_domain::identity::verify_code::VerifyCode;
    use zs_shared::clock::SystemClock;

    use super::*;

    #[derive(Default)]
    pub struct MemCodes(pub Mutex<Vec<VerifyCode>>);

    #[async_trait]
    impl VerifyCodeRepo for MemCodes {
        async fn latest(&self, email: &str, purpose: Purpose) -> Result<Option<VerifyCode>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|c| c.email == email && c.purpose == purpose.as_str())
                .cloned())
        }
        async fn create(&self, c: &NewVerifyCode) -> Result<()> {
            let mut v = self.0.lock().unwrap();
            let id = Id::try_from(v.len()).unwrap() + 1;
            v.push(VerifyCode {
                id,
                email: c.email.clone(),
                user_id: c.user_id,
                purpose: c.purpose.as_str().into(),
                code: c.code.clone(),
                expires_at: c.expires_at,
                verified_at: None,
                attempt_count: 0,
                sent_at: c.sent_at,
            });
            Ok(())
        }
        async fn mark_verified(&self, id: Id, at: DateTime<Utc>) -> Result<bool> {
            let mut v = self.0.lock().unwrap();
            let c = v.iter_mut().find(|c| c.id == id).unwrap();
            if c.verified_at.is_some() {
                return Ok(false);
            }
            c.verified_at = Some(at);
            Ok(true)
        }
        async fn increment_attempt(&self, id: Id) -> Result<()> {
            let mut v = self.0.lock().unwrap();
            v.iter_mut().find(|c| c.id == id).unwrap().attempt_count += 1;
            Ok(())
        }
    }

    #[derive(Default)]
    pub struct MemMailer(pub Mutex<Vec<Email>>);

    #[async_trait]
    impl Mailer for MemMailer {
        async fn send(&self, email: &Email) -> Result<()> {
            self.0.lock().unwrap().push(email.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    pub struct MemSettings(pub Mutex<HashMap<String, Value>>);

    #[async_trait]
    impl SettingsStore for MemSettings {
        async fn get(&self, key: &str) -> Result<Option<Value>> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        async fn set(&self, key: &str, value: &Value) -> Result<()> {
            self.0.lock().unwrap().insert(key.into(), value.clone());
            Ok(())
        }
    }

    fn service() -> (
        VerifyCodeService,
        Arc<MemCodes>,
        Arc<MemMailer>,
        Arc<MemSettings>,
    ) {
        let codes = Arc::new(MemCodes::default());
        let mailer = Arc::new(MemMailer::default());
        let settings = Arc::new(MemSettings::default());
        let svc = VerifyCodeService::new(
            codes.clone(),
            mailer.clone(),
            settings.clone(),
            Policy::default(),
            Arc::new(SystemClock),
        );
        (svc, codes, mailer, settings)
    }

    #[tokio::test]
    async fn send_verify_and_resend_interval() {
        let (svc, codes, mailer, _) = service();
        svc.send("a@b.co", Purpose::Register, "en-US")
            .await
            .unwrap();
        let code = codes.0.lock().unwrap()[0].code.clone();
        assert_eq!(code.len(), 6);
        assert!(mailer.0.lock().unwrap()[0].body.contains(&code));
        assert_eq!(
            svc.send("a@b.co", Purpose::Register, "en-US")
                .await
                .unwrap_err()
                .key(),
            verify_code::keys::TOO_FREQUENT
        );
        // Other purposes are independent.
        svc.send("a@b.co", Purpose::Reset, "en-US").await.unwrap();
        assert_eq!(
            svc.verify("a@b.co", Purpose::Register, "nope")
                .await
                .unwrap_err()
                .key(),
            verify_code::keys::INVALID
        );
        assert_eq!(codes.0.lock().unwrap()[0].attempt_count, 1);
        svc.verify("a@b.co", Purpose::Register, &code)
            .await
            .unwrap();
        // Consumed.
        assert_eq!(
            svc.verify("a@b.co", Purpose::Register, &code)
                .await
                .unwrap_err()
                .key(),
            verify_code::keys::INVALID
        );
    }

    #[tokio::test]
    async fn attempts_exhaust_and_settings_override_policy() {
        let (svc, codes, _, settings) = service();
        settings.0.lock().unwrap().insert(
            "smtp_config".into(),
            serde_json::json!({"verify_code": {"max_attempts": 2, "length": 8}}),
        );
        assert_eq!(svc.policy().await.unwrap().max_attempts, 2);
        svc.send("x@y.io", Purpose::Reset, "zh-CN").await.unwrap();
        let code = codes.0.lock().unwrap()[0].code.clone();
        assert_eq!(code.len(), 8);
        for _ in 0..2 {
            assert!(svc.verify("x@y.io", Purpose::Reset, "0").await.is_err());
        }
        assert_eq!(
            svc.verify("x@y.io", Purpose::Reset, &code)
                .await
                .unwrap_err()
                .key(),
            verify_code::keys::ATTEMPTS_EXCEEDED
        );
    }
}
