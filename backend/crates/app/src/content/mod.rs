//! `content` use cases: settings, public config, uploads/media, posts, post
//! categories, banners and the sitemap.

pub mod banner;
pub mod media;
pub mod post;
pub mod public_config;
pub mod settings;
pub mod sitemap;
pub mod sniff;

/// Services of the `content` group.
#[derive(Debug, Clone)]
pub struct ContentServices {
    pub settings: settings::SettingsService,
    pub public_config: public_config::PublicConfigService,
    pub posts: post::PostService,
    pub post_categories: post::PostCategoryService,
    pub banners: banner::BannerService,
    pub media: media::MediaService,
    pub upload: media::UploadService,
    pub sitemap: sitemap::SitemapService,
}
