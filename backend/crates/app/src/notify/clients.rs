//! Channel client administration and request authentication (port of
//! `modules/channelclient/application`).

use std::sync::Arc;

use rand::RngCore;
use zs_domain::notify::channel::{
    AuthFailure, AuthResult, CHANNEL_TYPE_TELEGRAM_BOT, CREDENTIAL_BYTES, ChannelAuthError,
    ChannelClient, ChannelClientRepo, ClientDetail, NewChannelClient, STATUS_ACTIVE, keys,
    mask_bot_token,
};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::crypto::Cipher;
use zs_shared::sign;

/// Fields of a new client (original `createRequest`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateClient {
    pub name: String,
    pub channel_type: String,
    pub description: String,
    pub bot_token: String,
    pub callback_url: String,
}

/// Partial update (original `updateRequest`): `None` keeps, `Some("")` clears.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateClient {
    pub name: String,
    pub description: String,
    pub bot_token: Option<String>,
    pub callback_url: Option<String>,
}

/// Decrypted callback endpoint of the active client of a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveEndpoint {
    pub client_id: Id,
    pub channel_key: String,
    pub callback_url: String,
    pub channel_secret: String,
}

/// A signed channel request to authenticate.
#[derive(Debug, Clone, Copy)]
pub struct SignedRequest<'a> {
    pub key: &'a str,
    pub timestamp: &'a str,
    pub signature: &'a str,
    pub method: &'a str,
    pub path: &'a str,
    pub body: &'a [u8],
}

/// Channel client service (cheap to clone).
#[derive(Clone)]
pub struct ChannelClientService {
    repo: Arc<dyn ChannelClientRepo>,
    cipher: Cipher,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for ChannelClientService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChannelClientService")
    }
}

