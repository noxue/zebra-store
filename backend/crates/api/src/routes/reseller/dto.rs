//! Response shapes of the reseller endpoints (`transport/http/presenter`).

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use zs_app::reseller::product_setting::SettingView;
use zs_domain::Id;
use zs_domain::reseller::orders::{ResellerOrderItem, ResellerOrderLine};
use zs_domain::reseller::ports::{OrderStats, SettingSummary};
use zs_domain::reseller::pricing::PreviewItem;
use zs_domain::reseller::site::footer_items;
use zs_domain::reseller::{
    BalanceAccount, LedgerEntry, ProductSetting, Profile, ResellerDomain, SiteConfig,
    WithdrawRequest,
};
use zs_shared::money::Amount;

/// Go's zero `time.Time` in JSON.
const ZERO_TIME: &str = "0001-01-01T00:00:00Z";

fn is_empty(s: &str) -> bool {
    s.is_empty()
}

#[derive(Debug, Serialize)]
pub struct ManagementProfileResp {
    pub id: Id,
    pub status: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub apply_reason: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reject_reason: String,
    pub default_markup_percent: Amount,
    pub max_markup_percent: Amount,
    pub settlement_status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Profile> for ManagementProfileResp {
    fn from(p: &Profile) -> Self {
        Self {
            id: p.id,
            status: p.status.as_str(),
            apply_reason: p.apply_reason.clone(),
            reject_reason: p.reject_reason.clone(),
            default_markup_percent: p.default_markup_percent,
            max_markup_percent: p.max_markup_percent,
            settlement_status: p.settlement_status.as_str(),
            reviewed_at: p.reviewed_at,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DomainResp {
    pub id: Id,
    pub domain: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub verification_token: String,
    pub verification_status: &'static str,
    pub status: &'static str,
    pub is_primary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&ResellerDomain> for DomainResp {
    fn from(d: &ResellerDomain) -> Self {
        Self {
            id: d.id,
            domain: d.domain.clone(),
            kind: d.kind.as_str(),
            verification_token: d.verification_token.clone(),
            verification_status: d.verification_status.as_str(),
            status: d.status.as_str(),
            is_primary: d.is_primary,
            verified_at: d.verified_at,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

pub fn domains(rows: &[ResellerDomain]) -> Vec<DomainResp> {
    rows.iter().map(DomainResp::from).collect()
}

/// `SiteConfigResp` (console).
pub fn site_config(c: &SiteConfig) -> Value {
    json!({
        "id": c.id,
        "site_name": c.site_name,
        "logo": c.logo,
        "favicon": c.favicon,
        "announcement": c.announcement,
        "support": c.support,
        "seo": c.seo,
        "footer_links": footer_items(&c.footer_links),
        "nav_config": c.nav_config,
        "updated_at": c.updated_at,
    })
}

fn profile_ref(p: &Profile) -> Value {
    let mut v = json!({
        "id": p.id,
        "user_id": p.user_id,
        "status": p.status,
        "settlement_status": p.settlement_status,
    });
    if let Some(u) = &p.user {
        let mut user = json!({"id": u.id});
        if !u.email.is_empty() {
            user["email"] = json!(u.email);
        }
        if !u.display_name.is_empty() {
            user["display_name"] = json!(u.display_name);
        }
        v["user"] = user;
    }
    v
}

/// `AdminResellerSiteConfigResp`; `config = None` renders the empty placeholder of
/// a reseller without saved configuration.
pub fn admin_site_config(
    reseller_id: Id,
    config: Option<&SiteConfig>,
    profile: Option<&Profile>,
) -> Value {
    let mut v = match config {
        Some(c) => json!({
            "id": c.id,
            "reseller_id": c.reseller_id,
            "site_name": c.site_name,
            "logo": c.logo,
            "favicon": c.favicon,
            "announcement": c.announcement,
            "support": c.support,
            "seo": c.seo,
            "footer_links": footer_items(&c.footer_links),
            "nav_config": c.nav_config,
            "created_at": c.created_at,
            "updated_at": c.updated_at,
        }),
        None => json!({
            "id": 0,
            "reseller_id": reseller_id,
            "site_name": "",
            "logo": "",
            "favicon": "",
            "announcement": null,
            "support": null,
            "seo": null,
            "footer_links": [],
            "nav_config": null,
            "created_at": ZERO_TIME,
            "updated_at": ZERO_TIME,
        }),
    };
    if let Some(p) = profile.or_else(|| config.and_then(|c| c.profile.as_ref())) {
        v["profile"] = profile_ref(p);
    }
    v
}

#[derive(Debug, Serialize)]
pub struct SettingResp {
    pub id: Id,
    pub product_id: Id,
    pub sku_id: Id,
    pub is_listed: bool,
    pub pricing_mode: &'static str,
    pub markup_percent: Amount,
    pub fixed_markup_amount: Amount,
    pub fixed_price_amount: Amount,
    #[serde(skip_serializing_if = "is_empty")]
    pub effective_price_amount: String,
    #[serde(skip_serializing_if = "is_empty")]
    pub rule_source: String,
    pub sort_order: i32,
    pub updated_at: DateTime<Utc>,
}

fn setting_resp(s: &ProductSetting, view: &SettingView) -> SettingResp {
    SettingResp {
        id: s.id,
        product_id: s.product_id,
        sku_id: s.sku_id,
        is_listed: s.is_listed,
        pricing_mode: s.pricing_mode.as_str(),
        markup_percent: s.markup_percent,
        fixed_markup_amount: s.fixed_markup_amount,
        fixed_price_amount: s.fixed_price_amount,
        effective_price_amount: effective(view, s.sku_id),
        rule_source: view
            .effective
            .sources
            .get(&s.sku_id)
            .map(|r| r.as_str().to_owned())
            .unwrap_or_default(),
        sort_order: s.sort_order,
        updated_at: s.updated_at,
    }
}

fn effective(view: &SettingView, key: Id) -> String {
    view.effective
        .prices
        .get(&key)
        .map(|p| Amount::new(*p).to_string())
        .unwrap_or_default()
}

/// `ResellerProductSettingDetailResp`.
pub fn setting_detail(view: &SettingView) -> Value {
    let p = &view.product;
    let find = |sku_id: Id| view.settings.iter().find(|s| s.sku_id == sku_id);
    let skus: Vec<Value> = p
        .skus
        .iter()
        .map(|sku| {
            let mut v = json!({
                "id": sku.id,
                "sku_code": sku.sku_code,
                "spec_values": sku.spec_values,
                "base_price_amount": sku.price,
                "is_active": sku.is_active,
            });
            if let Some(s) = find(sku.id) {
                v["setting"] = json!(setting_resp(s, view));
            }
            let price = effective(view, sku.id);
            if !price.is_empty() {
                v["effective_price_amount"] = json!(price);
            }
            v
        })
        .collect();
    let mut out = json!({
        "product": {
            "id": p.id,
            "slug": p.slug,
            "title": p.title,
            "price_amount": p.price,
            "is_active": p.is_active,
        },
        "skus": skus,
    });
    if let Some(s) = find(0) {
        out["product_setting"] = json!(setting_resp(s, view));
    }
    out
}

/// `ResellerProductSettingPreviewResp`.
pub fn preview(items: &[PreviewItem]) -> Value {
    let items: Vec<Value> = items
        .iter()
        .map(|i| {
            let mut v = json!({
                "sku_id": i.sku_id,
                "is_listed": i.is_listed,
                "base_price_amount": Amount::new(i.base_price),
                "effective_price_amount": Amount::new(i.effective_price),
                "valid": i.valid,
            });
            if !i.error_code.is_empty() {
                v["error_code"] = json!(i.error_code);
            }
            v
        })
        .collect();
    json!({ "items": items })
}

/// `AdminResellerProductSettingResp`.
pub fn admin_setting(s: &ProductSetting) -> Value {
    let mut v = json!({
        "id": s.id,
        "reseller_id": s.reseller_id,
        "product_id": s.product_id,
        "sku_id": s.sku_id,
        "is_listed": s.is_listed,
        "pricing_mode": s.pricing_mode,
        "markup_percent": s.markup_percent,
        "fixed_markup_amount": s.fixed_markup_amount,
        "fixed_price_amount": s.fixed_price_amount,
        "sort_order": s.sort_order,
        "created_at": s.created_at,
        "updated_at": s.updated_at,
    });
    if let Some(p) = &s.profile {
        let mut profile = json!({
            "id": p.id,
            "user_id": p.user_id,
            "status": p.status,
            "settlement_status": p.settlement_status,
        });
        if let Some(u) = &p.user {
            profile["user"] = json!({"id": u.id, "email": u.email, "display_name": u.display_name});
        }
        v["profile"] = profile;
    }
    if let Some(p) = &s.product {
        v["product"] = json!(p);
    }
    v
}

#[derive(Debug, Serialize)]
pub struct BalanceResp {
    pub id: Id,
    pub currency: String,
    pub status: &'static str,
    pub available_amount: Amount,
    pub locked_amount: Amount,
    pub negative_amount: Amount,
    pub updated_at: DateTime<Utc>,
}

impl From<&BalanceAccount> for BalanceResp {
    fn from(b: &BalanceAccount) -> Self {
        Self {
            id: b.id,
            currency: b.currency.clone(),
            status: b.status.as_str(),
            available_amount: b.available_amount_cache,
            locked_amount: b.locked_amount_cache,
            negative_amount: b.negative_amount_cache,
            updated_at: b.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LedgerResp {
    pub id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<Id>,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub amount: Amount,
    pub currency: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub withdraw_request_id: Option<Id>,
    pub created_at: DateTime<Utc>,
}

impl From<&LedgerEntry> for LedgerResp {
    fn from(e: &LedgerEntry) -> Self {
        Self {
            id: e.id,
            order_id: e.order_id,
            kind: e.kind.as_str(),
            amount: e.amount,
            currency: e.currency.clone(),
            status: e.status.as_str(),
            available_at: e.available_at,
            withdraw_request_id: e.withdraw_request_id,
            created_at: e.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WithdrawResp {
    pub id: Id,
    pub amount: Amount,
    pub currency: String,
    pub channel: String,
    pub account: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reject_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<&WithdrawRequest> for WithdrawResp {
    fn from(w: &WithdrawRequest) -> Self {
        Self {
            id: w.id,
            amount: w.amount,
            currency: w.currency.clone(),
            channel: w.channel.clone(),
            account: w.account.clone(),
            status: w.status.as_str(),
            reject_reason: w.reject_reason.clone(),
            processed_at: w.processed_at,
            created_at: w.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OrderResp {
    pub order_no: String,
    pub status: String,
    pub currency: String,
    pub total_amount: Amount,
    pub base_amount: Amount,
    pub profit_amount: Amount,
    pub profit_status: &'static str,
    pub domain: String,
    pub buyer_label: String,
    pub items_count: usize,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paid_at: Option<DateTime<Utc>>,
}

impl From<&ResellerOrderItem> for OrderResp {
    fn from(o: &ResellerOrderItem) -> Self {
        Self {
            order_no: o.order_no.clone(),
            status: o.status.clone(),
            currency: o.currency.clone(),
            total_amount: o.total_amount,
            base_amount: o.base_amount,
            profit_amount: o.profit_amount,
            profit_status: o.profit_status,
            domain: o.domain.clone(),
            buyer_label: o.buyer_label.clone(),
            items_count: o.items_count,
            created_at: o.created_at,
            paid_at: o.paid_at,
        }
    }
}

/// `ResellerOrderDetailResp`.
pub fn order_detail(item: &ResellerOrderItem, lines: &[ResellerOrderLine]) -> Value {
    let mut v = json!(OrderResp::from(item));
    let items: Vec<Value> = lines
        .iter()
        .map(|l| {
            let mut i = json!({
                "title": l.item.title,
                "sku_snapshot": l.item.sku_snapshot,
                "quantity": l.item.quantity,
                "unit_price": l.item.unit_price,
                "total_price": l.item.total_price,
            });
            for (key, value) in [
                ("base_unit_amount", &l.pricing.base_unit_amount),
                ("reseller_unit_amount", &l.pricing.reseller_unit_amount),
                ("base_total_amount", &l.pricing.base_total_amount),
                ("reseller_total_amount", &l.pricing.reseller_total_amount),
                ("profit_amount", &l.pricing.profit_amount),
            ] {
                if !value.is_empty() {
                    i[key] = json!(value);
                }
            }
            i
        })
        .collect();
    v["items"] = json!(items);
    v
}

/// `ResellerOrderStatsResp`.
pub fn order_stats(s: &OrderStats) -> Value {
    json!({"total": s.total, "by_status": s.by_status, "by_currency": s.by_currency})
}

/// `AdminResellerProductSummaryResp`.
pub fn summary(s: &SettingSummary) -> Value {
    json!({
        "configured_products": s.configured_products,
        "hidden_products": s.hidden_products,
        "sku_overrides": s.sku_overrides,
        "pricing_overrides": s.pricing_overrides,
    })
}
