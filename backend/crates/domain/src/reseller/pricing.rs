//! Reseller pricing: rule resolution (SKU → product → profile default → inherit),
//! floor / markup-cap validation, previews, storefront display prices (RSL-09) and
//! the order pricing context used by the order group (RSL-07).
//!
//! Every path (save, preview, display, order) uses [`resolve_unit_amount`] and
//! [`validate_unit_amount`] so the previewed price is exactly the charged price.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rust_decimal::{Decimal, RoundingStrategy};
use serde_json::{Value, json};
use zs_shared::money::Amount;

use super::keys;
use super::model::{PricingMode, ProductSetting, Profile};
use crate::{Error, Id, Result};

/// Where the effective rule came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleSource {
    Sku,
    Product,
    Profile,
    Inherit,
}

impl RuleSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sku => "sku",
            Self::Product => "product",
            Self::Profile => "profile",
            Self::Inherit => "inherit",
        }
    }
}

/// The rule that produced a unit price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PricingRule {
    pub mode: PricingMode,
    pub source: RuleSource,
    pub setting_id: Option<Id>,
}

fn round2(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn round4(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(4, RoundingStrategy::MidpointAwayFromZero)
}

/// `base * (100 + percent) / 100`, rounded to cents.
pub fn apply_markup_percent(base: Decimal, percent: Decimal) -> Decimal {
    round2(base * (Decimal::ONE_HUNDRED + percent) / Decimal::ONE_HUNDRED)
}

/// Unit price of one stored rule.
pub fn apply_rule(
    setting: &ProductSetting,
    source: RuleSource,
    base: Decimal,
) -> (Decimal, PricingRule) {
    let rule = PricingRule {
        mode: setting.pricing_mode,
        source,
        setting_id: Some(setting.id),
    };
    let price = match setting.pricing_mode {
        PricingMode::MarkupPercent => apply_markup_percent(base, setting.markup_percent.decimal()),
        PricingMode::FixedMarkup => round2(base + setting.fixed_markup_amount.decimal()),
        PricingMode::FixedPrice => round2(setting.fixed_price_amount.decimal()),
        PricingMode::Inherit => round2(base),
    };
    (price, rule)
}

/// Resolves the reseller unit price: SKU rule → product rule → profile default markup
/// → base price (`ResolveUnitAmount`). `inherit` rules fall through.
pub fn resolve_unit_amount(
    profile: &Profile,
    product_setting: Option<&ProductSetting>,
    sku_setting: Option<&ProductSetting>,
    base: Decimal,
) -> (Decimal, PricingRule) {
    if let Some(s) = sku_setting.filter(|s| s.pricing_mode != PricingMode::Inherit) {
        return apply_rule(s, RuleSource::Sku, base);
    }
    if let Some(s) = product_setting.filter(|s| s.pricing_mode != PricingMode::Inherit) {
        return apply_rule(s, RuleSource::Product, base);
    }
    let default_markup = profile.default_markup_percent.decimal();
    if default_markup > Decimal::ZERO {
        return (
            apply_markup_percent(base, default_markup),
            PricingRule {
                mode: PricingMode::MarkupPercent,
                source: RuleSource::Profile,
                setting_id: None,
            },
        );
    }
    (
        round2(base),
        PricingRule {
            mode: PricingMode::Inherit,
            source: RuleSource::Inherit,
            setting_id: None,
        },
    )
}

/// Price floor and markup cap (`ValidateUnitAmount`): the reseller price must be
/// positive, at least the base price and the cost price (when set), and the implied
/// markup must not exceed `max_markup_percent` (when set).
pub fn validate_unit_amount(
    profile: &Profile,
    cost: Decimal,
    base: Decimal,
    reseller: Decimal,
) -> Result<()> {
    let base = round2(base);
    let reseller = round2(reseller);
    if reseller <= Decimal::ZERO || reseller < base {
        return Err(Error::bad_request(keys::PRICE_INVALID));
    }
    if cost > Decimal::ZERO && reseller < round2(cost) {
        return Err(Error::bad_request(keys::PRICE_INVALID));
    }
    let max = profile.max_markup_percent.decimal();
    if max > Decimal::ZERO && base > Decimal::ZERO {
        let implied = round4((reseller - base) / base * Decimal::ONE_HUNDRED);
        if implied > round4(max) {
            return Err(Error::bad_request(keys::MARKUP_EXCEEDED));
        }
    }
    Ok(())
}

/// A SKU as seen by the pricing rules.
#[derive(Debug, Clone, PartialEq)]
pub struct PricedSku {
    pub id: Id,
    pub sku_code: String,
    pub spec_values: Value,
    pub price: Amount,
    pub cost_price: Amount,
    pub is_active: bool,
}

/// A product as seen by the pricing rules.
#[derive(Debug, Clone, PartialEq)]
pub struct PricedProduct {
    pub id: Id,
    pub slug: String,
    pub title: Value,
    pub price: Amount,
    pub cost_price: Amount,
    pub is_active: bool,
    pub skus: Vec<PricedSku>,
}

/// One rule submitted for save / preview.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingInput {
    pub sku_id: Id,
    pub is_listed: bool,
    pub pricing_mode: String,
    pub markup_percent: Decimal,
    pub fixed_markup_amount: Decimal,
    pub fixed_price_amount: Decimal,
    pub sort_order: i32,
}

