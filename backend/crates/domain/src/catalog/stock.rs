//! Stock semantics shared by the storefront, channel and upstream catalogs.
//!
//! Manual stock uses "remaining stock" semantics: `manual_stock_total` is the
//! quantity still sellable and [`MANUAL_STOCK_UNLIMITED`] (`-1`) means unlimited
//! (ORD-06). Auto stock is the number of `available` card secrets.

use serde::{Deserialize, Serialize};

/// `manual_stock_total` value meaning "unlimited" (original `constants.ManualStockUnlimited`).
pub const MANUAL_STOCK_UNLIMITED: i32 = -1;

/// Low-stock threshold of the storefront and channel catalogs (original `StorefrontLowStockThreshold`).
pub const STOREFRONT_LOW_STOCK_THRESHOLD: i64 = 5;
/// Low-stock threshold of the upstream API (original `UpstreamLowStockThreshold`).
pub const UPSTREAM_LOW_STOCK_THRESHOLD: i64 = 20;

/// Public stock status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockStatus {
    Unlimited,
    InStock,
    LowStock,
    OutOfStock,
}

impl StockStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unlimited => "unlimited",
            Self::InStock => "in_stock",
            Self::LowStock => "low_stock",
            Self::OutOfStock => "out_of_stock",
        }
    }
}

/// How a product exposes its stock publicly (`products.stock_display_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockDisplayMode {
    #[default]
    Exact,
    Status,
    Range,
    Hidden,
}

impl StockDisplayMode {
    /// Strict parse used on writes: blank → exact, unknown → `None` (rejected).
    pub fn parse_input(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "exact" => Some(Self::Exact),
            "status" => Some(Self::Status),
            "range" => Some(Self::Range),
            "hidden" => Some(Self::Hidden),
            _ => None,
        }
    }

    /// Lenient parse used on reads: unknown values fall back to exact.
    pub fn from_stored(raw: &str) -> Self {
        match raw.trim() {
            "status" => Self::Status,
            "range" => Self::Range,
            "hidden" => Self::Hidden,
            _ => Self::Exact,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Status => "status",
            Self::Range => "range",
            Self::Hidden => "hidden",
        }
    }
}

/// Maps an available quantity to a [`StockStatus`] for one consumer context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockPolicy {
    pub low_stock_threshold: i64,
}

impl StockPolicy {
    pub const STOREFRONT: Self = Self {
        low_stock_threshold: STOREFRONT_LOW_STOCK_THRESHOLD,
    };
    pub const UPSTREAM: Self = Self {
        low_stock_threshold: UPSTREAM_LOW_STOCK_THRESHOLD,
    };

    /// Status for a quantity; negative quantities mean unlimited.
    pub fn status(self, quantity: i64) -> StockStatus {
        let threshold = self.low_stock_threshold.max(0);
        match quantity {
            q if q < 0 => StockStatus::Unlimited,
            0 => StockStatus::OutOfStock,
            q if q <= threshold => StockStatus::LowStock,
            _ => StockStatus::InStock,
        }
    }

    /// Keeps an already-resolved status; negative quantities always normalise to unlimited.
    pub fn normalize_status(self, status: Option<StockStatus>, quantity: i64) -> StockStatus {
        if quantity < 0 {
            return StockStatus::Unlimited;
        }
        status.unwrap_or_else(|| self.status(quantity))
    }

    /// Builds the public display metadata of a stock quantity.
    pub fn display(
        self,
        mode: StockDisplayMode,
        status: Option<StockStatus>,
        quantity: i64,
    ) -> StockDisplay {
        let status = self.normalize_status(status, quantity);
        let mut view = StockDisplay {
            mode,
            status,
            display: status.as_str().to_owned(),
            range_min: None,
            range_max: None,
            quantity_hidden: mode != StockDisplayMode::Exact,
        };
        let has_stock = matches!(status, StockStatus::InStock | StockStatus::LowStock);
        match mode {
            StockDisplayMode::Range if has_stock => {
                let (label, min, max) = stock_range(quantity);
                view.display = label.to_owned();
                view.range_min = Some(min);
                view.range_max = max;
            }
            StockDisplayMode::Hidden if has_stock => view.display = "hidden".to_owned(),
            StockDisplayMode::Exact => view.display = "exact".to_owned(),
            _ => {}
        }
        view
    }
}

/// Public stock display decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockDisplay {
    pub mode: StockDisplayMode,
    pub status: StockStatus,
    pub display: String,
    pub range_min: Option<i32>,
    pub range_max: Option<i32>,
    pub quantity_hidden: bool,
}

