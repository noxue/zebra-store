//! Admin login (with 2FA challenge), password and self-service 2FA.

use axum::body::Bytes;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::identity::admin_2fa::SecondFactor;
use zs_app::identity::admin_auth::LoginOutcome;
use zs_domain::Error;
use zs_domain::identity::admin::Admin;
use zs_domain::identity::captcha::{CaptchaPayload, scenes};

use super::{parse_id, rfc3339};
use crate::client::Client;
use crate::extract::{Bind, BindField, BindRules, Body, bind_json, req};
use crate::middleware::auth::CurrentAdmin;
use axum::http::StatusCode;
use zs_app::identity::rate_limit::RateLimiter;

use super::json_field;
use crate::middleware::rate_limit::{
    MSG_LOGIN_TOO_MANY, key_by_ip_and_field, key_by_ip_and_header,
};
use crate::response::{ApiError, ApiResult, Data, ok};
use crate::routes::Routes;
use crate::state::AppState;

/// Unauthenticated admin routes (rate limited by IP).
pub(super) fn open() -> Routes {
    Routes::new("/admin")
        .post("/login", login)
        .post("/login/verify-2fa", verify_2fa)
}

/// Authenticated admin routes of this module.
pub(super) fn authenticated() -> Routes {
    Routes::new("/admin")
        .put("/password", change_password)
        .get("/authz/me", authz_me)
        .get("/2fa/status", totp_status)
        .post("/2fa/setup", totp_setup)
        .post("/2fa/enable", totp_enable)
        .post("/2fa/disable", totp_disable)
        .post("/2fa/recovery-codes/regenerate", totp_regenerate)
        .post("/authz/admins/{id}/2fa/reset", reset_admin_totp)
        .delete("/users/{id}/2fa", reset_user_totp)
}

fn token_response(admin: &Admin, token: &str, expires_at: DateTime<Utc>) -> Value {
    json!({
        "requires_totp": false,
        "token": token,
        "user": { "id": admin.id, "username": admin.username },
        "expires_at": rfc3339(expires_at),
    })
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    captcha_payload: CaptchaPayload,
}

/// Original `LoginRequest`: `Username`, `Password` are `binding:"required"`.
impl BindRules for LoginRequest {
    const FIELDS: &'static [BindField] =
        &[req("username", "Username"), req("password", "Password")];
}

