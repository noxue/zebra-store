//! Optional product browse attributes. These never identify a purchasable SKU.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize};
use zs_shared::i18n::{LOCALES, LocalizedText};

/// Product-level dimensions. Durations belong to real SKU specifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FacetKey {
    Family,
    Project,
    Country,
    City,
    IpType,
    Line,
}

impl FacetKey {
    pub const ALL: [Self; 6] = [
        Self::Family,
        Self::Project,
        Self::Country,
        Self::City,
        Self::IpType,
        Self::Line,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Family => "family",
            Self::Project => "project",
            Self::Country => "country",
            Self::City => "city",
            Self::IpType => "ip_type",
            Self::Line => "line",
        }
    }
}

/// Bounded opaque identity, independent of a translated or mutable label.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FacetValueId(String);

pub const FACET_SCHEMA_VERSION: u32 = 1;

const MAX_VALUE_ID_BYTES: usize = 128;
const MAX_LABEL_CHARACTERS: usize = 160;
const MAX_PRODUCT_FACETS: usize = 6;

/// Stable validation categories; errors never include input or supplier content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FacetError {
    #[error("invalid_catalog_facet_value_id")]
    InvalidValueId,
    #[error("invalid_catalog_facet_label")]
    InvalidLabel,
    #[error("duplicate_catalog_facet_dimension")]
    DuplicateDimension,
    #[error("too_many_catalog_facets")]
    TooManyFacets,
}

impl FacetValueId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for FacetValueId {
    type Error = FacetError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value.len() > MAX_VALUE_ID_BYTES
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.'))
        {
            return Err(FacetError::InvalidValueId);
        }
        Ok(Self(value))
    }
}

impl From<FacetValueId> for String {
    fn from(value: FacetValueId) -> Self {
        value.0
    }
}

/// A single product's value for one dimension; it has no pricing authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Facet {
    pub key: FacetKey,
    pub value_id: FacetValueId,
    pub label: LocalizedText,
}

/// Validated canonical set. Existing products may have an empty set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct FacetSet(Vec<Facet>);

impl FacetSet {
    pub fn entries(&self) -> &[Facet] {
        &self.0
    }
}

impl TryFrom<Vec<Facet>> for FacetSet {
    type Error = FacetError;

    fn try_from(mut values: Vec<Facet>) -> Result<Self, Self::Error> {
        if values.len() > MAX_PRODUCT_FACETS {
            return Err(FacetError::TooManyFacets);
        }
        let mut keys = BTreeSet::new();
        for value in &values {
            if !keys.insert(value.key) {
                return Err(FacetError::DuplicateDimension);
            }
            // LocalizedText preserves extension locales elsewhere; this versioned
            // browse contract deliberately accepts only the storefront's locales.
            let labels =
                serde_json::to_value(&value.label).map_err(|_| FacetError::InvalidLabel)?;
            let labels = labels.as_object().ok_or(FacetError::InvalidLabel)?;
            if labels.is_empty()
                || value.label.is_blank()
                || labels.iter().any(|(locale, text)| {
                    !LOCALES.contains(&locale.as_str())
                        || text.as_str().is_none_or(|text| {
                            text.chars().count() > MAX_LABEL_CHARACTERS
                                || text.chars().any(char::is_control)
                        })
                })
            {
                return Err(FacetError::InvalidLabel);
            }
        }
        values.sort_by_key(|value| value.key);
        Ok(Self(values))
    }
}

impl<'de> Deserialize<'de> for FacetSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(Vec::<Facet>::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A product's revision is retained even after all browse attributes are cleared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FacetDocument {
    pub schema_version: u32,
    pub revision: u64,
    pub facets: FacetSet,
}

/// Persistence never changes prices, SKU identity or purchase/stock permissions.
#[async_trait::async_trait]
pub trait FacetRepo: Send + Sync {
    async fn get(&self, product_id: crate::Id) -> crate::Result<FacetDocument>;
    async fn replace(
        &self,
        product_id: crate::Id,
        expected_revision: u64,
        facets: &FacetSet,
    ) -> crate::Result<FacetDocument>;
}

/// Single selections whose identities never depend on display text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FacetFilters(BTreeMap<FacetKey, FacetValueId>);

