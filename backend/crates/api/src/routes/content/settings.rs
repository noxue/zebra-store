//! `/admin/settings/*` endpoints (port of `settings/transport/http`).

use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_domain::Error;
use zs_domain::settings::keys;
use zs_domain::settings::schema::captcha::CaptchaPatch;
use zs_domain::settings::schema::integration::AffiliateSetting;
use zs_domain::settings::schema::login::{GoogleAuthPatch, TelegramAuthPatch};
use zs_domain::settings::schema::notification::NotificationCenterPatch;
use zs_domain::settings::schema::order_email::OrderEmailTemplatePatch;
use zs_domain::settings::schema::smtp::SmtpPatch;
use zs_domain::settings::schema::telegram_bot::TelegramBotSetting;

use crate::extract::{Bind, BindField, BindRules, Body, Query, req};
use crate::response::{ApiResult, Data, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn routes() -> Routes {
    Routes::new("/admin")
        .get("/settings", get_setting)
        .put("/settings", put_setting)
        .get("/settings/smtp", get_smtp)
        .put("/settings/smtp", put_smtp)
        .post("/settings/smtp/test", test_smtp)
        .get("/settings/captcha", get_captcha)
        .put("/settings/captcha", put_captcha)
        .get("/settings/telegram-auth", get_telegram_auth)
        .put("/settings/telegram-auth", put_telegram_auth)
        .get("/settings/google-auth", get_google_auth)
        .put("/settings/google-auth", put_google_auth)
        .get("/settings/affiliate", get_affiliate)
        .put("/settings/affiliate", put_affiliate)
        .get("/settings/order-email-template", get_order_email_template)
        .put("/settings/order-email-template", put_order_email_template)
        .post(
            "/settings/order-email-template/reset",
            reset_order_email_template,
        )
        .get("/settings/notification-center", get_notification_center)
        .put("/settings/notification-center", put_notification_center)
        .get("/settings/notifications", get_notification_center)
        .put("/settings/notifications", put_notification_center)
        .get("/settings/telegram-bot", get_telegram_bot)
        .put("/settings/telegram-bot", put_telegram_bot)
        .get(
            "/settings/telegram-bot/runtime-status",
            get_telegram_bot_status,
        )
}

#[derive(Debug, Deserialize)]
struct KeyQuery {
    #[serde(default)]
    key: Option<String>,
}

async fn get_setting(
    State(s): State<AppState>,
    Query(q): Query<KeyQuery>,
) -> ApiResult<Data<Value>> {
    let key = q
        .key
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| keys::SITE_CONFIG.to_owned());
    ok(s.svc.content.settings.admin_view(key.trim()).await?)
}

#[derive(Debug, Deserialize)]
struct UpdateRequest {
    key: String,
    value: serde_json::Map<String, Value>,
}

impl BindRules for UpdateRequest {
    const FIELDS: &'static [BindField] = &[req("key", "Key"), req("value", "Value")];
}

async fn put_setting(
    State(s): State<AppState>,
    Bind(req): Bind<UpdateRequest>,
) -> ApiResult<Data<Value>> {
    if req.key.is_empty() {
        return Err(Error::invalid().into());
    }
    if req.key.trim() == keys::GOOGLE_AUTH_CONFIG {
        return Err(Error::bad_request(
            "google_auth_config must be updated through /admin/settings/google-auth",
        )
        .into());
    }
    let value = Value::Object(req.value);
    ok(s.svc
        .content
        .settings
        .admin_update(req.key.trim(), &value)
        .await?)
}

async fn get_smtp(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    ok(s.svc.content.settings.smtp().await?.masked())
}

async fn put_smtp(State(s): State<AppState>, Body(p): Body<SmtpPatch>) -> ApiResult<Data<Value>> {
    ok(s.svc.content.settings.patch_smtp(p).await?.masked())
}

#[derive(Debug, Deserialize)]
struct SmtpTestRequest {
    to_email: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    body: String,
}

impl BindRules for SmtpTestRequest {
    const FIELDS: &'static [BindField] = &[req("to_email", "ToEmail")];
}

async fn test_smtp(
    State(s): State<AppState>,
    Bind(r): Bind<SmtpTestRequest>,
) -> ApiResult<Data<Value>> {
    s.svc
        .content
        .settings
        .test_smtp(&r.to_email, &r.subject, &r.body)
        .await?;
    ok(json!({"sent": true}))
}

async fn get_captcha(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    ok(s.svc.content.settings.captcha().await?.masked())
}

async fn put_captcha(
    State(s): State<AppState>,
    Body(p): Body<CaptchaPatch>,
) -> ApiResult<Data<Value>> {
    ok(s.svc.content.settings.patch_captcha(p).await?.masked())
}

async fn get_telegram_auth(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    ok(s.svc.content.settings.telegram_auth().await?.masked())
}

async fn put_telegram_auth(
    State(s): State<AppState>,
    Body(p): Body<TelegramAuthPatch>,
) -> ApiResult<Data<Value>> {
    ok(s.svc
        .content
        .settings
        .patch_telegram_auth(p)
        .await?
        .masked())
}

async fn get_google_auth(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    ok(s.svc.content.settings.google_auth().await?.encode())
}

async fn put_google_auth(
    State(s): State<AppState>,
    Body(p): Body<GoogleAuthPatch>,
) -> ApiResult<Data<Value>> {
    ok(s.svc.content.settings.patch_google_auth(p).await?.encode())
}

async fn get_affiliate(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    ok(s.svc.content.settings.affiliate().await?.encode())
}

async fn put_affiliate(
    State(s): State<AppState>,
    Body(p): Body<AffiliateSetting>,
) -> ApiResult<Data<Value>> {
    ok(s.svc.content.settings.update_affiliate(p).await?.encode())
}

async fn get_order_email_template(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    ok(s.svc
        .content
        .settings
        .order_email_template()
        .await?
        .encode())
}

async fn put_order_email_template(
    State(s): State<AppState>,
    Body(p): Body<OrderEmailTemplatePatch>,
) -> ApiResult<Data<Value>> {
    ok(s.svc
        .content
        .settings
        .patch_order_email_template(p)
        .await?
        .encode())
}

async fn reset_order_email_template(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    ok(s.svc
        .content
        .settings
        .reset_order_email_template()
        .await?
        .encode())
}

async fn get_notification_center(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    ok(s.svc.content.settings.notification_center().await?.masked())
}

async fn put_notification_center(
    State(s): State<AppState>,
    Body(p): Body<NotificationCenterPatch>,
) -> ApiResult<Data<Value>> {
    ok(s.svc
        .content
        .settings
        .patch_notification_center(p)
        .await?
        .masked())
}

async fn get_telegram_bot(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    ok(s.svc.content.settings.telegram_bot().await?.encode())
}

async fn put_telegram_bot(
    State(s): State<AppState>,
    Body(cfg): Body<TelegramBotSetting>,
) -> ApiResult<Data<Value>> {
    ok(s.svc
        .content
        .settings
        .update_telegram_bot(cfg)
        .await?
        .encode())
}

async fn get_telegram_bot_status(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    ok(s.svc
        .content
        .settings
        .telegram_bot_runtime_status()
        .await?
        .encode())
}
