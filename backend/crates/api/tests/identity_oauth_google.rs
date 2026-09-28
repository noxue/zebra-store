//! Google login, binding and redirect (`form_post`) flow against a mocked JWKS.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests: failures should abort the test"
)]

mod identity_common;
mod identity_oauth_common;

use axum::http::StatusCode;
use identity_common::{App, data};
use identity_oauth_common::*;
use serde_json::{Value, json};
use zs_domain::identity::oauth::keys;

const PW: &str = "Passw0rd!";
const HOST: &str = "shop.test";
const FORM: &str = "application/x-www-form-urlencoded";

async fn google_login(app: &App, credential: &str) -> Value {
    app.call(
        "POST",
        "/api/v1/auth/google/login",
        Some(json!({"credential": credential})),
        None,
    )
    .await
}

fn credential(sub: &str, email: &str) -> String {
    sign(&google_claims(sub, email), KID)
}

fn with(mut claims: Value, patch: Value) -> Value {
    for (k, v) in patch.as_object().unwrap() {
        claims[k] = v.clone();
    }
    claims
}

async fn user_id(app: &App, token: &str) -> i64 {
    let me = app.call("GET", "/api/v1/me", None, Some(token)).await;
    data(&me)["id"].as_i64().unwrap()
}

#[tokio::test]
async fn login_creates_gmail_account() {
    let (app, mock) = app_with_mock().await;
    enable_google(&app).await;
    let level = default_level(&app).await;
    let res = google_login(&app, &credential("g-1", "New.User@Gmail.com")).await;
    let d = data(&res);
    assert_eq!(d["requires_totp"], false);
    assert_eq!(d["user"]["email"], "new.user@gmail.com");
    assert_eq!(d["user"]["nickname"], "Goo Gle");
    assert!(d["user"]["email_verified_at"].is_string());
    let token = d["token"].as_str().unwrap().to_owned();
    let me = app.call("GET", "/api/v1/me", None, Some(&token)).await;
    assert_eq!(data(&me)["member_level_id"], level);
    assert_eq!(data(&me)["password_change_mode"], "set_without_old");
    assert_eq!(
        last_login_log(&app).await,
        ("success".into(), String::new(), "google".into())
    );

    // Same subject again: same account; JWKS served from cache.
    let again = google_login(&app, &credential("g-1", "new.user@gmail.com")).await;
    assert_eq!(data(&again)["user"]["id"], d["user"]["id"]);
    assert_eq!(mock.state.lock().unwrap().google_jwks_hits, 1);

    let binding = app
        .call("GET", "/api/v1/me/google", None, Some(&token))
        .await;
    let b = data(&binding);
    assert_eq!(b["bound"], true);
    assert_eq!(b["provider"], "google");
    assert_eq!(b["provider_user_id"], "g-1");
    assert_eq!(b["username"], "new.user@gmail.com");
    assert_eq!(b["email"], "new.user@gmail.com");
    assert_eq!(b["display_name"], "Goo Gle");
    assert_eq!(b["avatar_url"], "https://lh3.googleusercontent.com/a/p");
    // AUTH-01: the only login method cannot be removed.
    assert_eq!(b["can_unbind"], false);
    let res = app
        .call("DELETE", "/api/v1/me/google/unbind", None, Some(&token))
        .await;
    fails(&res, 400, keys::GOOGLE_UNBIND_LOCKED);
}

