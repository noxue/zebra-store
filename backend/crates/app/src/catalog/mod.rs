//! Catalog use cases.

pub mod card_secret;
pub mod category;
pub mod facets;
pub mod product;

/// Services of the catalog group.
#[derive(Debug, Clone)]
pub struct CatalogServices {
    pub facets: facets::FacetService,
    pub browse: facets::FacetBrowseService,
    pub category: category::CategoryService,
    pub product: product::ProductService,
    pub card_secret: card_secret::CardSecretService,
}
