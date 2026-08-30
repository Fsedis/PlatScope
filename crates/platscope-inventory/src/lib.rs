#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::BuildHasher;

use chrono::{DateTime, Utc};
use platscope_domain::{
    CatalogItem, EquipmentKind, EquippedModInstance, InventoryItem, InventoryModPlacement,
    InventoryResolution, InventorySnapshotMetadata, InventorySource, ItemCatalog, MarketVariantKey,
    Platform, PlayerInventory, ResolvedInventoryItem, ResolvedInventorySnapshot,
    ResolvedModPlacement, Tradeability,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const IMPORT_SCHEMA_VERSION: u32 = 1;
pub const COMPANION_SCHEMA_VERSION: u32 = 1;
pub const MAX_IMPORT_BYTES: usize = 8 * 1024 * 1024;
const OVERWOLF_COMPANION_PRODUCER: &str = "platscope-overwolf-companion";
const WARFRAME_OVERWOLF_GAME_ID: u32 = 8_954;
const MAX_IMPORT_ITEMS: usize = 100_000;
// Long-lived accounts can legitimately hold more than one million common
// resources. Keep arithmetic bounded without rejecting real DE snapshots.
const MAX_QUANTITY: u32 = 100_000_000;
const MAX_IDENTIFIER_LENGTH: usize = 256;
const MAX_SUBTYPE_LENGTH: usize = 64;
const MAX_RANK: u16 = 100;
const MAX_HELPER_DEPTH: usize = 64;
const MAX_HELPER_NODES: usize = 250_000;
const MAX_EQUIPMENT_CONFIGS: usize = 16;
const MAX_CONFIG_UPGRADES: usize = 64;
const READ_ONLY_INVENTORY_CATEGORIES: [&str; 12] = [
    "MiscItems",
    "Recipes",
    "RawUpgrades",
    "Upgrades",
    "Suits",
    "LongGuns",
    "Pistols",
    "Melee",
    "SpaceGuns",
    "SpaceMelee",
    "Sentinels",
    "SentinelWeapons",
];
const READ_ONLY_EQUIPMENT_CATEGORIES: [(&str, EquipmentKind); 29] = [
    ("Suits", EquipmentKind::Warframe),
    ("LongGuns", EquipmentKind::Primary),
    ("Pistols", EquipmentKind::Secondary),
    ("Melee", EquipmentKind::Melee),
    ("SpecialItems", EquipmentKind::Other),
    ("Sentinels", EquipmentKind::Companion),
    ("SentinelWeapons", EquipmentKind::CompanionWeapon),
    ("SpaceSuits", EquipmentKind::Archwing),
    ("SpaceGuns", EquipmentKind::Archgun),
    ("SpaceMelee", EquipmentKind::Archmelee),
    ("Hoverboards", EquipmentKind::Other),
    ("OperatorAmps", EquipmentKind::Amp),
    ("Antiques", EquipmentKind::Other),
    ("MoaPets", EquipmentKind::Companion),
    ("Scoops", EquipmentKind::Other),
    ("Horses", EquipmentKind::Other),
    ("DrifterGuns", EquipmentKind::Secondary),
    ("DrifterMelee", EquipmentKind::Melee),
    ("Motorcycles", EquipmentKind::Other),
    ("CrewShips", EquipmentKind::Other),
    ("DataKnives", EquipmentKind::Melee),
    ("MechSuits", EquipmentKind::Necramech),
    ("CrewShipHarnesses", EquipmentKind::Other),
    ("KubrowPets", EquipmentKind::Companion),
    ("CrewShipWeapons", EquipmentKind::Other),
    ("CrewShipSalvagedWeapons", EquipmentKind::Other),
    ("OperatorSuits", EquipmentKind::Other),
    ("OperatorMasks", EquipmentKind::Other),
    ("OperatorAccessories", EquipmentKind::Other),
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportDocument {
    schema_version: u32,
    observed_at: DateTime<Utc>,
    item_count: u64,
    items: Vec<ImportItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportItem {
    canonical_game_id: String,
    quantity: u32,
    rank: Option<u16>,
    subtype: Option<String>,
    tradeability: Tradeability,
    #[serde(default)]
    leveled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompanionDocument {
    schema_version: u32,
    producer: String,
    observed_at: DateTime<Utc>,
    game_id: u32,
    feature: String,
    key: String,
    complete: bool,
    value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GroupKey {
    canonical_game_id: String,
    rank: Option<u16>,
    subtype: Option<String>,
}

#[derive(Debug, Default)]
struct GroupQuantities {
    owned: u32,
    tradeable: u32,
    untradeable: u32,
    unknown: u32,
    leveled: u32,
}

#[derive(Debug, Clone)]
struct EquippedModSource {
    canonical_game_id: String,
    rank: u16,
    tradeability: Tradeability,
    placements: Vec<InventoryModPlacement>,
}

/// Разбирает собственный versioned JSON-формат и валидирует coherent snapshot.
///
/// # Errors
///
/// Возвращает [`InventoryError`], если payload превышает лимит, нарушает JSON schema
/// или содержит несогласованные/небезопасные значения.
pub fn parse_platscope_json(raw: &str) -> Result<PlayerInventory, InventoryError> {
    if raw.len() > MAX_IMPORT_BYTES {
        return Err(InventoryError::PayloadTooLarge);
    }
    let document: ImportDocument = serde_json::from_str(raw)?;
    if document.schema_version != IMPORT_SCHEMA_VERSION {
        return Err(InventoryError::UnsupportedSchema(document.schema_version));
    }
    if document.items.len() > MAX_IMPORT_ITEMS {
        return Err(InventoryError::TooManyItems(document.items.len()));
    }
    if document.item_count != document.items.len() as u64 {
        return Err(InventoryError::ItemCountMismatch {
            declared: document.item_count,
            actual: document.items.len(),
        });
    }

    let mut identities = HashSet::with_capacity(document.items.len());
    let mut items = Vec::with_capacity(document.items.len());
    for item in document.items {
        let canonical_game_id = item.canonical_game_id.trim().to_owned();
        if canonical_game_id.is_empty() || canonical_game_id.len() > MAX_IDENTIFIER_LENGTH {
            return Err(InventoryError::InvalidIdentifier);
        }
        if item.quantity == 0 || item.quantity > MAX_QUANTITY {
            return Err(InventoryError::InvalidQuantity(item.quantity));
        }
        if item.rank.is_some_and(|rank| rank > MAX_RANK) {
            return Err(InventoryError::InvalidRank(item.rank.unwrap_or_default()));
        }
        let subtype = item.subtype.map(|value| value.trim().to_lowercase());
        if subtype
            .as_ref()
            .is_some_and(|value| value.is_empty() || value.len() > MAX_SUBTYPE_LENGTH)
        {
            return Err(InventoryError::InvalidSubtype);
        }
        let identity = (
            canonical_game_id.to_lowercase(),
            item.rank,
            subtype.clone(),
            item.tradeability,
            item.leveled,
        );
        if !identities.insert(identity) {
            return Err(InventoryError::DuplicateItem);
        }
        items.push(InventoryItem {
            canonical_game_id,
            quantity: item.quantity,
            rank: item.rank,
            subtype,
            tradeability: item.tradeability,
            leveled: item.leveled,
        });
    }

    Ok(PlayerInventory {
        metadata: InventorySnapshotMetadata {
            source: InventorySource::PlatscopeJson,
            observed_at: document.observed_at,
            schema_version: document.schema_version,
            item_count: document.item_count,
            checksum_sha256: hex::encode(Sha256::digest(raw.as_bytes())),
        },
        items,
        mod_usage_scanned: false,
        equipped_mods: Vec::new(),
    })
}

/// Автоматически определяет собственный `PlatScope` JSON v1 либо внешний helper inventory export.
///
/// # Errors
///
/// Возвращает [`InventoryError`] при превышении лимитов, schema drift или некорректных значениях.
pub fn parse_inventory_json(raw: &str) -> Result<PlayerInventory, InventoryError> {
    if raw.len() > MAX_IMPORT_BYTES {
        return Err(InventoryError::PayloadTooLarge);
    }
    let root: Value = serde_json::from_str(raw)?;
    if root.get("producer").is_some() {
        parse_companion_value(raw, root)
    } else if root.get("schemaVersion").is_some() {
        parse_platscope_json(raw)
    } else {
        parse_helper_value(raw, root)
    }
}

/// Разбирает `inventory.json`, созданный совместимым helper/Overwolf export.
/// Внешний формат не доказывает tradeability, поэтому все строки нормализуются как `unknown`.
///
/// # Errors
///
/// Возвращает [`InventoryError`] при превышении лимитов, schema drift или отсутствии item rows.
pub fn parse_helper_json(raw: &str) -> Result<PlayerInventory, InventoryError> {
    if raw.len() > MAX_IMPORT_BYTES {
        return Err(InventoryError::PayloadTooLarge);
    }
    let root: Value = serde_json::from_str(raw)?;
    parse_helper_value(raw, root)
}

/// Разбирает сырой ответ inventory endpoint, полученный встроенным read-only scanner.
/// Формат проходит те же bounds и нормализацию, что helper export, но сохраняет
/// отдельный доверенный источник для UI, диагностики и истории.
///
/// # Errors
///
/// Возвращает [`InventoryError`] при превышении лимитов, schema drift или
/// отсутствии распознаваемых строк `ItemType`.
pub fn parse_read_only_scan_json(raw: &str) -> Result<PlayerInventory, InventoryError> {
    if raw.len() > MAX_IMPORT_BYTES {
        return Err(InventoryError::PayloadTooLarge);
    }
    let root: Value = serde_json::from_str(raw)?;
    let payload = unwrap_helper_payload(root)?;
    normalize_read_only_payload(raw, &payload)
}

fn normalize_read_only_payload(
    raw: &str,
    payload: &Value,
) -> Result<PlayerInventory, InventoryError> {
    let root = payload
        .get("Inventory")
        .filter(|value| value.is_object())
        .unwrap_or(payload);
    let root = root
        .as_object()
        .ok_or(InventoryError::InvalidHelperField("Inventory"))?;
    let mut items = Vec::new();

    for category in READ_ONLY_INVENTORY_CATEGORIES {
        let Some(entries) = root.get(category) else {
            continue;
        };
        let entries = entries
            .as_array()
            .ok_or(InventoryError::InvalidHelperField("inventory category"))?;
        for entry in entries {
            let map = entry
                .as_object()
                .ok_or(InventoryError::InvalidHelperField("inventory item"))?;
            let Some(item_type) = map.get("ItemType").or_else(|| map.get("Type")) else {
                continue;
            };
            let canonical_game_id = item_type
                .as_str()
                .ok_or(InventoryError::InvalidHelperField("ItemType"))?
                .trim()
                .to_owned();
            if canonical_game_id.is_empty() || canonical_game_id.len() > MAX_IDENTIFIER_LENGTH {
                return Err(InventoryError::InvalidIdentifier);
            }
            let quantity = helper_u32(map.get("ItemCount"), "ItemCount", 1)?;
            if quantity == 0 || quantity > MAX_QUANTITY {
                return Err(InventoryError::InvalidQuantity(quantity));
            }
            let rank = read_only_rank(category, map)?;
            let xp = helper_u32(map.get("XP"), "XP", 0)?;
            items.push(InventoryItem {
                canonical_game_id,
                quantity,
                rank,
                subtype: None,
                tradeability: if xp > 0 {
                    Tradeability::Untradeable
                } else {
                    Tradeability::Tradeable
                },
                leveled: xp > 0 || rank.is_some_and(|value| value > 0),
            });
            if items.len() > MAX_IMPORT_ITEMS {
                return Err(InventoryError::TooManyItems(items.len()));
            }
        }
    }
    if items.is_empty() {
        return Err(InventoryError::HelperItemsMissing);
    }

    let equipped_mods = read_only_equipped_mods(root)?;

    Ok(PlayerInventory {
        metadata: InventorySnapshotMetadata {
            source: InventorySource::ReadOnlyScan,
            observed_at: Utc::now(),
            schema_version: IMPORT_SCHEMA_VERSION,
            item_count: items.len() as u64,
            checksum_sha256: hex::encode(Sha256::digest(raw.as_bytes())),
        },
        items,
        mod_usage_scanned: true,
        equipped_mods,
    })
}

fn read_only_equipped_mods(
    root: &serde_json::Map<String, Value>,
) -> Result<Vec<EquippedModInstance>, InventoryError> {
    let mut instances = read_only_upgrade_instances(root)?;
    attach_equipment_placements(root, &mut instances)?;
    let mut equipped = instances
        .into_values()
        .filter(|source| !source.placements.is_empty())
        .map(|source| EquippedModInstance {
            canonical_game_id: source.canonical_game_id,
            rank: source.rank,
            tradeability: source.tradeability,
            placements: source.placements,
        })
        .collect::<Vec<_>>();
    equipped.sort_by(|left, right| {
        left.canonical_game_id
            .cmp(&right.canonical_game_id)
            .then(left.rank.cmp(&right.rank))
    });
    Ok(equipped)
}

fn read_only_upgrade_instances(
    root: &serde_json::Map<String, Value>,
) -> Result<HashMap<String, EquippedModSource>, InventoryError> {
    let mut instances = HashMap::<String, EquippedModSource>::new();
    if let Some(upgrades) = root.get("Upgrades") {
        let upgrades = upgrades
            .as_array()
            .ok_or(InventoryError::InvalidHelperField("Upgrades"))?;
        for upgrade in upgrades {
            let map = upgrade
                .as_object()
                .ok_or(InventoryError::InvalidHelperField("upgrade"))?;
            let Some(item_id) = helper_reference_id(map.get("ItemId"))? else {
                continue;
            };
            let canonical_game_id = map
                .get("ItemType")
                .or_else(|| map.get("Type"))
                .and_then(Value::as_str)
                .ok_or(InventoryError::InvalidHelperField("ItemType"))?
                .trim()
                .to_owned();
            if canonical_game_id.is_empty() || canonical_game_id.len() > MAX_IDENTIFIER_LENGTH {
                return Err(InventoryError::InvalidIdentifier);
            }
            let rank = read_only_rank("Upgrades", map)?.unwrap_or_default();
            let xp = helper_u32(map.get("XP"), "XP", 0)?;
            let source = EquippedModSource {
                canonical_game_id,
                rank,
                tradeability: if xp > 0 {
                    Tradeability::Untradeable
                } else {
                    Tradeability::Tradeable
                },
                placements: Vec::new(),
            };
            if let Some(previous) = instances.insert(item_id, source.clone())
                && (previous.canonical_game_id != source.canonical_game_id
                    || previous.rank != source.rank
                    || previous.tradeability != source.tradeability)
            {
                return Err(InventoryError::InvalidHelperField("duplicate ItemId"));
            }
        }
    }
    Ok(instances)
}

fn attach_equipment_placements(
    root: &serde_json::Map<String, Value>,
    instances: &mut HashMap<String, EquippedModSource>,
) -> Result<(), InventoryError> {
    for (category, equipment_kind) in READ_ONLY_EQUIPMENT_CATEGORIES {
        let Some(entries) = root.get(category) else {
            continue;
        };
        let entries = entries
            .as_array()
            .ok_or(InventoryError::InvalidHelperField("equipment category"))?;
        for (equipment_index, entry) in entries.iter().enumerate() {
            let map = entry
                .as_object()
                .ok_or(InventoryError::InvalidHelperField("equipment"))?;
            let Some(equipment_game_id) = map
                .get("ItemType")
                .or_else(|| map.get("Type"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if equipment_game_id.len() > MAX_IDENTIFIER_LENGTH {
                return Err(InventoryError::InvalidIdentifier);
            }
            let equipment_custom_name = map
                .get("ItemName")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            if equipment_custom_name
                .as_ref()
                .is_some_and(|name| name.len() > MAX_IDENTIFIER_LENGTH)
            {
                return Err(InventoryError::InvalidHelperField("ItemName"));
            }
            let raw_equipment_id = helper_reference_id(map.get("ItemId"))?
                .unwrap_or_else(|| format!("{category}:{equipment_index}:{equipment_game_id}"));
            let equipment_instance_key = hex::encode(Sha256::digest(raw_equipment_id.as_bytes()));
            let Some(configs) = map.get("Configs") else {
                continue;
            };
            let configs = configs
                .as_array()
                .ok_or(InventoryError::InvalidHelperField("Configs"))?;
            if configs.len() > MAX_EQUIPMENT_CONFIGS {
                return Err(InventoryError::InvalidHelperField("Configs"));
            }
            for (config_index, config) in configs.iter().enumerate() {
                let config = config
                    .as_object()
                    .ok_or(InventoryError::InvalidHelperField("Config"))?;
                let Some(upgrades) = config.get("Upgrades") else {
                    continue;
                };
                let upgrades = upgrades
                    .as_array()
                    .ok_or(InventoryError::InvalidHelperField("Config.Upgrades"))?;
                if upgrades.len() > MAX_CONFIG_UPGRADES {
                    return Err(InventoryError::InvalidHelperField("Config.Upgrades"));
                }
                let config_index = u16::try_from(config_index)
                    .map_err(|_| InventoryError::InvalidHelperField("Config index"))?;
                for upgrade in upgrades {
                    let Some(upgrade_id) = helper_reference_id(Some(upgrade))? else {
                        continue;
                    };
                    // Intrinsic/default upgrades use canonical Lotus paths rather
                    // than inventory instance IDs and cannot be traded.
                    if upgrade_id.starts_with("/Lotus/") {
                        continue;
                    }
                    let Some(source) = instances.get_mut(&upgrade_id) else {
                        continue;
                    };
                    let placement = InventoryModPlacement {
                        equipment_instance_key: equipment_instance_key.clone(),
                        equipment_game_id: equipment_game_id.to_owned(),
                        equipment_custom_name: equipment_custom_name.clone(),
                        equipment_kind,
                        config_index,
                    };
                    if !source.placements.contains(&placement) {
                        source.placements.push(placement);
                    }
                }
            }
        }
    }
    Ok(())
}

fn helper_reference_id(value: Option<&Value>) -> Result<Option<String>, InventoryError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let candidate = match value {
        Value::String(value) => Some(value.as_str()),
        Value::Object(map) => map
            .get("$id")
            .or_else(|| map.get("$oid"))
            .and_then(Value::as_str),
        Value::Null => None,
        _ => return Err(InventoryError::InvalidHelperField("ItemId")),
    };
    let candidate = candidate.map(str::trim).filter(|value| !value.is_empty());
    if candidate.is_some_and(|value| value.len() > MAX_IDENTIFIER_LENGTH) {
        return Err(InventoryError::InvalidHelperField("ItemId"));
    }
    Ok(candidate.map(ToOwned::to_owned))
}

fn read_only_rank(
    category: &str,
    map: &serde_json::Map<String, Value>,
) -> Result<Option<u16>, InventoryError> {
    match category {
        // DE stores unranked stackable mods and arcanes separately from
        // instantiated upgrades. Their omitted rank is an exact rank zero.
        "RawUpgrades" => Ok(Some(0)),
        // Instantiated upgrades encode their rank inside a JSON string. `lvl`
        // is omitted for rank zero, so absence inside a valid fingerprint is
        // also exact rather than unknown.
        "Upgrades" => {
            let fingerprint = map
                .get("UpgradeFingerprint")
                .and_then(Value::as_str)
                .ok_or(InventoryError::InvalidHelperField("UpgradeFingerprint"))?;
            let decoded: Value = serde_json::from_str(fingerprint)
                .map_err(|_| InventoryError::InvalidHelperField("UpgradeFingerprint"))?;
            let object = decoded
                .as_object()
                .ok_or(InventoryError::InvalidHelperField("UpgradeFingerprint"))?;
            Ok(helper_optional_rank(object.get("lvl"))?.or(Some(0)))
        }
        _ => helper_optional_rank(map.get("Rank").or_else(|| map.get("UpgradeLevel"))),
    }
}

fn parse_helper_value(raw: &str, root: Value) -> Result<PlayerInventory, InventoryError> {
    let payload = unwrap_helper_payload(root)?;
    normalize_helper_payload(raw, &payload, InventorySource::HelperImport, Utc::now(), 1)
}

fn parse_companion_value(raw: &str, root: Value) -> Result<PlayerInventory, InventoryError> {
    let document: CompanionDocument = serde_json::from_value(root)?;
    if document.schema_version != COMPANION_SCHEMA_VERSION {
        return Err(InventoryError::UnsupportedSchema(document.schema_version));
    }
    if document.producer != OVERWOLF_COMPANION_PRODUCER {
        return Err(InventoryError::InvalidCompanionEnvelope("producer"));
    }
    if document.game_id != WARFRAME_OVERWOLF_GAME_ID {
        return Err(InventoryError::InvalidCompanionEnvelope("gameId"));
    }
    if document.feature != "match_info" {
        return Err(InventoryError::InvalidCompanionEnvelope("feature"));
    }
    if document.key != "inventory" {
        return Err(InventoryError::InvalidCompanionEnvelope("key"));
    }
    if !document.complete {
        return Err(InventoryError::IncompleteCompanionSnapshot);
    }
    let payload = match document.value {
        Value::String(encoded) => serde_json::from_str(&encoded)?,
        value @ (Value::Object(_) | Value::Array(_)) => value,
        _ => return Err(InventoryError::InvalidCompanionEnvelope("value")),
    };
    normalize_helper_payload(
        raw,
        &payload,
        InventorySource::OverwolfCompanion,
        document.observed_at,
        document.schema_version,
    )
}

fn normalize_helper_payload(
    raw: &str,
    payload: &Value,
    source: InventorySource,
    observed_at: DateTime<Utc>,
    schema_version: u32,
) -> Result<PlayerInventory, InventoryError> {
    let mut nodes = 0;
    let mut rows = Vec::new();
    collect_helper_rows(payload, 0, &mut nodes, &mut rows)?;
    if rows.is_empty() {
        return Err(InventoryError::HelperItemsMissing);
    }

    let mut grouped: BTreeMap<GroupKey, u32> = BTreeMap::new();
    for (canonical_game_id, quantity, rank) in rows {
        let key = GroupKey {
            canonical_game_id,
            rank,
            subtype: None,
        };
        let total = grouped.entry(key).or_default();
        *total = total
            .checked_add(quantity)
            .ok_or(InventoryError::InvalidQuantity(u32::MAX))?;
        if *total > MAX_QUANTITY {
            return Err(InventoryError::InvalidQuantity(*total));
        }
    }
    if grouped.len() > MAX_IMPORT_ITEMS {
        return Err(InventoryError::TooManyItems(grouped.len()));
    }

    let items = grouped
        .into_iter()
        .map(|(key, quantity)| InventoryItem {
            canonical_game_id: key.canonical_game_id,
            quantity,
            rank: key.rank,
            subtype: None,
            tradeability: Tradeability::Unknown,
            leveled: key.rank.is_some_and(|rank| rank > 0),
        })
        .collect::<Vec<_>>();
    Ok(PlayerInventory {
        metadata: InventorySnapshotMetadata {
            source,
            observed_at,
            schema_version,
            item_count: items.len() as u64,
            checksum_sha256: hex::encode(Sha256::digest(raw.as_bytes())),
        },
        items,
        mod_usage_scanned: false,
        equipped_mods: Vec::new(),
    })
}

fn unwrap_helper_payload(root: Value) -> Result<Value, InventoryError> {
    let Some(value) = root.get("value") else {
        return Ok(root);
    };
    match value {
        Value::String(encoded) => serde_json::from_str(encoded).map_err(InventoryError::Json),
        Value::Object(_) | Value::Array(_) => Ok(value.clone()),
        _ => Ok(root),
    }
}

fn collect_helper_rows(
    value: &Value,
    depth: usize,
    nodes: &mut usize,
    rows: &mut Vec<(String, u32, Option<u16>)>,
) -> Result<(), InventoryError> {
    if depth > MAX_HELPER_DEPTH {
        return Err(InventoryError::HelperNestingTooDeep);
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > MAX_HELPER_NODES {
        return Err(InventoryError::HelperNodeLimit);
    }
    match value {
        Value::Object(map) => {
            if let Some(item_type) = map.get("ItemType") {
                let canonical_game_id = item_type
                    .as_str()
                    .ok_or(InventoryError::InvalidHelperField("ItemType"))?
                    .trim()
                    .to_owned();
                if canonical_game_id.is_empty() || canonical_game_id.len() > MAX_IDENTIFIER_LENGTH {
                    return Err(InventoryError::InvalidIdentifier);
                }
                let quantity = helper_u32(map.get("ItemCount"), "ItemCount", 1)?;
                if quantity == 0 || quantity > MAX_QUANTITY {
                    return Err(InventoryError::InvalidQuantity(quantity));
                }
                let rank =
                    helper_optional_rank(map.get("Rank").or_else(|| map.get("UpgradeLevel")))?;
                rows.push((canonical_game_id, quantity, rank));
                if rows.len() > MAX_IMPORT_ITEMS {
                    return Err(InventoryError::TooManyItems(rows.len()));
                }
                return Ok(());
            }
            for child in map.values() {
                collect_helper_rows(child, depth + 1, nodes, rows)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_helper_rows(child, depth + 1, nodes, rows)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn helper_u32(
    value: Option<&Value>,
    field: &'static str,
    default: u32,
) -> Result<u32, InventoryError> {
    let Some(value) = value else {
        return Ok(default);
    };
    let value = value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(InventoryError::InvalidHelperField(field))?;
    Ok(value)
}

fn helper_optional_rank(value: Option<&Value>) -> Result<Option<u16>, InventoryError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let rank = value
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(InventoryError::InvalidHelperField("Rank"))?;
    if rank > MAX_RANK {
        return Err(InventoryError::InvalidRank(rank));
    }
    Ok(Some(rank))
}

/// Сопоставляет inventory с каталогом и только с точными рыночными вариантами.
#[must_use]
pub fn resolve_inventory<S: BuildHasher>(
    inventory: &PlayerInventory,
    catalog: &ItemCatalog,
    available_variants: &HashSet<MarketVariantKey, S>,
    platform: Platform,
    keep_copies: u32,
) -> ResolvedInventorySnapshot {
    let lookup = catalog_lookup(&catalog.items);
    let mut groups: BTreeMap<GroupKey, GroupQuantities> = BTreeMap::new();
    let mut equipped_by_group: BTreeMap<GroupKey, Vec<&EquippedModInstance>> = BTreeMap::new();
    for item in &inventory.items {
        let key = GroupKey {
            canonical_game_id: item.canonical_game_id.clone(),
            rank: item.rank,
            subtype: item.subtype.clone(),
        };
        let quantities = groups.entry(key).or_default();
        quantities.owned = quantities.owned.saturating_add(item.quantity);
        match item.tradeability {
            Tradeability::Tradeable => {
                quantities.tradeable = quantities.tradeable.saturating_add(item.quantity);
            }
            Tradeability::Untradeable => {
                quantities.untradeable = quantities.untradeable.saturating_add(item.quantity);
            }
            Tradeability::Unknown => {
                quantities.unknown = quantities.unknown.saturating_add(item.quantity);
            }
        }
        if item.leveled {
            quantities.leveled = quantities.leveled.saturating_add(item.quantity);
        }
    }

    for equipped in &inventory.equipped_mods {
        equipped_by_group
            .entry(GroupKey {
                canonical_game_id: equipped.canonical_game_id.clone(),
                rank: Some(equipped.rank),
                subtype: None,
            })
            .or_default()
            .push(equipped);
    }

    let items = groups
        .into_iter()
        .map(|(group, quantities)| {
            let equipped = equipped_by_group.remove(&group).unwrap_or_default();
            resolve_group(
                group,
                &quantities,
                &equipped,
                &lookup,
                available_variants,
                platform,
                keep_copies,
            )
        })
        .collect();
    ResolvedInventorySnapshot {
        metadata: inventory.metadata.clone(),
        keep_copies,
        mod_usage_scanned: inventory.mod_usage_scanned,
        items,
    }
}

/// Пересчитывает резерв без повторного resolver/import.
#[must_use]
pub fn apply_keep_copies(
    snapshot: &ResolvedInventorySnapshot,
    keep_copies: u32,
) -> ResolvedInventorySnapshot {
    let mut updated = snapshot.clone();
    updated.keep_copies = keep_copies;
    for item in &mut updated.items {
        item.sellable_quantity = sellable_quantity(
            item.resolution,
            item.owned_quantity,
            item.tradeable_quantity,
            item.untradeable_quantity,
            item.unknown_quantity,
            item.equipped_tradeable_quantity,
            keep_copies,
        );
    }
    updated
}

fn resolve_group<S: BuildHasher>(
    group: GroupKey,
    quantities: &GroupQuantities,
    equipped: &[&EquippedModInstance],
    lookup: &HashMap<String, Vec<&CatalogItem>>,
    available_variants: &HashSet<MarketVariantKey, S>,
    platform: Platform,
    keep_copies: u32,
) -> ResolvedInventoryItem {
    let lookup_key = normalized_identifier(&group.canonical_game_id);
    let candidates = lookup
        .get(&lookup_key)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let (display_name_en, display_name_ru, tags, key, subtype, resolution) = match candidates {
        [] => (
            None,
            None,
            Vec::new(),
            None,
            group.subtype.clone(),
            InventoryResolution::UnknownItem,
        ),
        [catalog_item] => {
            let subtype = group.subtype.clone().or_else(|| {
                catalog_item
                    .subtypes
                    .iter()
                    .any(|candidate| candidate == "regular")
                    .then(|| "regular".to_owned())
            });
            let key = MarketVariantKey::new(
                catalog_item.slug.clone(),
                platform,
                group.rank,
                subtype.clone(),
            )
            .expect("validated inventory identity creates a valid market key");
            let resolution = if market_shape_available(available_variants, &key) {
                InventoryResolution::Resolved
            } else {
                InventoryResolution::ExactVariantUnavailable
            };
            (
                Some(catalog_item.display_name_en.clone()),
                catalog_item.display_name_ru.clone(),
                catalog_item.tags.clone(),
                Some(key),
                subtype,
                resolution,
            )
        }
        _ => (
            None,
            None,
            Vec::new(),
            None,
            group.subtype.clone(),
            InventoryResolution::AmbiguousItem,
        ),
    };
    let (equipped_quantity, equipped_tradeable_quantity, equipped_placements) =
        resolve_equipped_mods(equipped, quantities, lookup);

    let sellable_quantity = sellable_quantity(
        resolution,
        quantities.owned,
        quantities.tradeable,
        quantities.untradeable,
        quantities.unknown,
        equipped_tradeable_quantity,
        keep_copies,
    );
    ResolvedInventoryItem {
        canonical_game_id: group.canonical_game_id,
        display_name_en,
        display_name_ru,
        tags,
        key,
        rank: group.rank,
        subtype,
        owned_quantity: quantities.owned,
        tradeable_quantity: quantities.tradeable,
        untradeable_quantity: quantities.untradeable,
        unknown_quantity: quantities.unknown,
        leveled_quantity: quantities.leveled,
        equipped_quantity,
        equipped_tradeable_quantity,
        equipped_placements,
        sellable_quantity,
        resolution,
    }
}

fn resolve_equipped_mods(
    equipped: &[&EquippedModInstance],
    quantities: &GroupQuantities,
    lookup: &HashMap<String, Vec<&CatalogItem>>,
) -> (u32, u32, Vec<ResolvedModPlacement>) {
    let equipped_quantity = u32::try_from(equipped.len())
        .unwrap_or(u32::MAX)
        .min(quantities.owned);
    let equipped_tradeable_quantity = u32::try_from(
        equipped
            .iter()
            .filter(|instance| instance.tradeability == Tradeability::Tradeable)
            .count(),
    )
    .unwrap_or(u32::MAX)
    .min(quantities.tradeable);
    let mut placements = equipped
        .iter()
        .flat_map(|instance| instance.placements.iter())
        .map(|placement| resolve_mod_placement(placement, lookup))
        .collect::<Vec<_>>();
    placements.sort_by(|left, right| {
        left.equipment_display_name_en
            .cmp(&right.equipment_display_name_en)
            .then(
                left.equipment_instance_key
                    .cmp(&right.equipment_instance_key),
            )
            .then(left.config_index.cmp(&right.config_index))
    });
    placements.dedup();
    (equipped_quantity, equipped_tradeable_quantity, placements)
}

fn resolve_mod_placement(
    placement: &InventoryModPlacement,
    lookup: &HashMap<String, Vec<&CatalogItem>>,
) -> ResolvedModPlacement {
    let candidates = lookup
        .get(&normalized_identifier(&placement.equipment_game_id))
        .map(Vec::as_slice)
        .unwrap_or_default();
    let catalog_item = candidates
        .iter()
        .copied()
        .find(|candidate| candidate.game_ref.as_deref() == Some(&placement.equipment_game_id))
        .or_else(|| candidates.first().copied());
    let fallback = humanize_game_id(&placement.equipment_game_id);
    let base_en = catalog_item.map_or_else(
        || fallback.clone(),
        |item| strip_set_suffix(&item.display_name_en),
    );
    let base_ru = catalog_item
        .and_then(|item| item.display_name_ru.as_deref())
        .map_or_else(|| fallback.clone(), strip_set_suffix);
    let display_name = |base: String| {
        placement
            .equipment_custom_name
            .as_ref()
            .map_or(base.clone(), |custom| format!("{custom} — {base}"))
    };
    ResolvedModPlacement {
        equipment_instance_key: placement.equipment_instance_key.clone(),
        equipment_game_id: placement.equipment_game_id.clone(),
        equipment_display_name_en: Some(display_name(base_en)),
        equipment_display_name_ru: Some(display_name(base_ru)),
        equipment_image_url: catalog_item
            .and_then(|item| item.thumb_ru.as_ref().or(item.thumb.as_ref()))
            .map(|thumb| format!("https://warframe.market/static/assets/{thumb}")),
        equipment_kind: placement.equipment_kind,
        config_index: placement.config_index,
    }
}

fn strip_set_suffix(value: &str) -> String {
    value
        .strip_suffix(" Set")
        .or_else(|| value.strip_suffix(": Комплект"))
        .or_else(|| value.strip_suffix(" Комплект"))
        .unwrap_or(value)
        .to_owned()
}

fn humanize_game_id(value: &str) -> String {
    let leaf = value.rsplit('/').next().unwrap_or(value);
    let mut result = String::with_capacity(leaf.len() + 8);
    for (index, character) in leaf.chars().enumerate() {
        if index > 0 && character.is_uppercase() {
            result.push(' ');
        }
        result.push(character);
    }
    result
}

fn market_shape_available<S: BuildHasher>(
    available_variants: &HashSet<MarketVariantKey, S>,
    key: &MarketVariantKey,
) -> bool {
    available_variants.iter().any(|candidate| {
        candidate.slug == key.slug
            && candidate.rank == key.rank
            && (candidate.subtype == key.subtype
                || (key.subtype.as_deref() == Some("regular") && candidate.subtype.is_none()))
            && candidate.amber_stars == key.amber_stars
            && candidate.cyan_stars == key.cyan_stars
    })
}

fn sellable_quantity(
    resolution: InventoryResolution,
    owned: u32,
    tradeable: u32,
    untradeable: u32,
    unknown: u32,
    equipped_tradeable: u32,
    keep_copies: u32,
) -> u32 {
    if resolution != InventoryResolution::Resolved || unknown > 0 {
        return 0;
    }
    let protected = untradeable.saturating_add(equipped_tradeable);
    owned
        .saturating_sub(keep_copies.max(protected))
        .min(tradeable.saturating_sub(equipped_tradeable))
}

fn catalog_lookup(items: &[CatalogItem]) -> HashMap<String, Vec<&CatalogItem>> {
    let mut lookup: HashMap<String, Vec<&CatalogItem>> = HashMap::new();
    for item in items {
        for identity in [
            Some(item.item_id.as_str()),
            Some(item.slug.as_str()),
            item.game_ref.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let candidates = lookup.entry(normalized_identifier(identity)).or_default();
            if !candidates
                .iter()
                .any(|candidate| candidate.item_id == item.item_id)
            {
                candidates.push(item);
            }
        }
    }
    lookup
}

fn normalized_identifier(value: &str) -> String {
    value.trim().to_lowercase()
}

#[derive(Debug, Error)]
pub enum InventoryError {
    #[error("inventory import exceeds the 8 MiB limit")]
    PayloadTooLarge,
    #[error("invalid inventory JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported inventory schema version {0}")]
    UnsupportedSchema(u32),
    #[error("inventory contains too many rows: {0}")]
    TooManyItems(usize),
    #[error("inventory item_count mismatch: declared {declared}, actual {actual}")]
    ItemCountMismatch { declared: u64, actual: usize },
    #[error("canonicalGameId must contain 1..=256 bytes")]
    InvalidIdentifier,
    #[error("quantity must be within 1..={MAX_QUANTITY}, received {0}")]
    InvalidQuantity(u32),
    #[error("rank must be within 0..={MAX_RANK}, received {0}")]
    InvalidRank(u16),
    #[error("subtype must contain 1..={MAX_SUBTYPE_LENGTH} bytes")]
    InvalidSubtype,
    #[error("inventory contains a duplicate exact row")]
    DuplicateItem,
    #[error("helper inventory does not contain recognizable ItemType rows")]
    HelperItemsMissing,
    #[error("helper inventory nesting exceeds the safe limit")]
    HelperNestingTooDeep,
    #[error("helper inventory contains too many JSON nodes")]
    HelperNodeLimit,
    #[error("helper inventory field {0} has an invalid type")]
    InvalidHelperField(&'static str),
    #[error("Overwolf companion envelope field {0} is invalid")]
    InvalidCompanionEnvelope(&'static str),
    #[error("Overwolf companion snapshot is not complete")]
    IncompleteCompanionSnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use platscope_domain::{CatalogMetadata, ProviderId};

    fn catalog() -> ItemCatalog {
        ItemCatalog {
            metadata: CatalogMetadata {
                provider: ProviderId::RelicsRun,
                fetched_at: Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).unwrap(),
                schema_version: 1,
                item_count: 1,
                checksum_sha256: "catalog".into(),
            },
            items: vec![CatalogItem {
                item_id: "item-id".into(),
                slug: "primed_flow".into(),
                display_name_en: "Primed Flow".into(),
                display_name_ru: Some("Поток Прайм".into()),
                thumb: None,
                thumb_ru: None,
                game_ref: Some("/Lotus/Upgrades/Mods/PrimedFlow".into()),
                bulk_tradable: false,
                max_rank: Some(10),
                subtypes: Vec::new(),
                tags: vec!["mod".into()],
            }],
        }
    }

    fn inventory(items: Vec<InventoryItem>) -> PlayerInventory {
        PlayerInventory {
            metadata: InventorySnapshotMetadata {
                source: InventorySource::TestFixture,
                observed_at: Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).unwrap(),
                schema_version: 1,
                item_count: items.len() as u64,
                checksum_sha256: "inventory".into(),
            },
            items,
            mod_usage_scanned: false,
            equipped_mods: Vec::new(),
        }
    }

    #[test]
    fn parser_rejects_incoherent_count() {
        let error = parse_platscope_json(
            r#"{"schemaVersion":1,"observedAt":"2026-08-27T00:00:00Z","itemCount":2,"items":[]}"#,
        )
        .expect_err("count mismatch must fail");
        assert!(matches!(error, InventoryError::ItemCountMismatch { .. }));
    }

    #[test]
    fn resolver_requires_exact_rank_and_applies_reserve() {
        let source = inventory(vec![InventoryItem {
            canonical_game_id: "/Lotus/Upgrades/Mods/PrimedFlow".into(),
            quantity: 3,
            rank: Some(0),
            subtype: None,
            tradeability: Tradeability::Tradeable,
            leveled: false,
        }]);
        let exact = MarketVariantKey::new("primed_flow", Platform::Pc, Some(0), None::<String>)
            .expect("key");
        let resolved = resolve_inventory(
            &source,
            &catalog(),
            &HashSet::from([exact.clone()]),
            Platform::Pc,
            1,
        );
        assert_eq!(resolved.items[0].key, Some(exact));
        assert_eq!(resolved.items[0].resolution, InventoryResolution::Resolved);
        assert_eq!(resolved.items[0].sellable_quantity, 2);

        let wrong_rank = HashSet::from([MarketVariantKey::new(
            "primed_flow",
            Platform::Pc,
            Some(10),
            None::<String>,
        )
        .expect("key")]);
        let unresolved = resolve_inventory(&source, &catalog(), &wrong_rank, Platform::Pc, 1);
        assert_eq!(
            unresolved.items[0].resolution,
            InventoryResolution::ExactVariantUnavailable
        );
        assert_eq!(unresolved.items[0].sellable_quantity, 0);
    }

    #[test]
    fn resolver_maps_missing_inventory_subtype_to_regular_market_variant() {
        let source = inventory(vec![InventoryItem {
            canonical_game_id: "/Lotus/Upgrades/Mods/PrimedFlow".into(),
            quantity: 2,
            rank: Some(5),
            subtype: None,
            tradeability: Tradeability::Tradeable,
            leveled: true,
        }]);
        let mut item_catalog = catalog();
        item_catalog.items[0].subtypes = vec!["regular".into(), "atragraph".into()];
        let legacy_market_variant =
            MarketVariantKey::new("primed_flow", Platform::Pc, Some(5), None::<String>)
                .expect("legacy key");

        let resolved = resolve_inventory(
            &source,
            &item_catalog,
            &HashSet::from([legacy_market_variant]),
            Platform::Pc,
            0,
        );

        assert_eq!(resolved.items[0].subtype.as_deref(), Some("regular"));
        assert_eq!(
            resolved.items[0]
                .key
                .as_ref()
                .and_then(|key| key.subtype.as_deref()),
            Some("regular")
        );
        assert_eq!(resolved.items[0].resolution, InventoryResolution::Resolved);
        assert_eq!(resolved.items[0].sellable_quantity, 2);
    }

    #[test]
    fn resolver_preserves_selected_platform_for_catalog_variant() {
        let source = inventory(vec![InventoryItem {
            canonical_game_id: "primed_flow".into(),
            quantity: 2,
            rank: Some(0),
            subtype: None,
            tradeability: Tradeability::Tradeable,
            leveled: false,
        }]);
        let pc_catalog_variant =
            MarketVariantKey::new("primed_flow", Platform::Pc, Some(0), None::<String>)
                .expect("catalog key");

        let resolved = resolve_inventory(
            &source,
            &catalog(),
            &HashSet::from([pc_catalog_variant]),
            Platform::Xbox,
            1,
        );

        assert_eq!(resolved.items[0].resolution, InventoryResolution::Resolved);
        assert_eq!(
            resolved.items[0].key.as_ref().map(|key| key.platform),
            Some(Platform::Xbox)
        );
        assert_eq!(resolved.items[0].sellable_quantity, 1);
    }

    #[test]
    fn unknown_tradeability_never_becomes_sellable() {
        let source = inventory(vec![InventoryItem {
            canonical_game_id: "primed_flow".into(),
            quantity: 4,
            rank: Some(0),
            subtype: None,
            tradeability: Tradeability::Unknown,
            leveled: false,
        }]);
        let variants = HashSet::from([MarketVariantKey::new(
            "primed_flow",
            Platform::Pc,
            Some(0),
            None::<String>,
        )
        .expect("key")]);
        let resolved = resolve_inventory(&source, &catalog(), &variants, Platform::Pc, 0);
        assert_eq!(resolved.items[0].unknown_quantity, 4);
        assert_eq!(resolved.items[0].sellable_quantity, 0);
    }

    #[test]
    fn helper_export_is_grouped_and_never_assumes_tradeability() {
        let parsed = parse_helper_json(include_str!(
            "../../../fixtures/inventory/helper_inventory.json"
        ))
        .expect("helper fixture parses");
        assert_eq!(parsed.metadata.source, InventorySource::HelperImport);
        assert_eq!(parsed.metadata.item_count, 3);
        let prime_part = parsed
            .items
            .iter()
            .find(|item| item.canonical_game_id.ends_with("NyxPrimeChassis"))
            .expect("prime part");
        assert_eq!(prime_part.quantity, 3);
        assert_eq!(prime_part.tradeability, Tradeability::Unknown);
        let mod_item = parsed
            .items
            .iter()
            .find(|item| item.canonical_game_id.ends_with("PrimedFlow"))
            .expect("ranked mod");
        assert_eq!(mod_item.rank, Some(10));
        assert!(mod_item.leveled);

        let variants = HashSet::from([MarketVariantKey::new(
            "primed_flow",
            Platform::Pc,
            Some(10),
            None::<String>,
        )
        .expect("exact helper variant")]);
        let resolved = resolve_inventory(&parsed, &catalog(), &variants, Platform::Pc, 0);
        let resolved_mod = resolved
            .items
            .iter()
            .find(|item| item.canonical_game_id.ends_with("PrimedFlow"))
            .expect("resolved helper mod");
        assert_eq!(resolved_mod.resolution, InventoryResolution::Resolved);
        assert_eq!(resolved_mod.owned_quantity, 1);
        assert_eq!(resolved_mod.unknown_quantity, 1);
        assert_eq!(resolved_mod.sellable_quantity, 0);
    }

    #[test]
    fn helper_wrapper_string_is_supported() {
        let raw = r#"{"feature":"match_info","key":"inventory","value":"{\"MiscItems\":[{\"ItemType\":\"/Lotus/Test/Part\",\"ItemCount\":2}]}"}"#;
        let parsed = parse_inventory_json(raw).expect("Overwolf-style wrapper parses");
        assert_eq!(parsed.metadata.source, InventorySource::HelperImport);
        assert_eq!(parsed.items[0].quantity, 2);
    }

    #[test]
    fn read_only_scan_has_a_distinct_trusted_source() {
        let raw = r#"{
            "Inventory": {
                "MiscItems": [{"ItemType":"/Lotus/Test/Part","ItemCount":2}],
                "RawUpgrades": [{"ItemType":"/Lotus/Test/Mod","ItemCount":3}],
                "Upgrades": [
                    {"ItemType":"/Lotus/Test/Mod","UpgradeFingerprint":"{\"lvl\":5}"},
                    {"ItemType":"/Lotus/Test/Arcane","UpgradeFingerprint":"{}"}
                ],
                "Suits": [{"ItemType":"/Lotus/Test/LeveledSuit","XP":15}],
                "LoadOutPresets": [{"ItemType":"/Lotus/Interface/Graphics/CustomUI/StalkerStyle"}]
            },
            "ProfileSettings": {"ItemType":"/Lotus/Interface/AnotherInternalValue"}
        }"#;
        let parsed = parse_read_only_scan_json(raw).expect("scanner response parses");
        assert_eq!(parsed.metadata.source, InventorySource::ReadOnlyScan);
        assert_eq!(parsed.items.len(), 5);
        assert_eq!(parsed.items[0].quantity, 2);
        assert_eq!(parsed.items[0].tradeability, Tradeability::Tradeable);
        assert_eq!(parsed.items[1].rank, Some(0));
        assert!(!parsed.items[1].leveled);
        assert_eq!(parsed.items[2].rank, Some(5));
        assert!(parsed.items[2].leveled);
        assert_eq!(parsed.items[3].rank, Some(0));
        assert!(!parsed.items[3].leveled);
        assert_eq!(parsed.items[4].tradeability, Tradeability::Untradeable);
        assert!(parsed.items[4].leveled);
        assert!(
            parsed
                .items
                .iter()
                .all(|item| !item.canonical_game_id.contains("/Interface/"))
        );
        assert!(parsed.mod_usage_scanned);
        assert!(parsed.equipped_mods.is_empty());
    }

    #[test]
    fn equipped_instances_are_counted_once_and_protected_from_sale() {
        let raw = r#"{
            "Upgrades": [
                {"ItemId":{"$oid":"mod-1"},"ItemType":"/Lotus/Upgrades/Mods/PrimedFlow","UpgradeFingerprint":"{\"lvl\":5}"},
                {"ItemId":{"$oid":"mod-2"},"ItemType":"/Lotus/Upgrades/Mods/PrimedFlow","UpgradeFingerprint":"{\"lvl\":5}"}
            ],
            "Suits": [{
                "ItemId":{"$oid":"volt-1"},
                "ItemType":"/Lotus/Powersuits/Volt/VoltPrime",
                "ItemName":"Скорость",
                "Configs":[
                    {"Upgrades":[{"$id":"mod-1"}]},
                    {"Upgrades":[{"$id":"mod-1"}]}
                ]
            }],
            "LongGuns": [{
                "ItemId":"braton-1",
                "ItemType":"/Lotus/Weapons/Tenno/Rifle/BratonPrime",
                "Configs":[{"Upgrades":["mod-2","/Lotus/Upgrades/Intrinsic/Test"]}]
            }]
        }"#;
        let parsed = parse_read_only_scan_json(raw).expect("equipped references parse");
        assert!(parsed.mod_usage_scanned);
        assert_eq!(parsed.equipped_mods.len(), 2);
        assert_eq!(
            parsed
                .equipped_mods
                .iter()
                .map(|instance| instance.placements.len())
                .sum::<usize>(),
            3
        );

        let variants = HashSet::from([MarketVariantKey::new(
            "primed_flow",
            Platform::Pc,
            Some(5),
            None::<String>,
        )
        .expect("variant")]);
        let resolved = resolve_inventory(&parsed, &catalog(), &variants, Platform::Pc, 0);
        let mod_item = resolved
            .items
            .iter()
            .find(|item| item.canonical_game_id.ends_with("PrimedFlow"))
            .expect("resolved mod");
        assert_eq!(mod_item.equipped_quantity, 2);
        assert_eq!(mod_item.equipped_tradeable_quantity, 2);
        assert_eq!(mod_item.equipped_placements.len(), 3);
        assert_eq!(mod_item.sellable_quantity, 0);
        assert!(mod_item.equipped_placements.iter().any(|placement| {
            placement.equipment_display_name_ru.as_deref() == Some("Скорость — Volt Prime")
                && placement.config_index == 1
        }));
    }

    #[test]
    fn read_only_upgrade_fingerprint_fails_closed_on_schema_drift() {
        let invalid_json =
            r#"{"Upgrades":[{"ItemType":"/Lotus/Test/Mod","UpgradeFingerprint":"not-json"}]}"#;
        assert!(matches!(
            parse_read_only_scan_json(invalid_json),
            Err(InventoryError::InvalidHelperField("UpgradeFingerprint"))
        ));

        let invalid_level = r#"{"Upgrades":[{"ItemType":"/Lotus/Test/Mod","UpgradeFingerprint":"{\"lvl\":\"five\"}"}]}"#;
        assert!(matches!(
            parse_read_only_scan_json(invalid_level),
            Err(InventoryError::InvalidHelperField("Rank"))
        ));
    }

    #[test]
    fn read_only_upgrade_stacks_and_instances_resolve_as_separate_ranks() {
        let raw = r#"{
            "RawUpgrades": [
                {"ItemType":"/Lotus/Upgrades/Mods/PrimedFlow","ItemCount":3}
            ],
            "Upgrades": [
                {"ItemType":"/Lotus/Upgrades/Mods/PrimedFlow","UpgradeFingerprint":"{\"lvl\":5}"},
                {"ItemType":"/Lotus/Upgrades/Mods/PrimedFlow","UpgradeFingerprint":"{\"lvl\":5}"}
            ]
        }"#;
        let parsed = parse_read_only_scan_json(raw).expect("read-only upgrades parse");
        let variants = HashSet::from([
            MarketVariantKey::new("primed_flow", Platform::Pc, Some(0), None::<String>)
                .expect("rank zero key"),
            MarketVariantKey::new("primed_flow", Platform::Pc, Some(5), None::<String>)
                .expect("rank five key"),
        ]);
        let resolved = resolve_inventory(&parsed, &catalog(), &variants, Platform::Pc, 0);

        assert_eq!(resolved.items.len(), 2);
        let rank_zero = resolved
            .items
            .iter()
            .find(|item| item.rank == Some(0))
            .expect("rank zero stack");
        assert_eq!(rank_zero.owned_quantity, 3);
        assert_eq!(rank_zero.sellable_quantity, 3);
        let rank_five = resolved
            .items
            .iter()
            .find(|item| item.rank == Some(5))
            .expect("rank five instances");
        assert_eq!(rank_five.owned_quantity, 2);
        assert_eq!(rank_five.sellable_quantity, 2);
    }

    #[test]
    fn versioned_overwolf_companion_envelope_preserves_observation_time() {
        let raw = include_str!("../../../fixtures/inventory/overwolf_companion_v1.json");
        let parsed = parse_inventory_json(raw).expect("companion envelope parses");
        assert_eq!(parsed.metadata.source, InventorySource::OverwolfCompanion);
        assert_eq!(
            parsed.metadata.observed_at,
            Utc.with_ymd_and_hms(2026, 8, 27, 10, 15, 30).unwrap()
        );
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].quantity, 1);
        assert_eq!(parsed.items[0].rank, Some(10));
        assert_eq!(parsed.items[0].tradeability, Tradeability::Unknown);
        assert_eq!(parsed.items[1].quantity, 2);
        assert_eq!(parsed.items[1].tradeability, Tradeability::Unknown);
    }

    #[test]
    fn overwolf_companion_envelope_fails_closed() {
        let incomplete = r#"{
            "schemaVersion":1,
            "producer":"platscope-overwolf-companion",
            "observedAt":"2026-08-27T10:15:30Z",
            "gameId":8954,
            "feature":"match_info",
            "key":"inventory",
            "complete":false,
            "value":{"Inventory":{"MiscItems":[
                {"ItemType":"/Lotus/Test/Part","ItemCount":2}
            ]}}
        }"#;
        assert!(matches!(
            parse_inventory_json(incomplete),
            Err(InventoryError::IncompleteCompanionSnapshot)
        ));

        let wrong_game = incomplete
            .replace("\"gameId\":8954", "\"gameId\":1")
            .replace("\"complete\":false", "\"complete\":true");
        assert!(matches!(
            parse_inventory_json(&wrong_game),
            Err(InventoryError::InvalidCompanionEnvelope("gameId"))
        ));
    }

    #[test]
    fn helper_schema_drift_fails_closed() {
        let error = parse_helper_json(r#"{"Inventory":{"MiscItems":[{"ItemType":7}]}}"#)
            .expect_err("invalid ItemType must fail");
        assert!(matches!(
            error,
            InventoryError::InvalidHelperField("ItemType")
        ));
        assert!(parse_helper_json(r#"{"Inventory":{"Credits":100}}"#).is_err());
    }
}