// AUTH-01: strict credential validation.
#[tokio::test]
async fn credential_validation() {
    let (app, mock) = app_with_mock().await;
    fails(
        &google_login(&app, &credential("s", "a@gmail.com")).await,
        400,
        keys::GOOGLE_DISABLED,
    );
    assert_eq!(last_login_log(&app).await.1, "google_config_invalid");
    enable_google(&app).await;
    let base = google_claims("s", "a@gmail.com");
    for (claims, key) in [
        (
            with(base.clone(), json!({"aud": "other-client"})),
            keys::GOOGLE_CREDENTIAL_INVALID,
        ),
        (
            with(base.clone(), json!({"email_verified": false})),
            keys::GOOGLE_EMAIL_UNVERIFIED,
        ),
        (
            with(base.clone(), json!({"exp": now() - 120})),
            keys::GOOGLE_CREDENTIAL_EXPIRED,
        ),
        (
            with(base.clone(), json!({"iss": "https://evil.example"})),
            keys::GOOGLE_CREDENTIAL_INVALID,
        ),
        (
            with(base.clone(), json!({"azp": "other-client"})),
            keys::GOOGLE_CREDENTIAL_INVALID,
        ),
    ] {
        fails(&google_login(&app, &sign(&claims, KID)).await, 400, key);
        assert_eq!(last_login_log(&app).await.1, "google_invalid");
    }
    // Unknown kid and a bad signature.
    fails(
        &google_login(&app, &sign(&base, "other-kid")).await,
        400,
        keys::GOOGLE_CREDENTIAL_INVALID,
    );
    fails(
        &google_login(
            &app,
            &swap_signature(
                &sign(&base, KID),
                &sign(&google_claims("x", "b@gmail.com"), KID),
            ),
        )
        .await,
        400,
        keys::GOOGLE_CREDENTIAL_INVALID,
    );
    fails(
        &google_login(&app, "not-a-jwt").await,
        400,
        keys::GOOGLE_CREDENTIAL_INVALID,
    );
    fails(&google_login(&app, "").await, 400, "Credential: 不能为空");

    // 64 KiB body limit → HTTP 413.
    let big = "x".repeat(70 * 1024);
    let reply = send(
        &app,
        "POST",
        "/api/v1/auth/google/login",
        None,
        HOST,
        "",
        Some(("application/json", json!({"credential": big}).to_string())),
    )
    .await;
    assert_eq!(reply.status, StatusCode::PAYLOAD_TOO_LARGE);
    fails(&reply.body, 400, keys::REQUEST_TOO_LARGE);
    // One initial fetch plus one forced refresh for the unknown kid.
    assert_eq!(mock.state.lock().unwrap().google_jwks_hits, 2);
}

#[tokio::test]
async fn jwks_outage_is_service_unavailable() {
    let (app, mock) = app_with_mock().await;
    enable_google(&app).await;
    mock.state.lock().unwrap().google_jwks_status = 500;
    let res = google_login(&app, &credential("s", "a@gmail.com")).await;
    fails(&res, 500, keys::GOOGLE_UNAVAILABLE);
    assert_eq!(last_login_log(&app).await.1, "google_config_invalid");
}

// AUTH-01: only authoritative addresses link to / create local accounts.
#[tokio::test]
async fn auto_link_rules() {
    let (app, _mock) = app_with_mock().await;
    enable_google(&app).await;
    let custom = app.register("owner@corp.example", PW).await;
    let custom_id = user_id(&app, &custom).await;
    let gmail = app.register("owner@gmail.com", PW).await;
    let gmail_id = user_id(&app, &gmail).await;

    // Custom domain without `hd` matching a local account → must bind manually.
    let res = google_login(&app, &credential("c-1", "owner@corp.example")).await;
    fails(&res, 403, keys::GOOGLE_AUTO_LINK_FORBIDDEN);
    // Custom domain, no account, no `hd` → no sign-up either.
    let before = user_count(&app).await;
    let res = google_login(&app, &credential("c-2", "new@corp.example")).await;
    fails(&res, 403, keys::GOOGLE_AUTO_LINK_FORBIDDEN);
    assert_eq!(user_count(&app).await, before);
    // Workspace (`hd`) accounts are authoritative.
    let hd = sign(
        &with(
            google_claims("c-1", "owner@corp.example"),
            json!({"hd": "corp.example"}),
        ),
        KID,
    );
    assert_eq!(
        data(&google_login(&app, &hd).await)["user"]["id"],
        custom_id
    );
    // Gmail with the same address links the existing account.
    let res = google_login(&app, &credential("g-9", "owner@gmail.com")).await;
    assert_eq!(data(&res)["user"]["id"], gmail_id);
    // A different Google subject for an already-linked account.
    let res = google_login(&app, &credential("g-10", "owner@gmail.com")).await;
    fails(&res, 400, keys::GOOGLE_ALREADY_BOUND);
}

// AUTH-05 / AUTH-07: registration switch and domain allowlist cover Google sign-up.
#[tokio::test]
async fn registration_rules_apply_to_signup() {
    let (app, _mock) = app_with_mock().await;
    enable_google(&app).await;
    app.set_setting(
        "registration_config",
        json!({"registration_enabled": false}),
    )
    .await;
    let before = user_count(&app).await;
    let res = google_login(&app, &credential("n-1", "new1@gmail.com")).await;
    fails(&res, 403, "error.registration_disabled");
    assert_eq!(user_count(&app).await, before);
    app.set_setting(
        "registration_config",
        json!({"registration_enabled": true, "email_domain_allowlist_enabled": true,
               "allowed_email_domains": ["example.com"]}),
    )
    .await;
    let res = google_login(&app, &credential("n-2", "new2@gmail.com")).await;
    fails(&res, 400, "error.email_domain_not_allowed");
    assert_eq!(user_count(&app).await, before);
}

