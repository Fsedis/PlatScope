use std::collections::{HashMap, HashSet};

use chrono::{DateTime, NaiveDate, Utc};
use platscope_domain::{
    CatalogItem, CatalogMetadata, ItemCatalog, MarketOrderType, MarketRecord, MarketVariantKey,
    NormalizedMarketSnapshot, Platform, SnapshotMetadata,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::{ProviderError, ProviderErrorCode, RawMarketDump, RawMetadataCatalog};

#[derive(Debug, Clone, Copy)]
pub struct ValidationProfile {
    pub minimum_catalog_items: usize,
    pub minimum_market_items: usize,
    pub minimum_market_records: usize,
    pub maximum_unknown_item_permyriad: usize,
    pub maximum_source_age_days: Option<i64>,
}

impl ValidationProfile {
    pub const fn production() -> Self {
        Self {
            minimum_catalog_items: 3_000,
            minimum_market_items: 3_000,
            minimum_market_records: 9_000,
            maximum_unknown_item_permyriad: 100,
            maximum_source_age_days: Some(10),
        }
    }

    pub const fn historical() -> Self {
        Self {
            minimum_catalog_items: 3_000,
            minimum_market_items: 3_000,
            minimum_market_records: 9_000,
            maximum_unknown_item_permyriad: 100,
            maximum_source_age_days: None,
        }
    }

    #[cfg(test)]
    const fn fixture() -> Self {
        Self {
            minimum_catalog_items: 1,
            minimum_market_items: 1,
            minimum_market_records: 1,
            maximum_unknown_item_permyriad: 2_500,
            maximum_source_age_days: None,
        }
    }
}

/// Нормализует и валидирует transport catalog.
///
/// # Errors
///
/// Возвращает [`ProviderError`] при invalid JSON, schema drift или semantic validation failure.
pub fn normalize_catalog(
    raw: &RawMetadataCatalog,
    profile: ValidationProfile,
) -> Result<ItemCatalog, ProviderError> {
    let value: Value = serde_json::from_slice(&raw.body).map_err(|error| invalid_json(&error))?;
    let array = value
        .as_array()
        .or_else(|| value.get("data").and_then(Value::as_array))
        .ok_or_else(|| {
            ProviderError::schema_changed("catalog root must be an array or contain data array")
        })?;
    if array.len() < profile.minimum_catalog_items {
        return Err(ProviderError::validation(format!(
            "catalog contains {} items; minimum is {}",
            array.len(),
            profile.minimum_catalog_items
        )));
    }

    let mut items = Vec::with_capacity(array.len());
    let mut ids = HashSet::with_capacity(array.len());
    let mut slugs = HashSet::with_capacity(array.len());
    for value in array {
        let object = object(value, "catalog item")?;
        let item_id = required_string(object, "id")?.to_owned();
        let slug = required_string(object, "slug")?.to_owned();
        let display_name_en = localized_catalog_field(object, "en", "name")
            .ok_or_else(|| ProviderError::schema_changed("catalog item lacks i18n.en.name"))?
            .to_owned();
        let display_name_ru = localized_catalog_field(object, "ru", "name").map(str::to_owned);
        let thumb = localized_catalog_field(object, "en", "thumb").map(str::to_owned);
        let thumb_ru = localized_catalog_field(object, "ru", "thumb").map(str::to_owned);
        if !ids.insert(item_id.clone()) || !slugs.insert(slug.clone()) {
            return Err(ProviderError::validation(
                "catalog contains duplicate id or slug",
            ));
        }

        let bulk_tradable = object
            .get("bulkTradable")
            .map(|value| {
                value.as_bool().ok_or_else(|| {
                    ProviderError::schema_changed("catalog bulkTradable is not boolean")
                })
            })
            .transpose()?
            .unwrap_or(false);
        let max_rank = optional_u16(object, "maxRank")?;
        let subtypes = string_array(object.get("subtypes"), "subtypes")?;
        let tags = string_array(object.get("tags"), "tags")?;
        let game_ref = object
            .get("gameRef")
            .and_then(Value::as_str)
            .map(str::to_owned);
        items.push(CatalogItem {
            item_id,
            slug,
            display_name_en,
            display_name_ru,
            thumb,
            thumb_ru,
            game_ref,
            bulk_tradable,
            max_rank,
            subtypes,
            tags,
        });
    }

    Ok(ItemCatalog {
        metadata: CatalogMetadata {
            provider: raw.provider,
            fetched_at: raw.fetched_at,
            schema_version: 3,
            item_count: items.len() as u64,
            checksum_sha256: checksum(&raw.body),
        },
        items,
    })
}

fn localized_catalog_field<'a>(
    object: &'a Map<String, Value>,
    locale: &str,
    field: &str,
) -> Option<&'a str> {
    object
        .get("i18n")
        .and_then(Value::as_object)
        .and_then(|i18n| i18n.get(locale))
        .and_then(Value::as_object)
        .and_then(|localized| localized.get(field))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

