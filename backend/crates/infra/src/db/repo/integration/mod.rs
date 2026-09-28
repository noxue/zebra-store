//! Integration repositories: credentials, connections, mappings (+ catalog writes of
//! import / sync), procurement, downstream refs, reconciliation and read projections.

pub mod connection;
pub mod credential;
pub mod downstream;
pub mod mapping;
pub mod orders;
pub mod procurement;
pub mod provide;
pub mod reconciliation;
pub mod supplier;
pub mod zs;
