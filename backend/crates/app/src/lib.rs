//! Use-case services of Zebra Store.
//!
//! Services depend only on port traits from `zs-domain`; `zs-server` wires the
//! concrete adapters from `zs-infra`.

pub mod affiliate;
pub mod catalog;
pub mod config;
pub mod content;
pub mod dashboard;
pub mod identity;
pub mod integration;
pub mod marketing;
pub mod notify;
pub mod order;
pub mod payment;
pub mod reseller;
pub mod services;
pub mod wallet;

pub use services::Services;
