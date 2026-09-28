//! `payment` endpoints: admin channels and payment records (compliance gated), gateway
//! callbacks/webhooks (default and admin-configured custom paths).

mod admin;
mod callback;
mod storefront;

use super::RouteSet;

/// Routes of the `payment` group.
pub fn routes() -> RouteSet {
    RouteSet {
        admin: admin::routes(),
        user_open: callback::routes(),
        root: callback::custom_routes(),
        user: storefront::user_routes(),
        guest: storefront::guest_routes(),
        ..RouteSet::default()
    }
}