impl FacetFilters {
    pub fn entries(&self) -> impl Iterator<Item = (&FacetKey, &FacetValueId)> {
        self.0.iter()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn without(&self, key: FacetKey) -> Self {
        let mut values = self.clone();
        values.0.remove(&key);
        values
    }
}

impl TryFrom<Vec<(FacetKey, FacetValueId)>> for FacetFilters {
    type Error = FacetError;
    fn try_from(values: Vec<(FacetKey, FacetValueId)>) -> Result<Self, Self::Error> {
        let mut result = BTreeMap::new();
        for (key, value) in values {
            if result.insert(key, value).is_some() {
                return Err(FacetError::DuplicateDimension);
            }
        }
        Ok(Self(result))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FacetOption {
    pub value_id: FacetValueId,
    pub label: LocalizedText,
    /// Visible products, never supplier stock or number counts.
    pub product_count: u64,
}

#[derive(Debug, Clone)]
pub struct FacetOptionsQuery {
    pub dimension: FacetKey,
    pub search: String,
    pub exact: Option<FacetValueId>,
    pub page: zs_shared::page::PageRequest,
}

#[async_trait::async_trait]
pub trait FacetBrowseRepo: Send + Sync {
    async fn options(
        &self,
        filter: &super::product::ProductFilter,
        query: &FacetOptionsQuery,
    ) -> crate::Result<zs_shared::page::Page<FacetOption>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn facet(key: FacetKey, id: &str, label: &str) -> Result<Facet, FacetError> {
        Ok(Facet {
            key,
            value_id: id.to_owned().try_into()?,
            label: LocalizedText::single(label),
        })
    }

    #[test]
    fn selections_reject_duplicate_dimensions_and_remove_only_the_requested_dimension() -> TestResult
    {
        let selected = FacetFilters::try_from(vec![
            (FacetKey::Country, "000A".to_owned().try_into()?),
            (FacetKey::City, "city:01".to_owned().try_into()?),
        ])?;
        let alternate = selected.without(FacetKey::Country);
        assert_eq!(
            alternate
                .entries()
                .map(|(key, id)| (*key, id.as_str()))
                .collect::<Vec<_>>(),
            vec![(FacetKey::City, "city:01")]
        );
        assert_eq!(selected.entries().count(), 2);
        assert!(
            selected
                .without(FacetKey::Country)
                .without(FacetKey::City)
                .is_empty()
        );
        assert_eq!(
            FacetFilters::try_from(vec![
                (FacetKey::City, "A".to_owned().try_into()?),
                (FacetKey::City, "B".to_owned().try_into()?)
            ]),
            Err(FacetError::DuplicateDimension)
        );
        Ok(())
    }

    // MCY-04: names and display order never become identity.
    #[test]
    fn renaming_a_label_keeps_the_exact_case_sensitive_value_identity() -> TestResult {
        let original = facet(FacetKey::City, "provider:city:000AbC", "Original city")?;
        let renamed = facet(FacetKey::City, "provider:city:000AbC", "Renamed city")?;
        assert_eq!(original.value_id, renamed.value_id);
        assert_eq!(renamed.value_id.as_str(), "provider:city:000AbC");
        assert_ne!(original.label, renamed.label);
        let different = facet(FacetKey::City, "provider:city:000abc", "Renamed city")?;
        assert_ne!(renamed.value_id, different.value_id);
        Ok(())
    }

    #[test]
    fn invalid_value_ids_are_rejected_without_trimming_or_url_interpretation() {
        for id in [
            "",
            " padded",
            "padded ",
            "city/name",
            "value?key=secret",
            "line\nbreak",
            "城市",
        ] {
            assert_eq!(
                FacetValueId::try_from(id.to_owned()),
                Err(FacetError::InvalidValueId)
            );
        }
        assert!(FacetValueId::try_from("x".repeat(128)).is_ok());
        assert_eq!(
            FacetValueId::try_from("x".repeat(129)),
            Err(FacetError::InvalidValueId)
        );
    }

    #[test]
    fn canonical_order_is_independent_of_source_display_order() -> TestResult {
        let set = FacetSet::try_from(vec![
            facet(FacetKey::Line, "line-1", "Line")?,
            facet(FacetKey::Country, "country-1", "Country")?,
            facet(FacetKey::Family, "static_ip", "Static IP")?,
        ])?;
        assert_eq!(
            set.entries().iter().map(|f| f.key).collect::<Vec<_>>(),
            vec![FacetKey::Family, FacetKey::Country, FacetKey::Line]
        );
        let encoded = serde_json::to_string(&set)?;
        let decoded: FacetSet = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, set);
        Ok(())
    }

    #[test]
    fn conflicting_product_dimensions_cannot_invent_multiple_sku_combinations() -> TestResult {
        let values = vec![
            facet(FacetKey::Country, "a", "A")?,
            facet(FacetKey::Country, "b", "B")?,
        ];
        assert_eq!(
            FacetSet::try_from(values),
            Err(FacetError::DuplicateDimension)
        );
        assert_eq!(serde_json::to_string(&FacetSet::default())?, "[]");
        assert_eq!(serde_json::from_str::<FacetSet>("[]")?, FacetSet::default());
        Ok(())
    }

    #[test]
    fn labels_are_bounded_plain_text_in_supported_locales() -> TestResult {
        for text in [
            String::new(),
            "  ".into(),
            "x".repeat(161),
            "line\nbreak".into(),
        ] {
            assert_eq!(
                FacetSet::try_from(vec![facet(FacetKey::Country, "a", &text)?]),
                Err(FacetError::InvalidLabel)
            );
        }
        let mut entry = facet(FacetKey::Country, "a", &"城".repeat(160))?;
        assert!(FacetSet::try_from(vec![entry.clone()]).is_ok());
        entry.label.set("unknown-locale", "Country");
        assert_eq!(
            FacetSet::try_from(vec![entry]),
            Err(FacetError::InvalidLabel)
        );
        Ok(())
    }

    #[test]
    fn unknown_dimensions_fields_and_non_string_ids_cannot_bypass_validation() {
        for raw in [
            r#"[{"key":"duration","value_id":"30","label":{"zh-CN":"30 days"}}]"#,
            r#"[{"key":"country","value_id":123,"label":{"zh-CN":"Country"}}]"#,
            r#"[{"key":"country","value_id":"x","label":{"zh-CN":"Country"},"price_amount":"1.00"}]"#,
            r#"[{"key":"country","value_id":"x","label":{}}]"#,
            r#"[{"key":"country","value_id":"x","label":{"zh-CN":""}}]"#,
        ] {
            assert!(serde_json::from_str::<FacetSet>(raw).is_err());
        }
    }

    #[test]
    fn oversized_metadata_is_rejected_as_a_whole() -> TestResult {
        let entry = facet(FacetKey::Family, "sms", "Messaging")?;
        assert_eq!(
            FacetSet::try_from(vec![entry; 7]),
            Err(FacetError::TooManyFacets)
        );
        Ok(())
    }
}
