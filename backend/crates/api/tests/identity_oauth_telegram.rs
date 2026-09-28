//! Telegram login and binding: Login Widget, Mini App and OIDC (with a mocked
//! token/JWKS upstream).

#![expect(
    clippy::unwrap_used,
    reason = "integration tests: failures should abort the test"
)]

mod identity_common;
mod identity_oauth_common;

use identity_common::{App, data, totp};
use identity_oauth_common::*;
use serde_json::{Value, json};
use zs_domain::identity::oauth::keys;

const PW: &str = "Passw0rd!";

async fn post(app: &App, uri: &str, body: Value, token: Option<&str>) -> Value {
    app.call("POST", uri, Some(body), token).await
}

async fn tg_login(app: &App, id: i64, auth_date: i64) -> Value {
    post(
        app,
        "/api/v1/auth/telegram/login",
        widget(id, auth_date),
        None,
    )
    .await
}

fn token_of(res: &Value) -> String {
    data(res)["token"].as_str().unwrap().to_owned()
}

// AUTH-05 / a551e8f8: a first Telegram login creates a placeholder account with
// the default member level; the next login reuses it.
#[tokio::test]
async fn widget_login_provisions_placeholder_account() {
    let (app, _mock) = app_with_mock().await;
    enable_telegram(&app, false).await;
    let level = default_level(&app).await;
    let before = user_count(&app).await;
    // One clock reading: the second login must differ from the first regardless of
    // how long the first request takes (otherwise it is rejected as a replay).
    let base = now();

    let res = tg_login(&app, 777_001, base).await;
    let d = data(&res);
    assert_eq!(d["requires_totp"], false);
    assert_eq!(d["user"]["email"], "telegram_777001@login.local");
    assert_eq!(d["user"]["nickname"], "Tele Gram");
    assert!(d["user"]["email_verified_at"].is_null());
    assert!(d["expires_at"].as_str().unwrap().ends_with('Z'));
    let token = token_of(&res);
    assert_eq!(user_count(&app).await, before + 1);

    let me = app.call("GET", "/api/v1/me", None, Some(&token)).await;
    assert_eq!(data(&me)["member_level_id"], level);
    // Placeholder accounts bind a real email and set a first password.
    assert_eq!(data(&me)["email_change_mode"], "bind_only");
    assert_eq!(data(&me)["password_change_mode"], "set_without_old");
    assert_eq!(
        last_login_log(&app).await,
        ("success".into(), String::new(), "telegram".into())
    );

    let again = tg_login(&app, 777_001, base - 1).await;
    assert_eq!(data(&again)["user"]["id"], d["user"]["id"]);
    assert_eq!(user_count(&app).await, before + 1);
    let rows = identities(&app).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "telegram");
    assert_eq!(rows[0].2, "777001");

    let binding = app
        .call("GET", "/api/v1/me/telegram", None, Some(&token))
        .await;
    let b = data(&binding);
    assert_eq!(b["bound"], true);
    assert_eq!(b["provider"], "telegram");
    assert_eq!(b["provider_user_id"], "777001");
    assert_eq!(b["username"], "tg777001");
    assert_eq!(b["avatar_url"], "https://t.me/i/userpic/1.jpg");
    assert!(b["auth_at"].is_string());
    // Telegram is the only login method of a placeholder account.
    assert_eq!(b["can_unbind"], false);
    let res = app
        .call("DELETE", "/api/v1/me/telegram/unbind", None, Some(&token))
        .await;
    fails(&res, 400, keys::TELEGRAM_UNBIND_REQUIRES_EMAIL);
}

// AUTH-04: signature, freshness and replay checks of the widget payload.
#[tokio::test]
async fn widget_payload_checks() {
    let (app, _mock) = app_with_mock().await;
    enable_telegram(&app, false).await;
    let t = now();

    let mut tampered = widget(1001, t);
    tampered["username"] = json!("attacker");
    let res = post(&app, "/api/v1/auth/telegram/login", tampered, None).await;
    fails(&res, 400, keys::TELEGRAM_SIGNATURE_INVALID);
    assert_eq!(
        last_login_log(&app).await,
        (
            "failed".into(),
            "telegram_invalid".into(),
            "telegram".into()
        )
    );

    // Two hours old with a one-hour window → expired.
    fails(
        &tg_login(&app, 1001, t - 7200).await,
        400,
        keys::TELEGRAM_EXPIRED,
    );
    assert_eq!(last_login_log(&app).await.1, "telegram_expired");
    // Two minutes in the future → invalid payload.
    fails(
        &tg_login(&app, 1001, t + 120).await,
        400,
        keys::TELEGRAM_PAYLOAD_INVALID,
    );

    let payload = widget(1001, t);
    data(&post(&app, "/api/v1/auth/telegram/login", payload.clone(), None).await);
    let replay = post(&app, "/api/v1/auth/telegram/login", payload, None).await;
    fails(&replay, 400, keys::TELEGRAM_REPLAYED);
    assert_eq!(last_login_log(&app).await.1, "telegram_replayed");

    // Required fields (gin `binding:"required"`).
    let res = post(
        &app,
        "/api/v1/auth/telegram/login",
        json!({"id": 1, "auth_date": t}),
        None,
    )
    .await;
    fails(&res, 400, "error.bad_request");
    assert_eq!(last_login_log(&app).await.1, "bad_request");
}