fn random_hex() -> String {
    let mut bytes = [0u8; CREDENTIAL_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    hex_encode(&bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn not_found() -> Error {
    Error::not_found("error.not_found")
}

impl ChannelClientService {
    pub fn new(repo: Arc<dyn ChannelClientRepo>, cipher: Cipher, clock: Arc<dyn Clock>) -> Self {
        Self {
            repo,
            cipher,
            clock,
        }
    }

    fn encrypt(&self, plain: &str) -> Result<String> {
        self.cipher.encrypt(plain).map_err(Error::internal)
    }

    fn decrypt(&self, cipher_text: &str) -> Result<String> {
        if cipher_text.is_empty() {
            return Ok(String::new());
        }
        self.cipher.decrypt(cipher_text).map_err(Error::internal)
    }

    /// Detail with the given plaintext secret (`None` = decrypt, empty on failure).
    fn detail(&self, c: &ChannelClient, secret: Option<String>) -> ClientDetail {
        let secret = secret.unwrap_or_else(|| self.decrypt(&c.channel_secret).unwrap_or_default());
        let bot_token = self
            .decrypt(&c.bot_token)
            .map(|t| mask_bot_token(&t))
            .unwrap_or_default();
        ClientDetail {
            id: c.id,
            name: c.name.clone(),
            channel_type: c.channel_type.clone(),
            channel_key: c.channel_key.clone(),
            channel_secret: secret,
            bot_token,
            bot_token_set: !c.bot_token.is_empty(),
            callback_url: c.callback_url.clone(),
            description: c.description.clone(),
            status: c.status,
        }
    }

    async fn load(&self, id: Id) -> Result<ChannelClient> {
        self.repo.get(id).await?.ok_or_else(not_found)
    }

    pub async fn list(&self) -> Result<Vec<ClientDetail>> {
        let clients = self
            .repo
            .list()
            .await
            .map_err(|e| e.or_internal(keys::LIST_FAILED))?;
        Ok(clients.iter().map(|c| self.detail(c, None)).collect())
    }

    pub async fn create(&self, input: CreateClient) -> Result<ClientDetail> {
        let secret = random_hex();
        let bot_token = input.bot_token.trim().to_owned();
        let new = NewChannelClient {
            name: input.name.trim().to_owned(),
            channel_type: input.channel_type.trim().to_owned(),
            channel_key: random_hex(),
            channel_secret: self.encrypt(&secret)?,
            bot_token: if bot_token.is_empty() {
                String::new()
            } else {
                self.encrypt(&bot_token)?
            },
            callback_url: input.callback_url.trim().to_owned(),
            description: input.description.trim().to_owned(),
        };
        let client = self
            .repo
            .create(&new, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::CREATE_FAILED))?;
        Ok(self.detail(&client, Some(secret)))
    }

    pub async fn get(&self, id: Id) -> Result<ClientDetail> {
        let c = self
            .load(id)
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))?;
        let secret = self
            .decrypt(&c.channel_secret)
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))?;
        Ok(self.detail(&c, Some(secret)))
    }

    pub async fn update(&self, id: Id, input: UpdateClient) -> Result<ClientDetail> {
        let fail = |e: Error| e.or_internal(keys::UPDATE_FAILED);
        let mut c = self.load(id).await.map_err(fail)?;
        if !input.name.trim().is_empty() {
            c.name = input.name.trim().to_owned();
        }
        c.description = input.description.trim().to_owned();
        if let Some(url) = input.callback_url {
            c.callback_url = url.trim().to_owned();
        }
        if let Some(token) = input.bot_token {
            let token = token.trim().to_owned();
            c.bot_token = if token.is_empty() {
                String::new()
            } else {
                self.encrypt(&token).map_err(fail)?
            };
        }
        self.repo.update(&c, self.clock.now()).await.map_err(fail)?;
        let secret = self.decrypt(&c.channel_secret).map_err(fail)?;
        Ok(self.detail(&c, Some(secret)))
    }

    pub async fn set_status(&self, id: Id, status: i32) -> Result<()> {
        let fail = |e: Error| e.or_internal(keys::UPDATE_FAILED);
        if status != 0 && status != 1 {
            return Err(Error::invalid());
        }
        let mut c = self.load(id).await.map_err(fail)?;
        c.status = status;
        self.repo.update(&c, self.clock.now()).await.map_err(fail)
    }

    pub async fn reset_secret(&self, id: Id) -> Result<ClientDetail> {
        let fail = |e: Error| e.or_internal(keys::RESET_FAILED);
        let mut c = self.load(id).await.map_err(fail)?;
        let secret = random_hex();
        c.channel_secret = self.encrypt(&secret).map_err(fail)?;
        self.repo.update(&c, self.clock.now()).await.map_err(fail)?;
        Ok(self.detail(&c, Some(secret)))
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        let fail = |e: Error| e.or_internal(keys::DELETE_FAILED);
        let c = self.load(id).await.map_err(fail)?;
        self.repo
            .soft_delete(c.id, self.clock.now())
            .await
            .map_err(fail)
    }

    /// Verifies the channel headers and HMAC signature (original `VerifyChannelSignature`).
    /// Missing/expired/unknown/bad-signature are indistinguishable for callers (NTF-06).
    pub async fn authenticate(&self, req: SignedRequest<'_>) -> AuthResult {
        use ChannelAuthError::Rejected;
        let (key, ts, signature) = (req.key.trim(), req.timestamp.trim(), req.signature.trim());
        if key.is_empty() || ts.is_empty() || signature.is_empty() {
            return Err(Rejected(AuthFailure::Missing));
        }
        let timestamp: i64 = ts.parse().map_err(|_| Rejected(AuthFailure::Missing))?;
        if !sign::timestamp_valid(timestamp, self.clock.now().timestamp()) {
            return Err(Rejected(AuthFailure::Expired));
        }
        let client = self
            .repo
            .find_by_key(key)
            .await?
            .ok_or(Rejected(AuthFailure::UnknownKey))?;
        if client.status != STATUS_ACTIVE {
            return Err(Rejected(AuthFailure::Disabled));
        }
        let secret = self.decrypt(&client.channel_secret)?;
        if secret.is_empty()
            || !sign::verify(
                &secret, req.method, req.path, signature, timestamp, req.body,
            )
        {
            return Err(Rejected(AuthFailure::BadSignature));
        }
        if let Err(error) = self.repo.touch(client.id, self.clock.now()).await {
            tracing::warn!(%error, "channel client last_used_at update failed");
        }
        Ok(client)
    }

    /// Decrypted bot token of a client (empty when unset or undecryptable).
    pub async fn bot_token_of(&self, client_id: Id) -> Result<String> {
        let c = self.load(client_id).await?;
        self.decrypt(&c.bot_token)
    }

    /// Bot token of the active `telegram_bot` client (broadcasts).
    pub async fn active_bot_token(&self) -> Result<Option<String>> {
        let Some(c) = self
            .repo
            .find_active_by_type(CHANNEL_TYPE_TELEGRAM_BOT)
            .await?
        else {
            return Ok(None);
        };
        let token = self.decrypt(&c.bot_token)?;
        Ok(Some(token.trim().to_owned()).filter(|t| !t.is_empty()))
    }

    /// Callback endpoint of the active client of `channel_type` (bot notifications).
    pub async fn active_endpoint(&self, channel_type: &str) -> Result<Option<ActiveEndpoint>> {
        let Some(c) = self.repo.find_active_by_type(channel_type.trim()).await? else {
            return Ok(None);
        };
        Ok(Some(ActiveEndpoint {
            client_id: c.id,
            channel_key: c.channel_key.clone(),
            callback_url: c.callback_url.clone(),
            channel_secret: self.decrypt(&c.channel_secret)?,
        }))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use zs_shared::clock::FixedClock;

    use super::*;

    #[derive(Default)]
    pub struct MemClients(pub Mutex<Vec<ChannelClient>>);

    #[async_trait]
    impl ChannelClientRepo for MemClients {
        async fn create(&self, c: &NewChannelClient, now: DateTime<Utc>) -> Result<ChannelClient> {
            let mut all = self.0.lock().unwrap();
            let row = ChannelClient {
                id: i64::try_from(all.len()).unwrap() + 1,
                name: c.name.clone(),
                channel_type: c.channel_type.clone(),
                channel_key: c.channel_key.clone(),
                channel_secret: c.channel_secret.clone(),
                bot_token: c.bot_token.clone(),
                callback_url: c.callback_url.clone(),
                status: 1,
                description: c.description.clone(),
                last_used_at: None,
                created_at: now,
                updated_at: now,
            };
            all.push(row.clone());
            Ok(row)
        }
        async fn get(&self, id: Id) -> Result<Option<ChannelClient>> {
            Ok(self.0.lock().unwrap().iter().find(|c| c.id == id).cloned())
        }
        async fn find_by_key(&self, key: &str) -> Result<Option<ChannelClient>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.channel_key == key)
                .cloned())
        }
        async fn find_active_by_type(&self, t: &str) -> Result<Option<ChannelClient>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.channel_type == t && c.status == 1)
                .cloned())
        }
        async fn list(&self) -> Result<Vec<ChannelClient>> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn update(&self, c: &ChannelClient, _now: DateTime<Utc>) -> Result<()> {
            let mut all = self.0.lock().unwrap();
            if let Some(slot) = all.iter_mut().find(|x| x.id == c.id) {
                *slot = c.clone();
            }
            Ok(())
        }
        async fn touch(&self, id: Id, at: DateTime<Utc>) -> Result<()> {
            let mut all = self.0.lock().unwrap();
            if let Some(slot) = all.iter_mut().find(|x| x.id == id) {
                slot.last_used_at = Some(at);
            }
            Ok(())
        }
        async fn soft_delete(&self, id: Id, _at: DateTime<Utc>) -> Result<()> {
            self.0.lock().unwrap().retain(|c| c.id != id);
            Ok(())
        }
    }

    pub fn service(now: DateTime<Utc>) -> (ChannelClientService, Arc<MemClients>) {
        let repo = Arc::new(MemClients::default());
        let svc = ChannelClientService::new(
            repo.clone(),
            Cipher::from_secret("k"),
            Arc::new(FixedClock(now)),
        );
        (svc, repo)
    }

    #[tokio::test]
    async fn create_encrypts_and_returns_plain_secret_once() {
        let (svc, repo) = service(Utc::now());
        let d = svc
            .create(CreateClient {
                name: " Bot ".into(),
                channel_type: "telegram_bot".into(),
                bot_token: "123456:ABCDEFGHIJ".into(),
                ..CreateClient::default()
            })
            .await
            .unwrap();
        assert_eq!(d.channel_key.len(), 64);
        assert_eq!(d.channel_secret.len(), 64);
        assert_eq!(d.bot_token, "1234*********GHIJ");
        assert!(d.bot_token_set);
        let stored = repo.0.lock().unwrap()[0].clone();
        assert_ne!(stored.channel_secret, d.channel_secret);
        assert_ne!(stored.bot_token, "123456:ABCDEFGHIJ");
        assert_eq!(
            svc.active_bot_token().await.unwrap().as_deref(),
            Some("123456:ABCDEFGHIJ")
        );

        let upd = svc
            .update(
                d.id,
                UpdateClient {
                    bot_token: Some(String::new()),
                    ..UpdateClient::default()
                },
            )
            .await
            .unwrap();
        assert!(!upd.bot_token_set);
        assert_eq!(upd.name, "Bot", "empty name keeps the old one");
        let reset = svc.reset_secret(d.id).await.unwrap();
        assert_ne!(reset.channel_secret, d.channel_secret);
    }

    // NTF-06 (2): unknown key and bad signature are the same failure class; disabled differs.
    #[tokio::test]
    async fn authenticate_checks_timestamp_key_signature_status() {
        let now = Utc::now();
        let (svc, _) = service(now);
        let d = svc
            .create(CreateClient {
                name: "b".into(),
                channel_type: "telegram_bot".into(),
                ..CreateClient::default()
            })
            .await
            .unwrap();
        let ts = now.timestamp().to_string();
        let body = br#"{"a":1}"#;
        let good = sign::sign(
            &d.channel_secret,
            "POST",
            "/api/v1/channel/me",
            now.timestamp(),
            body,
        );
        let ok = svc
            .authenticate(SignedRequest {
                key: &d.channel_key,
                timestamp: &ts,
                signature: &good,
                method: "POST",
                path: "/api/v1/channel/me",
                body,
            })
            .await;
        assert!(ok.is_ok());
        let fail = |r: AuthResult| match r {
            Err(ChannelAuthError::Rejected(f)) => f,
            other => panic!("unexpected {other:?}"),
        };
        let base = SignedRequest {
            key: &d.channel_key,
            timestamp: &ts,
            signature: &good,
            method: "POST",
            path: "/api/v1/channel/me",
            body,
        };
        assert_eq!(
            fail(
                svc.authenticate(SignedRequest {
                    key: "nope",
                    ..base
                })
                .await
            ),
            AuthFailure::UnknownKey
        );
        assert_eq!(
            fail(
                svc.authenticate(SignedRequest {
                    body: b"{}",
                    ..base
                })
                .await
            ),
            AuthFailure::BadSignature
        );
        let old = (now.timestamp() - 61).to_string();
        assert_eq!(
            fail(
                svc.authenticate(SignedRequest {
                    timestamp: &old,
                    ..base
                })
                .await
            ),
            AuthFailure::Expired
        );
        assert_eq!(
            fail(
                svc.authenticate(SignedRequest {
                    signature: "",
                    ..base
                })
                .await
            ),
            AuthFailure::Missing
        );
        svc.set_status(d.id, 0).await.unwrap();
        assert_eq!(fail(svc.authenticate(base).await), AuthFailure::Disabled);
    }
}