fn parse_mode(raw: &str) -> Result<PricingMode> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(PricingMode::Inherit);
    }
    PricingMode::parse(raw).ok_or_else(|| Error::bad_request(keys::PRICE_INVALID))
}

/// Builds an unsaved rule from input (mode validated, amounts rounded).
pub fn draft_setting(input: &SettingInput) -> Result<ProductSetting> {
    let mut s = ProductSetting::draft(
        input.sku_id,
        input.is_listed,
        parse_mode(&input.pricing_mode)?,
    );
    s.markup_percent = Amount::new(input.markup_percent);
    s.fixed_markup_amount = Amount::new(input.fixed_markup_amount);
    s.fixed_price_amount = Amount::new(input.fixed_price_amount);
    s.sort_order = input.sort_order;
    Ok(s)
}

/// Validates a rule before saving (`normalizeProductSettingInput`). `product` must
/// carry all its non-deleted SKUs; hidden rules are accepted without price checks.
pub fn normalize_setting(
    profile: &Profile,
    product: &PricedProduct,
    input: &SettingInput,
) -> Result<ProductSetting> {
    let setting = draft_setting(input)?;
    if !setting.is_listed {
        return Ok(setting);
    }
    if input.sku_id > 0 {
        let sku = product
            .skus
            .iter()
            .find(|s| s.id == input.sku_id && s.is_active)
            .ok_or_else(|| Error::bad_request(keys::SKU_INVALID))?;
        let base = round2(sku.price.decimal());
        let (price, _) = resolve_unit_amount(profile, None, Some(&setting), base);
        validate_unit_amount(profile, sku.cost_price.decimal(), base, price)?;
        return Ok(setting);
    }
    if product.skus.is_empty() {
        let base = round2(product.price.decimal());
        let (price, _) = resolve_unit_amount(profile, Some(&setting), None, base);
        validate_unit_amount(profile, product.cost_price.decimal(), base, price)?;
        return Ok(setting);
    }
    for sku in product.skus.iter().filter(|s| s.is_active) {
        let base = round2(sku.price.decimal());
        let (price, _) = resolve_unit_amount(profile, Some(&setting), None, base);
        validate_unit_amount(profile, sku.cost_price.decimal(), base, price)?;
    }
    Ok(setting)
}

/// Product-level and SKU-level rules of one product.
#[derive(Debug, Clone, Default)]
pub struct SettingIndex {
    by_product: HashMap<Id, ProductSetting>,
    by_sku: HashMap<(Id, Id), ProductSetting>,
}

impl SettingIndex {
    /// Indexes rules (`BuildSettingIndexes`); rows without a product are ignored.
    pub fn new<'a>(settings: impl IntoIterator<Item = &'a ProductSetting>) -> Self {
        let mut index = Self::default();
        for s in settings {
            if s.product_id == 0 {
                continue;
            }
            if s.sku_id == 0 {
                index.by_product.insert(s.product_id, s.clone());
            } else {
                index.by_sku.insert((s.product_id, s.sku_id), s.clone());
            }
        }
        index
    }

    pub fn product(&self, product_id: Id) -> Option<&ProductSetting> {
        self.by_product.get(&product_id)
    }

    pub fn sku(&self, product_id: Id, sku_id: Id) -> Option<&ProductSetting> {
        self.by_sku.get(&(product_id, sku_id))
    }
}

