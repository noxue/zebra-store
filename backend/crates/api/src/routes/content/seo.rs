//! `GET /sitemap.xml` and `GET /robots.txt` (site root).

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::public::request_host;
use crate::middleware::tenant::Tenant;
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn routes() -> Routes {
    Routes::new("")
        .get("/sitemap.xml", sitemap)
        .get("/robots.txt", robots)
}

/// SET-03: the configured `brand.site_url` wins; forwarded headers are never trusted.
async fn sitemap(State(s): State<AppState>, headers: HeaderMap) -> Response {
    let svc = &s.svc.content.sitemap;
    let configured = svc.configured_base_url().await;
    let generated = match reseller_site(&s, &headers).await {
        Err(response) => return *response,
        // LQA-R1: a reseller site lists its own URLs, without its unlisted products.
        Ok(Some(tenant)) => {
            let scheme = if configured.starts_with("http://") {
                "http"
            } else {
                "https"
            };
            let base = format!("{scheme}://{}", tenant.snapshot_domain());
            let hidden = match tenant.reseller_id {
                Some(id) => s.svc.catalog.product.hidden_on(id).await,
                None => Ok(Vec::new()),
            };
            match hidden {
                Ok(hidden) => svc.site_sitemap(&base, &hidden).await,
                Err(e) => Err(e),
            }
        }
        Ok(None) => {
            let (base, cacheable) = if configured.is_empty() {
                (format!("http://{}", request_host(&headers)), false)
            } else {
                (configured, true)
            };
            svc.sitemap(&base, cacheable).await
        }
    };
    match generated {
        Ok(xml) => (
            [
                (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
                (header::CACHE_CONTROL, "public, max-age=300"),
            ],
            xml,
        )
            .into_response(),
        Err(error) => {
            tracing::error!(%error, "sitemap_generate_failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

/// The reseller site of the request (`None` = main shop); an unavailable site is 404.
async fn reseller_site(s: &AppState, headers: &HeaderMap) -> Result<Option<Tenant>, Box<Response>> {
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    let forwarded = headers
        .get("x-forwarded-host")
        .and_then(|v| v.to_str().ok());
    let resolver = &s.svc.reseller.tenant;
    match resolver
        .resolve(&resolver.request_host(host, forwarded))
        .await
    {
        Ok(t) if t.unavailable => Err(Box::new(StatusCode::NOT_FOUND.into_response())),
        Ok(t) if t.is_reseller() => Ok(Some(t)),
        Ok(_) => Ok(None),
        Err(error) => {
            tracing::error!(%error, "sitemap tenant resolution failed");
            Err(Box::new(
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response(),
            ))
        }
    }
}

async fn robots(State(s): State<AppState>) -> Response {
    let svc = &s.svc.content.sitemap;
    let base = svc.configured_base_url().await;
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        svc.robots(&base),
    )
        .into_response()
}
