//! Content routes: settings, `/public/config`, uploads/media, posts, post
//! categories, banners, sitemap/robots and the safe `/uploads` file service.

mod admin;
mod public;
mod seo;
mod settings;
pub mod uploads;

use serde::Deserialize;
use zs_shared::page::PageRequest;

use super::RouteSet;

/// Routes of the `content` group.
pub fn routes() -> RouteSet {
    RouteSet {
        public: public::routes(),
        admin: settings::routes().merge(admin::routes()),
        root: seo::routes(),
        ..RouteSet::default()
    }
}

/// Lenient `page` / `page_size` query (non-numeric values fall back to defaults like `Atoi`).
#[derive(Debug, Default, Deserialize)]
struct PageQuery {
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    page_size: Option<String>,
}

impl PageQuery {
    fn request(&self) -> PageRequest {
        let parse = |v: &Option<String>| v.as_deref().and_then(|s| s.trim().parse::<u64>().ok());
        PageRequest::new(parse(&self.page), parse(&self.page_size))
    }
}
