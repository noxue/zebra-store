//! Wiring of the `content` group.

use std::sync::Arc;

use zs_app::content::ContentServices;
use zs_app::content::banner::BannerService;
use zs_app::content::media::{MediaService, UploadPolicy, UploadService};
use zs_app::content::post::{PostCategoryService, PostService};
use zs_app::content::public_config::PublicConfigService;
use zs_app::content::settings::SettingsService;
use zs_app::content::sitemap::SitemapService;
use zs_domain::content::media::FileStore;
use zs_domain::content::post::PostRepo;

use super::WireCtx;
use crate::content::files::LocalFileStore;
use crate::content::smtp::LettreSmtpSender;
use crate::db::repo::content::banner::{SeaBannerRepo, SeaMediaRepo};
use crate::db::repo::content::post::{SeaPostCategoryRepo, SeaPostRepo};
use crate::db::repo::content::readers::{SeaPaymentChannelReader, SeaSitemapCatalogReader};
use crate::queue::JobRegistry;

/// Builds the `content` services.
pub fn build(ctx: &WireCtx) -> ContentServices {
    let db = &ctx.db;
    let settings = SettingsService::new(
        ctx.settings.clone(),
        &ctx.cfg,
        Arc::new(LettreSmtpSender),
        ctx.clock.clone(),
    );
    let posts: Arc<dyn PostRepo> = Arc::new(SeaPostRepo::new(db.clone()));
    let categories = Arc::new(SeaPostCategoryRepo::new(db.clone()));
    let files: Arc<dyn FileStore> = Arc::new(LocalFileStore::new(&ctx.cfg.upload.dir));
    let upload = &ctx.cfg.upload;
    ContentServices {
        public_config: PublicConfigService::new(
            settings.clone(),
            Arc::new(SeaPaymentChannelReader::new(db.clone())),
            ctx.clock.clone(),
        ),
        sitemap: SitemapService::new(
            settings.clone(),
            Arc::new(SeaSitemapCatalogReader::new(db.clone())),
            posts.clone(),
            ctx.clock.clone(),
        ),
        posts: PostService::new(posts, categories.clone(), ctx.clock.clone()),
        post_categories: PostCategoryService::new(categories),
        banners: BannerService::new(Arc::new(SeaBannerRepo::new(db.clone())), ctx.clock.clone()),
        media: MediaService::new(Arc::new(SeaMediaRepo::new(db.clone())), files.clone()),
        upload: UploadService::new(
            UploadPolicy {
                max_size: upload.max_size,
                allowed_types: upload.allowed_types.clone(),
                allowed_extensions: upload.allowed_extensions.clone(),
                max_width: upload.max_width,
                max_height: upload.max_height,
            },
            files,
            ctx.clock.clone(),
        ),
        settings,
    }
}

/// Registers the `content` job handlers and periodic jobs.
pub fn jobs(_ctx: &WireCtx, _services: &zs_app::Services, _registry: &mut JobRegistry) {}
