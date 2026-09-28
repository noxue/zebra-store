//! [`AffiliateHooks`] over the affiliate group's service.

use std::sync::Arc;

use async_trait::async_trait;
use zs_domain::order::ports::AffiliateHooks;
use zs_domain::{Id, Result};

use crate::affiliate::AffiliateService;

/// Order lifecycle → affiliate service (`ResolveOrderAffiliateSnapshot`,
/// `HandleOrderPaid`, `HandleOrderCanceled`).
#[derive(Clone)]
pub struct ServiceAffiliateHooks(pub Arc<AffiliateService>);

impl std::fmt::Debug for ServiceAffiliateHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ServiceAffiliateHooks")
    }
}

#[async_trait]
impl AffiliateHooks for ServiceAffiliateHooks {
    async fn resolve_snapshot(
        &self,
        user_id: Id,
        code: &str,
        visitor_key: &str,
    ) -> Result<(Option<Id>, String)> {
        Ok(
            match self
                .0
                .resolve_order_snapshot(user_id, code, visitor_key)
                .await?
            {
                Some((id, code)) => (Some(id), code),
                None => (None, String::new()),
            },
        )
    }

    async fn order_paid(&self, order_id: Id) -> Result<()> {
        self.0.handle_order_paid(order_id).await
    }

    async fn order_canceled(&self, order_id: Id, reason: &str) -> Result<()> {
        self.0.handle_order_canceled(order_id, reason).await
    }
}