/// Effective prices of a product's active SKUs (key `0` = product-level rule).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectivePrices {
    pub prices: BTreeMap<Id, Decimal>,
    pub sources: BTreeMap<Id, RuleSource>,
}

/// `computeProductEffectivePrices` over the rules of `product`: hidden products/SKUs get
/// no price.
pub fn effective_prices(
    profile: &Profile,
    product: &PricedProduct,
    settings: &[ProductSetting],
) -> EffectivePrices {
    // `settings` are the rules of this product.
    let product_setting = settings.iter().find(|s| s.sku_id == 0);
    let mut out = EffectivePrices::default();
    if let Some(ps) = product_setting.filter(|s| s.is_listed) {
        let (price, rule) =
            resolve_unit_amount(profile, Some(ps), None, round2(product.price.decimal()));
        out.prices.insert(0, price);
        out.sources.insert(0, rule.source);
    }
    if product_setting.is_some_and(|s| !s.is_listed) {
        return out;
    }
    for sku in product.skus.iter().filter(|s| s.is_active) {
        let sku_setting = settings.iter().find(|s| s.sku_id == sku.id);
        if sku_setting.is_some_and(|s| !s.is_listed) {
            continue;
        }
        let (price, rule) = resolve_unit_amount(
            profile,
            product_setting,
            sku_setting,
            round2(sku.price.decimal()),
        );
        out.prices.insert(sku.id, price);
        out.sources.insert(sku.id, rule.source);
    }
    out
}

/// Preview row of one product-level (`sku_id = 0`) or SKU rule.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewItem {
    pub sku_id: Id,
    pub is_listed: bool,
    pub base_price: Decimal,
    pub effective_price: Decimal,
    pub valid: bool,
    pub error_code: &'static str,
}

fn preview_code(err: &Error) -> &'static str {
    if err.key() == keys::MARKUP_EXCEEDED {
        "markup_exceeded"
    } else {
        "price_invalid"
    }
}

/// Prices the proposed rules without saving (`previewSettings`). `product` carries
/// its active SKUs only.
pub fn preview(
    profile: &Profile,
    product: &PricedProduct,
    inputs: &[SettingInput],
) -> Result<Vec<PreviewItem>> {
    let mut product_setting: Option<ProductSetting> = None;
    let mut sku_settings: HashMap<Id, ProductSetting> = HashMap::new();
    for input in inputs {
        let s = draft_setting(input)?;
        if input.sku_id == 0 {
            product_setting = Some(s);
        } else {
            sku_settings.insert(input.sku_id, s);
        }
    }
    let product_base = round2(product.price.decimal());
    let mut head = PreviewItem {
        sku_id: 0,
        is_listed: product_setting.as_ref().is_none_or(|s| s.is_listed),
        base_price: product_base,
        effective_price: Decimal::ZERO,
        valid: true,
        error_code: "",
    };
    if let Some(ps) = product_setting.as_ref().filter(|s| s.is_listed) {
        let (price, _) = resolve_unit_amount(profile, Some(ps), None, product_base);
        head.effective_price = price;
        let check = if product.skus.is_empty() {
            validate_unit_amount(profile, product.cost_price.decimal(), product_base, price)
        } else {
            product
                .skus
                .iter()
                .filter(|s| s.is_active)
                .try_for_each(|sku| {
                    let base = round2(sku.price.decimal());
                    let (p, _) = resolve_unit_amount(profile, Some(ps), None, base);
                    validate_unit_amount(profile, sku.cost_price.decimal(), base, p)
                })
        };
        if let Err(e) = check {
            head.valid = false;
            head.error_code = preview_code(&e);
        }
    }
    let mut items = vec![head];
    for sku in product.skus.iter().filter(|s| s.is_active) {
        let sku_setting = sku_settings.get(&sku.id);
        let base = round2(sku.price.decimal());
        let mut item = PreviewItem {
            sku_id: sku.id,
            is_listed: true,
            base_price: base,
            effective_price: Decimal::ZERO,
            valid: true,
            error_code: "",
        };
        if product_setting.as_ref().is_some_and(|s| !s.is_listed)
            || sku_setting.is_some_and(|s| !s.is_listed)
        {
            item.is_listed = false;
            items.push(item);
            continue;
        }
        let (price, _) = resolve_unit_amount(profile, product_setting.as_ref(), sku_setting, base);
        item.effective_price = price;
        if let Err(e) = validate_unit_amount(profile, sku.cost_price.decimal(), base, price) {
            item.valid = false;
            item.error_code = preview_code(&e);
        }
        items.push(item);
    }
    Ok(items)
}

