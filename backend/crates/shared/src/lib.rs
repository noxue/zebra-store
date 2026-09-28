//! Framework-free primitives shared by every Zebra Store crate.
//!
//! Nothing in here may depend on a web framework, ORM or async runtime.

pub mod clock;
pub mod crypto;
pub mod i18n;
pub mod money;
pub mod page;
pub mod serial;
pub mod sign;
pub mod zs;
