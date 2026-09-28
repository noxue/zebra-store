//! Storefront authentication, `/me` and the public image captcha.

use std::collections::HashMap;

use axum::Extension;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::identity::admin_2fa::SecondFactor;
use zs_app::identity::user_account::{
    LoginInput, RegisterInput, Session, UserBrief, UserLoginOutcome,
};
use zs_domain::Error;
use zs_domain::identity::captcha::CaptchaPayload;
use zs_shared::page::Pagination;

use super::admin::{CodeRequest, DisableRequest, VerifyRequest};
use super::{brand_scope, json_field, page_of, parse_json, rfc3339};
use crate::client::Client;
use crate::extract::{Bind, BindField, BindRules, Body, Query, bind_json, req};
use crate::i18n;
use crate::middleware::auth::CurrentUser;
use crate::middleware::rate_limit::{self, MSG_LOGIN_TOO_MANY, key_by_ip_and_field};
use crate::middleware::tenant::Tenant;
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn public() -> Routes {
    Routes::new("/public").get("/captcha/image", captcha_image)
}

pub(super) fn auth() -> Routes {
    Routes::new("/auth")
        .post("/send-verify-code", send_verify_code)
        .post("/register", register)
        .post("/login", login)
        .post("/login/verify-2fa", verify_2fa)
        .post("/forgot-password", forgot_password)
}

pub(super) fn me() -> Routes {
    Routes::new("")
        .get("/me", profile)
        .put("/me/profile", update_profile)
        .post("/me/email/send-verify-code", send_change_email_code)
        .post("/me/email/change", change_email)
        .put("/me/password", change_password)
        .get("/me/login-logs", login_logs)
        .get("/me/2fa/status", totp_status)
        .post("/me/2fa/setup", totp_setup)
        .post("/me/2fa/enable", totp_enable)
        .post("/me/2fa/disable", totp_disable)
        .post("/me/2fa/recovery-codes/regenerate", totp_regenerate)
}

/// Locale of the request (`?lang=` → `X-Lang` → `Accept-Language`).
fn locale(uri: &axum::http::Uri, headers: &HeaderMap) -> &'static str {
    i18n::resolve_locale(uri.query(), headers)
}

fn session_response(s: &Session) -> Value {
    json!({
        "requires_totp": false,
        "user": UserBrief::from(&s.user),
        "token": s.token,
        "expires_at": rfc3339(s.expires_at),
    })
}

async fn captcha_image(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    let (id, image) = s.svc.identity.captcha.generate_image().await?;
    ok(json!({ "captcha_id": id, "image_base64": image }))
}

#[derive(Debug, Deserialize)]
struct SendCodeRequest {
    #[serde(default)]
    email: String,
    #[serde(default)]
    purpose: String,
    #[serde(default)]
    captcha_payload: CaptchaPayload,
}

impl BindRules for SendCodeRequest {
    const FIELDS: &'static [BindField] = &[req("email", "Email"), req("purpose", "Purpose")];
}

async fn send_verify_code(
    State(s): State<AppState>,
    Client(client): Client,
    tenant: Option<Extension<Tenant>>,
    uri: axum::http::Uri,
    headers: HeaderMap,
    Bind(req): Bind<SendCodeRequest>,
) -> ApiResult<Data<Value>> {
    if req.email.is_empty() || req.purpose.is_empty() {
        return Err(Error::invalid().into());
    }
    s.svc
        .identity
        .users
        .send_verify_code(
            &req.email,
            &req.purpose,
            &req.captcha_payload,
            locale(&uri, &headers),
            &client.ip,
            &brand_scope(tenant.as_deref()),
        )
        .await?;
    ok(json!({ "sent": true }))
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    #[serde(default)]
    email: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    agreement_accepted: bool,
}

impl BindRules for RegisterRequest {
    const FIELDS: &'static [BindField] = &[req("email", "Email"), req("password", "Password")];
}

