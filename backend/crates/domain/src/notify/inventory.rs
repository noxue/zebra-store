//! Inventory anomaly detection for the periodic alert check (port of the
//! original dashboard `GetInventoryAlertItems` collectors).

use std::collections::HashMap;

use serde_json::Value;

use super::center::{InventoryAlertRow, alert_types};
use crate::Id;
use crate::catalog::product::DEFAULT_SKU_CODE;
use crate::catalog::stock::MANUAL_STOCK_UNLIMITED;

/// An active SKU of a product being checked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StockSku {
    pub id: Id,
    pub sku_code: String,
    pub spec_values: Value,
    pub is_active: bool,
    pub manual_stock_total: i64,
}

/// An active product being checked; `skus` ordered `sort_order DESC, created_at ASC`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StockProduct {
    pub id: Id,
    pub title: Value,
    pub fulfillment_type: String,
    pub manual_stock_total: i64,
    pub skus: Vec<StockSku>,
}

/// Upstream stock of one mapped local SKU.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpstreamSkuStock {
    pub local_sku_id: Id,
    pub upstream_stock: i64,
    pub upstream_is_active: bool,
}

/// `out_of_stock_products` when nothing is left, `low_stock_products` up to the threshold.
pub fn classify(available: i64, low_stock_threshold: i64) -> Option<&'static str> {
    if available <= 0 {
        Some(alert_types::OUT_OF_STOCK_PRODUCTS)
    } else if available <= low_stock_threshold {
        Some(alert_types::LOW_STOCK_PRODUCTS)
    } else {
        None
    }
}

fn product_row(
    p: &StockProduct,
    kind: &str,
    alert_type: &str,
    available: i64,
) -> InventoryAlertRow {
    InventoryAlertRow {
        product_id: p.id,
        product_title: p.title.clone(),
        fulfillment_type: kind.to_owned(),
        alert_type: alert_type.to_owned(),
        available_stock: available,
        ..InventoryAlertRow::default()
    }
}

fn sku_row(
    p: &StockProduct,
    sku: &StockSku,
    kind: &str,
    alert_type: &str,
    available: i64,
) -> InventoryAlertRow {
    InventoryAlertRow {
        sku_id: sku.id,
        sku_code: sku.sku_code.trim().to_owned(),
        sku_spec_values: sku.spec_values.clone(),
        ..product_row(p, kind, alert_type, available)
    }
}

fn active(p: &StockProduct) -> Vec<&StockSku> {
    p.skus.iter().filter(|s| s.is_active).collect()
}

/// Rows of a manually fulfilled product (unlimited stock never alerts).
pub fn collect_manual(p: &StockProduct, low: i64) -> Vec<InventoryAlertRow> {
    let skus = active(p);
    if skus.is_empty() {
        if p.manual_stock_total == i64::from(MANUAL_STOCK_UNLIMITED) {
            return Vec::new();
        }
        let available = p.manual_stock_total.max(0);
        return classify(available, low)
            .map(|t| vec![product_row(p, "manual", t, available)])
            .unwrap_or_default();
    }
    skus.into_iter()
        .filter(|s| s.manual_stock_total != i64::from(MANUAL_STOCK_UNLIMITED))
        .filter_map(|s| {
            let available = s.manual_stock_total.max(0);
            classify(available, low).map(|t| sku_row(p, s, "manual", t, available))
        })
        .collect()
}

/// Rows of an upstream product; only active mappings of active SKUs count.
pub fn collect_upstream(
    p: &StockProduct,
    mappings: &[UpstreamSkuStock],
    low: i64,
) -> Vec<InventoryAlertRow> {
    let by_sku: HashMap<Id, &UpstreamSkuStock> = mappings
        .iter()
        .filter(|m| m.local_sku_id > 0)
        .map(|m| (m.local_sku_id, m))
        .collect();
    let skus = active(p);
    if skus.is_empty() {
        let total: i64 = by_sku
            .values()
            .filter(|m| m.upstream_is_active)
            .map(|m| m.upstream_stock.max(0))
            .sum();
        return classify(total, low)
            .map(|t| vec![product_row(p, "upstream", t, total)])
            .unwrap_or_default();
    }
    skus.into_iter()
        .filter_map(|s| {
            let m = by_sku.get(&s.id).filter(|m| m.upstream_is_active)?;
            let available = m.upstream_stock.max(0);
            classify(available, low).map(|t| sku_row(p, s, "upstream", t, available))
        })
        .collect()
}

