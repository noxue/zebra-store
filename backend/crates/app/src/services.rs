//! The container of every use-case service, built once in `zs-server`.
//!
//! Each business group owns its `*Services` struct in `crate::<group>`; this
//! file only aggregates them.

/// All application services, grouped by business module.
#[derive(Debug, Clone)]
pub struct Services {
    pub identity: crate::identity::IdentityServices,
    pub catalog: crate::catalog::CatalogServices,
    pub content: crate::content::ContentServices,
    pub marketing: crate::marketing::MarketingServices,
    pub order: crate::order::OrderServices,
    pub payment: crate::payment::PaymentServices,
    pub wallet: crate::wallet::WalletServices,
    pub affiliate: crate::affiliate::AffiliateServices,
    pub reseller: crate::reseller::ResellerServices,
    pub integration: crate::integration::IntegrationServices,
    /// Provider-compat facades of the integration group (acg-faka, mcy OpenApi),
    /// wired by `zs_infra::wire::provide`.
    pub provide: crate::integration::provide::ProvideServices,
    pub notify: crate::notify::NotifyServices,
    pub dashboard: crate::dashboard::DashboardServices,
}