/// Нормализует price dump, связывая каждую запись с независимым catalog по `item_id`.
///
/// # Errors
///
/// Возвращает [`ProviderError`] при invalid JSON, schema drift или semantic validation failure.
#[allow(clippy::too_many_lines)] // Последовательность validation gates читается как единая транзакция.
pub fn normalize_market_dump(
    raw: &RawMarketDump,
    catalog: &ItemCatalog,
    profile: ValidationProfile,
) -> Result<NormalizedMarketSnapshot, ProviderError> {
    if let Some(maximum_age) = profile.maximum_source_age_days {
        let age = Utc::now()
            .date_naive()
            .signed_duration_since(raw.source_date)
            .num_days();
        if !(0..=maximum_age).contains(&age) {
            return Err(ProviderError::validation(format!(
                "source date {} has unsupported age {age} days",
                raw.source_date
            )));
        }
    }
    let value: Value = serde_json::from_slice(&raw.body).map_err(|error| invalid_json(&error))?;
    let root = value
        .as_object()
        .ok_or_else(|| ProviderError::schema_changed("price root must be an object"))?;
    if root.len() < profile.minimum_market_items {
        return Err(ProviderError::validation(format!(
            "price dump contains {} item buckets; minimum is {}",
            root.len(),
            profile.minimum_market_items
        )));
    }

    let by_id: HashMap<&str, &CatalogItem> = catalog
        .items
        .iter()
        .map(|item| (item.item_id.as_str(), item))
        .collect();
    let total_records = root.values().try_fold(0_usize, |count, value| {
        value
            .as_array()
            .map(|records| count.saturating_add(records.len()))
            .ok_or_else(|| ProviderError::schema_changed("price bucket must be an array"))
    })?;
    if total_records < profile.minimum_market_records {
        return Err(ProviderError::validation(format!(
            "price dump contains {total_records} records; minimum is {}",
            profile.minimum_market_records
        )));
    }

    let mut records = Vec::with_capacity(total_records);
    let mut unknown_items = 0_usize;
    let mut identities = HashSet::with_capacity(total_records);
    for (bucket_name, values) in root {
        let values = values
            .as_array()
            .ok_or_else(|| ProviderError::schema_changed("price bucket must be an array"))?;
        for value in values {
            let object = object(value, "market record")?;
            let item_id = required_string(object, "item_id")?;
            let Some(item) = by_id.get(item_id).copied() else {
                unknown_items = unknown_items.saturating_add(1);
                continue;
            };
            let observed_at = required_datetime(object, "datetime")?;
            if observed_at.date_naive() != raw.source_date {
                return Err(ProviderError::validation(format!(
                    "record date {} differs from source date {}",
                    observed_at.date_naive(),
                    raw.source_date
                )));
            }
            let rank = optional_u16(object, "mod_rank")?;
            if rank
                .zip(item.max_rank)
                .is_some_and(|(rank, max)| rank > max)
            {
                return Err(ProviderError::validation(format!(
                    "rank exceeds maxRank for {}",
                    item.slug
                )));
            }
            let subtype = object
                .get("subtype")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let amber_stars = optional_u16(object, "amber_stars")?;
            let cyan_stars = optional_u16(object, "cyan_stars")?;
            if subtype.as_ref().is_some_and(|subtype| {
                !item.subtypes.is_empty() && !item.subtypes.contains(subtype)
            }) {
                return Err(ProviderError::validation(format!(
                    "unknown subtype for {}",
                    item.slug
                )));
            }
            let order_type = parse_order_type(required_string(object, "order_type")?)?;
            let key = MarketVariantKey::new(item.slug.clone(), Platform::Pc, rank, subtype.clone())
                .map_err(|error| ProviderError::validation(error.to_string()))?
                .with_stars(amber_stars, cyan_stars);
            let identity = (
                item.slug.clone(),
                rank,
                subtype.clone(),
                amber_stars,
                cyan_stars,
                order_type,
            );
            if !identities.insert(identity) {
                return Err(ProviderError::validation(format!(
                    "duplicate market variant for {}",
                    item.slug
                )));
            }

            let record = MarketRecord {
                key,
                external_item_id: item_id.to_owned(),
                display_name_en: bucket_name.clone(),
                observed_at,
                order_type,
                median: Some(required_number(object, "median")?),
                average: optional_number(object, "avg_price")?,
                min_price: optional_number(object, "min_price")?,
                max_price: optional_number(object, "max_price")?,
                volume: required_number(object, "volume")?,
                raw_json: serde_json::to_string(value).map_err(|error| invalid_json(&error))?,
            };
            record
                .validate()
                .map_err(|error| ProviderError::validation(error.to_string()))?;
            records.push(record);
        }
    }

    if unknown_items.saturating_mul(10_000)
        > total_records.saturating_mul(profile.maximum_unknown_item_permyriad)
    {
        return Err(ProviderError::validation(format!(
            "unknown item count {unknown_items}/{total_records} exceeds configured ratio"
        )));
    }

    Ok(NormalizedMarketSnapshot {
        metadata: SnapshotMetadata {
            provider: raw.provider,
            source_date: raw.source_date,
            fetched_at: raw.fetched_at,
            schema_version: 1,
            item_count: root.len() as u64,
            record_count: records.len() as u64,
            checksum_sha256: checksum(&raw.body),
        },
        records,
    })
}

