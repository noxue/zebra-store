//! Telegram and Google login / binding routes (original
//! `user_telegram_handler.go`, `user_telegram_oidc_handler.go`,
//! `user_google_handler.go`).

use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::identity::oauth::OAuthService;
use zs_app::identity::oauth::google::{
    HANDOFF_TTL_SECONDS, INTENT_TTL_SECONDS, REDIRECT_STATE_KEYS, RedirectFlow,
    callback_error_code, valid_handle,
};
use zs_app::identity::oauth::telegram::OidcIntent;
use zs_app::identity::user_account::UserLoginOutcome;
use zs_domain::identity::google::MAX_CREDENTIAL_BYTES;
use zs_domain::identity::oauth::{
    RedirectTenant, SOURCE_GOOGLE, SOURCE_TELEGRAM, constant_time_eq, keys, parse_query,
};
use zs_domain::identity::telegram::WidgetPayload;
use zs_domain::{Error, ErrorKind};

use super::{parse_json, rfc3339};
use crate::client::Client;
use crate::extract::{BindField, BindRules, bind_json, req};
use crate::i18n;
use crate::middleware::auth::CurrentUser;
use crate::middleware::rate_limit::{self, MSG_LOGIN_TOO_MANY};
use crate::middleware::request_id::RequestId;
use crate::middleware::tenant::Tenant;
use crate::response::{ApiError, ApiResult, Data, ok};
use crate::routes::Routes;
use crate::state::AppState;

/// `__Host-` cookie holding the redirect state (SameSite=None: Google posts back cross-site).
const INTENT_COOKIE: &str = "__Host-dujiao_google_state";
/// `__Host-` cookie holding the verified-claims handoff (SameSite=Lax).
const HANDOFF_COOKIE: &str = "__Host-dujiao_google_handoff";
/// Google Identity Services double-submit CSRF cookie / field.
const GIS_CSRF: &str = "g_csrf_token";
/// Storefront page receiving the redirect result.
const CALLBACK_PAGE: &str = "/auth/google/callback";

pub(super) fn auth() -> Routes {
    Routes::new("/auth")
        .post("/telegram/login", telegram_login)
        .post("/telegram/miniapp/login", telegram_miniapp_login)
        .get("/telegram/oidc/start", telegram_oidc_start)
        .post("/telegram/oidc/callback", telegram_oidc_callback)
        .post("/google/login", google_login)
        .post("/google/redirect/intent", google_redirect_intent)
        .post("/google/redirect/callback", google_redirect_callback)
        .post("/google/redirect/exchange", google_redirect_exchange)
}

pub(super) fn me() -> Routes {
    Routes::new("")
        .get("/me/telegram", telegram_binding)
        .post("/me/telegram/bind", telegram_bind)
        .post("/me/telegram/miniapp/bind", telegram_miniapp_bind)
        .delete("/me/telegram/unbind", telegram_unbind)
        .get("/me/telegram/oidc/start", telegram_oidc_bind_start)
        .post("/me/telegram/oidc/callback", telegram_oidc_bind_callback)
        .get("/me/google", google_binding)
        .post("/me/google/bind", google_bind)
        .post("/me/google/redirect/intent", google_redirect_bind_intent)
        .post(
            "/me/google/redirect/exchange",
            google_redirect_bind_exchange,
        )
        .delete("/me/google/unbind", google_unbind)
}

fn oauth(s: &AppState) -> &OAuthService {
    &s.svc.identity.oauth
}

/// `loginRule` keyed by IP (original `KeyByIP`).
fn limit_ip(s: &AppState, ip: &str) -> Result<(), ApiError> {
    rate_limit::check(&s.svc.identity.login_limits.user, ip, MSG_LOGIN_TOO_MANY)
}

/// `loginRule` keyed by `user|ip` (original `KeyByUserIDAndIP`).
fn limit_user_ip(s: &AppState, user_id: i64, ip: &str) -> Result<(), ApiError> {
    rate_limit::check(
        &s.svc.identity.login_limits.user,
        &format!("{user_id}|{ip}"),
        MSG_LOGIN_TOO_MANY,
    )
}