/// Storefront prices of one product on a reseller site.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisplayPrices {
    pub visible: bool,
    pub product_id: Id,
    pub display_sku_id: Id,
    pub display_price: Amount,
    pub sku_prices: BTreeMap<Id, Amount>,
    pub hidden_sku_ids: BTreeSet<Id>,
}

/// `ResolveDisplayPrices` (RSL-09): a SKU whose saved rule became invalid (base or
/// cost price changed) is hidden instead of failing the whole listing; the order path
/// stays strict ([`price_order_line`]).
pub fn display_prices(
    profile: &Profile,
    product: &PricedProduct,
    settings: &[ProductSetting],
) -> DisplayPrices {
    let index = SettingIndex::new(settings);
    let product_setting = index.product(product.id);
    if product_setting.is_some_and(|s| !s.is_listed) {
        return DisplayPrices {
            product_id: product.id,
            ..DisplayPrices::default()
        };
    }
    let mut out = DisplayPrices {
        product_id: product.id,
        ..DisplayPrices::default()
    };
    for sku in product.skus.iter().filter(|s| s.is_active) {
        let sku_setting = index.sku(product.id, sku.id);
        if sku_setting.is_some_and(|s| !s.is_listed) {
            out.hidden_sku_ids.insert(sku.id);
            continue;
        }
        let base = round2(sku.price.decimal());
        let (price, _) = resolve_unit_amount(profile, product_setting, sku_setting, base);
        if validate_unit_amount(profile, sku.cost_price.decimal(), base, price).is_err() {
            // Dirty rule (base / cost price changed after saving): hide the SKU only.
            out.hidden_sku_ids.insert(sku.id);
            continue;
        }
        let amount = Amount::new(price);
        out.sku_prices.insert(sku.id, amount);
        if !out.visible {
            out.visible = true;
            out.display_sku_id = sku.id;
            out.display_price = amount;
        }
    }
    if product.skus.is_empty() {
        let (price, _) = resolve_unit_amount(
            profile,
            product_setting,
            None,
            round2(product.price.decimal()),
        );
        out.visible = true;
        out.display_price = Amount::new(price);
    }
    out
}

/// Profit block reasons (self-dealing, RSL-07).
pub const PROFIT_BLOCK_OWNER: &str = "self_dealing_owner";
pub const PROFIT_BLOCK_RELATED_ACCOUNT: &str = "self_dealing_related_account";

/// One priced order line (`OrderPricingItem`).
#[derive(Debug, Clone, PartialEq)]
pub struct OrderPricingItem {
    pub product_id: Id,
    pub sku_id: Id,
    pub quantity: i32,
    pub child_order_id: Id,
    pub base_unit_amount: Decimal,
    pub reseller_unit_amount: Decimal,
    pub base_total_amount: Decimal,
    pub reseller_total_amount: Decimal,
    pub profit_amount: Decimal,
    pub pricing_mode: PricingMode,
    pub rule_source: RuleSource,
    pub setting_id: Option<Id>,
    pub order_id: Id,
    pub order_item_id: Id,
}

/// A line to price for a reseller order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderLine {
    pub product_id: Id,
    pub sku_id: Id,
    pub quantity: i32,
    /// SKU base (main-site) unit price.
    pub base_unit: Amount,
    /// SKU cost price (`0` = none).
    pub cost: Amount,
}

