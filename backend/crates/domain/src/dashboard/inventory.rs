//! Stock statistics and inventory alerts (port of
//! `dashboard/infrastructure/gormstore/inventory.go`).
//!
//! Stock semantics (ORD-06, ORD-12): manual stock is the remaining quantity in
//! `manual_stock_total` (`-1` = unlimited, negatives count as 0); auto stock
//! is the number of available, non-deleted card secrets; upstream stock is the
//! mapped upstream SKU stock.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use super::stats::is_empty_json;
use crate::Id;

/// `manual_stock_total` value meaning unlimited stock.
pub const MANUAL_STOCK_UNLIMITED: i64 = -1;
/// Code of the default SKU (legacy `sku_id = 0` secrets belong to it).
pub const DEFAULT_SKU_CODE: &str = "DEFAULT";
/// Alert type of sold-out products/SKUs.
pub const ALERT_OUT_OF_STOCK: &str = "out_of_stock_products";
/// Alert type of low-stock products/SKUs.
pub const ALERT_LOW_STOCK: &str = "low_stock_products";

pub const FULFILLMENT_AUTO: &str = "auto";
pub const FULFILLMENT_MANUAL: &str = "manual";
pub const FULFILLMENT_UPSTREAM: &str = "upstream";

/// An active SKU of an active product.
#[derive(Debug, Clone)]
pub struct InvSku {
    pub id: Id,
    pub sku_code: String,
    pub spec_values: Option<Value>,
    pub manual_stock_total: i64,
}

/// An active product with its active SKUs ordered `sort_order DESC, created_at ASC`.
#[derive(Debug, Clone)]
pub struct InvProduct {
    pub id: Id,
    pub title: Option<Value>,
    pub fulfillment_type: String,
    pub manual_stock_total: i64,
    pub skus: Vec<InvSku>,
}

/// Upstream stock of a mapped local SKU.
#[derive(Debug, Clone, Copy)]
pub struct UpstreamSku {
    pub local_sku_id: Id,
    pub upstream_stock: i64,
    pub upstream_is_active: bool,
}

/// Everything the stock statistics need.
#[derive(Debug, Clone, Default)]
pub struct InventorySnapshot {
    /// Active products ordered `sort_order DESC, created_at DESC`.
    pub products: Vec<InvProduct>,
    /// Available card secrets of auto products: product → sku (0 = legacy) → count.
    pub secrets: HashMap<Id, HashMap<Id, i64>>,
    /// Upstream SKU mappings of upstream products: product → mappings.
    pub upstream: HashMap<Id, Vec<UpstreamSku>>,
}

/// Stock KPIs of the overview (original `StockStatsRow`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StockStats {
    pub out_of_stock_products: i64,
    pub low_stock_products: i64,
    pub out_of_stock_skus: i64,
    pub low_stock_skus: i64,
    pub auto_available_secrets: i64,
    pub manual_available_units: i64,
}

/// One inventory alert row (`GET /admin/dashboard/inventory-alerts`).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InventoryAlert {
    pub product_id: Id,
    #[serde(skip_serializing_if = "is_zero")]
    pub sku_id: Id,
    pub product_title: Option<Value>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sku_code: String,
    #[serde(skip_serializing_if = "is_empty_json")]
    pub sku_spec_values: Option<Value>,
    pub fulfillment_type: String,
    pub alert_type: String,
    pub available_stock: i64,
}

fn is_zero(v: &Id) -> bool {
    *v == 0
}

/// `out_of_stock_products` when nothing is left, `low_stock_products` up to
/// the threshold, `None` otherwise.
pub fn classify(available: i64, low_stock_threshold: i64) -> Option<&'static str> {
    if available <= 0 {
        Some(ALERT_OUT_OF_STOCK)
    } else if available <= low_stock_threshold {
        Some(ALERT_LOW_STOCK)
    } else {
        None
    }
}