/// Maps a positive quantity to a stable public bucket `(label, min, max)`.
pub fn stock_range(quantity: i64) -> (&'static str, i32, Option<i32>) {
    match quantity {
        q if q <= 5 => ("range_1_5", 1, Some(5)),
        q if q <= 20 => ("range_6_20", 6, Some(20)),
        q if q <= 50 => ("range_21_50", 21, Some(50)),
        q if q <= 100 => ("range_51_100", 51, Some(100)),
        _ => ("range_100_plus", 100, None),
    }
}

/// Picks the computed available quantity for a fulfillment type.
pub fn stock_quantity(is_auto: bool, auto_available: i64, manual_available: i32) -> i64 {
    if is_auto {
        auto_available
    } else {
        i64::from(manual_available)
    }
}

/// Hides exact quantities outside `exact` mode while keeping sold-out / unlimited semantics (ORD-08).
pub fn mask_stock(mode: StockDisplayMode, value: i64) -> i64 {
    if mode == StockDisplayMode::Exact {
        return value;
    }
    match value {
        v if v == i64::from(MANUAL_STOCK_UNLIMITED) => v,
        v if v <= 0 => 0,
        _ => 1,
    }
}

/// Hides sold counts outside `exact` mode so the real stock cannot be inferred.
pub fn mask_sold(mode: StockDisplayMode, value: i32) -> i32 {
    if mode == StockDisplayMode::Exact {
        value
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_thresholds_per_context() {
        let s = StockPolicy::STOREFRONT;
        assert_eq!(s.status(-1), StockStatus::Unlimited);
        assert_eq!(s.status(0), StockStatus::OutOfStock);
        assert_eq!(s.status(5), StockStatus::LowStock);
        assert_eq!(s.status(6), StockStatus::InStock);
        assert_eq!(StockPolicy::UPSTREAM.status(20), StockStatus::LowStock);
        assert_eq!(StockPolicy::UPSTREAM.status(21), StockStatus::InStock);
        let negative = StockPolicy {
            low_stock_threshold: -3,
        };
        assert_eq!(negative.status(1), StockStatus::InStock);
    }

    // ORD-08
    #[test]
    fn display_modes_and_ranges() {
        let p = StockPolicy::STOREFRONT;
        let range = p.display(StockDisplayMode::Range, None, 37);
        assert_eq!(range.display, "range_21_50");
        assert_eq!((range.range_min, range.range_max), (Some(21), Some(50)));
        assert!(range.quantity_hidden);
        let top = p.display(StockDisplayMode::Range, None, 150);
        assert_eq!(
            (top.display.as_str(), top.range_min, top.range_max),
            ("range_100_plus", Some(100), None)
        );

        let status = p.display(StockDisplayMode::Status, None, 5);
        assert_eq!(status.display, "low_stock");
        assert!(status.quantity_hidden);

        let hidden = p.display(StockDisplayMode::Hidden, None, 37);
        assert_eq!(hidden.display, "hidden");
        let hidden_out = p.display(StockDisplayMode::Hidden, None, 0);
        assert_eq!(hidden_out.display, "out_of_stock");

        let exact = p.display(StockDisplayMode::Exact, None, 6);
        assert_eq!(exact.display, "exact");
        assert!(!exact.quantity_hidden);

        let unlimited = p.display(StockDisplayMode::Range, Some(StockStatus::InStock), -1);
        assert_eq!(unlimited.status, StockStatus::Unlimited);
        assert_eq!(unlimited.display, "unlimited");
    }

    // ORD-08
    #[test]
    fn masking_hides_exact_values() {
        assert_eq!(stock_quantity(true, 9, 4), 9);
        assert_eq!(stock_quantity(false, 9, 4), 4);
        assert_eq!(mask_stock(StockDisplayMode::Status, 42), 1);
        assert_eq!(mask_stock(StockDisplayMode::Hidden, 37), 1);
        assert_eq!(mask_stock(StockDisplayMode::Status, -1), -1);
        assert_eq!(mask_stock(StockDisplayMode::Range, 0), 0);
        assert_eq!(mask_stock(StockDisplayMode::Exact, 37), 37);
        assert_eq!(mask_sold(StockDisplayMode::Hidden, 17), 0);
        assert_eq!(mask_sold(StockDisplayMode::Exact, 17), 17);
    }

    #[test]
    fn display_mode_parsing() {
        assert_eq!(
            StockDisplayMode::parse_input(""),
            Some(StockDisplayMode::Exact)
        );
        assert_eq!(
            StockDisplayMode::parse_input(" Hidden "),
            Some(StockDisplayMode::Hidden)
        );
        assert_eq!(StockDisplayMode::parse_input("invalid"), None);
        assert_eq!(
            StockDisplayMode::from_stored("bogus"),
            StockDisplayMode::Exact
        );
    }
}