#[tokio::test]
async fn google_login_requires_2fa_when_enabled() {
    let (app, _mock) = app_with_mock().await;
    enable_google(&app).await;
    let token = app.register("two@gmail.com", PW).await;
    enable_2fa(&app, &token).await;
    let res = google_login(&app, &credential("t-1", "two@gmail.com")).await;
    let d = data(&res);
    assert_eq!(d["requires_totp"], true);
    assert!(d["challenge_token"].is_string());
    assert!(d.get("token").is_none());
}

#[tokio::test]
async fn binding_rules() {
    let (app, _mock) = app_with_mock().await;
    enable_google(&app).await;
    let alice = app.register("alice@b.io", PW).await;
    let bob = app.register("bob@b.io", PW).await;
    let empty = app
        .call("GET", "/api/v1/me/google", None, Some(&alice))
        .await;
    assert_eq!(data(&empty), &json!({"bound": false, "can_unbind": false}));

    // A custom-domain Google account can be bound explicitly.
    let res = app
        .call(
            "POST",
            "/api/v1/me/google/bind",
            Some(json!({"credential": credential("b-1", "alice@corp.example")})),
            Some(&alice),
        )
        .await;
    let b = data(&res);
    assert_eq!(b["bound"], true);
    assert_eq!(b["email"], "alice@corp.example");
    assert_eq!(b["display_name"], "alice");
    assert_eq!(b["can_unbind"], true);

    let res = app
        .call(
            "POST",
            "/api/v1/me/google/bind",
            Some(json!({"credential": credential("b-1", "alice@corp.example")})),
            Some(&bob),
        )
        .await;
    fails(&res, 400, keys::GOOGLE_BIND_CONFLICT);
    let res = app
        .call(
            "POST",
            "/api/v1/me/google/bind",
            Some(json!({"credential": credential("b-2", "other@gmail.com")})),
            Some(&alice),
        )
        .await;
    fails(&res, 400, keys::GOOGLE_ALREADY_BOUND);

    // The bound (non-authoritative) identity logs Alice in.
    let res = google_login(&app, &credential("b-1", "alice@corp.example")).await;
    assert_eq!(data(&res)["user"]["email"], "alice@b.io");

    let res = app
        .call("DELETE", "/api/v1/me/google/unbind", None, Some(&alice))
        .await;
    assert_eq!(data(&res), &json!({"unbound": true}));
    let res = app
        .call("DELETE", "/api/v1/me/google/unbind", None, Some(&alice))
        .await;
    fails(&res, 400, keys::GOOGLE_NOT_BOUND);
}

// A Google-only account that also has Telegram (usable) may drop Google.
#[tokio::test]
async fn unbind_keeps_another_usable_provider() {
    let (app, _mock) = app_with_mock().await;
    enable_google(&app).await;
    enable_telegram(&app, false).await;
    let res = google_login(&app, &credential("u-1", "only@gmail.com")).await;
    let token = data(&res)["token"].as_str().unwrap().to_owned();
    let res = app
        .call(
            "POST",
            "/api/v1/me/telegram/bind",
            Some(widget(4242, now())),
            Some(&token),
        )
        .await;
    assert_eq!(data(&res)["can_unbind"], true);
    let view = app
        .call("GET", "/api/v1/me/google", None, Some(&token))
        .await;
    assert_eq!(data(&view)["can_unbind"], true);
    let res = app
        .call("DELETE", "/api/v1/me/google/unbind", None, Some(&token))
        .await;
    data(&res);
    // Telegram is now the last method.
    let res = app
        .call("DELETE", "/api/v1/me/telegram/unbind", None, Some(&token))
        .await;
    fails(&res, 400, keys::TELEGRAM_UNBIND_REQUIRES_EMAIL);
}

// --- redirect flow ---------------------------------------------------------

async fn intent(app: &App, host: &str, token: Option<&str>) -> (String, Reply) {
    let uri = if token.is_some() {
        "/api/v1/me/google/redirect/intent"
    } else {
        "/api/v1/auth/google/redirect/intent"
    };
    let reply = send(app, "POST", uri, token, host, "", None).await;
    let state = data(&reply.body)["state"].as_str().unwrap().to_owned();
    (state, reply)
}

async fn callback(
    app: &App,
    host: &str,
    state: &str,
    cookie_state: &str,
    credential: &str,
) -> Reply {
    let body = format!("credential={credential}&state={state}&g_csrf_token=csrf1");
    let cookies = format!("g_csrf_token=csrf1; __Host-dujiao_google_state={cookie_state}");
    send(
        app,
        "POST",
        "/api/v1/auth/google/redirect/callback",
        None,
        host,
        &cookies,
        Some((FORM, body)),
    )
    .await
}

