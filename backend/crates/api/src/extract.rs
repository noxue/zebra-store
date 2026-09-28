//! Extractors whose rejections render as `error.bad_request` envelopes.

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;
use zs_domain::{Error, Id};

use crate::response::ApiError;

/// JSON body; malformed input yields `error.bad_request`.
#[derive(Debug)]
pub struct Body<T>(pub T);

impl<S, T> FromRequest<S> for Body<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(v)) => Ok(Self(v)),
            Err(rejection) => Err(bad_json(&rejection)),
        }
    }
}

fn bad_json(rejection: &JsonRejection) -> ApiError {
    tracing::debug!(%rejection, "invalid json body");
    Error::invalid().into()
}

/// Message key of a Go-validator style bind error; each argument is one failed
/// `Field\ttag[\tparam]` triple, rendered by [`crate::i18n::bind_message`] like the original
/// `ginutil.formatFieldError` (`Field: <rule text>`).
pub const BIND_VALIDATION_KEY: &str = "validation.__bind__";

/// One go-playground/validator tag of a `binding:"..."` rule.
#[derive(Debug, Clone, Copy)]
pub enum Rule {
    /// `required` on a value type (string, number, bool, slice, map): fails when the field is
    /// absent, `null`, or the Go zero value (`""`, `0`, `false`); empty slices/maps pass.
    Required,
    /// `required` on a pointer (`*bool`, `*[]T`): fails only when absent or `null`.
    RequiredPtr,
    /// `min=N`: slice/map length, string rune count, or number value `>= N`.
    Min(&'static str),
    /// `oneof=a b`: the value (absent = Go zero value `0`/`""`) is one of the space-separated items.
    OneOf(&'static str),
}

impl Rule {
    const fn tag(self) -> &'static str {
        match self {
            Self::Required | Self::RequiredPtr => "required",
            Self::Min(_) => "min",
            Self::OneOf(_) => "oneof",
        }
    }

    const fn param(self) -> &'static str {
        match self {
            Self::Required | Self::RequiredPtr => "",
            Self::Min(p) | Self::OneOf(p) => p,
        }
    }

    fn passes(self, v: Option<&serde_json::Value>) -> bool {
        use serde_json::Value;
        match self {
            Self::Required => !is_zero(v),
            Self::RequiredPtr => !matches!(v, None | Some(Value::Null)),
            Self::Min(p) => {
                let min = p.parse::<f64>().unwrap_or(0.0);
                let size = match v {
                    None | Some(Value::Null) => 0.0,
                    Some(Value::Array(a)) => a.len() as f64,
                    Some(Value::Object(o)) => o.len() as f64,
                    Some(Value::String(s)) => s.chars().count() as f64,
                    Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
                    Some(Value::Bool(_)) => return true,
                };
                size >= min
            }
            Self::OneOf(p) => {
                let numeric = p.split_whitespace().all(|o| o.parse::<f64>().is_ok());
                let actual = match v {
                    None | Some(Value::Null) if numeric => "0".to_owned(),
                    None | Some(Value::Null) => String::new(),
                    Some(Value::String(s)) => s.clone(),
                    Some(other) => other.to_string(),
                };
                p.split_whitespace().any(|o| o == actual)
            }
        }
    }
}

/// A validated field of the original Go request struct: its JSON name, the Go field name the
/// original prints (`"Slug: 不能为空"`), and its `binding` tags in order.
#[derive(Debug, Clone, Copy)]
pub struct BindField {
    pub json: &'static str,
    pub go: &'static str,
    pub rules: &'static [Rule],
}

/// `binding:"required"` on a value-typed field.
pub const fn req(json: &'static str, go: &'static str) -> BindField {
    BindField {
        json,
        go,
        rules: &[Rule::Required],
    }
}

/// `binding:"required"` on a pointer field (`*bool`): only `null`/absent fails.
pub const fn req_ptr(json: &'static str, go: &'static str) -> BindField {
    BindField {
        json,
        go,
        rules: &[Rule::RequiredPtr],
    }
}

/// `binding:"required,min=1"`.
pub const fn req_min1(json: &'static str, go: &'static str) -> BindField {
    BindField {
        json,
        go,
        rules: &[Rule::Required, Rule::Min("1")],
    }
}

/// The `binding:"..."` fields of the original Go request struct, in declaration order. Only
/// the first failing tag of each field is reported, like go-playground/validator.
pub trait BindRules {
    const FIELDS: &'static [BindField];
}

/// JSON body bound like gin's `ShouldBindJSON` + `RespondBindError`: unparsable input yields
/// `error.bad_request`; fields failing their `binding` rules yield the per-field message.
#[derive(Debug)]
pub struct Bind<T>(pub T);

impl<S, T> FromRequest<S> for Bind<T>
where
    T: DeserializeOwned + BindRules,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::Bytes::from_request(req, state)
            .await
            .map_err(|_| ApiError::from(Error::invalid()))?;
        bind_json(&bytes).map(Self)
    }
}

/// Why a body failed to bind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindFailure {
    /// Not JSON, wrong types, or not an object: the generic `error.bad_request`.
    Invalid,
    /// Validation failures as `Field\ttag[\tparam]` arguments of [`BIND_VALIDATION_KEY`].
    Fields(Vec<String>),
}

