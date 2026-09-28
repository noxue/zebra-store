//! The single error type crossing domain → app → api boundaries.
//!
//! Every error carries an i18n message key (e.g. `error.product_not_found`)
//! identical to the original project's keys, plus a coarse [`ErrorKind`] that
//! the HTTP layer maps to the envelope `status_code`.

use std::borrow::Cow;
use std::fmt;

/// Coarse error category, mapped to the envelope `status_code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 400
    BadRequest,
    /// 401
    Unauthorized,
    /// 403
    Forbidden,
    /// 404
    NotFound,
    /// 429
    TooManyRequests,
    /// 500
    Internal,
}

impl ErrorKind {
    /// Envelope status code used by the original API.
    pub fn code(self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::Unauthorized => 401,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::TooManyRequests => 429,
            Self::Internal => 500,
        }
    }
}

type Source = Box<dyn std::error::Error + Send + Sync + 'static>;

/// A business or infrastructure error with an i18n message key.
pub struct Error {
    kind: ErrorKind,
    key: Cow<'static, str>,
    args: Vec<String>,
    source: Option<Source>,
}

/// Result alias used across domain, app and infra.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn new(kind: ErrorKind, key: impl Into<Cow<'static, str>>) -> Self {
        Self {
            kind,
            key: key.into(),
            args: Vec::new(),
            source: None,
        }
    }

    pub fn bad_request(key: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::BadRequest, key)
    }

    pub fn unauthorized(key: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::Unauthorized, key)
    }

    pub fn forbidden(key: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::Forbidden, key)
    }

    pub fn not_found(key: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::NotFound, key)
    }

    pub fn too_many(key: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::TooManyRequests, key)
    }

    /// Generic invalid-parameter error (`error.bad_request`).
    pub fn invalid() -> Self {
        Self::bad_request(keys::BAD_REQUEST)
    }

    /// Internal error wrapping an underlying cause; the cause is logged, never shown.
    pub fn internal(source: impl Into<Source>) -> Self {
        Self {
            kind: ErrorKind::Internal,
            key: Cow::Borrowed(keys::INTERNAL),
            args: Vec::new(),
            source: Some(source.into()),
        }
    }

    /// Internal error with a plain message.
    pub fn internal_msg(msg: impl Into<String>) -> Self {
        Self::internal(msg.into())
    }

    /// Adds a positional argument substituted into `%d` / `%s` placeholders.
    #[must_use]
    pub fn arg(mut self, value: impl ToString) -> Self {
        self.args.push(value.to_string());
        self
    }

    /// Replaces the message key of an internal error with a context-specific one
    /// (e.g. `error.category_create_failed`), keeping the cause for logging.
    #[must_use]
    pub fn or_internal(mut self, key: &'static str) -> Self {
        if self.kind == ErrorKind::Internal {
            self.key = Cow::Borrowed(key);
        }
        self
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn is_not_found(&self) -> bool {
        self.kind == ErrorKind::NotFound
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Error");
        d.field("kind", &self.kind).field("key", &self.key);
        if !self.args.is_empty() {
            d.field("args", &self.args);
        }
        if let Some(source) = &self.source {
            d.field("source", &format_args!("{source}"));
        }
        d.finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.source {
            Some(source) => write!(f, "{}: {source}", self.key),
            None => f.write_str(&self.key),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|s| s as &(dyn std::error::Error + 'static))
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::internal(value)
    }
}

/// Frequently used message keys (the full table lives in `zs-api`).
pub mod keys {
    pub const BAD_REQUEST: &str = "error.bad_request";
    pub const INTERNAL: &str = "error.internal_error";
    pub const UNAUTHORIZED: &str = "error.unauthorized";
    pub const FORBIDDEN: &str = "error.forbidden";
    pub const NOT_FOUND: &str = "error.not_found";
    pub const TOKEN_INVALID: &str = "error.token_invalid";
    pub const TOKEN_REVOKED: &str = "error.token_revoked";
}