pub(crate) fn extract_source_date(body: &[u8]) -> Result<NaiveDate, ProviderError> {
    let value: Value = serde_json::from_slice(body).map_err(|error| invalid_json(&error))?;
    let root = value
        .as_object()
        .ok_or_else(|| ProviderError::schema_changed("price root must be an object"))?;
    let record = root
        .values()
        .find_map(|bucket| bucket.as_array().and_then(|records| records.first()))
        .and_then(Value::as_object)
        .ok_or_else(|| ProviderError::schema_changed("price dump has no records"))?;
    required_datetime(record, "datetime").map(|datetime| datetime.date_naive())
}

fn object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>, ProviderError> {
    value
        .as_object()
        .ok_or_else(|| ProviderError::schema_changed(format!("{context} must be an object")))
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, ProviderError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ProviderError::schema_changed(format!("missing string field {field}")))
}

fn required_datetime(
    object: &Map<String, Value>,
    field: &str,
) -> Result<DateTime<Utc>, ProviderError> {
    required_string(object, field)?
        .parse::<DateTime<Utc>>()
        .map_err(|_| ProviderError::schema_changed(format!("invalid RFC3339 field {field}")))
}

fn required_number(object: &Map<String, Value>, field: &str) -> Result<f64, ProviderError> {
    optional_number(object, field)?
        .ok_or_else(|| ProviderError::schema_changed(format!("missing numeric field {field}")))
}