impl BindFailure {
    /// The response message in `locale` (`RespondBindError` / channel `BindError`).
    pub fn message(&self, locale: &str) -> String {
        match self {
            Self::Invalid => crate::i18n::translate(locale, "error.bad_request"),
            Self::Fields(args) => crate::i18n::bind_message(locale, args),
        }
    }
}

/// go-playground/validator `ValidationErrors.Error()` for `struct_name` (the raw text the
/// upstream API echoes after `"invalid request body: "`), one line per failed field.
pub fn validator_text(struct_name: &str, args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            let mut parts = arg.split('\t');
            let field = parts.next().unwrap_or_default();
            let tag = parts.next().unwrap_or("required");
            format!(
                "Key: '{struct_name}.{field}' Error:Field validation for '{field}' failed on the '{tag}' tag"
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl From<BindFailure> for ApiError {
    fn from(f: BindFailure) -> Self {
        match f {
            BindFailure::Invalid => Error::invalid().into(),
            BindFailure::Fields(args) => args
                .into_iter()
                .fold(Error::bad_request(BIND_VALIDATION_KEY), Error::arg)
                .into(),
        }
    }
}

/// [`Bind`] for handlers that must read the raw body first (e.g. rate limiting by a field).
pub fn bind_json<T: DeserializeOwned + BindRules>(bytes: &[u8]) -> Result<T, ApiError> {
    bind_value(bytes).map_err(ApiError::from)
}

/// Decodes `bytes` into `T` and applies `T::FIELDS`, like gin's `ShouldBindJSON`.
pub fn bind_value<T: DeserializeOwned + BindRules>(bytes: &[u8]) -> Result<T, BindFailure> {
    let mut value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| {
        tracing::debug!(error = %e, "invalid json body");
        BindFailure::Invalid
    })?;
    // Go decodes a JSON `null` body into the zero struct, which then fails validation.
    if value.is_null() {
        value = serde_json::Value::Object(serde_json::Map::new());
    }
    bind_fields(&value)
}

/// Applies `T::FIELDS` to an already-parsed JSON object (or query map) and deserializes it.
pub fn bind_fields<T: DeserializeOwned + BindRules>(
    value: &serde_json::Value,
) -> Result<T, BindFailure> {
    // Go's decoder leaves a field untouched on `null`; drop null rule fields so a non-optional
    // Rust field reports the rule failure instead of a type error.
    let mut cleaned = value.clone();
    if let Some(obj) = cleaned.as_object_mut() {
        for f in T::FIELDS {
            if obj.get(f.json).is_some_and(serde_json::Value::is_null) {
                obj.remove(f.json);
            }
        }
    }
    let parsed = T::deserialize(&cleaned);
    // gin decodes first: a type error is `error.bad_request` even when fields are missing.
    if let Err(e) = &parsed
        && (!value.is_object() || !e.to_string().starts_with("missing field"))
    {
        tracing::debug!(error = %e, "invalid json body");
        return Err(BindFailure::Invalid);
    }
    let failed: Vec<String> = T::FIELDS
        .iter()
        .filter_map(|f| {
            let v = value.get(f.json);
            f.rules.iter().find(|r| !r.passes(v)).map(|r| {
                if r.param().is_empty() {
                    format!("{}\t{}", f.go, r.tag())
                } else {
                    format!("{}\t{}\t{}", f.go, r.tag(), r.param())
                }
            })
        })
        .collect();
    if !failed.is_empty() {
        return Err(BindFailure::Fields(failed));
    }
    parsed.map_err(|e| {
        tracing::debug!(error = %e, "invalid json body");
        BindFailure::Invalid
    })
}

/// go-playground/validator `required`: absent, null, or the Go zero value (`""`, `0`,
/// `false`); non-nil maps and slices pass even when empty.
fn is_zero(v: Option<&serde_json::Value>) -> bool {
    use serde_json::Value;
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Number(n)) => n.as_f64() == Some(0.0),
        Some(Value::Bool(b)) => !b,
        Some(Value::Array(_) | Value::Object(_)) => false,
    }
}

/// Query string; malformed input yields `error.bad_request`.
#[derive(Debug)]
pub struct Query<T>(pub T);

impl<S, T> FromRequestParts<S> for Query<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(v)) => Ok(Self(v)),
            Err(rejection) => {
                let rejection: QueryRejection = rejection;
                tracing::debug!(%rejection, "invalid query");
                Err(Error::invalid().into())
            }
        }
    }
}

/// A numeric `:id` path parameter.
#[derive(Debug, Clone, Copy)]
pub struct PathId(pub Id);

impl<S: Send + Sync> FromRequestParts<S> for PathId {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Path::<Id>::from_request_parts(parts, state).await {
            Ok(axum::extract::Path(id)) if id > 0 => Ok(Self(id)),
            Ok(_) => Err(Error::invalid().into()),
            Err(rejection) => {
                let rejection: PathRejection = rejection;
                tracing::debug!(%rejection, "invalid path id");
                Err(Error::invalid().into())
            }
        }
    }
}
