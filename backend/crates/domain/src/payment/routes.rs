//! Admin-configurable callback paths (`callback_routes_config`).

use serde_json::Value;

/// Default public callback paths.
pub const DEFAULT_PAYMENT_CALLBACK: &str = "/api/v1/payments/callback";
pub const DEFAULT_DUJIAOPAY_WEBHOOK: &str = "/api/v1/payments/webhook/dujiaopay";
pub const DEFAULT_PAYPAL_WEBHOOK: &str = "/api/v1/payments/webhook/paypal";
pub const DEFAULT_STRIPE_WEBHOOK: &str = "/api/v1/payments/webhook/stripe";

/// Prefixes a custom callback path may never shadow.
const RESERVED_PREFIXES: [&str; 7] = [
    "/api/v1/public/",
    "/api/v1/admin/",
    "/api/v1/auth/",
    "/api/v1/guest/",
    "/api/v1/channel/",
    "/api/v1/upstream/api/",
    "/api/v1/user/",
];

/// Which handler a callback path dispatches to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackKind {
    Shared,
    DujiaoPay,
    Paypal,
    Stripe,
}

/// Normalized custom callback routes; empty = default path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallbackRoutes {
    pub payment_callback: String,
    pub dujiaopay_webhook: String,
    pub paypal_webhook: String,
    pub stripe_webhook: String,
    pub upstream_callback: String,
}

/// `normalizeCallbackRoutePath`: strip query/fragment and trailing `/`, require `/api/`,
/// reject reserved prefixes.
pub fn normalize_route_path(path: &str) -> String {
    let path = path.trim();
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let path = path.trim_end_matches('/');
    if !path.starts_with("/api/") {
        return String::new();
    }
    let with_slash = format!("{path}/");
    if RESERVED_PREFIXES
        .iter()
        .any(|p| with_slash.starts_with(p) || p.starts_with(&with_slash))
    {
        return String::new();
    }
    path.to_owned()
}

impl CallbackRoutes {
    /// Decodes the settings value (`DecodeCallbackRoutesSetting`).
    pub fn decode(value: &Value) -> Self {
        let read = |key: &str| {
            normalize_route_path(value.get(key).and_then(Value::as_str).unwrap_or_default())
        };
        Self {
            payment_callback: read("payment_callback"),
            dujiaopay_webhook: read("dujiaopay_webhook"),
            paypal_webhook: read("paypal_webhook"),
            stripe_webhook: read("stripe_webhook"),
            upstream_callback: read("upstream_callback"),
        }
    }

    pub fn has_custom_routes(&self) -> bool {
        !(self.payment_callback.is_empty()
            && self.dujiaopay_webhook.is_empty()
            && self.paypal_webhook.is_empty()
            && self.stripe_webhook.is_empty()
            && self.upstream_callback.is_empty())
    }

    /// Handler for a custom path (trailing `/` ignored); GET is only allowed for the shared callback.
    pub fn match_custom(&self, path: &str, is_post: bool) -> Option<CallbackKind> {
        let path = path.trim_end_matches('/');
        if path.is_empty() {
            return None;
        }
        if path == self.payment_callback {
            return Some(CallbackKind::Shared);
        }
        if !is_post {
            return None;
        }
        [
            (&self.dujiaopay_webhook, CallbackKind::DujiaoPay),
            (&self.paypal_webhook, CallbackKind::Paypal),
            (&self.stripe_webhook, CallbackKind::Stripe),
        ]
        .into_iter()
        .find(|(p, _)| p.as_str() == path)
        .map(|(_, kind)| kind)
    }

    /// True when the default path of `kind` is hidden because a custom path is configured.
    pub fn default_hidden(&self, kind: CallbackKind) -> bool {
        match kind {
            CallbackKind::Shared => !self.payment_callback.is_empty(),
            CallbackKind::DujiaoPay => !self.dujiaopay_webhook.is_empty(),
            CallbackKind::Paypal => !self.paypal_webhook.is_empty(),
            CallbackKind::Stripe => !self.stripe_webhook.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_and_matches_custom_routes() {
        let routes = CallbackRoutes::decode(&json!({
            "payment_callback": "/api/pay/notify/?x=1",
            "stripe_webhook": "/api/v1/admin/hook",
            "paypal_webhook": "/hooks/paypal",
            "dujiaopay_webhook": "/api/djp"
        }));
        assert_eq!(routes.payment_callback, "/api/pay/notify");
        assert_eq!(routes.stripe_webhook, "");
        assert_eq!(routes.paypal_webhook, "");
        assert_eq!(
            routes.match_custom("/api/pay/notify/", false),
            Some(CallbackKind::Shared)
        );
        assert_eq!(routes.match_custom("/api/djp", false), None);
        assert_eq!(
            routes.match_custom("/api/djp", true),
            Some(CallbackKind::DujiaoPay)
        );
        assert!(routes.default_hidden(CallbackKind::Shared));
        assert!(!routes.default_hidden(CallbackKind::Stripe));
        assert!(!CallbackRoutes::default().has_custom_routes());
        assert_eq!(normalize_route_path("/api/v1"), "");
    }
}