/// Login response shared by every login endpoint.
fn login_response(outcome: UserLoginOutcome) -> ApiResult<Data<Value>> {
    match outcome {
        UserLoginOutcome::Token(session) => ok(json!({
            "requires_totp": false,
            "user": zs_app::identity::user_account::UserBrief::from(&session.user),
            "token": session.token,
            "expires_at": rfc3339(session.expires_at),
        })),
        UserLoginOutcome::Challenge { token, expires_at } => ok(json!({
            "requires_totp": true,
            "challenge_token": token,
            "challenge_expires_at": rfc3339(expires_at),
        })),
    }
}

fn to_value<T: serde::Serialize>(v: &T) -> Result<Value, ApiError> {
    serde_json::to_value(v).map_err(|e| Error::from(e).into())
}

// --- Telegram -------------------------------------------------------------

/// Widget payload; `id`, `auth_date` and `hash` are required (gin `binding:"required"`).
#[derive(Debug, Deserialize)]
struct WidgetRequest {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    photo_url: String,
    #[serde(default)]
    auth_date: i64,
    #[serde(default)]
    hash: String,
}

impl BindRules for WidgetRequest {
    const FIELDS: &'static [BindField] = &[
        req("id", "ID"),
        req("auth_date", "AuthDate"),
        req("hash", "Hash"),
    ];
}

/// `detailed`: the handler answers with `RespondBindError` (field list) rather than the
/// plain `error.bad_request` of the login endpoints.
fn widget_payload(body: &Bytes, detailed: bool) -> Result<WidgetPayload, ApiError> {
    let r: WidgetRequest = if detailed {
        bind_json(body)?
    } else {
        parse_json(body)?
    };
    if r.id == 0 || r.auth_date == 0 || r.hash.is_empty() {
        return Err(Error::invalid().into());
    }
    Ok(WidgetPayload {
        id: r.id,
        first_name: r.first_name,
        last_name: r.last_name,
        username: r.username,
        photo_url: r.photo_url,
        auth_date: r.auth_date,
        hash: r.hash,
    })
}

/// Mini App body: `init_data` or `initData`.
fn init_data(body: &Bytes) -> Result<String, ApiError> {
    #[derive(Deserialize)]
    struct MiniAppRequest {
        #[serde(default)]
        init_data: String,
        #[serde(default, rename = "initData")]
        init_data_camel: String,
    }
    let r: MiniAppRequest = parse_json(body)?;
    let value = if r.init_data.trim().is_empty() {
        r.init_data_camel.trim().to_owned()
    } else {
        r.init_data.trim().to_owned()
    };
    if value.is_empty() {
        return Err(Error::invalid().into());
    }
    Ok(value)
}

/// OIDC callback body; `code` and `state` are required (`detailed` as in [`widget_payload`]).
fn oidc_callback(body: &Bytes, detailed: bool) -> Result<(String, String), ApiError> {
    #[derive(Deserialize)]
    struct CallbackRequest {
        #[serde(default)]
        code: String,
        #[serde(default)]
        state: String,
    }
    impl BindRules for CallbackRequest {
        const FIELDS: &'static [BindField] = &[req("code", "Code"), req("state", "State")];
    }
    let r: CallbackRequest = if detailed {
        bind_json(body)?
    } else {
        parse_json(body)?
    };
    if r.code.is_empty() || r.state.is_empty() {
        return Err(Error::invalid().into());
    }
    Ok((r.code, r.state))
}

async fn telegram_login(
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    limit_ip(&s, &client.ip)?;
    let payload = match widget_payload(&body, false) {
        Ok(p) => p,
        Err(e) => {
            oauth(&s).record_bad_request(SOURCE_TELEGRAM, &client).await;
            return Err(e);
        }
    };
    login_response(oauth(&s).telegram_login(&payload, &client).await?)
}

async fn telegram_miniapp_login(
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    limit_ip(&s, &client.ip)?;
    let data = match init_data(&body) {
        Ok(d) => d,
        Err(e) => {
            oauth(&s).record_bad_request(SOURCE_TELEGRAM, &client).await;
            return Err(e);
        }
    };
    login_response(oauth(&s).telegram_miniapp_login(&data, &client).await?)
}

async fn telegram_oidc_start(
    State(s): State<AppState>,
    Client(client): Client,
) -> ApiResult<Data<Value>> {
    limit_ip(&s, &client.ip)?;
    let url = oauth(&s).telegram_oidc_start(OidcIntent::Login, 0).await?;
    ok(json!({ "auth_url": url }))
}