#[tokio::test]
async fn disabled_and_misconfigured_telegram() {
    let (app, _mock) = app_with_mock().await;
    fails(
        &tg_login(&app, 1, now()).await,
        400,
        keys::TELEGRAM_DISABLED,
    );
    assert_eq!(last_login_log(&app).await.1, "telegram_config_invalid");
    app.set_setting(
        "telegram_auth_config",
        json!({"enabled": true, "bot_username": "b", "bot_token": ""}),
    )
    .await;
    fails(
        &tg_login(&app, 1, now()).await,
        500,
        keys::TELEGRAM_CONFIG_INVALID,
    );
    // Widget mode: OIDC start is a configuration error.
    enable_telegram(&app, false).await;
    let res = app
        .call("GET", "/api/v1/auth/telegram/oidc/start", None, None)
        .await;
    fails(&res, 500, keys::TELEGRAM_CONFIG_INVALID);
}

// AUTH-05: closed registration blocks Telegram sign-up but not bound users.
#[tokio::test]
async fn registration_switch_covers_telegram_signup() {
    let (app, _mock) = app_with_mock().await;
    enable_telegram(&app, false).await;
    data(&tg_login(&app, 42, now()).await);
    app.set_setting(
        "registration_config",
        json!({"registration_enabled": false}),
    )
    .await;
    let before = user_count(&app).await;
    let res = tg_login(&app, 43, now()).await;
    fails(&res, 403, "error.registration_disabled");
    assert_eq!(user_count(&app).await, before);
    assert_eq!(last_login_log(&app).await.1, "bad_request");
    // Already bound: still logs in.
    data(&tg_login(&app, 42, now() - 5).await);
}

// AUTH-07: the email-domain allowlist does not apply to Telegram sign-up.
#[tokio::test]
async fn domain_allowlist_does_not_block_telegram_signup() {
    let (app, _mock) = app_with_mock().await;
    enable_telegram(&app, false).await;
    app.set_setting(
        "registration_config",
        json!({"registration_enabled": true, "email_domain_allowlist_enabled": true,
               "allowed_email_domains": ["example.com"]}),
    )
    .await;
    let res = tg_login(&app, 7070, now()).await;
    assert_eq!(data(&res)["user"]["email"], "telegram_7070@login.local");
}

// AUTH-03: Telegram login of a 2FA account returns a challenge, never a token.
#[tokio::test]
async fn telegram_login_requires_2fa_when_enabled() {
    let (app, _mock) = app_with_mock().await;
    enable_telegram(&app, false).await;
    let token = app.register("tfa@b.io", PW).await;
    let bound = post(
        &app,
        "/api/v1/me/telegram/bind",
        widget(555, now()),
        Some(&token),
    )
    .await;
    assert_eq!(data(&bound)["bound"], true);
    let token = app.user_login("tfa@b.io", PW).await;
    let secret = enable_2fa(&app, &token).await;

    let res = tg_login(&app, 555, now() - 3).await;
    let d = data(&res);
    assert_eq!(d["requires_totp"], true);
    assert!(d.get("token").is_none());
    let challenge = d["challenge_token"].as_str().unwrap().to_owned();
    assert!(d["challenge_expires_at"].is_string());
    // The challenge is not an access token.
    let me = app.call("GET", "/api/v1/me", None, Some(&challenge)).await;
    assert_eq!(me["status_code"], 401);

    let done = post(
        &app,
        "/api/v1/auth/login/verify-2fa",
        json!({"challenge_token": challenge, "code": totp(&secret)}),
        None,
    )
    .await;
    assert_eq!(data(&done)["user"]["email"], "tfa@b.io");
    // The second step keeps the source of the first.
    assert_eq!(
        last_login_log(&app).await,
        ("success".into(), String::new(), "telegram".into())
    );
}