fn optional_number(object: &Map<String, Value>, field: &str) -> Result<Option<f64>, ProviderError> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let number = value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
        .ok_or_else(|| ProviderError::schema_changed(format!("invalid numeric field {field}")))?;
    if !number.is_finite() || number < 0.0 {
        return Err(ProviderError::validation(format!(
            "field {field} must be finite and non-negative"
        )));
    }
    Ok(Some(number))
}

fn optional_u16(object: &Map<String, Value>, field: &str) -> Result<Option<u16>, ProviderError> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let number = value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
        .and_then(|number| u16::try_from(number).ok())
        .ok_or_else(|| ProviderError::schema_changed(format!("invalid integer field {field}")))?;
    Ok(Some(number))
}

fn string_array(value: Option<&Value>, field: &str) -> Result<Vec<String>, ProviderError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| ProviderError::schema_changed(format!("field {field} must be an array")))?
        .iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                ProviderError::schema_changed(format!("{field} must contain strings"))
            })
        })
        .collect()
}

fn parse_order_type(value: &str) -> Result<MarketOrderType, ProviderError> {
    match value {
        "closed" => Ok(MarketOrderType::Closed),
        "buy" => Ok(MarketOrderType::Buy),
        "sell" => Ok(MarketOrderType::Sell),
        _ => Err(ProviderError::schema_changed(format!(
            "unknown order_type {value}"
        ))),
    }
}

fn checksum(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))
}