/// Prices one order line strictly (`ApplyToOrderBuildResult` loop body): hidden
/// product/SKU → `error.reseller_product_not_listed`; invalid price → price errors.
/// Coupons, member, promotion and wholesale prices never apply to reseller orders.
pub fn price_order_line(
    profile: &Profile,
    index: &SettingIndex,
    line: &OrderLine,
) -> Result<OrderPricingItem> {
    let product_setting = index.product(line.product_id);
    let sku_setting = index.sku(line.product_id, line.sku_id);
    if product_setting.is_some_and(|s| !s.is_listed) || sku_setting.is_some_and(|s| !s.is_listed) {
        return Err(Error::bad_request(keys::PRODUCT_NOT_LISTED));
    }
    let base = round2(line.base_unit.decimal());
    let (unit, rule) = resolve_unit_amount(profile, product_setting, sku_setting, base);
    validate_unit_amount(profile, line.cost.decimal(), base, unit)?;
    let qty = Decimal::from(line.quantity);
    let base_total = round2(base * qty);
    let reseller_total = round2(unit * qty);
    Ok(OrderPricingItem {
        product_id: line.product_id,
        sku_id: line.sku_id,
        quantity: line.quantity,
        child_order_id: 0,
        base_unit_amount: base,
        reseller_unit_amount: unit,
        base_total_amount: base_total,
        reseller_total_amount: reseller_total,
        profit_amount: round2(reseller_total - base_total),
        pricing_mode: rule.mode,
        rule_source: rule.source,
        setting_id: rule.setting_id,
        order_id: 0,
        order_item_id: 0,
    })
}

/// Reseller order pricing context (`OrderPricingContext`), built by the order group
/// before its create-order transaction and persisted as an [`super::OrderSnapshot`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OrderPricingContext {
    pub reseller_id: Id,
    pub domain: String,
    pub currency: String,
    pub reseller_user_id: Id,
    /// `0` for guests.
    pub buyer_user_id: Id,
    pub base_amount: Decimal,
    pub reseller_amount: Decimal,
    pub profit_amount: Decimal,
    pub effective_profit: Decimal,
    pub profit_eligible: bool,
    pub profit_block_reason: String,
    pub items: Vec<OrderPricingItem>,
    pub risk_snapshot: Option<Value>,
}

impl OrderPricingContext {
    pub fn new(
        reseller_id: Id,
        domain: &str,
        currency: &str,
        reseller_user_id: Id,
        buyer_user_id: Id,
    ) -> Self {
        Self {
            reseller_id,
            domain: domain.to_owned(),
            currency: currency.to_owned(),
            reseller_user_id,
            buyer_user_id,
            profit_eligible: true,
            ..Self::default()
        }
    }

    /// Adds a priced line to the totals.
    pub fn push(&mut self, item: OrderPricingItem) {
        self.base_amount = round2(self.base_amount + item.base_total_amount);
        self.reseller_amount = round2(self.reseller_amount + item.reseller_total_amount);
        self.profit_amount = round2(self.profit_amount + item.profit_amount);
        self.items.push(item);
    }

    /// Self-dealing risk (`ApplySelfDealingRisk`): the reseller owner or an active related
    /// account buying on their own site earns no profit. Also fixes `effective_profit`.
    pub fn apply_self_dealing_risk(&mut self, owner_user_id: Id, related_account_match: bool) {
        let mut owner_match = false;
        let mut related_match = false;
        if self.buyer_user_id > 0 && self.buyer_user_id == owner_user_id {
            owner_match = true;
            self.profit_eligible = false;
            PROFIT_BLOCK_OWNER.clone_into(&mut self.profit_block_reason);
        } else if related_account_match {
            related_match = true;
            self.profit_eligible = false;
            PROFIT_BLOCK_RELATED_ACCOUNT.clone_into(&mut self.profit_block_reason);
        }
        self.effective_profit = if self.profit_eligible {
            self.profit_amount
        } else {
            Decimal::ZERO
        };
        self.risk_snapshot = Some(json!({
            "buyer_user_id": self.buyer_user_id,
            "reseller_user_id": self.reseller_user_id,
            "profit_eligible": self.profit_eligible,
            "profit_block_reason": self.profit_block_reason,
            "guest_buyer": self.buyer_user_id == 0,
            "self_dealing_deferred": "same_contact_and_risk_detected_account_linking",
            "self_dealing": {
                "owner_match": owner_match,
                "related_account_match": related_match,
            },
        }));
    }

    /// Records the persisted child order / order item of line `index`.
    pub fn bind_created_order_item(&mut self, index: usize, child_order_id: Id, order_item_id: Id) {
        if let Some(item) = self.items.get_mut(index) {
            item.child_order_id = child_order_id;
            item.order_id = child_order_id;
            item.order_item_id = order_item_id;
        }
    }