/// Remaining manual stock of a product (sum of SKUs when it has any);
/// `None` when unlimited.
fn manual_available(p: &InvProduct) -> Option<i64> {
    if p.skus.is_empty() {
        return (p.manual_stock_total != MANUAL_STOCK_UNLIMITED)
            .then(|| p.manual_stock_total.max(0));
    }
    let mut total = 0;
    for s in &p.skus {
        if s.manual_stock_total == MANUAL_STOCK_UNLIMITED {
            return None;
        }
        total += s.manual_stock_total.max(0);
    }
    Some(total)
}

/// Index of the SKU that owns legacy `sku_id = 0` secrets: the DEFAULT SKU,
/// else the first active one.
fn legacy_target(skus: &[InvSku]) -> Option<usize> {
    skus.iter()
        .position(|s| s.sku_code.trim().eq_ignore_ascii_case(DEFAULT_SKU_CODE))
        .or(if skus.is_empty() { None } else { Some(0) })
}

fn bump(stats: &mut StockStats, kind: Option<&str>, sku_level: bool) {
    match (kind, sku_level) {
        (Some(ALERT_OUT_OF_STOCK), false) => stats.out_of_stock_products += 1,
        (Some(ALERT_LOW_STOCK), false) => stats.low_stock_products += 1,
        (Some(ALERT_OUT_OF_STOCK), true) => stats.out_of_stock_skus += 1,
        (Some(ALERT_LOW_STOCK), true) => stats.low_stock_skus += 1,
        _ => {}
    }
}

/// Stock KPIs (original `GetStockStats`; upstream products are not counted).
pub fn stock_stats(snap: &InventorySnapshot, low: i64) -> StockStats {
    let mut st = StockStats::default();
    let empty = HashMap::new();
    for p in &snap.products {
        match p.fulfillment_type.trim() {
            FULFILLMENT_AUTO => {
                let counts = snap.secrets.get(&p.id).unwrap_or(&empty);
                // Only secrets of active SKUs and legacy sku_id = 0 are counted.
                let counted = |sku: Id| sku == 0 || p.skus.iter().any(|s| s.id == sku);
                let total: i64 = counts
                    .iter()
                    .filter(|(sku, _)| counted(**sku))
                    .map(|(_, n)| *n)
                    .sum();
                st.auto_available_secrets += total;
                bump(&mut st, classify(total, low), false);
                for (idx, s) in p.skus.iter().enumerate() {
                    let mut avail = counts.get(&s.id).copied().unwrap_or(0);
                    if idx == 0 {
                        avail += counts.get(&0).copied().unwrap_or(0);
                    }
                    bump(&mut st, classify(avail, low), true);
                }
            }
            FULFILLMENT_MANUAL => {
                let Some(avail) = manual_available(p) else {
                    continue;
                };
                st.manual_available_units += avail;
                bump(&mut st, classify(avail, low), false);
                for s in &p.skus {
                    if s.manual_stock_total == MANUAL_STOCK_UNLIMITED {
                        continue;
                    }
                    bump(&mut st, classify(s.manual_stock_total.max(0), low), true);
                }
            }
            _ => {}
        }
    }
    st
}

fn alert(
    p: &InvProduct,
    sku: Option<&InvSku>,
    fulfillment: &str,
    kind: &str,
    available: i64,
) -> InventoryAlert {
    InventoryAlert {
        product_id: p.id,
        sku_id: sku.map_or(0, |s| s.id),
        product_title: p.title.clone(),
        sku_code: sku
            .map(|s| s.sku_code.trim().to_owned())
            .unwrap_or_default(),
        sku_spec_values: sku.and_then(|s| s.spec_values.clone()),
        fulfillment_type: fulfillment.to_owned(),
        alert_type: kind.to_owned(),
        available_stock: available,
    }
}

fn manual_alerts(p: &InvProduct, low: i64) -> Vec<InventoryAlert> {
    if p.skus.is_empty() {
        if p.manual_stock_total == MANUAL_STOCK_UNLIMITED {
            return Vec::new();
        }
        let avail = p.manual_stock_total.max(0);
        return classify(avail, low)
            .map(|k| alert(p, None, FULFILLMENT_MANUAL, k, avail))
            .into_iter()
            .collect();
    }
    p.skus
        .iter()
        .filter(|s| s.manual_stock_total != MANUAL_STOCK_UNLIMITED)
        .filter_map(|s| {
            let avail = s.manual_stock_total.max(0);
            classify(avail, low).map(|k| alert(p, Some(s), FULFILLMENT_MANUAL, k, avail))
        })
        .collect()
}

