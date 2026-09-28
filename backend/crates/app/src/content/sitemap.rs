//! `sitemap.xml` / `robots.txt` generation (port of `modules/sitemap`).
//!
//! SET-03: the base URL comes from `brand.site_url`; only that configured value
//! is cached, so request headers can never poison or bloat the cache.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use zs_domain::content::post::{Post, PostRepo};
use zs_domain::content::public::SitemapCatalogReader;
use zs_domain::{Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::PageRequest;

use zs_domain::content::post::{PostOrder, PostQuery};

use super::settings::SettingsService;

/// Cache lifetime (original: 5 minutes).
const CACHE_TTL: Duration = Duration::from_secs(300);
/// Upper bound of rows fetched per kind.
const MAX_FETCH: u64 = 50_000;

const STATIC_PAGES: [(&str, &str, &str); 7] = [
    ("/", "daily", "1.0"),
    ("/products", "daily", "0.9"),
    ("/blog", "weekly", "0.6"),
    ("/notice", "weekly", "0.5"),
    ("/about", "monthly", "0.3"),
    ("/terms", "yearly", "0.2"),
    ("/privacy", "yearly", "0.2"),
];

const ROBOTS_DISALLOW: [&str; 10] = [
    "/api/",
    "/admin/",
    "/me/",
    "/cart",
    "/checkout",
    "/pay",
    "/orders/",
    "/recharge-orders/",
    "/guest/",
    "/auth/",
];

/// Generates SEO resources.
#[derive(Clone)]
pub struct SitemapService {
    settings: SettingsService,
    catalog: Arc<dyn SitemapCatalogReader>,
    posts: Arc<dyn PostRepo>,
    clock: Arc<dyn Clock>,
    cache: Arc<Mutex<Option<(Instant, String, String)>>>,
}

impl std::fmt::Debug for SitemapService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SitemapService")
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Percent-encodes a path segment (`url.PathEscape`).
fn path_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'_' | b'.' | b'~' | b'$' | b'&' | b'+' | b',' | b';' | b'=' | b':' | b'@'
            )
        {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl SitemapService {
    pub fn new(
        settings: SettingsService,
        catalog: Arc<dyn SitemapCatalogReader>,
        posts: Arc<dyn PostRepo>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            settings,
            catalog,
            posts,
            clock,
            cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Configured `brand.site_url` without trailing slash (empty when unset).
    pub async fn configured_base_url(&self) -> String {
        self.settings
            .site_brand()
            .await
            .map(|b| b.site_url)
            .unwrap_or_default()
    }

    /// `sitemap.xml` for `base_url`; cached only when `base_url` is the configured site URL.
    pub async fn sitemap(&self, base_url: &str, cacheable: bool) -> Result<String> {
        let base = base_url.trim().trim_end_matches('/').to_owned();
        if cacheable {
            let cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some((at, key, xml)) = cache.as_ref()
                && *key == base
                && at.elapsed() < CACHE_TTL
            {
                return Ok(xml.clone());
            }
        }
        let xml = self.render(&base, &[]).await?;
        if cacheable {
            *self.cache.lock().unwrap_or_else(PoisonError::into_inner) =
                Some((Instant::now(), base, xml.clone()));
        }
        Ok(xml)
    }

    /// `sitemap.xml` of a reseller site (LQA-R1): its own base URL, without the
    /// products the reseller unlisted. Never cached (one entry per site would let
    /// hosts bloat the cache).
    pub async fn site_sitemap(&self, base_url: &str, hidden_products: &[Id]) -> Result<String> {
        let base = base_url.trim().trim_end_matches('/').to_owned();
        self.render(&base, hidden_products).await
    }

    async fn render(&self, base: &str, hidden_products: &[Id]) -> Result<String> {
        let today = self.clock.now().format("%Y-%m-%d").to_string();
        let mut urls: Vec<(String, String, &str, &str)> = STATIC_PAGES
            .iter()
            .map(|(p, f, pr)| (format!("{base}{p}"), today.clone(), *f, *pr))
            .collect();
        for c in self.catalog.active_categories().await? {
            urls.push((
                format!("{base}/categories/{}", path_escape(&c.slug)),
                c.modified_at.format("%Y-%m-%d").to_string(),
                "weekly",
                "0.7",
            ));
        }
        for p in self.catalog.active_products(MAX_FETCH).await? {
            if hidden_products.contains(&p.id) {
                continue;
            }
            urls.push((
                format!("{base}/products/{}", path_escape(&p.slug)),
                p.modified_at.format("%Y-%m-%d").to_string(),
                "daily",
                "0.8",
            ));
        }
        let posts: Vec<Post> = self
            .posts
            .list(&PostQuery {
                page: PageRequest {
                    page: 1,
                    page_size: MAX_FETCH,
                },
                kind: String::new(),
                search: String::new(),
                only_published: true,
                order: PostOrder::PublishedDesc,
            })
            .await?
            .items;
        for post in posts {
            let modified = post.published_at.unwrap_or(post.created_at);
            // Blog posts and notices share the /blog/:slug detail page.
            urls.push((
                format!("{base}/blog/{}", path_escape(&post.slug)),
                modified.format("%Y-%m-%d").to_string(),
                "monthly",
                "0.5",
            ));
        }
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
        );
        for (loc, lastmod, freq, priority) in urls {
            xml.push_str(&format!(
                "  <url>\n    <loc>{}</loc>\n    <lastmod>{lastmod}</lastmod>\n    <changefreq>{freq}</changefreq>\n    <priority>{priority}</priority>\n  </url>\n",
                xml_escape(&loc)
            ));
        }
        xml.push_str("</urlset>\n");
        Ok(xml)
    }

    /// `robots.txt`; the `Sitemap:` line is only emitted for a known base URL.
    pub fn robots(&self, base_url: &str) -> String {
        let base = base_url.trim().trim_end_matches('/');
        let mut out = String::from("User-agent: *\n");
        for path in ROBOTS_DISALLOW {
            out.push_str(&format!("Disallow: {path}\n"));
        }
        if !base.is_empty() {
            out.push_str(&format!("\nSitemap: {base}/sitemap.xml\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes() {
        assert_eq!(path_escape("a b/c"), "a%20b%2Fc");
        assert_eq!(xml_escape("a&b"), "a&amp;b");
    }
}