    /// `pricing_snapshot_json` (`BuildPricingSnapshotJSON`).
    pub fn pricing_snapshot(&self) -> Value {
        let items: Vec<Value> = self
            .items
            .iter()
            .map(|i| {
                json!({
                    "product_id": i.product_id,
                    "sku_id": i.sku_id,
                    "quantity": i.quantity,
                    "child_order_id": i.child_order_id,
                    "base_unit_amount": money(i.base_unit_amount),
                    "reseller_unit_amount": money(i.reseller_unit_amount),
                    "base_total_amount": money(i.base_total_amount),
                    "reseller_total_amount": money(i.reseller_total_amount),
                    "profit_amount": money(i.profit_amount),
                    "pricing_mode": i.pricing_mode.as_str(),
                    "rule_source": i.rule_source.as_str(),
                    "order_id": if i.order_id == 0 { i.child_order_id } else { i.order_id },
                    "order_item_id": i.order_item_id,
                    "setting_id": i.setting_id,
                })
            })
            .collect();
        json!({
            "currency": self.currency,
            "base_amount": money(self.base_amount),
            "reseller_amount": money(self.reseller_amount),
            "profit_amount": money(self.profit_amount),
            "items": items,
        })
    }

    /// `risk_snapshot_json` (`BuildRiskSnapshotJSON`).
    pub fn risk_snapshot(&self) -> Value {
        self.risk_snapshot.clone().unwrap_or_else(|| {
            json!({
                "buyer_user_id": self.buyer_user_id,
                "reseller_user_id": self.reseller_user_id,
                "profit_eligible": self.profit_eligible,
                "profit_block_reason": self.profit_block_reason,
            })
        })
    }
}