fn invalid_json(error: &serde_json::Error) -> ProviderError {
    ProviderError::new(ProviderErrorCode::InvalidJson, error.to_string(), false)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use platscope_domain::ProviderId;

    use super::*;

    const CATALOG: &[u8] = br#"[
      {"id":"normal","slug":"normal_item","gameRef":"/Normal","tags":["weapon"],"i18n":{"en":{"name":"Normal Item","thumb":"items/images/en/thumbs/normal.png"},"ru":{"name":"\u041e\u0431\u044b\u0447\u043d\u044b\u0439 \u043f\u0440\u0435\u0434\u043c\u0435\u0442","thumb":"items/images/ru/thumbs/normal.png"}}},
      {"id":"ranked","slug":"ranked_mod","tags":["mod"],"bulkTradable":true,"maxRank":10,"i18n":{"en":{"name":"Ranked Mod"}}},
      {"id":"relic","slug":"axi_test_relic","tags":["relic"],"subtypes":["intact","radiant"],"i18n":{"en":{"name":"Axi Test Relic"}}}
    ]"#;

    const PRICES: &[u8] = br#"{
      "Normal Item":[{"datetime":"2026-08-26T00:00:00Z","volume":"9","min_price":25,"max_price":33,"avg_price":29,"median":"30","item_id":"normal","order_type":"closed"}],
      "Ranked Mod":[{"datetime":"2026-08-26T00:00:00Z","volume":3,"median":12,"mod_rank":10,"item_id":"ranked","order_type":"sell"}],
      "Axi Test Relic":[{"datetime":"2026-08-26T00:00:00Z","volume":4,"median":7,"subtype":"radiant","item_id":"relic","order_type":"buy"}]
    }"#;

    fn catalog() -> ItemCatalog {
        normalize_catalog(
            &RawMetadataCatalog {
                provider: ProviderId::RelicsRun,
                fetched_at: Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).unwrap(),
                body: CATALOG.to_vec(),
            },
            ValidationProfile::fixture(),
        )
        .expect("fixture catalog")
    }

    #[test]
    fn catalog_preserves_market_thumbnail_path() {
        let item = catalog()
            .items
            .into_iter()
            .find(|item| item.slug == "normal_item")
            .expect("normal fixture item");
        assert_eq!(
            item.thumb.as_deref(),
            Some("items/images/en/thumbs/normal.png")
        );
        assert_eq!(item.display_name_ru.as_deref(), Some("Обычный предмет"));
        assert_eq!(
            item.thumb_ru.as_deref(),
            Some("items/images/ru/thumbs/normal.png")
        );
        assert_eq!(catalog().metadata.schema_version, 3);
    }

    #[test]
    fn accepts_wfm_v2_data_envelope() {
        let body = br#"{
          "apiVersion":"2.0",
          "data":[{"id":"normal","slug":"normal_item","i18n":{"en":{"name":"Normal Item"},"ru":{"name":"\u041e\u0431\u044b\u0447\u043d\u044b\u0439 \u043f\u0440\u0435\u0434\u043c\u0435\u0442"}}}]
        }"#;
        let catalog = normalize_catalog(
            &RawMetadataCatalog {
                provider: ProviderId::WarframeMarket,
                fetched_at: Utc.with_ymd_and_hms(2026, 8, 29, 0, 0, 0).unwrap(),
                body: body.to_vec(),
            },
            ValidationProfile::fixture(),
        )
        .expect("WFM v2 catalog envelope");

        assert_eq!(catalog.items.len(), 1);
        assert_eq!(
            catalog.items[0].display_name_ru.as_deref(),
            Some("Обычный предмет")
        );
    }

    #[test]
    fn catalog_preserves_bulk_tradable_flag() {
        let item = catalog()
            .items
            .into_iter()
            .find(|item| item.slug == "ranked_mod")
            .expect("ranked fixture item");
        assert!(item.bulk_tradable);
    }

    #[test]
    fn parses_normal_ranked_and_subtyped_records() {
        let snapshot = normalize_market_dump(
            &RawMarketDump {
                provider: ProviderId::RelicsRun,
                source_date: NaiveDate::from_ymd_opt(2026, 8, 26).unwrap(),
                fetched_at: Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).unwrap(),
                body: PRICES.to_vec(),
            },
            &catalog(),
            ValidationProfile::fixture(),
        )
        .expect("fixture prices");

        assert_eq!(snapshot.records.len(), 3);
        let normal = snapshot
            .records
            .iter()
            .find(|record| record.key.slug == "normal_item")
            .expect("normal record");
        assert!((normal.volume - 9.0).abs() < f64::EPSILON);
        assert!(
            snapshot
                .records
                .iter()
                .any(|record| record.key.rank == Some(10))
        );
        assert!(
            snapshot
                .records
                .iter()
                .any(|record| record.key.subtype.as_deref() == Some("radiant"))
        );
    }

    #[test]
    fn rejects_date_mismatch_without_partial_snapshot() {
        let error = normalize_market_dump(
            &RawMarketDump {
                provider: ProviderId::RelicsRun,
                source_date: NaiveDate::from_ymd_opt(2026, 8, 25).unwrap(),
                fetched_at: Utc::now(),
                body: PRICES.to_vec(),
            },
            &catalog(),
            ValidationProfile::fixture(),
        )
        .expect_err("mismatched date rejected");
        assert_eq!(error.code, ProviderErrorCode::ValidationFailed);
    }

    #[test]
    fn rejects_invalid_catalog_root() {
        let error = normalize_catalog(
            &RawMetadataCatalog {
                provider: ProviderId::RelicsRun,
                fetched_at: Utc::now(),
                body: b"{}".to_vec(),
            },
            ValidationProfile::fixture(),
        )
        .expect_err("object is not a catalog");
        assert_eq!(error.code, ProviderErrorCode::UpstreamSchemaChanged);
    }

    #[test]
    fn historical_profile_keeps_validation_limits_without_rejecting_old_dates() {
        let current = ValidationProfile::production();
        let historical = ValidationProfile::historical();

        assert_eq!(
            historical.minimum_catalog_items,
            current.minimum_catalog_items
        );
        assert_eq!(
            historical.minimum_market_items,
            current.minimum_market_items
        );
        assert_eq!(
            historical.minimum_market_records,
            current.minimum_market_records
        );
        assert_eq!(
            historical.maximum_unknown_item_permyriad,
            current.maximum_unknown_item_permyriad
        );
        assert_eq!(current.maximum_source_age_days, Some(10));
        assert_eq!(historical.maximum_source_age_days, None);
    }
}
