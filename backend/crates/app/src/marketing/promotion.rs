//! Promotion use cases.

use std::sync::Arc;

use zs_domain::marketing::promotion::{
    Promotion, PromotionFilter, PromotionInput, PromotionRepo, apply_promotion, keys,
    validate_promotion,
};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

/// Promotion service.
#[derive(Clone)]
pub struct PromotionService {
    repo: Arc<dyn PromotionRepo>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for PromotionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PromotionService")
    }
}

impl PromotionService {
    pub fn new(repo: Arc<dyn PromotionRepo>, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
    }

    pub async fn list(
        &self,
        filter: PromotionFilter,
        page: PageRequest,
    ) -> Result<Page<Promotion>> {
        self.repo
            .list(&filter, page)
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))
    }

    pub async fn create(&self, input: PromotionInput) -> Result<Promotion> {
        let fields = validate_promotion(&input, None)?;
        self.repo
            .create(&fields, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::CREATE_FAILED))
    }

    pub async fn update(&self, id: Id, input: PromotionInput) -> Result<Promotion> {
        if id <= 0 {
            return Err(Error::bad_request(keys::INVALID));
        }
        let existing = self
            .repo
            .get(id)
            .await?
            .ok_or_else(|| Error::not_found(keys::NOT_FOUND))?;
        let fields = validate_promotion(&input, Some(existing.is_active))?;
        self.repo
            .update(id, &fields, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::UPDATE_FAILED))?;
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| Error::not_found(keys::NOT_FOUND))
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        if id <= 0 {
            return Err(Error::bad_request(keys::INVALID));
        }
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| Error::not_found(keys::NOT_FOUND))?;
        self.repo
            .delete(id, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::DELETE_FAILED))
    }

    /// Effective promotions of a product (`min_amount ASC`), for display.
    pub async fn effective(&self, product_id: Id) -> Result<Vec<Promotion>> {
        self.repo.list_effective(product_id, self.clock.now()).await
    }

    /// Promotion price of `quantity` units at `unit_price` (the pricing engine passes the SKU price).
    pub async fn apply(
        &self,
        product_id: Id,
        unit_price: Amount,
        quantity: i32,
    ) -> Result<Option<(Promotion, Amount)>> {
        let promotions = self.effective(product_id).await?;
        apply_promotion(&promotions, unit_price, quantity)
    }
}
