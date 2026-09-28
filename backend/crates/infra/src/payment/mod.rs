//! Payment gateway adapters (one module per provider) and their shared helpers.

pub mod alipay;
pub mod bepusdt;
pub mod common;
pub mod dujiaopay;
pub mod epay;
pub mod epusdt;
pub mod http;
pub mod okpay;
pub mod paypal;
pub mod raw_json;
pub mod stripe;
pub mod tokenpay;
pub mod wechatpay;

#[cfg(test)]
pub(crate) mod test_support;

use std::sync::Arc;

use zs_domain::payment::GatewayRegistry;
use zs_domain::payment::types::{channel_type, provider};

pub use common::GatewayEnv;

/// Registers every gateway like the original `bootstrap.go`: official gateways per channel
/// type, aggregators provider-wide.
pub fn build_registry(env: &GatewayEnv) -> GatewayRegistry {
    let mut r = GatewayRegistry::new();
    r.register(
        provider::OFFICIAL,
        channel_type::STRIPE,
        Arc::new(stripe::StripeGateway::new(env.clone())),
    );
    r.register(
        provider::OFFICIAL,
        channel_type::PAYPAL,
        Arc::new(paypal::PaypalGateway::new(env.clone())),
    );
    r.register(
        provider::OFFICIAL,
        channel_type::WECHAT,
        Arc::new(wechatpay::WechatpayGateway::new(env.clone())),
    );
    r.register(
        provider::OFFICIAL,
        channel_type::ALIPAY,
        Arc::new(alipay::AlipayGateway::new(env.clone())),
    );
    r.register(
        provider::EPAY,
        "",
        Arc::new(epay::EpayGateway::new(env.clone())),
    );
    r.register(
        provider::EPUSDT,
        "",
        Arc::new(epusdt::EpusdtGateway::new(env.clone())),
    );
    r.register(
        provider::BEPUSDT,
        "",
        Arc::new(bepusdt::BepusdtGateway::new(env.clone())),
    );
    r.register(
        provider::DUJIAOPAY,
        "",
        Arc::new(dujiaopay::DujiaoPayGateway::new(env.clone())),
    );
    r.register(
        provider::TOKENPAY,
        "",
        Arc::new(tokenpay::TokenpayGateway::new(env.clone())),
    );
    r.register(
        provider::OKPAY,
        "",
        Arc::new(okpay::OkpayGateway::new(env.clone())),
    );
    r
}