fn upstream_alerts(p: &InvProduct, mappings: &[UpstreamSku], low: i64) -> Vec<InventoryAlert> {
    let mut by_sku: HashMap<Id, UpstreamSku> = HashMap::new();
    for m in mappings.iter().filter(|m| m.local_sku_id > 0) {
        by_sku.insert(m.local_sku_id, *m);
    }
    if p.skus.is_empty() {
        let total: i64 = by_sku
            .values()
            .filter(|m| m.upstream_is_active)
            .map(|m| m.upstream_stock.max(0))
            .sum();
        return classify(total, low)
            .map(|k| alert(p, None, FULFILLMENT_UPSTREAM, k, total))
            .into_iter()
            .collect();
    }
    p.skus
        .iter()
        .filter_map(|s| {
            let m = by_sku.get(&s.id).filter(|m| m.upstream_is_active)?;
            let avail = m.upstream_stock.max(0);
            classify(avail, low).map(|k| alert(p, Some(s), FULFILLMENT_UPSTREAM, k, avail))
        })
        .collect()
}

fn auto_alerts(p: &InvProduct, counts: &HashMap<Id, i64>, low: i64) -> Vec<InventoryAlert> {
    let total: i64 = counts.values().sum();
    let legacy_inactive: i64 = counts
        .iter()
        .filter(|(sku, _)| **sku != 0 && !p.skus.iter().any(|s| s.id == **sku))
        .map(|(_, n)| *n)
        .sum();
    if p.skus.is_empty() {
        return classify(total, low)
            .map(|k| alert(p, None, FULFILLMENT_AUTO, k, total))
            .into_iter()
            .collect();
    }
    let target = legacy_target(&p.skus);
    let mut out = Vec::new();
    let mut has_positive = false;
    for (idx, s) in p.skus.iter().enumerate() {
        let mut avail = counts.get(&s.id).copied().unwrap_or(0);
        if Some(idx) == target {
            avail += counts.get(&0).copied().unwrap_or(0);
        }
        if p.skus.len() == 1 {
            avail += legacy_inactive;
        }
        if avail > 0 {
            has_positive = true;
        }
        if let Some(k) = classify(avail, low) {
            out.push(alert(p, Some(s), FULFILLMENT_AUTO, k, avail));
        }
    }
    if has_positive || legacy_inactive <= 0 {
        return out;
    }
    // Only disabled SKUs still hold stock: fall back to a product-level alert (DLV-04).
    match classify(total, low) {
        Some(k) => vec![alert(p, None, FULFILLMENT_AUTO, k, total)],
        None => out,
    }
}

