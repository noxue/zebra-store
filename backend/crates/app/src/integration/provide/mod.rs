//! Provider side of site-to-site trading: one protocol-neutral core and one thin
//! facade per wire protocol we serve to other shop systems.
//!
//! ```text
//!   acg-faka /shared/*  ──► acg_faka::AcgFaka ─┐
//!   mcy /plugin/open-api ─► mcy::McyOpenApi ───┼─► access::CompatAccess (app_id + app_key)
//!   (future protocol)  ──► <facade>          ─┘   desk::SupplyDesk (catalog, pricing, orders)
//! ```
//!
//! - [`desk`]: the core ([`desk::SupplyDesk`]), also used by the legacy
//!   (`SupplierService`) and zebra-store (`ZsSupplier`) protocols for the catalog
//!   projection and site identity.
//! - [`access`]: compat keys (`app_id` = user id, separate `app_key`) and caller
//!   authentication for the PHP-born protocols.
//! - [`form`]: PHP `$_POST` parsing + md5 signing shared by those protocols.
//! - [`acg_faka`], [`mcy`]: the facades (signature check + request / response mapping).
//!
//! Adding a protocol: see `docs/BACKEND_GUIDE.md` §10.

pub mod access;
pub mod acg_faka;
pub mod desk;
pub mod form;
pub mod mcy;

use std::collections::HashSet;

use crate::identity::rate_limit::RateLimiter;
use desk::{Offer, OfferSku};

/// Services of the provider-compat facades.
#[derive(Debug, Clone)]
pub struct ProvideServices {
    pub desk: desk::SupplyDesk,
    pub access: access::CompatAccess,
    pub acg: acg_faka::AcgFaka,
    pub mcy: mcy::McyOpenApi,
}

impl ProvideServices {
    /// Wires the facades on one desk, one access service and one limiter (keys are
    /// prefixed by protocol).
    pub fn new(desk: desk::SupplyDesk, access: access::CompatAccess, limiter: RateLimiter) -> Self {
        Self {
            acg: acg_faka::AcgFaka::new(desk.clone(), access.clone(), limiter.clone()),
            mcy: mcy::McyOpenApi::new(desk.clone(), access.clone(), limiter),
            desk,
            access,
        }
    }
}

/// Unique, protocol-safe labels of an offer's SKUs (acg-faka `race` names / mcy SKU
/// names are INI keys on the acg-faka side: no `=`, `.`, `[`, `]`, line breaks), in
/// SKU order. Duplicates get `#<sku id>` appended.
pub fn sku_labels(offer: &Offer) -> Vec<(String, &OfferSku)> {
    let mut used = HashSet::new();
    offer
        .skus
        .iter()
        .map(|s| {
            let clean: String = s
                .name
                .chars()
                .map(|c| match c {
                    '=' | '.' | '[' | ']' | ';' | '"' => '_',
                    c if c.is_control() => ' ',
                    c => c,
                })
                .collect();
            let mut label = clean.trim().to_owned();
            if label.is_empty() {
                label = format!("SKU{}", s.sku_id);
            }
            if !used.insert(label.clone()) {
                label = format!("{label}#{}", s.sku_id);
                used.insert(label.clone());
            }
            (label, s)
        })
        .collect()
}

/// An amount as a JSON number (the PHP systems use floats for money).
pub fn money_number(amount: zs_shared::money::Amount) -> serde_json::Value {
    amount
        .to_string()
        .parse::<f64>()
        .ok()
        .and_then(serde_json::Number::from_f64)
        .map_or(serde_json::Value::from(0), serde_json::Value::Number)
}
