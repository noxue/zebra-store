//! Telegram / Google test fixtures: an in-process mock of the OIDC token and
//! JWKS endpoints, RS256 signing with a fixed test key, Telegram payload
//! signing and raw request helpers (cookies, forms, headers).

#![allow(
    dead_code,
    reason = "shared by several identity OAuth test binaries that use different helpers"
)]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test harness: failures should abort the test"
)]

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http_body_util::BodyExt;
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::signature::{SignatureEncoding, Signer};
use rsa::traits::PublicKeyParts;
use serde_json::{Value, json};
use sha2::Sha256;
use tower::ServiceExt;
use zs_domain::identity::oauth::OAuthEndpoints;
use zs_domain::identity::telegram;

use super::identity_common::App;

/// Fixed 2048-bit RSA key used to sign test tokens (test fixture only).
const TEST_KEY_PEM: &str = include_str!("test_key.pem");
/// Kid of the published test key.
pub const KID: &str = "test-kid";
/// Bot token of the Telegram tests (bot id 123456 → OIDC client id).
pub const BOT_TOKEN: &str = "123456:TEST-bot-token";
/// Telegram OIDC client id derived from [`BOT_TOKEN`].
pub const TG_CLIENT_ID: &str = "123456";
pub const TG_CLIENT_SECRET: &str = "tg-client-secret";
pub const TG_REDIRECT: &str = "https://shop.test/auth/telegram/callback";
/// Google client id of the tests.
pub const GOOGLE_CLIENT_ID: &str = "test-client.apps.googleusercontent.com";

pub fn key() -> RsaPrivateKey {
    RsaPrivateKey::from_pkcs8_pem(TEST_KEY_PEM).unwrap()
}

/// Signs `claims` as an RS256 JWT with `kid`.
pub fn sign(claims: &Value, kid: &str) -> String {
    let header = json!({"alg": "RS256", "typ": "JWT", "kid": kid});
    let input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let signer = SigningKey::<Sha256>::new(key());
    let sig = signer.sign(input.as_bytes()).to_vec();
    format!("{input}.{}", URL_SAFE_NO_PAD.encode(sig))
}

/// JWKS publishing the test key under `kid`.
pub fn jwks(kid: &str) -> Value {
    let public = key().to_public_key();
    json!({"keys": [{
        "kty": "RSA", "kid": kid, "use": "sig", "alg": "RS256",
        "n": URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
        "e": URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
    }]})
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Observable state of the mock upstream.
#[derive(Debug, Default)]
pub struct MockState {
    /// `id_token` returned by the token endpoint.
    pub id_token: String,
    /// Status of the token endpoint (200 when 0).
    pub token_status: u16,
    /// Last token request: `(authorization header, form body)`.
    pub last_token_request: Option<(String, String)>,
    pub google_jwks_hits: usize,
    pub google_jwks_status: u16,
}

#[derive(Clone)]
pub struct Mock {
    pub base: String,
    pub state: Arc<Mutex<MockState>>,
}

async fn google_jwks(State(st): State<Arc<Mutex<MockState>>>) -> axum::response::Response {
    let status = {
        let mut s = st.lock().unwrap();
        s.google_jwks_hits += 1;
        s.google_jwks_status
    };
    if status != 0 && status != 200 {
        return StatusCode::from_u16(status).unwrap().into_response();
    }
    (
        [(header::CACHE_CONTROL, "public, max-age=3600")],
        axum::Json(jwks(KID)),
    )
        .into_response()
}

async fn telegram_jwks() -> axum::Json<Value> {
    axum::Json(jwks(KID))
}

async fn token(
    State(st): State<Arc<Mutex<MockState>>>,
    headers: HeaderMap,
    body: String,
) -> axum::response::Response {
    let mut s = st.lock().unwrap();
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    s.last_token_request = Some((auth, body));
    if s.token_status != 0 && s.token_status != 200 {
        return StatusCode::from_u16(s.token_status)
            .unwrap()
            .into_response();
    }
    axum::Json(json!({"access_token": "at", "token_type": "Bearer", "id_token": s.id_token}))
        .into_response()
}

impl Mock {
    /// Starts the mock on an ephemeral port.
    pub async fn start() -> Self {
        let state = Arc::new(Mutex::new(MockState::default()));
        let router = Router::new()
            .route("/google/jwks", get(google_jwks))
            .route("/telegram/jwks", get(telegram_jwks))
            .route("/telegram/token", post(token))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            base: format!("http://{addr}"),
            state,
        }
    }

    pub fn endpoints(&self) -> OAuthEndpoints {
        OAuthEndpoints {
            telegram_auth: "https://oauth.telegram.org/auth".into(),
            telegram_token: format!("{}/telegram/token", self.base),
            telegram_jwks: format!("{}/telegram/jwks", self.base),
            google_jwks: format!("{}/google/jwks", self.base),
        }
    }

    pub fn set_id_token(&self, token: &str) {
        self.state.lock().unwrap().id_token = token.to_owned();
    }
}