async fn telegram_oidc_callback(
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    limit_ip(&s, &client.ip)?;
    let (code, state) = match oidc_callback(&body, false) {
        Ok(v) => v,
        Err(e) => {
            oauth(&s).record_bad_request(SOURCE_TELEGRAM, &client).await;
            return Err(e);
        }
    };
    login_response(
        oauth(&s)
            .telegram_oidc_login(&code, &state, &client)
            .await?,
    )
}

async fn telegram_binding(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Data<Value>> {
    ok(to_value(&oauth(&s).telegram_binding(me.id).await?)?)
}

async fn telegram_bind(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    let payload = widget_payload(&body, true)?;
    ok(to_value(&oauth(&s).telegram_bind(me.id, &payload).await?)?)
}

async fn telegram_miniapp_bind(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    let data = init_data(&body)?;
    ok(to_value(
        &oauth(&s).telegram_miniapp_bind(me.id, &data).await?,
    )?)
}

async fn telegram_unbind(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Data<Value>> {
    oauth(&s).telegram_unbind(me.id).await?;
    ok(json!({ "unbound": true }))
}

async fn telegram_oidc_bind_start(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Data<Value>> {
    let url = oauth(&s)
        .telegram_oidc_start(OidcIntent::Bind, me.id)
        .await?;
    ok(json!({ "auth_url": url }))
}

async fn telegram_oidc_bind_callback(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    let (code, state) = oidc_callback(&body, true)?;
    ok(to_value(
        &oauth(&s).telegram_oidc_bind(me.id, &code, &state).await?,
    )?)
}

// --- Google ---------------------------------------------------------------

/// `{credential}` body limited to 64 KiB (HTTP 413 `error.request_too_large`).
fn credential(body: &Bytes) -> Result<String, ApiError> {
    #[derive(Deserialize)]
    struct CredentialRequest {
        #[serde(default)]
        credential: String,
    }
    impl BindRules for CredentialRequest {
        const FIELDS: &'static [BindField] = &[req("credential", "Credential")];
    }
    if body.len() > MAX_CREDENTIAL_BYTES {
        return Err(ApiError::with_http_status(
            Error::bad_request(keys::REQUEST_TOO_LARGE),
            StatusCode::PAYLOAD_TOO_LARGE,
        ));
    }
    let r: CredentialRequest = bind_json(body)?;
    if r.credential.is_empty() {
        return Err(Error::invalid().into());
    }
    Ok(r.credential)
}

async fn google_login(
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    limit_ip(&s, &client.ip)?;
    let cred = match credential(&body) {
        Ok(c) => c,
        Err(e) => {
            oauth(&s).record_bad_request(SOURCE_GOOGLE, &client).await;
            return Err(e);
        }
    };
    login_response(oauth(&s).google_login(&cred, &client).await?)
}

async fn google_binding(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Data<Value>> {
    ok(to_value(&oauth(&s).google_binding(me.id).await?)?)
}

async fn google_bind(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    Client(client): Client,
    body: Bytes,
) -> ApiResult<Data<Value>> {
    limit_user_ip(&s, me.id, &client.ip)?;
    let cred = credential(&body)?;
    ok(to_value(&oauth(&s).google_bind(me.id, &cred).await?)?)
}

async fn google_unbind(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Data<Value>> {
    oauth(&s).google_unbind(me.id).await?;
    ok(json!({ "unbound": true }))
}

/// Tenant of the request as bound into redirect state.
fn redirect_tenant(tenant: Option<&Tenant>) -> RedirectTenant {
    tenant.map_or_else(RedirectTenant::default, |t| RedirectTenant {
        host: t.host.clone(),
        is_main: t.is_main,
        reseller_id: t.reseller_id,
    })
}

/// `Set-Cookie` value like Go's `http.Cookie.String()` (Path=/, HttpOnly, Secure).
fn cookie(name: &str, value: &str, max_age: i64, same_site: &str) -> String {
    let (expires, max_age) = if max_age > 0 {
        (Utc::now() + Duration::seconds(max_age), max_age.to_string())
    } else {
        (
            chrono::DateTime::from_timestamp(1, 0).unwrap_or_default(),
            "0".to_owned(),
        )
    };
    format!(
        "{name}={value}; Path=/; Expires={}; Max-Age={max_age}; HttpOnly; Secure; SameSite={same_site}",
        expires.format("%a, %d %b %Y %H:%M:%S GMT")
    )
}

/// Values of every `name` cookie in the request.
fn cookies<'a>(headers: &'a HeaderMap, name: &str) -> Vec<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| {
            let (k, v) = pair.trim().split_once('=')?;
            (k.trim() == name).then(|| v.trim().trim_matches('"'))
        })
        .collect()
}

/// The only non-empty `name` cookie (duplicates are rejected).
fn single_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    match cookies(headers, name).as_slice() {
        [value] if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// `Cache-Control: no-store`, `Pragma: no-cache`, `Referrer-Policy: no-referrer`.
fn no_store(res: &mut Response) {
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
}

fn add_cookie(res: &mut Response, value: &str) {
    if let Ok(v) = HeaderValue::from_str(value) {
        res.headers_mut().append(header::SET_COOKIE, v);
    }
}

/// Error mapping of the redirect state endpoints (`respondGoogleRedirectAPIError`).
fn redirect_api_error(err: Error) -> ApiError {
    match err.key() {
        k if REDIRECT_STATE_KEYS.contains(&k) => err.into(),
        keys::GOOGLE_DISABLED | keys::GOOGLE_CONFIG_INVALID => err.into(),
        keys::GOOGLE_UNAVAILABLE => {
            ApiError::with_http_status(err, StatusCode::SERVICE_UNAVAILABLE)
        }
        _ => Error::internal(err)
            .or_internal(keys::GOOGLE_UNAVAILABLE)
            .into(),
    }
}

/// Locale and request id of a redirect endpoint. These endpoints render their
/// error envelopes themselves so the `no-store` and `Set-Cookie` headers survive
/// (`render_errors` rebuilds error responses from scratch).
#[derive(Debug)]
struct Envelope {
    locale: &'static str,
    request_id: String,
}

impl Envelope {
    fn of(uri: &Uri, headers: &HeaderMap, rid: Option<&RequestId>) -> Self {
        Self {
            locale: i18n::resolve_locale(uri.query(), headers),
            request_id: rid.map(|r| r.0.clone()).unwrap_or_default(),
        }
    }

    fn render(&self, result: Result<Data<Value>, ApiError>) -> Response {
        match result {
            Ok(d) => d.into_response(),
            Err(e) => {
                if e.error.kind() == ErrorKind::Internal {
                    tracing::error!(error = %e.error, "internal error");
                }
                let body = json!({
                    "status_code": e.error.kind().code(),
                    "msg": i18n::translate_args(self.locale, e.error.key(), e.error.args()),
                    "data": { "request_id": self.request_id },
                });
                (e.http_status.unwrap_or(StatusCode::OK), axum::Json(body)).into_response()
            }
        }
    }
}

async fn create_intent(
    s: &AppState,
    env: &Envelope,
    flow: RedirectFlow,
    user_id: i64,
    tenant: &RedirectTenant,
) -> Response {
    let mut res = match oauth(s).google_redirect_intent(flow, user_id, tenant).await {
        Ok(state) if valid_handle(&state) => {
            let mut res = Data(json!({
                "state": state,
                "expires_in": INTENT_TTL_SECONDS,
                "issued_at": rfc3339(Utc::now()),
            }))
            .into_response();
            add_cookie(
                &mut res,
                &cookie(INTENT_COOKIE, &state, INTENT_TTL_SECONDS, "None"),
            );
            res
        }
        Ok(_) => env.render(Err(Error::internal_msg("invalid google redirect state")
            .or_internal(keys::GOOGLE_CONFIG_INVALID)
            .into())),
        Err(e) => env.render(Err(redirect_api_error(e))),
    };
    no_store(&mut res);
    res
}

async fn google_redirect_intent(
    State(s): State<AppState>,
    Client(client): Client,
    tenant: Option<Extension<Tenant>>,
    rid: Option<Extension<RequestId>>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let env = Envelope::of(&uri, &headers, rid.as_deref());
    if let Err(e) = limit_ip(&s, &client.ip) {
        return env.render(Err(e));
    }
    let tenant = redirect_tenant(tenant.as_deref());
    create_intent(&s, &env, RedirectFlow::Login, 0, &tenant).await
}

async fn google_redirect_bind_intent(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    Client(client): Client,
    tenant: Option<Extension<Tenant>>,
    rid: Option<Extension<RequestId>>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let env = Envelope::of(&uri, &headers, rid.as_deref());
    if let Err(e) = limit_user_ip(&s, me.id, &client.ip) {
        return env.render(Err(e));
    }
    let tenant = redirect_tenant(tenant.as_deref());
    create_intent(&s, &env, RedirectFlow::Bind, me.id, &tenant).await
}

/// 303 to the storefront callback page with the flow and an optional error code.
fn redirect_to_page(flow: RedirectFlow, error: Option<&str>) -> Response {
    let mut location = format!("{CALLBACK_PAGE}?flow={}", flow.as_str());
    if let Some(code) = error {
        location.push_str("&error=");
        location.push_str(code);
    }
    let mut res = StatusCode::SEE_OTHER.into_response();
    if let Ok(v) = HeaderValue::from_str(&location) {
        res.headers_mut().insert(header::LOCATION, v);
    }
    res
}

/// Exactly one value of `key` in a parsed form.
fn single_field(
    form: &std::collections::BTreeMap<Vec<u8>, Vec<Vec<u8>>>,
    key: &str,
) -> Option<String> {
    match form.get(key.as_bytes()).map(Vec::as_slice) {
        Some([value]) => String::from_utf8(value.clone()).ok(),
        _ => None,
    }
}

/// Google `form_post` target. Never puts credentials or state into the URL;
/// state cookies are cleared only after the GIS double-submit CSRF check.
async fn google_redirect_callback(
    State(s): State<AppState>,
    Client(client): Client,
    tenant: Option<Extension<Tenant>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let mut res = callback_response(&s, &client, tenant.as_deref(), &headers, &body).await;
    no_store(&mut res);
    res
}

async fn callback_response(
    s: &AppState,
    client: &zs_app::identity::admin_auth::ClientInfo,
    tenant: Option<&Tenant>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Response {
    let login = RedirectFlow::Login;
    let is_form = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|m| {
            m.trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        });
    if !is_form || body.len() > MAX_CREDENTIAL_BYTES {
        return redirect_to_page(login, Some("invalid_request"));
    }
    let Some(form) = std::str::from_utf8(body).ok().and_then(parse_query) else {
        return redirect_to_page(login, Some("invalid_request"));
    };
    let credential = single_field(&form, "credential").filter(|c| !c.trim().is_empty());
    let state = single_field(&form, "state").filter(|c| !c.trim().is_empty());
    let form_csrf = single_field(&form, GIS_CSRF).filter(|c| !c.is_empty());
    let cookie_csrf = single_cookie(headers, GIS_CSRF);
    let csrf_ok = matches!((&form_csrf, cookie_csrf), (Some(f), Some(c)) if constant_time_eq(f.as_bytes(), c.as_bytes()));
    if !csrf_ok {
        // A forged cross-site POST must not clear a legitimate in-flight intent.
        return redirect_to_page(login, Some("csrf_mismatch"));
    }
    let intent_state = single_cookie(headers, INTENT_COOKIE).map(str::to_owned);
    let clear_intent = cookie(INTENT_COOKIE, "", -1, "None");
    let finish = |mut res: Response| {
        add_cookie(&mut res, &clear_intent);
        res
    };
    let Some(intent_state) = intent_state else {
        return finish(redirect_to_page(login, Some("session_expired")));
    };
    let (Some(credential), Some(state)) = (credential, state) else {
        return finish(redirect_to_page(login, Some("invalid_request")));
    };
    if !constant_time_eq(state.as_bytes(), intent_state.as_bytes()) {
        return finish(redirect_to_page(login, Some("csrf_mismatch")));
    }
    if !valid_handle(&state) {
        return finish(redirect_to_page(login, Some("session_expired")));
    }
    let tenant = redirect_tenant(tenant);
    let completion = oauth(s)
        .google_redirect_complete(&state, &credential, &tenant, client)
        .await;
    match completion.handoff {
        Ok(handle) if valid_handle(&handle) => {
            let mut res = finish(redirect_to_page(completion.flow, None));
            add_cookie(
                &mut res,
                &cookie(HANDOFF_COOKIE, &handle, HANDOFF_TTL_SECONDS, "Lax"),
            );
            res
        }
        Ok(_) => finish(redirect_to_page(completion.flow, Some("internal_error"))),
        Err(e) => finish(redirect_to_page(
            completion.flow,
            Some(callback_error_code(&e)),
        )),
    }
}

/// Takes the handoff cookie, always clearing it first.
fn take_handoff(headers: &HeaderMap) -> (Option<String>, String) {
    let handle = single_cookie(headers, HANDOFF_COOKIE)
        .filter(|h| valid_handle(h))
        .map(str::to_owned);
    (handle, cookie(HANDOFF_COOKIE, "", -1, "Lax"))
}

fn exchange_error(err: Error) -> ApiError {
    if REDIRECT_STATE_KEYS.contains(&err.key()) {
        redirect_api_error(err)
    } else {
        err.into()
    }
}

fn finish_exchange(env: &Envelope, result: Result<Data<Value>, ApiError>, clear: &str) -> Response {
    let mut res = env.render(result);
    add_cookie(&mut res, clear);
    no_store(&mut res);
    res
}

async fn google_redirect_exchange(
    State(s): State<AppState>,
    Client(client): Client,
    tenant: Option<Extension<Tenant>>,
    rid: Option<Extension<RequestId>>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let env = Envelope::of(&uri, &headers, rid.as_deref());
    let (handle, clear) = take_handoff(&headers);
    let result = match handle {
        None => Err(exchange_error(Error::bad_request(
            keys::GOOGLE_REDIRECT_SESSION_EXPIRED,
        ))),
        Some(handle) => {
            let tenant = redirect_tenant(tenant.as_deref());
            match oauth(&s)
                .google_redirect_exchange_login(&handle, &tenant, &client)
                .await
            {
                Ok(outcome) => login_response(outcome),
                Err(e) => Err(exchange_error(e)),
            }
        }
    };
    finish_exchange(&env, result, &clear)
}

async fn google_redirect_bind_exchange(
    State(s): State<AppState>,
    CurrentUser(me): CurrentUser,
    tenant: Option<Extension<Tenant>>,
    rid: Option<Extension<RequestId>>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let env = Envelope::of(&uri, &headers, rid.as_deref());
    let (handle, clear) = take_handoff(&headers);
    let result = match handle {
        None => Err(exchange_error(Error::bad_request(
            keys::GOOGLE_REDIRECT_SESSION_EXPIRED,
        ))),
        Some(handle) => {
            let tenant = redirect_tenant(tenant.as_deref());
            match oauth(&s)
                .google_redirect_exchange_bind(&handle, me.id, &tenant)
                .await
            {
                Ok(binding) => to_value(&binding).map(Data),
                Err(e) => Err(exchange_error(e)),
            }
        }
    };
    finish_exchange(&env, result, &clear)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_format_and_parsing() {
        let c = cookie(INTENT_COOKIE, "abc", 600, "None");
        assert!(c.starts_with("__Host-dujiao_google_state=abc; Path=/; Expires="));
        assert!(c.ends_with("; Max-Age=600; HttpOnly; Secure; SameSite=None"));
        let cleared = cookie(HANDOFF_COOKIE, "", -1, "Lax");
        assert!(cleared.contains("Expires=Thu, 01 Jan 1970 00:00:01 GMT; Max-Age=0"));

        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; g_csrf_token=x"),
        );
        assert_eq!(single_cookie(&h, GIS_CSRF), Some("x"));
        h.append(header::COOKIE, HeaderValue::from_static("g_csrf_token=y"));
        assert_eq!(single_cookie(&h, GIS_CSRF), None);
        assert_eq!(single_cookie(&h, "missing"), None);
    }

    #[test]
    fn error_kinds_of_redirect_errors() {
        let e = redirect_api_error(Error::bad_request(keys::GOOGLE_DISABLED));
        assert_eq!(e.error.kind(), ErrorKind::BadRequest);
        let e = redirect_api_error(zs_domain::identity::oauth::google_unavailable("x"));
        assert_eq!(e.http_status, Some(StatusCode::SERVICE_UNAVAILABLE));
        let e = redirect_api_error(Error::invalid());
        assert_eq!(e.error.key(), keys::GOOGLE_UNAVAILABLE);
    }
}