/// Per-product/SKU alert rows (original `GetInventoryAlertItems`).
pub fn inventory_alerts(snap: &InventorySnapshot, low: i64) -> Vec<InventoryAlert> {
    let empty = HashMap::new();
    let mut out = Vec::new();
    for p in &snap.products {
        match p.fulfillment_type.trim() {
            FULFILLMENT_AUTO => {
                out.extend(auto_alerts(
                    p,
                    snap.secrets.get(&p.id).unwrap_or(&empty),
                    low,
                ));
            }
            FULFILLMENT_MANUAL => out.extend(manual_alerts(p, low)),
            FULFILLMENT_UPSTREAM => out.extend(upstream_alerts(
                p,
                snap.upstream.get(&p.id).map_or(&[][..], Vec::as_slice),
                low,
            )),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sku(id: Id, code: &str, stock: i64) -> InvSku {
        InvSku {
            id,
            sku_code: code.into(),
            spec_values: Some(json!({"size": code})),
            manual_stock_total: stock,
        }
    }

    fn product(id: Id, kind: &str, stock: i64, skus: Vec<InvSku>) -> InvProduct {
        InvProduct {
            id,
            title: Some(json!({"zh-CN": format!("商品{id}")})),
            fulfillment_type: kind.into(),
            manual_stock_total: stock,
            skus,
        }
    }

    fn counts(pairs: &[(Id, i64)]) -> HashMap<Id, i64> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn classify_boundaries() {
        assert_eq!(classify(0, 5), Some(ALERT_OUT_OF_STOCK));
        assert_eq!(classify(-3, 5), Some(ALERT_OUT_OF_STOCK));
        assert_eq!(classify(5, 5), Some(ALERT_LOW_STOCK));
        assert_eq!(classify(6, 5), None);
    }

    // DB-07: two SKUs with 3 and 0 available secrets are told apart by sku_id.
    #[test]
    fn auto_skus_are_counted_separately() {
        let mut snap = InventorySnapshot::default();
        snap.products.push(product(
            1,
            "auto",
            0,
            vec![sku(11, "A", 0), sku(12, "B", 0)],
        ));
        snap.secrets.insert(1, counts(&[(11, 3)]));
        let alerts = inventory_alerts(&snap, 5);
        let rows: Vec<_> = alerts
            .iter()
            .map(|a| (a.sku_id, a.alert_type.as_str(), a.available_stock))
            .collect();
        assert_eq!(
            rows,
            vec![(11, ALERT_LOW_STOCK, 3), (12, ALERT_OUT_OF_STOCK, 0)]
        );
        let st = stock_stats(&snap, 5);
        assert_eq!(st.auto_available_secrets, 3);
        assert_eq!((st.low_stock_products, st.out_of_stock_products), (1, 0));
        assert_eq!((st.low_stock_skus, st.out_of_stock_skus), (1, 1));
    }

    #[test]
    fn legacy_secrets_go_to_default_sku() {
        let mut snap = InventorySnapshot::default();
        snap.products.push(product(
            1,
            "auto",
            0,
            vec![sku(11, "A", 0), sku(12, "default", 0)],
        ));
        snap.secrets.insert(1, counts(&[(0, 10)]));
        let alerts = inventory_alerts(&snap, 5);
        assert_eq!(alerts.len(), 1);
        assert_eq!(
            (alerts[0].sku_id, alerts[0].alert_type.as_str()),
            (11, ALERT_OUT_OF_STOCK)
        );
    }

    // DLV-04: only disabled SKUs hold stock → product-level alert with the total.
    #[test]
    fn inactive_sku_stock_falls_back_to_product_alert() {
        let mut snap = InventorySnapshot::default();
        snap.products.push(product(
            1,
            "auto",
            0,
            vec![sku(11, "A", 0), sku(12, "B", 0)],
        ));
        snap.secrets.insert(1, counts(&[(99, 2)]));
        let alerts = inventory_alerts(&snap, 5);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].sku_id, 0);
        assert_eq!(alerts[0].alert_type, ALERT_LOW_STOCK);
        assert_eq!(alerts[0].available_stock, 2);
        // Stats ignore secrets of disabled SKUs.
        let st = stock_stats(&snap, 5);
        assert_eq!(st.auto_available_secrets, 0);
        assert_eq!(st.out_of_stock_products, 1);
    }

    // MISC-03 (3): a disabled SKU with 100 secrets and an enabled SKU with 0 → out of stock.
    #[test]
    fn disabled_sku_stock_does_not_hide_sold_out() {
        let mut snap = InventorySnapshot::default();
        snap.products
            .push(product(1, "auto", 0, vec![sku(11, "A", 0)]));
        snap.secrets.insert(1, counts(&[(99, 100)]));
        let st = stock_stats(&snap, 5);
        assert_eq!(st.out_of_stock_products, 1);
        assert_eq!(st.out_of_stock_skus, 1);
        assert_eq!(st.auto_available_secrets, 0);
    }

    #[test]
    fn single_sku_absorbs_inactive_stock() {
        let mut snap = InventorySnapshot::default();
        snap.products
            .push(product(1, "auto", 0, vec![sku(11, "A", 0)]));
        snap.secrets.insert(1, counts(&[(11, 4), (99, 3)]));
        let alerts = inventory_alerts(&snap, 5);
        assert!(alerts.is_empty(), "4 + 3 = 7 > threshold");
    }

    // ORD-06: -1 means unlimited; negative remaining counts as 0.
    #[test]
    fn manual_stock_semantics() {
        let mut snap = InventorySnapshot::default();
        snap.products.push(product(1, "manual", -1, vec![]));
        snap.products.push(product(2, "manual", 3, vec![]));
        snap.products.push(product(
            3,
            "manual",
            0,
            vec![sku(31, "A", -1), sku(32, "B", 0)],
        ));
        snap.products.push(product(
            4,
            "manual",
            0,
            vec![sku(41, "A", -5), sku(42, "B", 100)],
        ));
        let st = stock_stats(&snap, 5);
        assert_eq!(st.manual_available_units, 103);
        assert_eq!((st.low_stock_products, st.out_of_stock_products), (1, 0));
        assert_eq!(st.out_of_stock_skus, 1);
        let alerts = inventory_alerts(&snap, 5);
        let rows: Vec<_> = alerts
            .iter()
            .map(|a| (a.product_id, a.sku_id, a.available_stock))
            .collect();
        assert_eq!(rows, vec![(2, 0, 3), (3, 32, 0), (4, 41, 0)]);
    }

    // MISC-05: upstream stock 0 → out of stock, 2 → low, inactive mapping → no alert.
    #[test]
    fn upstream_uses_mapped_stock() {
        let mut snap = InventorySnapshot::default();
        snap.products.push(product(
            1,
            "upstream",
            0,
            vec![
                sku(11, "A", 0),
                sku(12, "B", 0),
                sku(13, "C", 0),
                sku(14, "D", 0),
            ],
        ));
        snap.products.push(product(2, "upstream", 0, vec![]));
        snap.upstream.insert(
            1,
            vec![
                UpstreamSku {
                    local_sku_id: 11,
                    upstream_stock: 2,
                    upstream_is_active: true,
                },
                UpstreamSku {
                    local_sku_id: 12,
                    upstream_stock: 0,
                    upstream_is_active: false,
                },
                UpstreamSku {
                    local_sku_id: 14,
                    upstream_stock: 0,
                    upstream_is_active: true,
                },
            ],
        );
        snap.upstream.insert(
            2,
            vec![
                UpstreamSku {
                    local_sku_id: 21,
                    upstream_stock: -4,
                    upstream_is_active: true,
                },
                UpstreamSku {
                    local_sku_id: 22,
                    upstream_stock: 50,
                    upstream_is_active: false,
                },
            ],
        );
        let alerts = inventory_alerts(&snap, 5);
        let rows: Vec<_> = alerts
            .iter()
            .map(|a| {
                (
                    a.product_id,
                    a.sku_id,
                    a.alert_type.as_str(),
                    a.available_stock,
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                (1, 11, ALERT_LOW_STOCK, 2),
                (1, 14, ALERT_OUT_OF_STOCK, 0),
                (2, 0, ALERT_OUT_OF_STOCK, 0)
            ]
        );
        assert_eq!(
            stock_stats(&snap, 5),
            StockStats::default(),
            "upstream is not in the KPIs"
        );
    }

    #[test]
    fn alert_json_shape() {
        let p = product(7, "manual", 2, vec![]);
        let json = serde_json::to_value(alert(&p, None, "manual", ALERT_LOW_STOCK, 2)).unwrap();
        assert_eq!(
            json,
            json!({
                "product_id": 7,
                "product_title": {"zh-CN": "商品7"},
                "fulfillment_type": "manual",
                "alert_type": "low_stock_products",
                "available_stock": 2
            })
        );
        let s = sku(9, "SKU-9", 1);
        let json = serde_json::to_value(alert(&p, Some(&s), "auto", ALERT_LOW_STOCK, 1)).unwrap();
        assert_eq!(json["sku_id"], 9);
        assert_eq!(json["sku_code"], "SKU-9");
        assert_eq!(json["sku_spec_values"]["size"], "SKU-9");
    }
}
