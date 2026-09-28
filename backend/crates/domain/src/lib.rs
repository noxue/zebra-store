//! Domain models, business rules and ports of Zebra Store.
//!
//! Each business module lives in its own directory and exposes:
//! - models (serializable in the original API's JSON shape),
//! - pure business rules,
//! - port traits implemented by `zs-infra`.

pub mod affiliate;
pub mod authz;
pub mod catalog;
pub mod content;
pub mod dashboard;
pub mod error;
pub mod identity;
pub mod integration;
pub mod marketing;
pub mod notify;
pub mod order;
pub mod payment;
pub mod queue;
pub mod reseller;
pub mod settings;
pub mod wallet;

pub use error::{Error, ErrorKind, Result};

/// Primary key type of every table.
pub type Id = i64;
