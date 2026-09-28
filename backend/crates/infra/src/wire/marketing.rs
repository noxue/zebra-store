//! Wiring of the `marketing` group.
//!
//! Ports for other groups: the order group builds
//! `db::repo::marketing::coupon::SeaCouponRepo` as `CouponLedger`; the wallet group uses
//! `db::repo::marketing::gift_card::{lock_for_redeem, mark_redeemed}` inside its own
//! transaction to implement `GiftCardRedeemer`; order/wallet call
//! `MarketingServices.member_level.on_order_paid / on_recharge_completed`.

use std::sync::Arc;

use zs_app::marketing::MarketingServices;
use zs_app::marketing::coupon::CouponService;
use zs_app::marketing::gift_card::GiftCardService;
use zs_app::marketing::member_level::MemberLevelService;
use zs_app::marketing::promotion::PromotionService;

use super::WireCtx;
use crate::db::repo::marketing::coupon::SeaCouponRepo;
use crate::db::repo::marketing::gift_card::SeaGiftCardRepo;
use crate::db::repo::marketing::member_level::SeaMemberLevelRepo;
use crate::db::repo::marketing::promotion::SeaPromotionRepo;
use crate::queue::JobRegistry;

/// Builds the `marketing` services.
pub fn build(ctx: &WireCtx) -> MarketingServices {
    let levels = Arc::new(SeaMemberLevelRepo::new(ctx.db.clone()));
    MarketingServices {
        coupon: CouponService::new(
            Arc::new(SeaCouponRepo::new(ctx.db.clone())),
            ctx.clock.clone(),
        ),
        promotion: PromotionService::new(
            Arc::new(SeaPromotionRepo::new(ctx.db.clone())),
            ctx.clock.clone(),
        ),
        member_level: MemberLevelService::new(levels.clone(), levels, ctx.clock.clone()),
        gift_card: GiftCardService::new(
            Arc::new(SeaGiftCardRepo::new(ctx.db.clone())),
            ctx.settings.clone(),
            ctx.clock.clone(),
        ),
    }
}

/// Registers the `marketing` job handlers and periodic jobs.
pub fn jobs(_ctx: &WireCtx, _services: &zs_app::Services, _registry: &mut JobRegistry) {}
