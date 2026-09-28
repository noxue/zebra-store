//! Coupon use cases: admin CRUD and coupon evaluation for the pricing engine.

use std::sync::Arc;

use zs_domain::marketing::coupon::{
    Coupon, CouponBuyer, CouponFilter, CouponInput, CouponRepo, Eligibility, EligibilityItem,
    evaluate_coupon, keys, validate_coupon,
};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

/// Coupon service.
#[derive(Clone)]
pub struct CouponService {
    repo: Arc<dyn CouponRepo>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for CouponService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CouponService")
    }
}

/// Buyer of an order being priced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Buyer {
    pub user_id: Id,
    pub is_guest: bool,
    pub member_level_id: Id,
}

/// A successful coupon evaluation.
#[derive(Debug, Clone)]
pub struct AppliedCoupon {
    pub coupon: Coupon,
    pub discount: Amount,
    pub eligible: Eligibility,
}

impl CouponService {
    pub fn new(repo: Arc<dyn CouponRepo>, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
    }

    pub async fn list(&self, filter: CouponFilter, page: PageRequest) -> Result<Page<Coupon>> {
        self.repo
            .list(&filter, page)
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))
    }

    pub async fn create(&self, input: CouponInput) -> Result<Coupon> {
        let fields = validate_coupon(&input, None)?;
        if self.repo.get_by_code(&fields.code).await?.is_some() {
            return Err(Error::bad_request(keys::INVALID));
        }
        self.repo
            .create(&fields, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::CREATE_FAILED))
    }

    pub async fn update(&self, id: Id, input: CouponInput) -> Result<Coupon> {
        if id <= 0 {
            return Err(Error::bad_request(keys::INVALID));
        }
        let existing = self
            .repo
            .get(id)
            .await?
            .ok_or_else(|| Error::not_found(keys::NOT_FOUND))?;
        let fields = validate_coupon(&input, Some(&existing))?;
        if fields.code != existing.code && self.repo.get_by_code(&fields.code).await?.is_some() {
            return Err(Error::bad_request(keys::INVALID));
        }
        let now = self.clock.now();
        self.repo
            .update(id, &fields, now)
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

    /// Evaluates `code` for an order (pre-transaction check; the order transaction must
    /// still claim the usage through `CouponLedger`, PRC-01).
    pub async fn apply(
        &self,
        code: &str,
        buyer: Buyer,
        items: &[EligibilityItem],
    ) -> Result<AppliedCoupon> {
        let code = code.trim();
        if code.is_empty() {
            return Err(Error::bad_request(keys::INVALID));
        }
        let coupon = self
            .repo
            .get_by_code(code)
            .await?
            .ok_or_else(|| Error::not_found(keys::NOT_FOUND))?;
        let used_by_user = if coupon.per_user_limit > 0 && buyer.user_id != 0 {
            self.repo
                .count_user_usages(coupon.id, buyer.user_id)
                .await?
        } else {
            0
        };
        let (discount, eligible) = evaluate_coupon(
            &coupon,
            &CouponBuyer {
                user_id: buyer.user_id,
                is_guest: buyer.is_guest,
                member_level_id: buyer.member_level_id,
                used_by_user,
            },
            items,
            self.clock.now(),
        )?;
        Ok(AppliedCoupon {
            coupon,
            discount,
            eligible,
        })
    }
}
