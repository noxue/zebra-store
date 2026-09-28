//! `order` endpoints: cart, checkout, storefront orders (user and guest), admin orders,
//! refunds and manual delivery, and the channel (Telegram bot) order / payment API.

mod admin;
mod channel;
mod channel_account;
pub mod common;
mod guest;
mod user;

use super::RouteSet;

/// Routes of the `order` group.
pub fn routes() -> RouteSet {
    RouteSet {
        user: user::routes(),
        guest: guest::routes(),
        admin: admin::routes(),
        channel: channel::routes().merge(channel_account::routes()),
        ..RouteSet::default()
    }
}
