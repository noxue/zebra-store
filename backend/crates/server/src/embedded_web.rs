//! Static storefront and admin assets compiled into release binaries.

use axum::body::Body;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, LOCATION, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, Method, StatusCode, Uri};
use axum::response::Response;

include!(concat!(env!("OUT_DIR"), "/embedded_web_assets.rs"));

pub async fn serve(method: Method, uri: Uri) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return empty(StatusCode::METHOD_NOT_ALLOWED);
    }

    let path = uri.path();
    if path == "/admin" {
        return Response::builder()
            .status(StatusCode::PERMANENT_REDIRECT)
            .header(LOCATION, "/admin/")
            .body(Body::empty())
            .unwrap_or_else(|_| empty(StatusCode::INTERNAL_SERVER_ERROR));
    }
    if reserved_backend_path(path) {
        return empty(StatusCode::NOT_FOUND);
    }

    let (assets, requested) = if let Some(admin_path) = path.strip_prefix("/admin/") {
        (ADMIN_ASSETS, admin_path)
    } else {
        (STOREFRONT_ASSETS, path.trim_start_matches('/'))
    };
    let Some(requested) = safe_path(requested) else {
        return empty(StatusCode::NOT_FOUND);
    };
    let asset = find(assets, requested)
        .or_else(|| (!looks_like_file(requested)).then(|| find(assets, "index.html"))?);
    let Some(asset) = asset else {
        return empty(StatusCode::NOT_FOUND);
    };

    let cache = if asset.path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(asset.bytes)
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type(asset.path))
        .header(CACHE_CONTROL, cache)
        .header(X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(body)
        .unwrap_or_else(|_| empty(StatusCode::INTERNAL_SERVER_ERROR))
}

fn find(assets: &'static [EmbeddedAsset], path: &str) -> Option<&'static EmbeddedAsset> {
    assets
        .binary_search_by_key(&path, |asset| asset.path)
        .ok()
        .map(|index| &assets[index])
}

fn safe_path(path: &str) -> Option<&str> {
    (!path.split('/').any(|part| part == "..") && !path.contains('\\')).then_some(path)
}

fn looks_like_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.contains('.'))
}

fn reserved_backend_path(path: &str) -> bool {
    ["/api/", "/uploads/", "/shared/", "/plugin/open-api/"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || matches!(path, "/sitemap.xml" | "/robots.txt" | "/health")
}

fn content_type(path: &str) -> HeaderValue {
    let value = match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        "xml" => "application/xml; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    };
    HeaderValue::from_static(value)
}

fn empty(status: StatusCode) -> Response {
    Response::builder()
        .status(status)
        .body(Body::empty())
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_safe_and_api_paths_are_reserved() {
        assert_eq!(safe_path("assets/app.js"), Some("assets/app.js"));
        assert_eq!(safe_path("../config.yml"), None);
        assert!(reserved_backend_path("/api/v1/missing"));
        assert!(reserved_backend_path("/uploads/missing.png"));
        assert!(!reserved_backend_path("/products/example"));
    }

    #[test]
    fn generated_asset_tables_are_sorted_and_complete() {
        for assets in [STOREFRONT_ASSETS, ADMIN_ASSETS] {
            assert!(assets.iter().any(|asset| asset.path == "index.html"));
            assert!(assets.windows(2).all(|pair| pair[0].path < pair[1].path));
        }
    }
}
