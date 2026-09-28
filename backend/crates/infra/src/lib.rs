//! Adapters (database, gateways, mail, cache, queue) of Zebra Store.

pub mod content;
pub mod db;
pub mod identity;
pub mod integration;
pub mod notify;
pub mod order;
pub mod payment;
pub mod queue;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
pub mod wire;
