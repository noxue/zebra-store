//! Wiring of the catalog group.
//!
//! Ports for other groups: the order group builds
//! `db::repo::catalog::ordering::SeaCatalogOrdering` (stock / card-secret movements).

use std::sync::Arc;

use zs_app::catalog::CatalogServices;
use zs_app::catalog::card_secret::CardSecretService;
use zs_app::catalog::category::CategoryService;
use zs_app::catalog::facets::{FacetBrowseService, FacetService};
use zs_app::catalog::product::{ProductDeps, ProductService};
use zs_app::reseller::PricingReads;

use super::WireCtx;
use crate::db::repo::catalog::card_secret::SeaCardSecretRepo;
use crate::db::repo::catalog::category::SeaCategoryRepo;
use crate::db::repo::catalog::facets::SeaFacetRepo;
use crate::db::repo::catalog::lookup::SeaCatalogLookup;
use crate::db::repo::catalog::product::SeaProductRepo;
use crate::db::repo::marketing::member_level::SeaMemberLevelRepo;
use crate::db::repo::marketing::promotion::SeaPromotionRepo;
use crate::db::repo::reseller::SeaResellerStore;
use crate::queue::JobRegistry;

/// Builds the catalog services.
pub fn build(ctx: &WireCtx) -> CatalogServices {
    let categories = Arc::new(SeaCategoryRepo::new(ctx.db.clone()));
    let products = Arc::new(SeaProductRepo::new(ctx.db.clone()));
    let secrets = Arc::new(SeaCardSecretRepo::new(ctx.db.clone()));
    let reseller = Arc::new(SeaResellerStore::new(ctx.db.clone()));
    let product = ProductService::new(ProductDeps {
        products: products.clone(),
        categories: categories.clone(),
        secrets: secrets.clone(),
        lookup: Arc::new(SeaCatalogLookup::new(ctx.db.clone())),
        promotions: Arc::new(SeaPromotionRepo::new(ctx.db.clone())),
        levels: Arc::new(SeaMemberLevelRepo::new(ctx.db.clone())),
        settings: ctx.settings.clone(),
        clock: ctx.clock.clone(),
        reseller: PricingReads {
            profiles: reseller.clone(),
            repo: reseller,
        },
    });
    let facets = Arc::new(SeaFacetRepo::new(ctx.db.clone()));
    CatalogServices {
        facets: FacetService::new(facets.clone()),
        browse: FacetBrowseService::new(facets, product.clone()),
        category: CategoryService::new(categories),
        product,

        card_secret: CardSecretService::new(secrets, products, ctx.clock.clone()),
    }
}

/// Registers the catalog job handlers.
pub fn jobs(_ctx: &WireCtx, _services: &zs_app::Services, _registry: &mut JobRegistry) {}