/// App with the mock upstream wired in.
pub async fn app_with_mock() -> (App, Mock) {
    let app = App::new().await;
    let mock = Mock::start().await;
    app.services.identity.oauth.set_endpoints(mock.endpoints());
    (app, mock)
}

/// Enables Telegram login (widget mode, or OIDC when `oidc`).
pub async fn enable_telegram(app: &App, oidc: bool) {
    let mut cfg = json!({
        "enabled": true, "bot_username": "zebra_test_bot", "bot_token": BOT_TOKEN,
        "login_expire_seconds": 3600, "replay_ttl_seconds": 3600,
        "mini_app_url": "https://t.me/zebra_test_bot/app"
    });
    if oidc {
        cfg["client_secret"] = json!(TG_CLIENT_SECRET);
        cfg["oidc_redirect_uri"] = json!(TG_REDIRECT);
    }
    app.set_setting("telegram_auth_config", cfg).await;
}

pub async fn enable_google(app: &App) {
    app.set_setting(
        "google_auth_config",
        json!({"enabled": true, "client_id": GOOGLE_CLIENT_ID}),
    )
    .await;
}

/// A signed Login Widget payload for `id` issued at `auth_date`.
pub fn widget(id: i64, auth_date: i64) -> Value {
    let payload = telegram::WidgetPayload {
        id,
        first_name: "Tele".into(),
        last_name: "Gram".into(),
        username: format!("tg{id}"),
        photo_url: "https://t.me/i/userpic/1.jpg".into(),
        auth_date,
        hash: String::new(),
    };
    let hash = telegram::widget_hash(BOT_TOKEN, &payload.data_check_string());
    json!({
        "id": id, "first_name": "Tele", "last_name": "Gram", "username": format!("tg{id}"),
        "photo_url": "https://t.me/i/userpic/1.jpg", "auth_date": auth_date, "hash": hash
    })
}

fn escape(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Signed Mini App `initData` (with the newer `signature` field, AUTH-04).
pub fn init_data(id: i64, auth_date: i64) -> String {
    let user = json!({"id": id, "first_name": "Mini", "username": format!("mini{id}")}).to_string();
    let fields = [
        ("auth_date", auth_date.to_string()),
        ("query_id", "AAHdF6IQ".to_owned()),
        ("signature", "sig-value".to_owned()),
        ("user", user),
    ];
    let dcs = fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");
    let hash = telegram::miniapp_hash(BOT_TOKEN, &dcs);
    let mut parts: Vec<String> = fields
        .iter()
        .map(|(k, v)| format!("{k}={}", escape(v)))
        .collect();
    parts.push(format!("hash={hash}"));
    parts.join("&")
}

/// Telegram OIDC `id_token` claims.
pub fn tg_claims(id: i64, sub: &str) -> Value {
    json!({
        "iss": "https://oauth.telegram.org", "aud": TG_CLIENT_ID, "sub": sub, "id": id,
        "exp": now() + 600, "iat": now(), "name": "Oidc User", "preferred_username": format!("oidc{id}"),
        "picture": "https://t.me/p.jpg"
    })
}

/// Google ID token claims for `email`.
pub fn google_claims(sub: &str, email: &str) -> Value {
    json!({
        "iss": "https://accounts.google.com", "aud": GOOGLE_CLIENT_ID, "azp": GOOGLE_CLIENT_ID,
        "sub": sub, "email": email, "email_verified": true, "name": "Goo Gle",
        "picture": "https://lh3.googleusercontent.com/a/p", "iat": now(), "exp": now() + 3600
    })
}

/// Raw response parts.
pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl Reply {
    /// All `Set-Cookie` header values.
    pub fn set_cookies(&self) -> Vec<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap().to_owned())
            .collect()
    }

    /// Value of the `name` cookie set by the response (empty when cleared).
    pub fn cookie(&self, name: &str) -> Option<String> {
        self.set_cookies().iter().rev().find_map(|c| {
            let first = c.split(';').next()?;
            let (k, v) = first.split_once('=')?;
            (k == name).then(|| v.to_owned())
        })
    }

    pub fn location(&self) -> String {
        self.headers
            .get(header::LOCATION)
            .map(|v| v.to_str().unwrap().to_owned())
            .unwrap_or_default()
    }
}

