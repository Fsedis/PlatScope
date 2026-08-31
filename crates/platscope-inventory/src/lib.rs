#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::BuildHasher;

use chrono::Utc;
use platscope_domain::{
    CatalogItem, EquipmentKind, EquippedModInstance, InventoryItem, InventoryModPlacement,
    InventoryResolution, InventorySnapshotMetadata, InventorySource, ItemCatalog, MarketVariantKey,
    Platform, PlayerInventory, ResolvedInventoryItem, ResolvedInventorySnapshot,
    ResolvedModPlacement, SyndicateStanding, Tradeability,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const READ_ONLY_SCHEMA_VERSION: u32 = 1;
pub const MAX_INVENTORY_BYTES: usize = 8 * 1024 * 1024;
const MAX_INVENTORY_ITEMS: usize = 100_000;
// Long-lived accounts can legitimately hold more than one million common
// resources. Keep arithmetic bounded without rejecting real DE snapshots.
const MAX_QUANTITY: u32 = 100_000_000;
const MAX_IDENTIFIER_LENGTH: usize = 256;
const MAX_RANK: u16 = 100;
const MAX_EQUIPMENT_CONFIGS: usize = 16;
const MAX_CONFIG_UPGRADES: usize = 64;
const MAX_AFFILIATIONS: usize = 128;
const MAX_WALLET_AMOUNT: u64 = 1_000_000_000_000_000;
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

/// Разбирает сырой ответ inventory endpoint, полученный встроенным read-only scanner.
/// Ответ проходит ограничение размера и строгую нормализацию, сохраняя
/// отдельный доверенный источник для UI, диагностики и истории.
///
/// # Errors
///
/// Возвращает [`InventoryError`] при превышении лимитов, schema drift или
/// отсутствии распознаваемых строк `ItemType`.
pub fn parse_read_only_scan_json(raw: &str) -> Result<PlayerInventory, InventoryError> {
    if raw.len() > MAX_INVENTORY_BYTES {
        return Err(InventoryError::PayloadTooLarge);
    }
    let root: Value = serde_json::from_str(raw)?;
    let payload = unwrap_inventory_payload(root)?;
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
        .ok_or(InventoryError::InvalidInventoryField("Inventory"))?;
    let mut items = Vec::new();

    for category in READ_ONLY_INVENTORY_CATEGORIES {
        let Some(entries) = root.get(category) else {
            continue;
        };
        let entries = entries
            .as_array()
            .ok_or(InventoryError::InvalidInventoryField("inventory category"))?;
        for entry in entries {
            let map = entry
                .as_object()
                .ok_or(InventoryError::InvalidInventoryField("inventory item"))?;
            let Some(item_type) = map.get("ItemType").or_else(|| map.get("Type")) else {
                continue;
            };
            let canonical_game_id = item_type
                .as_str()
                .ok_or(InventoryError::InvalidInventoryField("ItemType"))?
                .trim()
                .to_owned();
            if canonical_game_id.is_empty() || canonical_game_id.len() > MAX_IDENTIFIER_LENGTH {
                return Err(InventoryError::InvalidIdentifier);
            }
            let quantity = inventory_u32(map.get("ItemCount"), "ItemCount", 1)?;
            if quantity == 0 || quantity > MAX_QUANTITY {
                return Err(InventoryError::InvalidQuantity(quantity));
            }
            let rank = read_only_rank(category, map)?;
            let xp = inventory_u32(map.get("XP"), "XP", 0)?;
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
            if items.len() > MAX_INVENTORY_ITEMS {
                return Err(InventoryError::TooManyItems(items.len()));
            }
        }
    }
    if items.is_empty() {
        return Err(InventoryError::InventoryItemsMissing);
    }

    // Equipment/configuration data is an optional extension of the inventory
    // response. A schema drift there must never make the verified item list
    // unusable: keep the inventory snapshot and mark mod usage as unavailable.
    let (mod_usage_scanned, equipped_mods) = match read_only_equipped_mods(root) {
        Ok(equipped_mods) => (true, equipped_mods),
        Err(_) => (false, Vec::new()),
    };
    let credits = read_only_credits(root).unwrap_or(None);
    let syndicates = read_only_syndicates(root).unwrap_or_default();

    Ok(PlayerInventory {
        metadata: InventorySnapshotMetadata {
            source: InventorySource::ReadOnlyScan,
            observed_at: Utc::now(),
            schema_version: READ_ONLY_SCHEMA_VERSION,
            item_count: items.len() as u64,
            checksum_sha256: hex::encode(Sha256::digest(raw.as_bytes())),
        },
        items,
        mod_usage_scanned,
        equipped_mods,
        credits,
        syndicates,
    })
}

fn read_only_credits(root: &serde_json::Map<String, Value>) -> Result<Option<u64>, InventoryError> {
    let Some(value) = root.get("Credits") else {
        return Ok(None);
    };
    let credits = value
        .as_u64()
        .ok_or(InventoryError::InvalidInventoryField("Credits"))?;
    if credits > MAX_WALLET_AMOUNT {
        return Err(InventoryError::InvalidInventoryField("Credits"));
    }
    Ok(Some(credits))
}

fn read_only_syndicates(
    root: &serde_json::Map<String, Value>,
) -> Result<Vec<SyndicateStanding>, InventoryError> {
    let Some(value) = root.get("Affiliations") else {
        return Ok(Vec::new());
    };
    let entries = value
        .as_array()
        .ok_or(InventoryError::InvalidInventoryField("Affiliations"))?;
    if entries.len() > MAX_AFFILIATIONS {
        return Err(InventoryError::InvalidInventoryField("Affiliations"));
    }
    let mut by_tag = BTreeMap::new();
    for entry in entries {
        let entry = entry
            .as_object()
            .ok_or(InventoryError::InvalidInventoryField("Affiliations item"))?;
        let Some(tag) = entry.get("Tag").and_then(Value::as_str) else {
            continue;
        };
        let tag = tag.trim();
        if tag.is_empty() || tag.len() > MAX_IDENTIFIER_LENGTH {
            continue;
        }
        let standing = entry
            .get("Standing")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        if !(-1_000_000_000..=1_000_000_000).contains(&standing) {
            continue;
        }
        let title = entry
            .get("Title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty() && title.len() <= MAX_IDENTIFIER_LENGTH)
            .map(str::to_owned);
        by_tag.insert(
            tag.to_owned(),
            SyndicateStanding {
                tag: tag.to_owned(),
                standing,
                title,
            },
        );
    }
    Ok(by_tag.into_values().collect())
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
            .ok_or(InventoryError::InvalidInventoryField("Upgrades"))?;
        for upgrade in upgrades {
            let map = upgrade
                .as_object()
                .ok_or(InventoryError::InvalidInventoryField("upgrade"))?;
            let Some(item_id) = inventory_reference_id(map.get("ItemId"))? else {
                continue;
            };
            let canonical_game_id = map
                .get("ItemType")
                .or_else(|| map.get("Type"))
                .and_then(Value::as_str)
                .ok_or(InventoryError::InvalidInventoryField("ItemType"))?
                .trim()
                .to_owned();
            if canonical_game_id.is_empty() || canonical_game_id.len() > MAX_IDENTIFIER_LENGTH {
                return Err(InventoryError::InvalidIdentifier);
            }
            let rank = read_only_rank("Upgrades", map)?.unwrap_or_default();
            let xp = inventory_u32(map.get("XP"), "XP", 0)?;
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
                return Err(InventoryError::InvalidInventoryField("duplicate ItemId"));
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
            .ok_or(InventoryError::InvalidInventoryField("equipment category"))?;
        for (equipment_index, entry) in entries.iter().enumerate() {
            let map = entry
                .as_object()
                .ok_or(InventoryError::InvalidInventoryField("equipment"))?;
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
                .and_then(equipment_custom_name);
            if equipment_custom_name
                .as_ref()
                .is_some_and(|name| name.len() > MAX_IDENTIFIER_LENGTH)
            {
                return Err(InventoryError::InvalidInventoryField("ItemName"));
            }
            let raw_equipment_id = inventory_reference_id(map.get("ItemId"))?
                .unwrap_or_else(|| format!("{category}:{equipment_index}:{equipment_game_id}"));
            let equipment_instance_key = hex::encode(Sha256::digest(raw_equipment_id.as_bytes()));
            let Some(configs) = map.get("Configs") else {
                continue;
            };
            let configs = configs
                .as_array()
                .ok_or(InventoryError::InvalidInventoryField("Configs"))?;
            if configs.len() > MAX_EQUIPMENT_CONFIGS {
                return Err(InventoryError::InvalidInventoryField("Configs"));
            }
            for (config_index, config) in configs.iter().enumerate() {
                // The live API uses null placeholders for empty loadout slots.
                // They are not malformed configurations and should be ignored.
                let Some(config) = config.as_object() else {
                    continue;
                };
                let Some(upgrades) = config.get("Upgrades") else {
                    continue;
                };
                let upgrades = upgrades
                    .as_array()
                    .ok_or(InventoryError::InvalidInventoryField("Config.Upgrades"))?;
                if upgrades.len() > MAX_CONFIG_UPGRADES {
                    return Err(InventoryError::InvalidInventoryField("Config.Upgrades"));
                }
                let config_index = u16::try_from(config_index)
                    .map_err(|_| InventoryError::InvalidInventoryField("Config index"))?;
                for upgrade in upgrades {
                    let Some(upgrade_id) = inventory_reference_id(Some(upgrade))? else {
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

fn inventory_reference_id(value: Option<&Value>) -> Result<Option<String>, InventoryError> {
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
        _ => return Err(InventoryError::InvalidInventoryField("ItemId")),
    };
    let candidate = candidate.map(str::trim).filter(|value| !value.is_empty());
    if candidate.is_some_and(|value| value.len() > MAX_IDENTIFIER_LENGTH) {
        return Err(InventoryError::InvalidInventoryField("ItemId"));
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
                .ok_or(InventoryError::InvalidInventoryField("UpgradeFingerprint"))?;
            let decoded: Value = serde_json::from_str(fingerprint)
                .map_err(|_| InventoryError::InvalidInventoryField("UpgradeFingerprint"))?;
            let object = decoded
                .as_object()
                .ok_or(InventoryError::InvalidInventoryField("UpgradeFingerprint"))?;
            Ok(inventory_optional_rank(object.get("lvl"))?.or(Some(0)))
        }
        _ => inventory_optional_rank(map.get("Rank").or_else(|| map.get("UpgradeLevel"))),
    }
}

fn unwrap_inventory_payload(root: Value) -> Result<Value, InventoryError> {
    let Some(value) = root.get("value") else {
        return Ok(root);
    };
    match value {
        Value::String(encoded) => serde_json::from_str(encoded).map_err(InventoryError::Json),
        Value::Object(_) | Value::Array(_) => Ok(value.clone()),
        _ => Ok(root),
    }
}

fn inventory_u32(
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
        .ok_or(InventoryError::InvalidInventoryField(field))?;
    Ok(value)
}

fn inventory_optional_rank(value: Option<&Value>) -> Result<Option<u16>, InventoryError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let rank = value
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(InventoryError::InvalidInventoryField("Rank"))?;
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
        credits: inventory.credits,
        syndicates: inventory.syndicates.clone(),
        items,
    }
}

/// Пересчитывает резерв без повторного сопоставления исходного снимка.
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

fn equipment_custom_name(value: &str) -> Option<String> {
    let value = value.trim();
    let visible = if value.starts_with("/Lotus/Language/") {
        value.split_once('|').map(|(_, custom)| custom.trim())?
    } else {
        value
    };
    (!visible.is_empty()).then(|| visible.to_owned())
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
    #[error("inventory response exceeds the 8 MiB limit")]
    PayloadTooLarge,
    #[error("invalid inventory JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("inventory contains too many rows: {0}")]
    TooManyItems(usize),
    #[error("canonicalGameId must contain 1..=256 bytes")]
    InvalidIdentifier,
    #[error("quantity must be within 1..={MAX_QUANTITY}, received {0}")]
    InvalidQuantity(u32),
    #[error("rank must be within 0..={MAX_RANK}, received {0}")]
    InvalidRank(u16),
    #[error("inventory does not contain recognizable ItemType rows")]
    InventoryItemsMissing,
    #[error("inventory field {0} has an invalid type")]
    InvalidInventoryField(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use platscope_domain::{CatalogMetadata, ProviderId};

    #[test]
    fn localization_key_is_never_used_as_equipment_custom_name() {
        assert_eq!(
            equipment_custom_name("/Lotus/Language/Weapons/CrpBEArcaPlasmorName|HILDI ONIA",)
                .as_deref(),
            Some("HILDI ONIA")
        );
        assert_eq!(
            equipment_custom_name("Скорость").as_deref(),
            Some("Скорость")
        );
        assert_eq!(
            equipment_custom_name("/Lotus/Language/Weapons/KuvaNukorName"),
            None
        );
    }

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
            credits: None,
            syndicates: Vec::new(),
        }
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
                "LoadOutPresets": [{"ItemType":"/Lotus/Interface/Graphics/CustomUI/StalkerStyle"}],
                "Credits": 1234567,
                "Affiliations": [
                    {"Tag":"CephalonSudaSyndicate","Standing":42000,"Title":"Genius"},
                    {"Tag":"RedVeilSyndicate","Standing":-5000}
                ]
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
        assert_eq!(parsed.credits, Some(1_234_567));
        assert_eq!(parsed.syndicates.len(), 2);
        assert_eq!(parsed.syndicates[0].tag, "CephalonSudaSyndicate");
        assert_eq!(parsed.syndicates[0].title.as_deref(), Some("Genius"));
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
    fn null_equipment_configs_are_ignored_without_losing_mod_usage() {
        let raw = r#"{
            "Upgrades": [
                {"ItemId":{"$oid":"mod-1"},"ItemType":"/Lotus/Upgrades/Mods/PrimedFlow","UpgradeFingerprint":"{\"lvl\":5}"}
            ],
            "Suits": [{
                "ItemId":{"$oid":"volt-1"},
                "ItemType":"/Lotus/Powersuits/Volt/VoltPrime",
                "Configs":[null,{"Upgrades":[{"$id":"mod-1"}]},null]
            }]
        }"#;

        let parsed = parse_read_only_scan_json(raw).expect("null config placeholders parse");
        assert!(parsed.mod_usage_scanned);
        assert_eq!(parsed.equipped_mods.len(), 1);
        assert_eq!(parsed.equipped_mods[0].placements.len(), 1);
        assert_eq!(parsed.equipped_mods[0].placements[0].config_index, 1);
    }

    #[test]
    fn equipment_schema_drift_does_not_block_inventory_refresh() {
        let raw = r#"{
            "MiscItems": [{"ItemType":"/Lotus/Test/Part","ItemCount":2}],
            "Suits": [{
                "ItemType":"/Lotus/Powersuits/Volt/VoltPrime",
                "Configs":"unexpected"
            }]
        }"#;

        let parsed = parse_read_only_scan_json(raw).expect("base inventory remains usable");
        assert_eq!(parsed.items.len(), 2);
        assert!(!parsed.mod_usage_scanned);
        assert!(parsed.equipped_mods.is_empty());
    }

    #[test]
    fn read_only_upgrade_fingerprint_fails_closed_on_schema_drift() {
        let invalid_json =
            r#"{"Upgrades":[{"ItemType":"/Lotus/Test/Mod","UpgradeFingerprint":"not-json"}]}"#;
        assert!(matches!(
            parse_read_only_scan_json(invalid_json),
            Err(InventoryError::InvalidInventoryField("UpgradeFingerprint"))
        ));

        let invalid_level = r#"{"Upgrades":[{"ItemType":"/Lotus/Test/Mod","UpgradeFingerprint":"{\"lvl\":\"five\"}"}]}"#;
        assert!(matches!(
            parse_read_only_scan_json(invalid_level),
            Err(InventoryError::InvalidInventoryField("Rank"))
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
}