async fn register(
    State(s): State<AppState>,
    Bind(req): Bind<RegisterRequest>,
) -> ApiResult<Data<Value>> {
    if req.email.is_empty() || req.password.is_empty() {
        return Err(Error::invalid().into());
    }
    let session = s
        .svc
        .identity
        .users
        .register(&RegisterInput {
            email: req.email,
            password: req.password,
            code: req.code,
            agreement_accepted: req.agreement_accepted,
        })
        .await?;
    ok(json!({
        "user": UserBrief::from(&session.user),
        "token": session.token,
        "expires_at": rfc3339(session.expires_at),
    }))
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    #[serde(default)]
    email: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    remember_me: bool,
    #[serde(default)]
    captcha_payload: CaptchaPayload,
}

async fn login(
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    let email_field = json_field(&body, "email");
    rate_limit::check(
        &s.svc.identity.login_limits.user,
        &key_by_ip_and_field(&client.ip, &email_field),
        MSG_LOGIN_TOO_MANY,
    )?;
    let users = &s.svc.identity.users;
    let req: LoginRequest = match parse_json(&body) {
        Ok(r) => r,
        Err(e) => {
            users.record_bad_login_request(&email_field, &client).await;
            return Err(e);
        }
    };
    if req.email.is_empty() || req.password.is_empty() {
        users.record_bad_login_request(&req.email, &client).await;
        return Err(Error::invalid().into());
    }
    let input = LoginInput {
        email: req.email,
        password: req.password,
        remember_me: req.remember_me,
        captcha: req.captcha_payload,
    };
    match users.login(&input, &client).await? {
        UserLoginOutcome::Token(session) => ok(session_response(&session)),
        UserLoginOutcome::Challenge { token, expires_at } => ok(json!({
            "requires_totp": true,
            "challenge_token": token,
            "challenge_expires_at": rfc3339(expires_at),
        })),
    }
}

async fn verify_2fa(
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    rate_limit::check(
        &s.svc.identity.login_limits.user,
        &client.ip,
        MSG_LOGIN_TOO_MANY,
    )?;
    let req: VerifyRequest = bind_json(&body)?;
    if req.challenge_token.is_empty() {
        return Err(Error::invalid().into());
    }
    let session = s
        .svc
        .identity
        .users
        .verify_login(
            &req.challenge_token,
            SecondFactor {
                code: &req.code,
                recovery_code: &req.recovery_code,
            },
            &client,
        )
        .await?;
    ok(session_response(&session))
}

#[derive(Debug, Deserialize)]
struct ForgotRequest {
    #[serde(default)]
    email: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    new_password: String,
}

impl BindRules for ForgotRequest {
    const FIELDS: &'static [BindField] = &[
        req("email", "Email"),
        req("code", "Code"),
        req("new_password", "NewPassword"),
    ];
}

async fn forgot_password(State(s): State<AppState>, body: Bytes) -> ApiResult<Data<Value>> {
    let users = &s.svc.identity.users;
    // The switch is checked before the body, like the original handler.
    let reg = users
        .registration()
        .await
        .map_err(|e| e.or_internal("error.reset_failed"))?;
    if !reg.email_verification_enabled {
        return Err(Error::forbidden(
            zs_domain::identity::registration::KEY_PASSWORD_RESET_DISABLED,
        )
        .into());
    }
    let req: ForgotRequest = bind_json(&body)?;
    if req.email.is_empty() || req.code.is_empty() || req.new_password.is_empty() {
        return Err(Error::invalid().into());
    }
    users
        .forgot_password(&req.email, &req.code, &req.new_password)
        .await?;
    ok(json!({ "reset": true }))
}