fn legacy_target_index(skus: &[&StockSku]) -> Option<usize> {
    skus.iter()
        .position(|s| s.sku_code.trim().eq_ignore_ascii_case(DEFAULT_SKU_CODE))
        .or(if skus.is_empty() { None } else { Some(0) })
}

/// Rows of an auto-delivered product; `available` maps `sku_id` (0 = legacy) to
/// the number of available card secrets.
pub fn collect_auto(
    p: &StockProduct,
    available: &HashMap<Id, i64>,
    low: i64,
) -> Vec<InventoryAlertRow> {
    let skus = active(p);
    let total: i64 = available.values().sum();
    if skus.is_empty() {
        return classify(total, low)
            .map(|t| vec![product_row(p, "auto", t, total)])
            .unwrap_or_default();
    }
    let legacy_inactive: i64 = available
        .iter()
        .filter(|(id, _)| **id != 0 && !skus.iter().any(|s| s.id == **id))
        .map(|(_, n)| *n)
        .sum();
    let target = legacy_target_index(&skus);
    let mut rows = Vec::new();
    let mut has_positive = false;
    for (idx, sku) in skus.iter().enumerate() {
        let mut n = available.get(&sku.id).copied().unwrap_or(0);
        if Some(idx) == target {
            n += available.get(&0).copied().unwrap_or(0);
        }
        if skus.len() == 1 {
            n += legacy_inactive;
        }
        if n > 0 {
            has_positive = true;
        }
        if let Some(t) = classify(n, low) {
            rows.push(sku_row(p, sku, "auto", t, n));
        }
    }
    if has_positive || legacy_inactive <= 0 {
        return rows;
    }
    match classify(total, low) {
        Some(t) => vec![product_row(p, "auto", t, total)],
        None => rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sku(id: Id, manual: i64) -> StockSku {
        StockSku {
            id,
            sku_code: format!("S{id}"),
            is_active: true,
            manual_stock_total: manual,
            ..StockSku::default()
        }
    }

    #[test]
    fn classify_thresholds() {
        assert_eq!(classify(0, 5), Some(alert_types::OUT_OF_STOCK_PRODUCTS));
        assert_eq!(classify(-3, 5), Some(alert_types::OUT_OF_STOCK_PRODUCTS));
        assert_eq!(classify(5, 5), Some(alert_types::LOW_STOCK_PRODUCTS));
        assert_eq!(classify(6, 5), None);
    }

    #[test]
    fn manual_skips_unlimited_and_inactive() {
        let mut inactive = sku(3, 0);
        inactive.is_active = false;
        let p = StockProduct {
            id: 1,
            fulfillment_type: "manual".into(),
            skus: vec![sku(1, -1), sku(2, 2), inactive],
            ..StockProduct::default()
        };
        let rows = collect_manual(&p, 5);
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].sku_id, rows[0].available_stock), (2, 2));
        assert_eq!(rows[0].alert_type, alert_types::LOW_STOCK_PRODUCTS);
    }

    #[test]
    fn auto_adds_legacy_stock_to_default_sku() {
        let mut default = sku(10, 0);
        default.sku_code = "DEFAULT".into();
        let p = StockProduct {
            id: 1,
            skus: vec![sku(9, 0), default],
            ..StockProduct::default()
        };
        let available = HashMap::from([(0, 3), (9, 10)]);
        let rows = collect_auto(&p, &available, 5);
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].sku_id, rows[0].available_stock), (10, 3));
    }

    #[test]
    fn upstream_ignores_inactive_mappings() {
        let p = StockProduct {
            id: 1,
            skus: vec![sku(1, 0), sku(2, 0)],
            ..StockProduct::default()
        };
        let maps = [
            UpstreamSkuStock {
                local_sku_id: 1,
                upstream_stock: 0,
                upstream_is_active: false,
            },
            UpstreamSkuStock {
                local_sku_id: 2,
                upstream_stock: 1,
                upstream_is_active: true,
            },
        ];
        let rows = collect_upstream(&p, &maps, 5);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].sku_id, 2);
    }
}
