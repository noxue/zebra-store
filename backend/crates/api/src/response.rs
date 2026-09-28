//! The response envelope `{status_code, msg, data, pagination?}` and error rendering.

use axum::Json;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;
use zs_domain::{Error, ErrorKind};
use zs_shared::page::Pagination;

use crate::i18n;
use crate::middleware::request_id::RequestId;

const MSG_SUCCESS: &str = "success";

/// A successful envelope.
#[derive(Debug)]
pub struct Data<T>(pub T);

/// Wraps a value in a successful envelope.
pub fn ok<T>(value: T) -> ApiResult<Data<T>> {
    Ok(Data(value))
}

impl<T: Serialize> IntoResponse for Data<T> {
    fn into_response(self) -> Response {
        Json(json!({ "status_code": 0, "msg": MSG_SUCCESS, "data": self.0 })).into_response()
    }
}

/// A successful paginated envelope.
#[derive(Debug)]
pub struct Paged<T>(pub Vec<T>, pub Pagination);

impl<T: Serialize> IntoResponse for Paged<T> {
    fn into_response(self) -> Response {
        Json(json!({
            "status_code": 0,
            "msg": MSG_SUCCESS,
            "data": self.0,
            "pagination": self.1,
        }))
        .into_response()
    }
}

/// Handler result type.
pub type ApiResult<T> = Result<T, ApiError>;

/// Error returned by handlers; rendered by [`render_errors`] with the request locale.
#[derive(Debug)]
pub struct ApiError {
    pub error: Error,
    /// When set, the response uses this real HTTP status (middleware errors only).
    pub http_status: Option<StatusCode>,
}

impl ApiError {
    pub fn with_http_status(error: Error, status: StatusCode) -> Self {
        Self {
            error,
            http_status: Some(status),
        }
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        Self {
            error,
            http_status: None,
        }
    }
}

/// Marker stored in response extensions so [`render_errors`] can translate the message.
#[derive(Debug, Clone)]
struct PendingError {
    kind: ErrorKind,
    key: String,
    args: Vec<String>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.error.kind() == ErrorKind::Internal {
            tracing::error!(error = %self.error, "internal error");
        }
        let status = self.http_status.unwrap_or(StatusCode::OK);
        let mut response = status.into_response();
        response.extensions_mut().insert(PendingError {
            kind: self.error.kind(),
            key: self.error.key().to_owned(),
            args: self.error.args().to_vec(),
        });
        response
    }
}

/// Middleware that turns [`ApiError`] responses into translated envelopes carrying the request id.
pub async fn render_errors(req: Request, next: Next) -> Response {
    let locale = i18n::resolve_locale(req.uri().query(), req.headers());
    let request_id = req
        .extensions()
        .get::<RequestId>()
        .map(|r| r.0.clone())
        .unwrap_or_default();
    let response = next.run(req).await;
    let Some(pending) = response.extensions().get::<PendingError>().cloned() else {
        return response;
    };
    let status = response.status();
    let body = json!({
        "status_code": pending.kind.code(),
        "msg": if pending.key == crate::extract::BIND_VALIDATION_KEY {
            i18n::bind_message(locale, &pending.args)
        } else {
            i18n::translate_args(locale, &pending.key, &pending.args)
        },
        "data": { "request_id": request_id },
    });
    (status, Json(body)).into_response()
}