/// Two-decimal string (`MoneyString`).
pub fn money(v: Decimal) -> String {
    Amount::new(v).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseller::model::ProfileStatus;
    use crate::reseller::rules::fixtures::{amt, d, profile};

    fn prof(default_markup: &str, max_markup: &str) -> Profile {
        let mut p = profile(ProfileStatus::Active);
        p.default_markup_percent = amt(default_markup);
        p.max_markup_percent = amt(max_markup);
        p
    }

    fn rule(sku_id: Id, mode: PricingMode, value: &str) -> ProductSetting {
        let mut s = ProductSetting::draft(sku_id, true, mode);
        s.id = 100 + sku_id;
        s.product_id = 1;
        match mode {
            PricingMode::MarkupPercent => s.markup_percent = amt(value),
            PricingMode::FixedMarkup => s.fixed_markup_amount = amt(value),
            PricingMode::FixedPrice => s.fixed_price_amount = amt(value),
            PricingMode::Inherit => {}
        }
        s
    }

    fn sku(id: Id, price: &str, cost: &str) -> PricedSku {
        PricedSku {
            id,
            sku_code: format!("S{id}"),
            spec_values: Value::Null,
            price: amt(price),
            cost_price: amt(cost),
            is_active: true,
        }
    }

    fn product(skus: Vec<PricedSku>) -> PricedProduct {
        PricedProduct {
            id: 1,
            slug: "p".into(),
            title: json!({"zh-CN": "P"}),
            price: amt("100"),
            cost_price: amt("0"),
            is_active: true,
            skus,
        }
    }

    #[test]
    fn pricing_modes() {
        let p = prof("0", "0");
        let base = d("100");
        assert_eq!(resolve_unit_amount(&p, None, None, base).0, d("100.00"));
        let (price, r) = resolve_unit_amount(
            &p,
            Some(&rule(0, PricingMode::MarkupPercent, "12.5")),
            None,
            base,
        );
        assert_eq!((price, r.source), (d("112.50"), RuleSource::Product));
        assert_eq!(
            resolve_unit_amount(
                &p,
                None,
                Some(&rule(2, PricingMode::FixedMarkup, "7.25")),
                base
            )
            .0,
            d("107.25")
        );
        let (price, r) = resolve_unit_amount(
            &p,
            Some(&rule(0, PricingMode::MarkupPercent, "50")),
            Some(&rule(2, PricingMode::FixedPrice, "130")),
            base,
        );
        assert_eq!(
            (price, r.source, r.setting_id),
            (d("130.00"), RuleSource::Sku, Some(102))
        );
        // inherit SKU rule falls through to the product rule, then the profile default
        let (price, r) = resolve_unit_amount(
            &p,
            Some(&rule(0, PricingMode::Inherit, "")),
            Some(&rule(2, PricingMode::Inherit, "")),
            base,
        );
        assert_eq!((price, r.source), (d("100.00"), RuleSource::Inherit));
        let p10 = prof("10", "0");
        let (price, r) =
            resolve_unit_amount(&p10, Some(&rule(0, PricingMode::Inherit, "")), None, base);
        assert_eq!((price, r.source), (d("110.00"), RuleSource::Profile));
        // half away from zero at cents
        assert_eq!(apply_markup_percent(d("0.05"), d("50")), d("0.08"));
    }

    // RSL-07: floor / cost / markup cap with base 100, cost 90, max 50%.
    #[test]
    fn rsl07_price_floor_and_markup_cap() {
        let p = prof("0", "50");
        let (base, cost) = (d("100"), d("90"));
        assert_eq!(
            validate_unit_amount(&p, cost, base, d("99"))
                .unwrap_err()
                .key(),
            keys::PRICE_INVALID
        );
        assert_eq!(
            validate_unit_amount(&p, cost, base, d("151"))
                .unwrap_err()
                .key(),
            keys::MARKUP_EXCEEDED
        );
        assert!(validate_unit_amount(&p, cost, base, d("150")).is_ok());
        assert!(validate_unit_amount(&p, cost, base, d("120")).is_ok());
        assert_eq!(
            validate_unit_amount(&p, d("130"), base, d("120"))
                .unwrap_err()
                .key(),
            keys::PRICE_INVALID
        );
        assert_eq!(
            validate_unit_amount(&p, cost, d("0"), d("0"))
                .unwrap_err()
                .key(),
            keys::PRICE_INVALID
        );
        // no cap when max = 0
        assert!(validate_unit_amount(&prof("0", "0"), cost, base, d("1000")).is_ok());
    }

    // RSL-07: order line profit = (reseller - base) × qty; hidden → not listed.
    #[test]
    fn rsl07_order_line_and_context() {
        let p = prof("0", "50");
        let settings = [rule(2, PricingMode::FixedPrice, "120")];
        let index = SettingIndex::new(&settings);
        let line = OrderLine {
            product_id: 1,
            sku_id: 2,
            quantity: 3,
            base_unit: amt("100"),
            cost: amt("90"),
        };
        let item = price_order_line(&p, &index, &line).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(item.profit_amount, d("60.00"));
        assert_eq!(item.reseller_total_amount, d("360.00"));
        let mut too_high = settings[0].clone();
        too_high.fixed_price_amount = amt("151");
        let index = SettingIndex::new([&too_high]);
        assert_eq!(
            price_order_line(&p, &index, &line).unwrap_err().key(),
            keys::MARKUP_EXCEEDED
        );
        let mut hidden = settings[0].clone();
        hidden.is_listed = false;
        let index = SettingIndex::new([&hidden]);
        assert_eq!(
            price_order_line(&p, &index, &line).unwrap_err().key(),
            keys::PRODUCT_NOT_LISTED
        );

        let mut ctx = OrderPricingContext::new(5, "shop.test", "CNY", 9, 9);
        ctx.push(item.clone());
        ctx.apply_self_dealing_risk(9, false);
        assert!(!ctx.profit_eligible);
        assert_eq!(ctx.profit_block_reason, PROFIT_BLOCK_OWNER);
        assert_eq!(ctx.effective_profit, Decimal::ZERO);
        assert_eq!(ctx.profit_amount, d("60.00"));
        assert_eq!(ctx.risk_snapshot()["self_dealing"]["owner_match"], true);

        let mut ctx = OrderPricingContext::new(5, "shop.test", "CNY", 9, 11);
        ctx.push(item);
        ctx.apply_self_dealing_risk(9, true);
        assert_eq!(ctx.profit_block_reason, PROFIT_BLOCK_RELATED_ACCOUNT);
        let mut guest = OrderPricingContext::new(5, "shop.test", "CNY", 9, 0);
        guest.apply_self_dealing_risk(9, false);
        assert!(guest.profit_eligible);
        assert_eq!(guest.risk_snapshot()["guest_buyer"], true);

        ctx.bind_created_order_item(0, 77, 88);
        let snap = ctx.pricing_snapshot();
        assert_eq!(snap["items"][0]["order_item_id"], 88);
        assert_eq!(snap["items"][0]["profit_amount"], "60.00");
        assert_eq!(snap["profit_amount"], "60.00");
        assert_eq!(snap["items"][0]["setting_id"], 102);
    }

    // RSL-09: invalid SKU rule hides that SKU only.
    #[test]
    fn rsl09_display_prices_hide_invalid_sku() {
        let p = prof("0", "0");
        let prod = PricedProduct {
            id: 1,
            ..product(vec![sku(11, "100", "0"), sku(12, "100", "0")])
        };
        let settings = [
            rule(11, PricingMode::FixedPrice, "130"),
            rule(12, PricingMode::FixedPrice, "80"),
        ];
        let out = display_prices(&p, &prod, &settings);
        assert!(out.visible);
        assert!(out.hidden_sku_ids.contains(&12));
        assert!(!out.sku_prices.contains_key(&12));
        assert_eq!(out.display_sku_id, 11);
        assert_eq!(out.display_price.to_string(), "130.00");
        // order path is strict
        let index = SettingIndex::new(&settings);
        let line = OrderLine {
            product_id: 1,
            sku_id: 12,
            quantity: 1,
            base_unit: amt("100"),
            cost: amt("0"),
        };
        assert_eq!(
            price_order_line(&p, &index, &line).unwrap_err().key(),
            keys::PRICE_INVALID
        );
        // hidden product
        let mut hidden = rule(0, PricingMode::Inherit, "");
        hidden.is_listed = false;
        assert!(!display_prices(&p, &prod, &[hidden]).visible);
    }

    #[test]
    fn normalize_and_preview() {
        let p = prof("0", "50");
        let prod = product(vec![sku(11, "100", "90"), sku(12, "200", "0")]);
        let input = |sku_id, mode: &str, v: &str| SettingInput {
            sku_id,
            is_listed: true,
            pricing_mode: mode.into(),
            markup_percent: d(v),
            fixed_markup_amount: d(v),
            fixed_price_amount: d(v),
            sort_order: 0,
        };
        assert!(normalize_setting(&p, &prod, &input(0, "markup_percent", "20")).is_ok());
        assert_eq!(
            normalize_setting(&p, &prod, &input(0, "markup_percent", "60"))
                .unwrap_err()
                .key(),
            keys::MARKUP_EXCEEDED
        );
        assert_eq!(
            normalize_setting(&p, &prod, &input(0, "weird", "1"))
                .unwrap_err()
                .key(),
            keys::PRICE_INVALID
        );
        assert_eq!(
            normalize_setting(&p, &prod, &input(99, "inherit", "0"))
                .unwrap_err()
                .key(),
            keys::SKU_INVALID
        );
        assert_eq!(
            normalize_setting(&p, &prod, &input(11, "fixed_price", "120"))
                .map(|s| s.sku_id)
                .ok(),
            Some(11)
        );
        assert_eq!(
            normalize_setting(&p, &prod, &input(11, "fixed_price", "95"))
                .unwrap_err()
                .key(),
            keys::PRICE_INVALID
        );
        let mut hidden = input(11, "fixed_price", "1");
        hidden.is_listed = false;
        assert!(
            normalize_setting(&p, &prod, &hidden).is_ok(),
            "hidden rules skip price checks"
        );

        let items = preview(
            &p,
            &prod,
            &[
                input(0, "markup_percent", "10"),
                input(12, "fixed_price", "500"),
            ],
        )
        .unwrap_or_default();
        assert_eq!(items.len(), 3);
        assert_eq!(
            (items[0].effective_price, items[0].valid),
            (d("110.00"), true)
        );
        assert_eq!(
            (items[1].sku_id, items[1].effective_price),
            (11, d("110.00"))
        );
        assert_eq!(
            (items[2].sku_id, items[2].valid, items[2].error_code),
            (12, false, "markup_exceeded")
        );

        let settings = [rule(0, PricingMode::MarkupPercent, "10"), {
            let mut r = rule(12, PricingMode::Inherit, "");
            r.is_listed = false;
            r
        }];
        let eff = effective_prices(&p, &prod, &settings);
        assert_eq!(eff.prices.get(&0), Some(&d("110.00")));
        assert_eq!(eff.prices.get(&11), Some(&d("110.00")));
        assert!(!eff.prices.contains_key(&12));
        assert_eq!(eff.sources.get(&11), Some(&RuleSource::Product));
    }
}
