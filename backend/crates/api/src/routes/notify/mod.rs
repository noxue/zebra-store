//! `notify` endpoints: notification center logs / test send, channel clients,
//! Telegram broadcasts (admin) and the channel API used by the Telegram bot.

pub mod channel;

use std::collections::HashMap;

use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use zs_app::notify::broadcast::CreateBroadcast;
use zs_app::notify::center::TestSend;
use zs_app::notify::clients::{CreateClient, UpdateClient};
use zs_domain::notify::broadcast::{Broadcast, BroadcastFilter, TelegramUser, TelegramUserQuery};
use zs_domain::notify::center::channels;
use zs_domain::notify::channel::ClientDetail;
use zs_domain::notify::log::{LogFilter, NotificationLog};
use zs_domain::{Error, Id};
use zs_shared::page::{PageRequest, Pagination};

use super::{RouteSet, Routes};
use crate::extract::{Bind, BindField, BindRules, Body, PathId, Query, Rule, req};
use crate::response::{ApiError, ApiResult, Data, Paged, ok};
use crate::state::AppState;

/// Routes of the `notify` group.
pub fn routes() -> RouteSet {
    RouteSet {
        admin: admin(),
        channel: channel::routes(),
        ..RouteSet::default()
    }
}

fn admin() -> Routes {
    Routes::new("/admin")
        .get("/settings/notification-center/logs", list_logs)
        .post("/settings/notification-center/test", test_send)
        .get("/settings/notifications/logs", list_logs)
        .post("/settings/notifications/test", test_send)
        .get("/channel-clients", list_clients)
        .post("/channel-clients", create_client)
        .get("/channel-clients/{id}", get_client)
        .put("/channel-clients/{id}", update_client)
        .delete("/channel-clients/{id}", delete_client)
        .put("/channel-clients/{id}/status", update_client_status)
        .post("/channel-clients/{id}/reset-secret", reset_client_secret)
        .get("/telegram-bot/broadcasts", list_broadcasts)
        .post("/telegram-bot/broadcasts", create_broadcast)
        .get("/telegram-bot/broadcasts/{id}", get_broadcast)
        .get("/telegram-bot/users", list_telegram_users)
}

type Params = Query<HashMap<String, String>>;

fn text(q: &HashMap<String, String>, key: &str) -> String {
    q.get(key).map(|s| s.trim().to_owned()).unwrap_or_default()
}

fn page_of(q: &HashMap<String, String>) -> PageRequest {
    let num = |k: &str| q.get(k).and_then(|v| v.trim().parse::<u64>().ok());
    PageRequest::new(num("page"), num("page_size"))
}

/// Optional RFC 3339 query parameter (original `ParseQueryTimeRange`).
fn opt_time(q: &HashMap<String, String>, key: &str) -> Result<Option<DateTime<Utc>>, ApiError> {
    match q.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => DateTime::parse_from_rfc3339(s)
            .map(|t| Some(t.with_timezone(&Utc)))
            .map_err(|_| Error::invalid().into()),
    }
}

/// Go `strconv.ParseBool` of an optional parameter (`None` when blank).
fn opt_bool(q: &HashMap<String, String>, key: &str) -> Result<Option<bool>, ApiError> {
    match q.get(key).map(|s| s.trim()).unwrap_or("") {
        "" => Ok(None),
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Ok(Some(true)),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Ok(Some(false)),
        _ => Err(Error::invalid().into()),
    }
}

// ---------------------------------------------------------------------------
// notification center
// ---------------------------------------------------------------------------

async fn list_logs(
    State(s): State<AppState>,
    Query(q): Params,
) -> ApiResult<Paged<NotificationLog>> {
    let page = page_of(&q);
    let filter = LogFilter {
        page,
        channel: text(&q, "channel").to_lowercase(),
        status: text(&q, "status").to_lowercase(),
        event_type: text(&q, "event_type").to_lowercase(),
        is_test: opt_bool(&q, "is_test")?,
        created_from: opt_time(&q, "created_from")?,
        created_to: opt_time(&q, "created_to")?,
    };
    let result = s.svc.notify.center.list_logs(&filter).await?;
    Ok(Paged(result.items, Pagination::new(page, result.total)))
}

#[derive(Debug, Deserialize)]
struct TestSendRequest {
    #[serde(default)]
    channel: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    scene: String,
    #[serde(default)]
    locale: String,
    #[serde(default)]
    variables: Option<Map<String, Value>>,
}

impl BindRules for TestSendRequest {
    const FIELDS: &'static [BindField] = &[req("channel", "Channel"), req("target", "Target")];
}

async fn test_send(
    State(s): State<AppState>,
    Bind(req): Bind<TestSendRequest>,
) -> ApiResult<Data<Value>> {
    let channel = req.channel.trim().to_lowercase();
    if channel.is_empty()
        || req.target.trim().is_empty()
        || !channels::ALL.contains(&channel.as_str())
    {
        return Err(Error::invalid().into());
    }
    s.svc
        .notify
        .center
        .send_test(TestSend {
            channel,
            target: req.target.trim().to_owned(),
            scene: req.scene.trim().to_owned(),
            locale: req.locale.trim().to_owned(),
            variables: req.variables.unwrap_or_default(),
        })
        .await?;
    ok(json!({"sent": true}))
}

// ---------------------------------------------------------------------------
// channel clients
// ---------------------------------------------------------------------------

async fn list_clients(State(s): State<AppState>) -> ApiResult<Data<Vec<ClientDetail>>> {
    ok(s.svc.notify.clients.list().await?)
}

