//! `marketing` use cases: coupons, promotions, member levels, gift cards.

pub mod coupon;
pub mod gift_card;
pub mod member_level;
pub mod promotion;

/// Services of the `marketing` group.
#[derive(Debug, Clone)]
pub struct MarketingServices {
    pub coupon: coupon::CouponService,
    pub promotion: promotion::PromotionService,
    pub member_level: member_level::MemberLevelService,
    pub gift_card: gift_card::GiftCardService,
}