async fn profile(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Data<Value>> {
    let p = s.svc.identity.users.profile(me.id).await?;
    ok(serde_json::to_value(p).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
struct ProfileRequest {
    #[serde(default)]
    nickname: Option<String>,
    #[serde(default)]
    locale: Option<String>,
}

async fn update_profile(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    Body(req): Body<ProfileRequest>,
) -> ApiResult<Data<Value>> {
    let p = s
        .svc
        .identity
        .users
        .update_profile(me.id, req.nickname.as_deref(), req.locale.as_deref())
        .await?;
    ok(serde_json::to_value(p).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
struct ChangeEmailCodeRequest {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    new_email: String,
}

impl BindRules for ChangeEmailCodeRequest {
    const FIELDS: &'static [BindField] = &[req("kind", "Kind")];
}

async fn send_change_email_code(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    tenant: Option<Extension<Tenant>>,
    uri: axum::http::Uri,
    headers: HeaderMap,
    Bind(req): Bind<ChangeEmailCodeRequest>,
) -> ApiResult<Data<Value>> {
    if req.kind.is_empty() {
        return Err(Error::invalid().into());
    }
    s.svc
        .identity
        .users
        .send_change_email_code(
            me.id,
            &req.kind,
            &req.new_email,
            locale(&uri, &headers),
            &brand_scope(tenant.as_deref()),
        )
        .await?;
    ok(json!({ "sent": true }))
}

#[derive(Debug, Deserialize)]
struct ChangeEmailRequest {
    #[serde(default)]
    new_email: String,
    #[serde(default)]
    old_code: String,
    #[serde(default)]
    new_code: String,
}

impl BindRules for ChangeEmailRequest {
    const FIELDS: &'static [BindField] =
        &[req("new_email", "NewEmail"), req("new_code", "NewCode")];
}

async fn change_email(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    Bind(req): Bind<ChangeEmailRequest>,
) -> ApiResult<Data<Value>> {
    if req.new_email.is_empty() || req.new_code.is_empty() {
        return Err(Error::invalid().into());
    }
    let p = s
        .svc
        .identity
        .users
        .change_email(me.id, &req.new_email, &req.old_code, &req.new_code)
        .await?;
    ok(serde_json::to_value(p).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
struct PasswordRequest {
    #[serde(default)]
    old_password: String,
    #[serde(default)]
    new_password: String,
}

impl BindRules for PasswordRequest {
    const FIELDS: &'static [BindField] = &[req("new_password", "NewPassword")];
}

async fn change_password(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    Bind(req): Bind<PasswordRequest>,
) -> ApiResult<Data<Value>> {
    if req.new_password.is_empty() {
        return Err(Error::invalid().into());
    }
    s.svc
        .identity
        .users
        .change_password(me.id, &req.old_password, &req.new_password)
        .await?;
    ok(json!({ "updated": true }))
}

async fn login_logs(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Paged<Value>> {
    let (page, req) = s
        .svc
        .identity
        .audit
        .list_own_logins(me.id, page_of(&q))
        .await
        .map_err(|e| e.or_internal("error.user_login_log_fetch_failed"))?;
    let items = page
        .items
        .into_iter()
        .map(|l| {
            json!({
                "id": l.id,
                "email": l.email,
                "status": l.status,
                "client_ip": l.client_ip,
                "user_agent": l.user_agent,
                "login_source": l.login_source,
                "created_at": l.created_at,
            })
        })
        .collect();
    Ok(Paged(items, Pagination::new(req, page.total)))
}

async fn totp_status(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Data<Value>> {
    let st = s.svc.identity.users.totp_status(me.id).await?;
    ok(serde_json::to_value(st).map_err(Error::from)?)
}

async fn totp_setup(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Data<Value>> {
    let res = s.svc.identity.users.totp_setup(me.id).await?;
    ok(serde_json::to_value(res).map_err(Error::from)?)
}

async fn totp_enable(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    Bind(req): Bind<CodeRequest>,
) -> ApiResult<Data<Value>> {
    if req.code.is_empty() {
        return Err(Error::invalid().into());
    }
    let res = s.svc.identity.users.totp_enable(me.id, &req.code).await?;
    ok(serde_json::to_value(res).map_err(Error::from)?)
}

async fn totp_disable(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    Body(req): Body<DisableRequest>,
) -> ApiResult<Data<()>> {
    s.svc
        .identity
        .users
        .totp_disable(
            me.id,
            SecondFactor {
                code: &req.code,
                recovery_code: &req.recovery_code,
            },
        )
        .await?;
    ok(())
}

async fn totp_regenerate(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    Bind(req): Bind<CodeRequest>,
) -> ApiResult<Data<Value>> {
    if req.code.is_empty() {
        return Err(Error::invalid().into());
    }
    let codes = s
        .svc
        .identity
        .users
        .totp_regenerate(me.id, &req.code)
        .await?;
    ok(json!({ "recovery_codes": codes }))
}
