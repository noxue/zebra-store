//! Catalog use cases.

pub mod card_secret;
pub mod category;
pub mod product;

/// Services of the catalog group.
#[derive(Debug, Clone)]
pub struct CatalogServices {
    pub category: category::CategoryService,
    pub product: product::ProductService,
    pub card_secret: card_secret::CardSecretService,
}
