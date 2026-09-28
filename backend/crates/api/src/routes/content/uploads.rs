//! Safe static serving of `/uploads` (UPL-01, UPL-04).
//!
//! Every response gets `X-Content-Type-Options: nosniff`; anything that is not a
//! raster image (SVG, attachments, unknown types) is additionally forced to
//! download and sandboxed, so an uploaded document can never run script in the
//! site's origin even if upload validation were bypassed.

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderValue, header};
use axum::middleware::{Next, from_fn};
use axum::response::Response;
use tower_http::services::ServeDir;

/// Extensions served inline (raster images rendered by `<img>`).
const INLINE_EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "gif", "webp", "ico", "bmp", "avif"];
/// Policy for downloaded documents: no script, no plugins, isolated origin.
const SANDBOX_CSP: &str =
    "sandbox; default-src 'none'; style-src 'unsafe-inline'; script-src 'none'";

/// Serves files below `dir`; mount with `nest_service("/uploads", uploads_router(dir))`.
pub fn uploads_router(dir: &str) -> Router {
    Router::new()
        .fallback_service(ServeDir::new(dir))
        .layer(from_fn(upload_headers))
}

fn is_inline(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.rsplit_once('.')
        .is_some_and(|(_, ext)| INLINE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

async fn upload_headers(req: Request, next: Next) -> Response {
    let inline = is_inline(req.uri().path());
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if !inline {
        h.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment"),
        );
        h.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(SANDBOX_CSP),
        );
    }
    res
}

#[cfg(test)]
mod tests {
    use super::is_inline;

    #[test]
    fn inline_only_for_raster_images() {
        assert!(is_inline("/post/2026/01/a.PNG"));
        assert!(!is_inline("/post/2026/01/a.svg"));
        assert!(!is_inline("/telegram/2026/01/a.zip"));
        assert!(!is_inline("/x/noext"));
    }
}