/// Sends a request with optional bearer token, host, cookies and body.
pub async fn send(
    app: &App,
    method: &str,
    uri: &str,
    token: Option<&str>,
    host: &str,
    cookies: &str,
    body: Option<(&str, String)>,
) -> Reply {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, host);
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    if !cookies.is_empty() {
        req = req.header(header::COOKIE, cookies);
    }
    let req = match body {
        Some((content_type, b)) => req
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(b))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

/// Number of `users` rows.
pub async fn user_count(app: &App) -> usize {
    use sea_orm::EntityTrait;
    zs_infra::db::entity::users::Entity::find()
        .all(&app.db)
        .await
        .unwrap()
        .len()
}

/// `user_oauth_identities` rows as `(user_id, provider, provider_user_id)`.
pub async fn identities(app: &App) -> Vec<(i64, String, String)> {
    use sea_orm::EntityTrait;
    zs_infra::db::entity::user_oauth_identities::Entity::find()
        .all(&app.db)
        .await
        .unwrap()
        .into_iter()
        .map(|m| (m.user_id, m.provider, m.provider_user_id))
        .collect()
}

/// Latest user login log as `(status, fail_reason, login_source)`.
pub async fn last_login_log(app: &App) -> (String, String, String) {
    use sea_orm::{EntityTrait, QueryOrder};
    use zs_infra::db::entity::user_login_logs as l;
    let m = l::Entity::find()
        .order_by_desc(l::Column::Id)
        .one(&app.db)
        .await
        .unwrap()
        .expect("a login log");
    (m.status, m.fail_reason, m.login_source)
}

/// Asserts an error envelope with `code` and the (zh-CN) message of `key`.
pub fn fails(v: &Value, code: i64, key: &str) {
    assert_eq!(v["status_code"], code, "unexpected envelope {v}");
    assert_eq!(
        v["msg"],
        zs_api::i18n::translate("zh-CN", key),
        "unexpected envelope {v} (expected {key})"
    );
}

/// Enables 2FA for the user of `token`; returns the TOTP secret.
pub async fn enable_2fa(app: &App, token: &str) -> String {
    use super::identity_common::{data, totp};
    let setup = app
        .call("POST", "/api/v1/me/2fa/setup", None, Some(token))
        .await;
    let secret = data(&setup)["secret"].as_str().unwrap().to_owned();
    let res = app
        .call(
            "POST",
            "/api/v1/me/2fa/enable",
            Some(json!({"code": totp(&secret)})),
            Some(token),
        )
        .await;
    data(&res);
    secret
}

/// Inserts an active default member level and returns its id.
pub async fn default_level(app: &App) -> i64 {
    use sea_orm::{ActiveModelTrait, Set};
    let now = chrono::Utc::now();
    zs_infra::db::entity::member_levels::ActiveModel {
        name_json: Set(Some(json!({"zh-CN": "普通"}))),
        slug: Set("normal".into()),
        icon: Set(String::new()),
        discount_rate: Set(sea_orm::prelude::Decimal::from(100)),
        recharge_threshold: Set(sea_orm::prelude::Decimal::ZERO),
        spend_threshold: Set(sea_orm::prelude::Decimal::ZERO),
        is_default: Set(true),
        sort_order: Set(0),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap()
    .id
}

/// `token` with the signature of `other` (a valid signature over other claims).
pub fn swap_signature(token: &str, other: &str) -> String {
    let (input, _) = token.rsplit_once('.').unwrap();
    let (_, sig) = other.rsplit_once('.').unwrap();
    format!("{input}.{sig}")
}