async fn login(
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    // Live QA I-3: only failed attempts count, keyed by (account, IP), so a team
    // behind one NAT is not locked out by its own successful logins.
    let limiter = &s.svc.identity.login_limits.admin;
    let limit_key = key_by_ip_and_field(&client.ip, &json_field(&body, "username"));
    blocked(limiter, &limit_key)?;
    let req: LoginRequest = bind_json(&body)?;
    s.svc
        .identity
        .captcha
        .verify(scenes::LOGIN, &req.captcha_payload, &client.ip)
        .await?;
    let outcome = s
        .svc
        .identity
        .admin_auth
        .login(&req.username, &req.password, &client)
        .await;
    let outcome = counted(limiter, &limit_key, outcome)?;
    match outcome {
        LoginOutcome::Token {
            admin,
            token,
            expires_at,
        } => ok(token_response(&admin, &token, expires_at)),
        LoginOutcome::Challenge {
            challenge_token,
            expires_at,
            ..
        } => ok(json!({
            "requires_totp": true,
            "challenge_token": challenge_token,
            "challenge_expires_at": rfc3339(expires_at),
        })),
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct VerifyRequest {
    #[serde(default)]
    pub challenge_token: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub recovery_code: String,
}

impl BindRules for VerifyRequest {
    const FIELDS: &'static [BindField] = &[req("challenge_token", "ChallengeToken")];
}

async fn verify_2fa(
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    let limiter = &s.svc.identity.login_limits.admin;
    let limit_key = key_by_ip_and_header(
        &client.ip,
        &format!("2fa:{}", json_field(&body, "challenge_token")),
    );
    blocked(limiter, &limit_key)?;
    let req: VerifyRequest = bind_json(&body)?;
    if req.challenge_token.is_empty() {
        return Err(Error::invalid().into());
    }
    let verified = s
        .svc
        .identity
        .admin_2fa
        .verify_login(
            &req.challenge_token,
            SecondFactor {
                code: &req.code,
                recovery_code: &req.recovery_code,
            },
            &client,
        )
        .await;
    let (admin, token, expires_at) = counted(limiter, &limit_key, verified)?;
    ok(token_response(&admin, &token, expires_at))
}

/// 429 when `key` has used up its failed attempts.
fn blocked(limiter: &RateLimiter, key: &str) -> Result<(), ApiError> {
    limiter.blocked(key).map_err(|wait| {
        ApiError::with_http_status(
            Error::too_many(MSG_LOGIN_TOO_MANY).arg(wait),
            StatusCode::TOO_MANY_REQUESTS,
        )
    })
}

/// Counts a rejected attempt (not server errors) and resets the counter on success.
fn counted<T>(
    limiter: &RateLimiter,
    key: &str,
    outcome: zs_domain::Result<T>,
) -> Result<T, ApiError> {
    match outcome {
        Ok(v) => {
            limiter.reset(key);
            Ok(v)
        }
        Err(e) => {
            if e.kind() != zs_domain::ErrorKind::Internal {
                limiter.record_failure(key);
            }
            Err(e.into())
        }
    }
}

#[derive(Debug, Deserialize)]
struct PasswordRequest {
    #[serde(default)]
    old_password: String,
    #[serde(default)]
    new_password: String,
}

impl BindRules for PasswordRequest {
    const FIELDS: &'static [BindField] = &[
        req("old_password", "OldPassword"),
        req("new_password", "NewPassword"),
    ];
}

async fn change_password(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Bind(req): Bind<PasswordRequest>,
) -> ApiResult<Data<()>> {
    if req.old_password.is_empty() || req.new_password.is_empty() {
        return Err(Error::invalid().into());
    }
    s.svc
        .identity
        .admin_auth
        .change_password(me.id, &req.old_password, &req.new_password)
        .await?;
    ok(())
}

async fn authz_me(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
) -> ApiResult<Data<Value>> {
    let authz = &s.svc.identity.authz;
    let roles = authz
        .admin_roles(me.id)
        .map_err(|e| e.or_internal("error.config_fetch_failed"))?;
    let policies = authz
        .admin_policies(me.id)
        .map_err(|e| e.or_internal("error.config_fetch_failed"))?;
    ok(json!({ "admin_id": me.id, "is_super": me.is_super, "roles": roles, "policies": policies }))
}

async fn totp_status(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
) -> ApiResult<Data<Value>> {
    ok(serde_json::to_value(s.svc.identity.admin_2fa.status(me.id).await?).map_err(Error::from)?)
}

async fn totp_setup(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
) -> ApiResult<Data<Value>> {
    let res = s.svc.identity.admin_2fa.setup(&me, &client).await?;
    ok(serde_json::to_value(res).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
pub(super) struct CodeRequest {
    #[serde(default)]
    pub code: String,
}

impl BindRules for CodeRequest {
    const FIELDS: &'static [BindField] = &[req("code", "Code")];
}

async fn totp_enable(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Bind(req): Bind<CodeRequest>,
) -> ApiResult<Data<Value>> {
    if req.code.is_empty() {
        return Err(Error::invalid().into());
    }
    let res = s
        .svc
        .identity
        .admin_2fa
        .enable(&me, &req.code, &client)
        .await?;
    ok(serde_json::to_value(res).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
pub(super) struct DisableRequest {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub recovery_code: String,
}

async fn totp_disable(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Body(req): Body<DisableRequest>,
) -> ApiResult<Data<()>> {
    s.svc
        .identity
        .admin_2fa
        .disable(
            &me,
            SecondFactor {
                code: &req.code,
                recovery_code: &req.recovery_code,
            },
            &client,
        )
        .await?;
    ok(())
}

async fn totp_regenerate(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Bind(req): Bind<CodeRequest>,
) -> ApiResult<Data<Value>> {
    if req.code.is_empty() {
        return Err(Error::invalid().into());
    }
    let codes = s
        .svc
        .identity
        .admin_2fa
        .regenerate(&me, &req.code, &client)
        .await?;
    ok(json!({ "recovery_codes": codes }))
}

async fn reset_admin_totp(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Path(id): Path<String>,
) -> ApiResult<Data<()>> {
    if !me.is_super {
        return Err(Error::forbidden("error.forbidden").into());
    }
    let target = parse_id(&id, "error.bad_request")?;
    s.svc
        .identity
        .admin_2fa
        .reset_admin(&me, target, &client)
        .await?;
    ok(())
}

async fn reset_user_totp(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Path(id): Path<String>,
) -> ApiResult<Data<()>> {
    let user_id = parse_id(&id, "error.user_id_invalid")?;
    s.svc
        .identity
        .users
        .admin_reset_totp(me.id, user_id, &client)
        .await?;
    ok(())
}
