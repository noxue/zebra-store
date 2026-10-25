//! Self-service personal API keys and the two realm-specific token exchanges.
use super::admin::{blocked, counted};
use crate::{
    client::Client,
    extract::{BindField, BindRules, bind_json, req},
    middleware::auth::{CurrentAdmin, CurrentUser},
    response::{ApiResult, Data, ok},
    routes::Routes,
    state::AppState,
};
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, header},
};
use serde::{Deserialize, Serialize};
use zs_domain::identity::api_key::ApiKeyRealm;
use zs_domain::{Id, Result};

pub(super) fn admin_open() -> Routes {
    Routes::new("/admin").post("/api-key/token", admin_token)
}
pub(super) fn user_open() -> Routes {
    Routes::new("/auth").post("/api-key/token", user_token)
}
pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .get("/api-key", admin_status)
        .post("/api-key", admin_create)
        .put("/api-key", admin_rotate)
        .delete("/api-key", admin_delete)
}
pub(super) fn user() -> Routes {
    Routes::new("")
        .get("/me/api-key", user_status)
        .post("/me/api-key", user_create)
        .put("/me/api-key", user_rotate)
        .delete("/me/api-key", user_delete)
}
fn private<T: Serialize>(value: T) -> ApiResult<(HeaderMap, Data<T>)> {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok((headers, ok(value)?))
}
#[derive(Deserialize)]
struct Exchange {
    api_key: String,
}
impl BindRules for Exchange {
    const FIELDS: &'static [BindField] = &[req("api_key", "ApiKey")];
}
async fn exchange(
    s: &AppState,
    realm: ApiKeyRealm,
    ip: &str,
    body: &[u8],
) -> ApiResult<(HeaderMap, Data<zs_app::identity::api_key::ApiKeySession>)> {
    let limiter = if realm == ApiKeyRealm::Admin {
        &s.svc.identity.login_limits.admin
    } else {
        &s.svc.identity.login_limits.user
    };
    let key = format!("personal-api-key|{ip}");
    blocked(limiter, &key)?;
    let req: Exchange = bind_json(body)?;
    let result = s.svc.identity.api_keys.exchange(realm, &req.api_key).await;
    private(counted(limiter, &key, result)?)
}
async fn admin_token(
    State(s): State<AppState>,
    Client(c): Client,
    body: Bytes,
) -> ApiResult<impl axum::response::IntoResponse> {
    exchange(&s, ApiKeyRealm::Admin, &c.ip, &body).await
}
async fn user_token(
    State(s): State<AppState>,
    Client(c): Client,
    body: Bytes,
) -> ApiResult<impl axum::response::IntoResponse> {
    exchange(&s, ApiKeyRealm::User, &c.ip, &body).await
}
async fn status(
    s: &AppState,
    realm: ApiKeyRealm,
    id: Id,
) -> Result<Option<zs_domain::identity::api_key::PersonalApiKey>> {
    s.svc.identity.api_keys.status(realm, id).await
}
macro_rules! own_handlers {
    ($status:ident, $create:ident, $rotate:ident, $delete:ident, $current:ident, $realm:expr) => {
        async fn $status(
            State(s): State<AppState>,
            $current(p): $current,
        ) -> ApiResult<impl axum::response::IntoResponse> {
            private(status(&s, $realm, p.id).await?)
        }
        async fn $create(
            State(s): State<AppState>,
            $current(p): $current,
        ) -> ApiResult<impl axum::response::IntoResponse> {
            private(
                s.svc
                    .identity
                    .api_keys
                    .generate($realm, p.id, false)
                    .await?,
            )
        }
        async fn $rotate(
            State(s): State<AppState>,
            $current(p): $current,
        ) -> ApiResult<impl axum::response::IntoResponse> {
            private(s.svc.identity.api_keys.generate($realm, p.id, true).await?)
        }
        async fn $delete(
            State(s): State<AppState>,
            $current(p): $current,
        ) -> ApiResult<impl axum::response::IntoResponse> {
            private(s.svc.identity.api_keys.delete($realm, p.id).await?)
        }
    };
}
own_handlers!(
    admin_status,
    admin_create,
    admin_rotate,
    admin_delete,
    CurrentAdmin,
    ApiKeyRealm::Admin
);
own_handlers!(
    user_status,
    user_create,
    user_rotate,
    user_delete,
    CurrentUser,
    ApiKeyRealm::User
);