// AUTH-04: Mini App initData (with the `signature` field) login, tamper and replay.
#[tokio::test]
async fn miniapp_login() {
    let (app, _mock) = app_with_mock().await;
    enable_telegram(&app, false).await;
    let raw = init_data(9001, now());
    let res = post(
        &app,
        "/api/v1/auth/telegram/miniapp/login",
        json!({"init_data": raw}),
        None,
    )
    .await;
    assert_eq!(data(&res)["user"]["email"], "telegram_9001@login.local");
    assert_eq!(data(&res)["user"]["nickname"], "Mini");

    let replay = post(
        &app,
        "/api/v1/auth/telegram/miniapp/login",
        json!({"initData": raw}),
        None,
    )
    .await;
    fails(&replay, 400, keys::TELEGRAM_REPLAYED);

    let tampered = init_data(9001, now() - 1).replace("mini9001", "mini9002");
    let res = post(
        &app,
        "/api/v1/auth/telegram/miniapp/login",
        json!({"init_data": tampered}),
        None,
    )
    .await;
    fails(&res, 400, keys::TELEGRAM_SIGNATURE_INVALID);

    let expired = init_data(9001, now() - 7200);
    let res = post(
        &app,
        "/api/v1/auth/telegram/miniapp/login",
        json!({"init_data": expired}),
        None,
    )
    .await;
    fails(&res, 400, keys::TELEGRAM_EXPIRED);

    let res = post(
        &app,
        "/api/v1/auth/telegram/miniapp/login",
        json!({"init_data": " "}),
        None,
    )
    .await;
    fails(&res, 400, "error.bad_request");
}

#[tokio::test]
async fn binding_rules() {
    // One clock reading so payloads for the same Telegram id never coincide (replay guard).
    let base = now();
    let (app, _mock) = app_with_mock().await;
    enable_telegram(&app, false).await;
    let alice = app.register("alice@b.io", PW).await;
    let bob = app.register("bob@b.io", PW).await;

    let empty = app
        .call("GET", "/api/v1/me/telegram", None, Some(&alice))
        .await;
    assert_eq!(data(&empty), &json!({"bound": false, "can_unbind": false}));

    let res = post(
        &app,
        "/api/v1/me/telegram/bind",
        widget(100, base),
        Some(&alice),
    )
    .await;
    let b = data(&res);
    assert_eq!(b["bound"], true);
    assert_eq!(b["provider_user_id"], "100");
    // A local password remains, so Telegram can be removed.
    assert_eq!(b["can_unbind"], true);

    // Someone else's Telegram account.
    let res = post(
        &app,
        "/api/v1/me/telegram/bind",
        widget(100, base - 1),
        Some(&bob),
    )
    .await;
    fails(&res, 400, keys::TELEGRAM_BIND_CONFLICT);
    // Alice already has another Telegram account.
    let res = post(
        &app,
        "/api/v1/me/telegram/bind",
        widget(101, base),
        Some(&alice),
    )
    .await;
    fails(&res, 400, keys::TELEGRAM_ALREADY_BOUND);
    // Re-binding the same account refreshes it.
    let mini = init_data(100, base - 3);
    let res = post(
        &app,
        "/api/v1/me/telegram/miniapp/bind",
        json!({"init_data": mini}),
        Some(&alice),
    )
    .await;
    assert_eq!(data(&res)["username"], "mini100");

    // The bound account now logs Alice in.
    let login = tg_login(&app, 100, base - 2).await;
    assert_eq!(data(&login)["user"]["email"], "alice@b.io");

    let res = app
        .call("DELETE", "/api/v1/me/telegram/unbind", None, Some(&alice))
        .await;
    assert_eq!(data(&res), &json!({"unbound": true}));
    let res = app
        .call("DELETE", "/api/v1/me/telegram/unbind", None, Some(&alice))
        .await;
    fails(&res, 400, keys::TELEGRAM_NOT_BOUND);

    let res = app.call("GET", "/api/v1/me/telegram", None, None).await;
    assert_eq!(res["status_code"], 401);
    let res = post(
        &app,
        "/api/v1/me/telegram/bind",
        json!({"id": 1}),
        Some(&alice),
    )
    .await;
    // binding under a session answers like `RespondBindError` (login stays generic)
    fails(&res, 400, "AuthDate: 不能为空; Hash: 不能为空");
}

