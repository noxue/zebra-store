//! `payment` domain: channels, payments, fee and eligibility rules, the gateway port,
//! callback fact validation and the settlement port.

pub mod alert;
pub mod callback;
pub mod channel;
pub mod eligibility;
pub mod errors;
pub mod fee;
pub mod form;
pub mod gateway;
pub mod model;
pub mod returns;
pub mod routes;
pub mod settlement;
pub mod types;
pub mod wallet_info;

pub use channel::{ChannelDraft, ChannelFilter, ChannelRepo, PaymentChannel};
pub use gateway::{GatewayError, GatewayRegistry, PaymentGateway};
pub use model::{Payment, PaymentRepo};
pub use settlement::PaymentSettlement;
pub use types::{FeePolicy, InteractionMode, PaymentStatus};