async fn exchange(app: &App, host: &str, handle: &str, token: Option<&str>) -> Reply {
    let uri = if token.is_some() {
        "/api/v1/me/google/redirect/exchange"
    } else {
        "/api/v1/auth/google/redirect/exchange"
    };
    let cookies = format!("__Host-dujiao_google_handoff={handle}");
    send(app, "POST", uri, token, host, &cookies, None).await
}

#[tokio::test]
async fn redirect_login_flow() {
    let (app, _mock) = app_with_mock().await;
    enable_google(&app).await;
    let (state, reply) = intent(&app, HOST, None).await;
    assert_eq!(state.len(), 43);
    let d = data(&reply.body);
    assert_eq!(d["expires_in"], 600);
    assert!(d["issued_at"].is_string());
    assert_eq!(reply.headers["cache-control"], "no-store");
    let set = reply.set_cookies().join("\n");
    assert!(set.contains(&format!("__Host-dujiao_google_state={state}; Path=/;")));
    assert!(set.contains("Max-Age=600; HttpOnly; Secure; SameSite=None"));

    let cred = credential("r-1", "redirect@gmail.com");
    // CSRF mismatch: redirected with an error; the intent cookie is kept.
    let forged = send(
        &app,
        "POST",
        "/api/v1/auth/google/redirect/callback",
        None,
        HOST,
        &format!("g_csrf_token=other; __Host-dujiao_google_state={state}"),
        Some((
            FORM,
            format!("credential={cred}&state={state}&g_csrf_token=csrf1"),
        )),
    )
    .await;
    assert_eq!(forged.status, StatusCode::SEE_OTHER);
    assert_eq!(
        forged.location(),
        "/auth/google/callback?flow=login&error=csrf_mismatch"
    );
    assert!(forged.set_cookies().is_empty());

    let done = callback(&app, HOST, &state, &state, &cred).await;
    assert_eq!(done.status, StatusCode::SEE_OTHER);
    assert_eq!(done.location(), "/auth/google/callback?flow=login");
    assert_eq!(
        done.cookie("__Host-dujiao_google_state").as_deref(),
        Some("")
    );
    let handle = done.cookie("__Host-dujiao_google_handoff").unwrap();
    assert_eq!(handle.len(), 43);
    assert!(
        done.set_cookies()
            .iter()
            .any(|c| c.contains("__Host-dujiao_google_handoff=") && c.contains("SameSite=Lax"))
    );

    // AUTH-01: the state is consumed; replaying the callback fails.
    let replay = callback(&app, HOST, &state, &state, &cred).await;
    assert_eq!(
        replay.location(),
        "/auth/google/callback?flow=login&error=session_expired"
    );

    let res = exchange(&app, HOST, &handle, None).await;
    let d = data(&res.body);
    assert_eq!(d["user"]["email"], "redirect@gmail.com");
    assert!(d["token"].is_string());
    assert_eq!(
        res.cookie("__Host-dujiao_google_handoff").as_deref(),
        Some("")
    );
    // The handoff is single use too.
    let again = exchange(&app, HOST, &handle, None).await;
    fails(&again.body, 400, keys::GOOGLE_REDIRECT_SESSION_EXPIRED);
    // Error responses still clear the cookie and are not cacheable.
    assert_eq!(
        again.cookie("__Host-dujiao_google_handoff").as_deref(),
        Some("")
    );
    assert_eq!(again.headers["cache-control"], "no-store");
    assert!(again.body["data"]["request_id"].is_string());
    let none = send(
        &app,
        "POST",
        "/api/v1/auth/google/redirect/exchange",
        None,
        HOST,
        "",
        None,
    )
    .await;
    fails(&none.body, 400, keys::GOOGLE_REDIRECT_SESSION_EXPIRED);
}