#[derive(Debug, Deserialize)]
struct CreateClientRequest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    channel_type: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    bot_token: String,
    #[serde(default)]
    callback_url: String,
}

impl BindRules for CreateClientRequest {
    const FIELDS: &'static [BindField] = &[req("name", "Name"), req("channel_type", "ChannelType")];
}

async fn create_client(
    State(s): State<AppState>,
    Bind(req): Bind<CreateClientRequest>,
) -> ApiResult<Data<ClientDetail>> {
    if req.name.trim().is_empty() || req.channel_type.trim().is_empty() {
        return Err(Error::invalid().into());
    }
    ok(s.svc
        .notify
        .clients
        .create(CreateClient {
            name: req.name,
            channel_type: req.channel_type,
            description: req.description,
            bot_token: req.bot_token,
            callback_url: req.callback_url,
        })
        .await?)
}

async fn get_client(
    State(s): State<AppState>,
    PathId(id): PathId,
) -> ApiResult<Data<ClientDetail>> {
    ok(s.svc.notify.clients.get(id).await?)
}

#[derive(Debug, Deserialize)]
struct UpdateClientRequest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    /// Absent = keep, `""` = clear.
    #[serde(default)]
    bot_token: Option<String>,
    #[serde(default)]
    callback_url: Option<String>,
}

async fn update_client(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(req): Body<UpdateClientRequest>,
) -> ApiResult<Data<ClientDetail>> {
    ok(s.svc
        .notify
        .clients
        .update(
            id,
            UpdateClient {
                name: req.name,
                description: req.description,
                bot_token: req.bot_token,
                callback_url: req.callback_url,
            },
        )
        .await?)
}

#[derive(Debug, Deserialize)]
struct StatusRequest {
    #[serde(default)]
    status: i32,
}

impl BindRules for StatusRequest {
    const FIELDS: &'static [BindField] = &[BindField {
        json: "status",
        go: "Status",
        rules: &[Rule::OneOf("0 1")],
    }];
}

async fn update_client_status(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(req): Bind<StatusRequest>,
) -> ApiResult<Data<()>> {
    s.svc.notify.clients.set_status(id, req.status).await?;
    ok(())
}

async fn reset_client_secret(
    State(s): State<AppState>,
    PathId(id): PathId,
) -> ApiResult<Data<ClientDetail>> {
    ok(s.svc.notify.clients.reset_secret(id).await?)
}

async fn delete_client(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<()>> {
    s.svc.notify.clients.delete(id).await?;
    ok(())
}

// ---------------------------------------------------------------------------
// broadcasts
// ---------------------------------------------------------------------------

async fn list_broadcasts(
    State(s): State<AppState>,
    Query(q): Params,
) -> ApiResult<Paged<Broadcast>> {
    let page = page_of(&q);
    let filter = BroadcastFilter {
        page,
        keyword: text(&q, "keyword"),
        recipient_type: text(&q, "recipient_type").to_lowercase(),
        status: text(&q, "status").to_lowercase(),
        created_from: opt_time(&q, "created_from")?,
        created_to: opt_time(&q, "created_to")?,
    };
    let result = s.svc.notify.broadcasts.list(&filter).await?;
    Ok(Paged(result.items, Pagination::new(page, result.total)))
}

#[derive(Debug, Deserialize)]
struct CreateBroadcastRequest {
    #[serde(default)]
    title: String,
    #[serde(default)]
    recipient_type: String,
    #[serde(default)]
    user_ids: Vec<Id>,
    #[serde(default)]
    filters: Option<Map<String, Value>>,
    #[serde(default)]
    attachment_url: String,
    #[serde(default)]
    attachment_name: String,
    #[serde(default)]
    message_html: String,
}

impl BindRules for CreateBroadcastRequest {
    const FIELDS: &'static [BindField] = &[
        req("title", "Title"),
        req("recipient_type", "RecipientType"),
        req("message_html", "MessageHTML"),
    ];
}

async fn create_broadcast(
    State(s): State<AppState>,
    Bind(req): Bind<CreateBroadcastRequest>,
) -> ApiResult<Data<Broadcast>> {
    ok(s.svc
        .notify
        .broadcasts
        .create(CreateBroadcast {
            title: req.title,
            recipient_type: req.recipient_type,
            user_ids: req.user_ids,
            filters: req.filters.unwrap_or_default(),
            attachment_url: req.attachment_url,
            attachment_name: req.attachment_name,
            message_html: req.message_html,
        })
        .await?)
}

async fn get_broadcast(
    State(s): State<AppState>,
    PathId(id): PathId,
) -> ApiResult<Data<Broadcast>> {
    ok(s.svc.notify.broadcasts.get(id).await?)
}

async fn list_telegram_users(
    State(s): State<AppState>,
    Query(q): Params,
) -> ApiResult<Paged<TelegramUser>> {
    let page = page_of(&q);
    let query = TelegramUserQuery {
        page: Some(page),
        user_ids: Vec::new(),
        keyword: text(&q, "keyword"),
        display_name: text(&q, "display_name"),
        telegram_username: text(&q, "telegram_username"),
        telegram_user_id: text(&q, "telegram_user_id"),
        created_from: opt_time(&q, "created_from")?,
        created_to: opt_time(&q, "created_to")?,
    };
    let result = s.svc.notify.broadcasts.users(&query).await?;
    Ok(Paged(result.items, Pagination::new(page, result.total)))
}