/// Runs the OIDC start endpoint and returns the state from the authorization URL.
async fn oidc_state(app: &App, token: Option<&str>) -> String {
    let uri = if token.is_some() {
        "/api/v1/me/telegram/oidc/start"
    } else {
        "/api/v1/auth/telegram/oidc/start"
    };
    let res = app.call("GET", uri, None, token).await;
    let url = data(&res)["auth_url"].as_str().unwrap().to_owned();
    url.split('&')
        .find_map(|p| p.strip_prefix("state="))
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn oidc_login_flow() {
    let (app, mock) = app_with_mock().await;
    enable_telegram(&app, true).await;

    let res = app
        .call("GET", "/api/v1/auth/telegram/oidc/start", None, None)
        .await;
    let url = data(&res)["auth_url"].as_str().unwrap().to_owned();
    assert!(url.starts_with("https://oauth.telegram.org/auth?client_id=123456&code_challenge="));
    assert!(url.contains("&code_challenge_method=S256&redirect_uri=https%3A%2F%2Fshop.test%2Fauth%2Ftelegram%2Fcallback&response_type=code&scope=openid+profile&state="));

    let state = oidc_state(&app, None).await;
    mock.set_id_token(&sign(&tg_claims(31337, "opaque-sub-1"), KID));
    let res = post(
        &app,
        "/api/v1/auth/telegram/oidc/callback",
        json!({"code": "c1", "state": state}),
        None,
    )
    .await;
    let d = data(&res);
    assert_eq!(d["user"]["email"], "telegram_31337@login.local");
    assert_eq!(d["user"]["nickname"], "Oidc User");
    let (auth, form) = mock
        .state
        .lock()
        .unwrap()
        .last_token_request
        .clone()
        .unwrap();
    // Basic base64("123456:tg-client-secret").
    assert_eq!(auth, "Basic MTIzNDU2OnRnLWNsaWVudC1zZWNyZXQ=");
    assert!(form.contains("grant_type=authorization_code"));
    assert!(form.contains("code=c1"));
    assert!(form.contains("client_id=123456"));
    assert!(form.contains("code_verifier="));

    // AUTH-02: the state is single use.
    let replay = post(
        &app,
        "/api/v1/auth/telegram/oidc/callback",
        json!({"code": "c1", "state": state}),
        None,
    )
    .await;
    fails(&replay, 400, keys::TELEGRAM_OIDC_STATE_INVALID);
    assert_eq!(last_login_log(&app).await.1, "telegram_invalid");

    // The same id_token cannot be used twice (fingerprint replay marker).
    let state = oidc_state(&app, None).await;
    let res = post(
        &app,
        "/api/v1/auth/telegram/oidc/callback",
        json!({"code": "c2", "state": state}),
        None,
    )
    .await;
    fails(&res, 400, keys::TELEGRAM_REPLAYED);
}

// AUTH-02: id_token checks.
#[tokio::test]
async fn oidc_id_token_validation() {
    let (app, mock) = app_with_mock().await;
    enable_telegram(&app, true).await;
    let mut cases = Vec::new();
    let mut wrong_aud = tg_claims(1, "s");
    wrong_aud["aud"] = json!("999");
    cases.push(sign(&wrong_aud, KID));
    let mut expired = tg_claims(1, "s");
    expired["exp"] = json!(now() - 10);
    cases.push(sign(&expired, KID));
    let mut issuer = tg_claims(1, "s");
    issuer["iss"] = json!("https://evil.example");
    cases.push(sign(&issuer, KID));
    let mut no_id = tg_claims(1, "s");
    no_id.as_object_mut().unwrap().remove("id");
    cases.push(sign(&no_id, KID));
    // Tampered signature.
    cases.push(swap_signature(
        &sign(&tg_claims(1, "s"), KID),
        &sign(&tg_claims(2, "s"), KID),
    ));
    for token in cases {
        mock.set_id_token(&token);
        let state = oidc_state(&app, None).await;
        let res = post(
            &app,
            "/api/v1/auth/telegram/oidc/callback",
            json!({"code": "c", "state": state}),
            None,
        )
        .await;
        fails(&res, 400, keys::TELEGRAM_OIDC_ID_TOKEN_INVALID);
    }

    mock.state.lock().unwrap().token_status = 400;
    let state = oidc_state(&app, None).await;
    let res = post(
        &app,
        "/api/v1/auth/telegram/oidc/callback",
        json!({"code": "c", "state": state}),
        None,
    )
    .await;
    fails(&res, 400, keys::TELEGRAM_OIDC_EXCHANGE_FAILED);

    let res = post(
        &app,
        "/api/v1/auth/telegram/oidc/callback",
        json!({"code": "c"}),
        None,
    )
    .await;
    fails(&res, 400, "error.bad_request");
}

// AUTH-02: a legacy binding stored under the OIDC `sub` logs in the same user and
// is migrated to the numeric Telegram id.
#[tokio::test]
async fn oidc_migrates_legacy_sub_binding() {
    use sea_orm::{ActiveModelTrait, Set};
    let (app, mock) = app_with_mock().await;
    enable_telegram(&app, true).await;
    let token = app.register("legacy@b.io", PW).await;
    let me = app.call("GET", "/api/v1/me", None, Some(&token)).await;
    let user_id = data(&me)["id"].as_i64().unwrap();
    let now_t = chrono::Utc::now();
    zs_infra::db::entity::user_oauth_identities::ActiveModel {
        user_id: Set(user_id),
        provider: Set("telegram".into()),
        provider_user_id: Set("legacy-sub".into()),
        username: Set("old".into()),
        avatar_url: Set(String::new()),
        auth_at: Set(None),
        created_at: Set(now_t),
        updated_at: Set(now_t),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();

    mock.set_id_token(&sign(&tg_claims(8080, "legacy-sub"), KID));
    let state = oidc_state(&app, None).await;
    let res = post(
        &app,
        "/api/v1/auth/telegram/oidc/callback",
        json!({"code": "c", "state": state}),
        None,
    )
    .await;
    assert_eq!(data(&res)["user"]["id"], user_id);
    assert_eq!(
        identities(&app).await,
        vec![(user_id, "telegram".into(), "8080".into())]
    );
}

// AUTH-02: bind callbacks require a bind state created by the same user.
#[tokio::test]
async fn oidc_bind_state_is_bound_to_intent_and_user() {
    let (app, mock) = app_with_mock().await;
    enable_telegram(&app, true).await;
    let alice = app.register("a@b.io", PW).await;
    let bob = app.register("b@b.io", PW).await;

    // A login state cannot finish a bind.
    mock.set_id_token(&sign(&tg_claims(61, "s61"), KID));
    let login_state = oidc_state(&app, None).await;
    let res = post(
        &app,
        "/api/v1/me/telegram/oidc/callback",
        json!({"code": "c", "state": login_state}),
        Some(&alice),
    )
    .await;
    fails(&res, 400, keys::TELEGRAM_PAYLOAD_INVALID);

    // Alice's bind state used by Bob.
    mock.set_id_token(&sign(&tg_claims(62, "s62"), KID));
    let alice_state = oidc_state(&app, Some(&alice)).await;
    let res = post(
        &app,
        "/api/v1/me/telegram/oidc/callback",
        json!({"code": "c", "state": alice_state}),
        Some(&bob),
    )
    .await;
    fails(&res, 400, keys::TELEGRAM_PAYLOAD_INVALID);

    // A bind state cannot log in.
    mock.set_id_token(&sign(&tg_claims(63, "s63"), KID));
    let bind_state = oidc_state(&app, Some(&alice)).await;
    let res = post(
        &app,
        "/api/v1/auth/telegram/oidc/callback",
        json!({"code": "c", "state": bind_state}),
        None,
    )
    .await;
    fails(&res, 400, keys::TELEGRAM_PAYLOAD_INVALID);

    mock.set_id_token(&sign(&tg_claims(64, "s64"), KID));
    let state = oidc_state(&app, Some(&alice)).await;
    let res = post(
        &app,
        "/api/v1/me/telegram/oidc/callback",
        json!({"code": "c", "state": state}),
        Some(&alice),
    )
    .await;
    let b = data(&res);
    assert_eq!(b["bound"], true);
    assert_eq!(b["provider_user_id"], "64");
    assert_eq!(b["username"], "oidc64");
    // Like the original, the OIDC bind response does not compute can_unbind.
    assert_eq!(b["can_unbind"], false);
    let view = app
        .call("GET", "/api/v1/me/telegram", None, Some(&alice))
        .await;
    assert_eq!(data(&view)["can_unbind"], true);
}

// The public config exposes what the storefront needs to pick a Telegram flow.
#[tokio::test]
async fn public_config_exposes_telegram_mode() {
    let (app, _mock) = app_with_mock().await;
    enable_telegram(&app, true).await;
    enable_google(&app).await;
    let res = app.call("GET", "/api/v1/public/config", None, None).await;
    let d = data(&res);
    assert_eq!(
        d["telegram_auth"],
        json!({"enabled": true, "bot_username": "zebra_test_bot",
               "mini_app_url": "https://t.me/zebra_test_bot/app", "mode": "oidc"})
    );
    assert_eq!(
        d["google_auth"],
        json!({"enabled": true, "client_id": GOOGLE_CLIENT_ID})
    );
}