#[tokio::test]
async fn redirect_rejects_other_tenant_and_bad_credentials() {
    let (app, _mock) = app_with_mock().await;
    enable_google(&app).await;
    let cred = credential("r-2", "tenant@gmail.com");
    let (state, _) = intent(&app, HOST, None).await;
    let reply = callback(&app, "other.test", &state, &state, &cred).await;
    assert_eq!(
        reply.location(),
        "/auth/google/callback?flow=login&error=tenant_mismatch"
    );

    let (state, _) = intent(&app, HOST, None).await;
    let expired = sign(
        &with(
            google_claims("r-2", "tenant@gmail.com"),
            json!({"exp": now() - 300}),
        ),
        KID,
    );
    let reply = callback(&app, HOST, &state, &state, &expired).await;
    assert_eq!(
        reply.location(),
        "/auth/google/callback?flow=login&error=credential_expired"
    );
    assert_eq!(last_login_log(&app).await.1, "google_invalid");

    // Handoff presented on another tenant.
    let (state, _) = intent(&app, HOST, None).await;
    let done = callback(&app, HOST, &state, &state, &cred).await;
    let handle = done.cookie("__Host-dujiao_google_handoff").unwrap();
    let res = exchange(&app, "other.test", &handle, None).await;
    fails(&res.body, 403, keys::GOOGLE_REDIRECT_CONTEXT_MISMATCH);

    // Client id changed after verification → the handoff is invalid.
    let (state, _) = intent(&app, HOST, None).await;
    let done = callback(&app, HOST, &state, &state, &cred).await;
    let handle = done.cookie("__Host-dujiao_google_handoff").unwrap();
    app.set_setting(
        "google_auth_config",
        json!({"enabled": true, "client_id": "rotated-client"}),
    )
    .await;
    let res = exchange(&app, HOST, &handle, None).await;
    fails(&res.body, 400, keys::GOOGLE_REDIRECT_SESSION_EXPIRED);

    // Wrong content type / missing state cookie.
    let res = send(
        &app,
        "POST",
        "/api/v1/auth/google/redirect/callback",
        None,
        HOST,
        "g_csrf_token=csrf1",
        Some(("application/json", "{}".into())),
    )
    .await;
    assert_eq!(
        res.location(),
        "/auth/google/callback?flow=login&error=invalid_request"
    );
    let res = send(
        &app,
        "POST",
        "/api/v1/auth/google/redirect/callback",
        None,
        HOST,
        "g_csrf_token=csrf1",
        Some((FORM, "credential=x&state=y&g_csrf_token=csrf1".into())),
    )
    .await;
    assert_eq!(
        res.location(),
        "/auth/google/callback?flow=login&error=session_expired"
    );
}

#[tokio::test]
async fn redirect_bind_flow_is_bound_to_the_user() {
    let (app, _mock) = app_with_mock().await;
    enable_google(&app).await;
    let alice = app.register("alice@b.io", PW).await;
    let bob = app.register("bob@b.io", PW).await;
    let cred = credential("rb-1", "alice.g@gmail.com");

    let (state, _) = intent(&app, HOST, Some(&alice)).await;
    let done = callback(&app, HOST, &state, &state, &cred).await;
    assert_eq!(done.location(), "/auth/google/callback?flow=bind");
    let handle = done.cookie("__Host-dujiao_google_handoff").unwrap();
    // Bob cannot consume Alice's bind handoff (and it is burnt).
    let res = exchange(&app, HOST, &handle, Some(&bob)).await;
    fails(&res.body, 403, keys::GOOGLE_REDIRECT_CONTEXT_MISMATCH);
    // A bind handoff cannot log in.
    let (state, _) = intent(&app, HOST, Some(&alice)).await;
    let done = callback(&app, HOST, &state, &state, &cred).await;
    let handle = done.cookie("__Host-dujiao_google_handoff").unwrap();
    let res = exchange(&app, HOST, &handle, None).await;
    fails(&res.body, 400, keys::GOOGLE_REDIRECT_SESSION_EXPIRED);

    let (state, _) = intent(&app, HOST, Some(&alice)).await;
    let done = callback(&app, HOST, &state, &state, &cred).await;
    let handle = done.cookie("__Host-dujiao_google_handoff").unwrap();
    let res = exchange(&app, HOST, &handle, Some(&alice)).await;
    let b = data(&res.body);
    assert_eq!(b["bound"], true);
    assert_eq!(b["provider_user_id"], "rb-1");
    assert_eq!(b["email"], "alice.g@gmail.com");

    let res = send(
        &app,
        "POST",
        "/api/v1/me/google/redirect/intent",
        None,
        HOST,
        "",
        None,
    )
    .await;
    assert_eq!(res.body["status_code"], 401);
}

#[tokio::test]
async fn redirect_intent_requires_enabled_google() {
    let (app, _mock) = app_with_mock().await;
    let reply = send(
        &app,
        "POST",
        "/api/v1/auth/google/redirect/intent",
        None,
        HOST,
        "",
        None,
    )
    .await;
    fails(&reply.body, 400, keys::GOOGLE_DISABLED);
    assert_eq!(reply.headers["cache-control"], "no-store");
}
