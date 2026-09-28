//! `marketing` domain: coupons, promotions, member levels and gift cards.
//!
//! The pure pricing rules here (promotion price, coupon eligibility/discount, member
//! price, wholesale tiers in `catalog::wholesale`) are shared by the storefront display
//! and the order pricing engine so both always compute the same numbers.

pub mod coupon;
pub mod gift_card;
pub mod member_level;
pub mod promotion;
