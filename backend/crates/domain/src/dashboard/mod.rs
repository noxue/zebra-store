//! `dashboard` domain: operational reports (port of `dashboard` + `reporting`),
//! the admin user directory (port of `identity/user` admin handlers) and
//! system information (version check, self-update capability, ad proxy).

pub mod inventory;
pub mod report;
pub mod stats;
pub mod system;
pub mod users;
