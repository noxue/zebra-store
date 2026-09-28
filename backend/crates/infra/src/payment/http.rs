//! Outbound HTTP for gateways behind a mockable transport (tests run offline).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

/// Default outbound timeout (`outboundctx.DefaultTimeout`, 15 s).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// An outbound request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    pub fn new(method: &str, url: impl Into<String>) -> Self {
        Self {
            method: method.to_owned(),
            url: url.into(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    #[must_use]
    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_owned(), value.into()));
        self
    }

    #[must_use]
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// First header value (case-insensitive).
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A received response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    #[must_use]
    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_owned(), value.into()));
        self
    }

    pub fn header_value(&self, name: &str) -> String {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim().to_owned())
            .unwrap_or_default()
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Sends HTTP requests; errors are transport failures (never HTTP status codes).
#[async_trait]
pub trait HttpTransport: Send + Sync + std::fmt::Debug {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, String>;
}

/// Production transport backed by reqwest (no redirects followed, bounded timeout).
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, String> {
        let method =
            reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| e.to_string())?;
        let mut builder = self.client.request(method, &request.url);
        for (k, v) in &request.headers {
            builder = builder.header(k, v);
        }
        if !request.body.is_empty() {
            builder = builder.body(request.body);
        }
        let resp = builder.send().await.map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_owned(),
                    v.to_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        let body = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

type Responder = dyn Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync;

/// Scripted transport for tests: records every request and answers with a closure.
#[derive(Clone)]
pub struct MockTransport {
    responder: Arc<Responder>,
    requests: Arc<Mutex<Vec<HttpRequest>>>,
}

impl std::fmt::Debug for MockTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MockTransport")
    }
}

impl MockTransport {
    pub fn new(
        responder: impl Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            responder: Arc::new(responder),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Always answers `status` with `body`.
    pub fn fixed(status: u16, body: &str) -> Self {
        let body = body.to_owned();
        Self::new(move |_| Ok(HttpResponse::new(status, body.clone())))
    }

    /// Requests sent so far.
    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }
}

#[async_trait]
impl HttpTransport for MockTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, String> {
        let resp = (self.responder)(&request);
        if let Ok(mut log) = self.requests.lock() {
            log.push(request);
        }
        resp
    }
}
