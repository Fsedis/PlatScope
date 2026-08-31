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

pub const READ_ONLY_SCHEMA_VERSION: u32 = 3;
pub const MAX_INVENTORY_BYTES: usize = 8 * 1024 * 1024;
const MAX_INVENTORY_ITEMS: usize = 100_000;
// Long-lived accounts can legitimately hold more than one million common
// resources. Keep arithmetic bounded without rejecting real DE snapshots.
const MAX_QUANTITY: u32 = 100_000_000;
const MAX_IDENTIFIER_LENGTH: usize = 256;
const MAX_RANK: u16 = 100;
const MAX_CHARGES: u16 = 1_000;
const MAX_EQUIPMENT_CONFIGS: usize = 16;
const MAX_CONFIG_UPGRADES: usize = 64;
const MAX_AFFILIATIONS: usize = 128;
const MAX_WALLET_AMOUNT: u64 = 1_000_000_000_000_000;
const READ_ONLY_INVENTORY_CATEGORIES: [&str; 13] = [
    "MiscItems",
    "Recipes",
    "RawUpgrades",
    "Upgrades",
    "FusionTreasures",
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
    charges: Option<u16>,
    subtype: Option<String>,
    amber_stars: Option<u16>,
    cyan_stars: Option<u16>,
    market_match_allowed: bool,
}

#[derive(Debug, Default)]
struct GroupQuantities {
    owned: u32,
    tradeable: u32,
    untradeable: u32,
    unknown: u32,
    leveled: u32,
}

/// Причина, по которой сохранённую строку инвентаря нельзя безопасно
/// сопоставить с точным вариантом Warframe Market.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryReresolutionIssueKind {
    /// В новом каталоге нет товара с точным `gameRef` сохранённой строки.
    CatalogItemMissing,
    /// Одному `gameRef` соответствуют несколько товаров каталога.
    CatalogItemAmbiguous,
    /// Собранный предмет совпал с рыночным товаром-комплектом (`*_set`).
    BuiltEquipmentSetAlias,
    /// Снимок не сохранил ранг, который рынок различает.
    RankMissing,
    /// Сохранённый ранг выходит за возможности нового каталога.
    RankOutOfRange,
    /// Снимок не сохранил число зарядов, которое рынок различает.
    ChargesMissing,
    /// Сохранённое число зарядов выходит за возможности нового каталога.
    ChargesOutOfRange,
    /// Старый снимок Аятан не содержит точного заполнения звёздами.
    AyatanStarsMissing,
    /// Сохранённое заполнение Аятан выходит за возможности нового каталога.
    AyatanStarsOutOfRange,
    /// Каталог различает варианты, но в снимке нет подтипа.
    SubtypeMissing,
    /// Сохранённый подтип отсутствует в новом каталоге.
    SubtypeUnsupported,
    /// Каталог содержит некорректный slug или подтип для рыночного ключа.
    InvalidCatalogVariant,
}

impl InventoryReresolutionIssueKind {
    /// Показывает, можно ли получить недостающую точную идентичность только
    /// новым чтением инвентаря Warframe.
    #[must_use]
    pub const fn requires_inventory_rescan(self) -> bool {
        matches!(
            self,
            Self::RankMissing
                | Self::ChargesMissing
                | Self::AyatanStarsMissing
                | Self::SubtypeMissing
        )
    }
}

/// Не разрешённая при повторной обработке строка и точная причина отказа.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryReresolutionIssue {
    pub canonical_game_id: String,
    pub kind: InventoryReresolutionIssueKind,
}

/// Итог безопасного повторного разрешения сохранённого снимка.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryReresolutionResult {
    pub snapshot: ResolvedInventorySnapshot,
    /// Число ранее не разрешённых строк, которые новый каталог разрешил точно.
    pub revived_item_count: u64,
    /// Число строк, для которых точный рыночный вариант по-прежнему неизвестен.
    pub unresolved_item_count: u64,
    /// `true`, если в сохранённом снимке нет данных, доступных только при новом
    /// чтении Warframe (например, маски звёзд старой скульптуры Аятан).
    pub requires_inventory_rescan: bool,
    pub issues: Vec<InventoryReresolutionIssue>,
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
            let charges = read_only_charges(category, map)?;
            let (subtype, riven_variant_known) =
                read_only_riven_market_variant(&canonical_game_id, map)?;
            let (amber_stars, cyan_stars, ayatan_variant_known) =
                read_only_ayatan_stars(category, &canonical_game_id, map)?;
            let xp = inventory_u32(map.get("XP"), "XP", 0)?;
            let equipment = is_equipment_inventory_category(category);
            items.push(InventoryItem {
                canonical_game_id,
                quantity,
                rank,
                charges,
                subtype,
                amber_stars,
                cyan_stars,
                market_match_allowed: !equipment && ayatan_variant_known && riven_variant_known,
                tradeability: if equipment {
                    // Inventory endpoint не сообщает все условия продажи
                    // собранной экипировки. В частности, XP=0 не превращает
                    // оружие или варфрейма в комплект деталей с тем же gameRef.
                    Tradeability::Unknown
                } else if xp > 0 {
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

fn read_only_charges(
    category: &str,
    map: &serde_json::Map<String, Value>,
) -> Result<Option<u16>, InventoryError> {
    if let Some(charges) = map.get("Charges") {
        return inventory_optional_charges(Some(charges));
    }
    if category != "Upgrades" {
        return Ok(None);
    }
    let Some(fingerprint) = map.get("UpgradeFingerprint").and_then(Value::as_str) else {
        return Ok(None);
    };
    let decoded: Value = serde_json::from_str(fingerprint)
        .map_err(|_| InventoryError::InvalidInventoryField("UpgradeFingerprint"))?;
    let object = decoded
        .as_object()
        .ok_or(InventoryError::InvalidInventoryField("UpgradeFingerprint"))?;
    inventory_optional_charges(object.get("charges"))
}

/// Определяет только те состояния мода Разлома, для которых Warframe Market
/// публикует обычную товарную цену. Стековый мод без показанного испытания —
/// `unrevealed`, экземпляр с испытанием — `revealed`. Открытый уникальный ролл
/// намеренно не сопоставляется с этой карточкой: для него нужна аукционная
/// оценка по оружию и характеристикам.
fn read_only_riven_market_variant(
    canonical_game_id: &str,
    map: &serde_json::Map<String, Value>,
) -> Result<(Option<String>, bool), InventoryError> {
    let leaf = canonical_game_id.rsplit('/').next().unwrap_or_default();
    if is_unrevealed_riven_game_ref(canonical_game_id) {
        return Ok((Some("unrevealed".to_owned()), true));
    }
    if !leaf.ends_with("RandomModRare") {
        return Ok((None, true));
    }

    let fingerprint = map
        .get("UpgradeFingerprint")
        .and_then(Value::as_str)
        .ok_or(InventoryError::InvalidInventoryField("UpgradeFingerprint"))?;
    let decoded: Value = serde_json::from_str(fingerprint)
        .map_err(|_| InventoryError::InvalidInventoryField("UpgradeFingerprint"))?;
    let object = decoded
        .as_object()
        .ok_or(InventoryError::InvalidInventoryField("UpgradeFingerprint"))?;

    let has_unique_roll = object
        .get("compat")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
        || object
            .get("buffs")
            .and_then(Value::as_array)
            .is_some_and(|values| !values.is_empty())
        || object
            .get("curses")
            .and_then(Value::as_array)
            .is_some_and(|values| !values.is_empty());
    if has_unique_roll {
        return Ok((None, false));
    }
    if object
        .get("challenge")
        .is_some_and(|value| !value.is_null())
    {
        return Ok((Some("revealed".to_owned()), true));
    }

    // Неизвестный fingerprint нельзя угадывать: ошибка здесь могла бы выставить
    // уникальный открытый мод по цене закрытого.
    Ok((None, false))
}

fn is_unrevealed_riven_game_ref(canonical_game_id: &str) -> bool {
    matches!(
        canonical_game_id.rsplit('/').next().unwrap_or_default(),
        "RawArchgunRandomMod"
            | "RawMeleeRandomMod"
            | "RawModularMeleeRandomMod"
            | "RawModularPistolRandomMod"
            | "RawPistolRandomMod"
            | "RawPrimaryRandomMod"
            | "RawRifleRandomMod"
            | "RawSecondaryRandomMod"
            | "RawSentinelWeaponRandomMod"
            | "RawShotgunRandomMod"
    )
}

fn unrevealed_riven_market_slug(canonical_game_id: &str) -> Option<&'static str> {
    match canonical_game_id.rsplit('/').next()? {
        "RawArchgunRandomMod" => Some("archgun_riven_mod_(veiled)"),
        "RawMeleeRandomMod" => Some("melee_riven_mod_(veiled)"),
        "RawModularMeleeRandomMod" => Some("zaw_riven_mod_(veiled)"),
        "RawModularPistolRandomMod" => Some("kitgun_riven_mod_(veiled)"),
        "RawPistolRandomMod" | "RawSecondaryRandomMod" => Some("pistol_riven_mod_(veiled)"),
        "RawPrimaryRandomMod" | "RawRifleRandomMod" => Some("rifle_riven_mod_(veiled)"),
        "RawSentinelWeaponRandomMod" => Some("companion_weapon_riven_mod_(veiled)"),
        "RawShotgunRandomMod" => Some("shotgun_riven_mod_(veiled)"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AyatanStarKind {
    Cyan,
    Amber,
}

fn read_only_ayatan_stars(
    category: &str,
    canonical_game_id: &str,
    map: &serde_json::Map<String, Value>,
) -> Result<(Option<u16>, Option<u16>, bool), InventoryError> {
    if category != "FusionTreasures" {
        return Ok((None, None, true));
    }
    // В старых схемах сами звёзды Аятан тоже могут находиться в этой
    // категории, но вариантов заполнения сокетов у них нет.
    if canonical_game_id.ends_with("OroFusexOrnamentA")
        || canonical_game_id.ends_with("OroFusexOrnamentB")
    {
        return Ok((None, None, true));
    }
    let Some(layout) = ayatan_socket_layout(canonical_game_id) else {
        // Новую неизвестную скульптуру нельзя считать пустой наугад, пока
        // расположение её сокетов не появилось в модели.
        return Ok((None, None, false));
    };
    let sockets = inventory_u32(map.get("Sockets"), "Sockets", 0)?;
    let valid_mask = (1_u32 << layout.len()) - 1;
    if sockets & !valid_mask != 0 {
        return Err(InventoryError::InvalidInventoryField("Sockets"));
    }
    let mut amber = 0_u16;
    let mut cyan = 0_u16;
    for (index, kind) in layout.iter().enumerate() {
        if sockets & (1_u32 << index) == 0 {
            continue;
        }
        match kind {
            AyatanStarKind::Amber => amber = amber.saturating_add(1),
            AyatanStarKind::Cyan => cyan = cyan.saturating_add(1),
        }
    }
    Ok((Some(amber), Some(cyan), true))
}

fn ayatan_socket_layout(canonical_game_id: &str) -> Option<&'static [AyatanStarKind]> {
    use AyatanStarKind::{Amber, Cyan};
    match canonical_game_id.rsplit('/').next()? {
        "OroFusexA" | "OroFusexH" | "OroFusexI" | "OroFusexJ" => Some(&[Cyan, Amber, Cyan]),
        "OroFusexB" => Some(&[Cyan, Cyan, Cyan]),
        "OroFusexC" => Some(&[Cyan, Amber, Cyan, Cyan]),
        "OroFusexD" => Some(&[Cyan, Cyan, Amber]),
        "OroFusexE" | "OroFusexG" => Some(&[Amber, Cyan, Cyan]),
        "OroFusexEntrati" => Some(&[Cyan, Amber, Cyan, Cyan, Cyan]),
        "OroFusexF" => Some(&[Amber, Cyan, Amber, Cyan]),
        _ => None,
    }
}

fn is_equipment_inventory_category(category: &str) -> bool {
    READ_ONLY_EQUIPMENT_CATEGORIES
        .iter()
        .any(|(candidate, _)| *candidate == category)
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

fn inventory_optional_charges(value: Option<&Value>) -> Result<Option<u16>, InventoryError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let charges = value
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(InventoryError::InvalidInventoryField("Charges"))?;
    if charges > MAX_CHARGES {
        return Err(InventoryError::InvalidCharges(charges));
    }
    Ok(Some(charges))
}

/// Сопоставляет inventory с возможностями вариантов из каталога.
/// Дневной bulk-снимок содержит цены, поэтому отсутствие строки в нём не означает,
/// что допустимый ранг, заряд или заполнение Аятан нельзя выставить на рынок.
#[must_use]
pub fn resolve_inventory<S: BuildHasher>(
    inventory: &PlayerInventory,
    catalog: &ItemCatalog,
    _available_variants: &HashSet<MarketVariantKey, S>,
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
            charges: item.charges,
            subtype: item.subtype.clone(),
            amber_stars: item.amber_stars,
            cyan_stars: item.cyan_stars,
            market_match_allowed: item.market_match_allowed,
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
                charges: None,
                subtype: None,
                amber_stars: None,
                cyan_stars: None,
                market_match_allowed: true,
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
                platform,
                keep_copies,
                inventory.mod_usage_scanned,
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

/// Повторно сопоставляет уже сохранённый снимок с обновлённым каталогом и
/// платформой без нового чтения Warframe.
///
/// Функция доверяет только точному и однозначному `gameRef`. Для уже разрешённой
/// строки дополнительно допустим её прежний уникальный slug. Возможности
/// вариантов (`maxRank`, заряды, звёзды и подтипы) берутся из каталога, а не из
/// наличия строк в текущем ценовом снимке. Игровой ранг остаётся в
/// [`ResolvedInventoryItem::rank`], тогда как в рыночный ключ попадает только
/// поддерживаемый каталогом ранг.
///
/// Старые снимки не содержат отдельные поля зарядов и звёзд: они могли быть
/// сохранены только в [`MarketVariantKey`]. Поэтому отсутствующие значения для
/// товара, который различает такие варианты, не угадываются. Строка остаётся
/// неразрешённой, а [`InventoryReresolutionResult::requires_inventory_rescan`]
/// сообщает, что нужен новый снимок Warframe. Количества по tradeability,
/// экипированные копии, размещения модов и данные аккаунта сохраняются.
#[must_use]
pub fn reresolve_inventory_snapshot(
    snapshot: &ResolvedInventorySnapshot,
    catalog: &ItemCatalog,
    platform: Platform,
) -> InventoryReresolutionResult {
    let lookup = catalog_lookup(&catalog.items);
    let mut updated = snapshot.clone();
    let mut revived_item_count = 0_u64;
    let mut unresolved_item_count = 0_u64;
    let mut issues = Vec::new();

    for item in &mut updated.items {
        let previous_resolution = item.resolution;
        let catalog_item = match saved_catalog_item(item, &lookup) {
            Ok(catalog_item) => catalog_item,
            Err(kind) => {
                item.key = None;
                item.resolution = match kind {
                    InventoryReresolutionIssueKind::CatalogItemAmbiguous => {
                        InventoryResolution::AmbiguousItem
                    }
                    _ => InventoryResolution::UnknownItem,
                };
                item.sellable_quantity = 0;
                issues.push(InventoryReresolutionIssue {
                    canonical_game_id: item.canonical_game_id.clone(),
                    kind,
                });
                unresolved_item_count = unresolved_item_count.saturating_add(1);
                continue;
            }
        };

        item.display_name_en = Some(catalog_item.display_name_en.clone());
        item.display_name_ru
            .clone_from(&catalog_item.display_name_ru);
        item.tags.clone_from(&catalog_item.tags);

        if is_built_equipment_set_alias(catalog_item, &item.canonical_game_id) {
            item.key = None;
            item.resolution = InventoryResolution::UnknownItem;
            item.sellable_quantity = 0;
            issues.push(InventoryReresolutionIssue {
                canonical_game_id: item.canonical_game_id.clone(),
                kind: InventoryReresolutionIssueKind::BuiltEquipmentSetAlias,
            });
            unresolved_item_count = unresolved_item_count.saturating_add(1);
            continue;
        }

        match saved_market_variant(item, catalog_item, platform) {
            Ok((key, subtype)) => {
                item.key = Some(key);
                item.subtype = subtype;
                item.resolution = InventoryResolution::Resolved;
            }
            Err(kind) => {
                item.key = None;
                item.resolution = if previous_resolution == InventoryResolution::UnknownItem {
                    InventoryResolution::UnknownItem
                } else {
                    InventoryResolution::ExactVariantUnavailable
                };
                issues.push(InventoryReresolutionIssue {
                    canonical_game_id: item.canonical_game_id.clone(),
                    kind,
                });
            }
        }

        item.sellable_quantity = if !updated.mod_usage_scanned && is_mod(&item.tags) {
            0
        } else {
            sellable_quantity(
                item.resolution,
                item.owned_quantity,
                item.tradeable_quantity,
                item.untradeable_quantity,
                item.unknown_quantity,
                item.equipped_tradeable_quantity,
                updated.keep_copies,
            )
        };
        if item.resolution == InventoryResolution::Resolved {
            if previous_resolution != InventoryResolution::Resolved {
                revived_item_count = revived_item_count.saturating_add(1);
            }
        } else {
            unresolved_item_count = unresolved_item_count.saturating_add(1);
        }
    }

    let requires_inventory_rescan = issues
        .iter()
        .any(|issue| issue.kind.requires_inventory_rescan());
    InventoryReresolutionResult {
        snapshot: updated,
        revived_item_count,
        unresolved_item_count,
        requires_inventory_rescan,
        issues,
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
        item.sellable_quantity = if !snapshot.mod_usage_scanned && is_mod(&item.tags) {
            0
        } else {
            sellable_quantity(
                item.resolution,
                item.owned_quantity,
                item.tradeable_quantity,
                item.untradeable_quantity,
                item.unknown_quantity,
                item.equipped_tradeable_quantity,
                keep_copies,
            )
        };
    }
    updated
}

#[allow(clippy::too_many_lines)] // Идентификация варианта и количества — одно атомарное решение.
fn resolve_group(
    group: GroupKey,
    quantities: &GroupQuantities,
    equipped: &[&EquippedModInstance],
    lookup: &HashMap<String, Vec<&CatalogItem>>,
    platform: Platform,
    keep_copies: u32,
    mod_usage_scanned: bool,
) -> ResolvedInventoryItem {
    let lookup_key = normalized_identifier(&group.canonical_game_id);
    let candidates = lookup
        .get(&lookup_key)
        .or_else(|| {
            unrevealed_riven_market_slug(&group.canonical_game_id)
                .and_then(|slug| lookup.get(&normalized_identifier(slug)))
        })
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
        [catalog_item] if is_built_equipment_set_alias(catalog_item, &group.canonical_game_id) => (
            Some(catalog_item.display_name_en.clone()),
            catalog_item.display_name_ru.clone(),
            catalog_item.tags.clone(),
            None,
            group.subtype.clone(),
            InventoryResolution::UnknownItem,
        ),
        [catalog_item] if !group.market_match_allowed => (
            Some(catalog_item.display_name_en.clone()),
            catalog_item.display_name_ru.clone(),
            catalog_item.tags.clone(),
            None,
            group.subtype.clone(),
            InventoryResolution::ExactVariantUnavailable,
        ),
        [catalog_item] => {
            let subtype = group.subtype.clone().or_else(|| {
                catalog_item
                    .subtypes
                    .iter()
                    .any(|candidate| candidate == "regular")
                    .then(|| "regular".to_owned())
            });
            let (rank, rank_supported) =
                normalized_variant_dimension(group.rank, catalog_item.max_rank);
            let (charges, charges_supported) =
                normalized_variant_dimension(group.charges, catalog_item.max_charges);
            let (amber_stars, amber_supported) =
                normalized_variant_dimension(group.amber_stars, catalog_item.max_amber_stars);
            let (cyan_stars, cyan_supported) =
                normalized_variant_dimension(group.cyan_stars, catalog_item.max_cyan_stars);
            let subtype_supported = if catalog_item.subtypes.is_empty() {
                group.subtype.is_none()
            } else {
                subtype
                    .as_ref()
                    .is_some_and(|value| catalog_item.subtypes.contains(value))
            };
            let key =
                MarketVariantKey::new(catalog_item.slug.clone(), platform, rank, subtype.clone())
                    .expect("validated inventory identity creates a valid market key")
                    .with_charges(charges)
                    .with_stars(amber_stars, cyan_stars);
            let exact_variant_supported = rank_supported
                && charges_supported
                && amber_supported
                && cyan_supported
                && subtype_supported;
            let resolution = if group.market_match_allowed && exact_variant_supported {
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

    let sellable_quantity = if !mod_usage_scanned && is_mod(&tags) {
        0
    } else {
        sellable_quantity(
            resolution,
            quantities.owned,
            quantities.tradeable,
            quantities.untradeable,
            quantities.unknown,
            equipped_tradeable_quantity,
            keep_copies,
        )
    };
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

fn saved_catalog_item<'catalog>(
    item: &ResolvedInventoryItem,
    lookup: &HashMap<String, Vec<&'catalog CatalogItem>>,
) -> Result<&'catalog CatalogItem, InventoryReresolutionIssueKind> {
    let canonical_id = normalized_identifier(&item.canonical_game_id);
    let exact_game_ref = lookup
        .get(&canonical_id)
        .into_iter()
        .flatten()
        .copied()
        .filter(|candidate| {
            candidate
                .game_ref
                .as_deref()
                .is_some_and(|game_ref| normalized_identifier(game_ref) == canonical_id)
        })
        .collect::<Vec<_>>();
    match exact_game_ref.as_slice() {
        [catalog_item] => return Ok(*catalog_item),
        [_, _, ..] => return Err(InventoryReresolutionIssueKind::CatalogItemAmbiguous),
        [] => {}
    }

    if let Some(alias_slug) = unrevealed_riven_market_slug(&item.canonical_game_id) {
        let normalized_slug = normalized_identifier(alias_slug);
        let matching_alias = lookup
            .get(&normalized_slug)
            .into_iter()
            .flatten()
            .copied()
            .filter(|candidate| normalized_identifier(&candidate.slug) == normalized_slug)
            .collect::<Vec<_>>();
        match matching_alias.as_slice() {
            [catalog_item] => return Ok(*catalog_item),
            [_, _, ..] => return Err(InventoryReresolutionIssueKind::CatalogItemAmbiguous),
            [] => {}
        }
    }

    // У неразрешённой строки прежний slug мог быть результатом ошибочной
    // эвристики. Повторно использовать его безопасно только для строки, которая
    // уже была точно разрешена до обновления каталога.
    if item.resolution == InventoryResolution::Resolved
        && let Some(saved_key) = &item.key
    {
        let normalized_slug = normalized_identifier(&saved_key.slug);
        let matching_slug = lookup
            .get(&normalized_slug)
            .into_iter()
            .flatten()
            .copied()
            .filter(|candidate| normalized_identifier(&candidate.slug) == normalized_slug)
            .collect::<Vec<_>>();
        return match matching_slug.as_slice() {
            [catalog_item] => Ok(*catalog_item),
            [_, _, ..] => Err(InventoryReresolutionIssueKind::CatalogItemAmbiguous),
            [] => Err(InventoryReresolutionIssueKind::CatalogItemMissing),
        };
    }

    Err(InventoryReresolutionIssueKind::CatalogItemMissing)
}

fn saved_market_variant(
    item: &ResolvedInventoryItem,
    catalog_item: &CatalogItem,
    platform: Platform,
) -> Result<(MarketVariantKey, Option<String>), InventoryReresolutionIssueKind> {
    let rank = saved_variant_dimension(
        item.rank,
        catalog_item.max_rank,
        InventoryReresolutionIssueKind::RankMissing,
        InventoryReresolutionIssueKind::RankOutOfRange,
    )?;
    let saved_charges = item.key.as_ref().and_then(|key| key.charges);
    let charges = saved_variant_dimension(
        saved_charges,
        catalog_item.max_charges,
        InventoryReresolutionIssueKind::ChargesMissing,
        InventoryReresolutionIssueKind::ChargesOutOfRange,
    )?;
    let saved_amber_stars = item.key.as_ref().and_then(|key| key.amber_stars);
    let saved_cyan_stars = item.key.as_ref().and_then(|key| key.cyan_stars);
    let amber_stars = saved_variant_dimension(
        saved_amber_stars,
        catalog_item.max_amber_stars,
        InventoryReresolutionIssueKind::AyatanStarsMissing,
        InventoryReresolutionIssueKind::AyatanStarsOutOfRange,
    )?;
    let cyan_stars = saved_variant_dimension(
        saved_cyan_stars,
        catalog_item.max_cyan_stars,
        InventoryReresolutionIssueKind::AyatanStarsMissing,
        InventoryReresolutionIssueKind::AyatanStarsOutOfRange,
    )?;
    let subtype = saved_market_subtype(item, catalog_item)?;
    let key = MarketVariantKey::new(catalog_item.slug.clone(), platform, rank, subtype.clone())
        .map_err(|_| InventoryReresolutionIssueKind::InvalidCatalogVariant)?
        .with_charges(charges)
        .with_stars(amber_stars, cyan_stars);
    Ok((key, subtype))
}

const fn saved_variant_dimension(
    value: Option<u16>,
    maximum: Option<u16>,
    missing: InventoryReresolutionIssueKind,
    out_of_range: InventoryReresolutionIssueKind,
) -> Result<Option<u16>, InventoryReresolutionIssueKind> {
    match (value, maximum) {
        (_, None) => Ok(None),
        (None, Some(_)) => Err(missing),
        (Some(value), Some(maximum)) if value <= maximum => Ok(Some(value)),
        (Some(_), Some(_)) => Err(out_of_range),
    }
}

fn saved_market_subtype(
    item: &ResolvedInventoryItem,
    catalog_item: &CatalogItem,
) -> Result<Option<String>, InventoryReresolutionIssueKind> {
    if catalog_item.subtypes.is_empty() {
        return Ok(None);
    }
    if unrevealed_riven_market_slug(&item.canonical_game_id)
        .is_some_and(|slug| slug == catalog_item.slug)
        && catalog_item
            .subtypes
            .iter()
            .any(|candidate| candidate == "unrevealed")
    {
        return Ok(Some("unrevealed".to_owned()));
    }
    let saved_subtype = item
        .subtype
        .as_deref()
        .or_else(|| item.key.as_ref().and_then(|key| key.subtype.as_deref()));
    if let Some(saved_subtype) = saved_subtype {
        return catalog_item
            .subtypes
            .iter()
            .any(|candidate| candidate == saved_subtype)
            .then(|| Some(saved_subtype.to_owned()))
            .ok_or(InventoryReresolutionIssueKind::SubtypeUnsupported);
    }
    catalog_item
        .subtypes
        .iter()
        .any(|candidate| candidate == "regular")
        .then(|| Some("regular".to_owned()))
        .ok_or(InventoryReresolutionIssueKind::SubtypeMissing)
}

const fn normalized_variant_dimension(
    value: Option<u16>,
    maximum: Option<u16>,
) -> (Option<u16>, bool) {
    match maximum {
        // Рынок не различает это измерение для предмета. Игровое значение
        // намеренно не переносится в ключ WFM.
        None => (None, true),
        Some(maximum) => (value, matches!(value, Some(value) if value <= maximum)),
    }
}

fn is_mod(tags: &[String]) -> bool {
    tags.iter().any(|tag| tag == "mod")
}

fn is_built_equipment_set_alias(catalog_item: &CatalogItem, canonical_game_id: &str) -> bool {
    (catalog_item.tags.iter().any(|tag| tag == "set") || catalog_item.slug.ends_with("_set"))
        && canonical_game_id.starts_with("/Lotus/")
        && !canonical_game_id.contains("/Types/Recipes/")
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
    if resolution != InventoryResolution::Resolved {
        return 0;
    }
    let protected = untradeable
        .saturating_add(unknown)
        .saturating_add(equipped_tradeable);
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
    #[error("charges must be within 0..={MAX_CHARGES}, received {0}")]
    InvalidCharges(u16),
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
                max_charges: None,
                max_amber_stars: None,
                max_cyan_stars: None,
                subtypes: Vec::new(),
                tags: vec!["mod".into()],
            }],
        }
    }

    fn veiled_rifle_riven_catalog() -> ItemCatalog {
        let mut result = catalog();
        let item = &mut result.items[0];
        item.item_id = "rifle-riven-id".into();
        item.slug = "rifle_riven_mod_(veiled)".into();
        item.display_name_en = "Rifle Riven Mod (Veiled)".into();
        item.display_name_ru = Some("Мод Разлома для винтовки (закрытый)".into());
        item.game_ref = Some("/Lotus/Upgrades/Mods/Randomized/LotusRifleRandomModRare".into());
        item.max_rank = None;
        item.subtypes = vec!["unrevealed".into(), "revealed".into()];
        item.tags = vec!["mod".into(), "riven_mod".into(), "veiled_riven".into()];
        result
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
            mod_usage_scanned: true,
            equipped_mods: Vec::new(),
            credits: None,
            syndicates: Vec::new(),
        }
    }

    fn saved_item(
        canonical_game_id: &str,
        rank: Option<u16>,
        resolution: InventoryResolution,
    ) -> ResolvedInventoryItem {
        ResolvedInventoryItem {
            canonical_game_id: canonical_game_id.into(),
            display_name_en: None,
            display_name_ru: None,
            tags: Vec::new(),
            key: None,
            rank,
            subtype: None,
            owned_quantity: 4,
            tradeable_quantity: 3,
            untradeable_quantity: 0,
            unknown_quantity: 1,
            leveled_quantity: 1,
            equipped_quantity: 1,
            equipped_tradeable_quantity: 1,
            equipped_placements: vec![ResolvedModPlacement {
                equipment_instance_key: "equipment-1".into(),
                equipment_game_id: "/Lotus/Test/Weapon".into(),
                equipment_display_name_en: Some("Test Weapon".into()),
                equipment_display_name_ru: Some("Тестовое оружие".into()),
                equipment_image_url: Some("https://example.test/weapon.png".into()),
                equipment_kind: EquipmentKind::Primary,
                config_index: 2,
            }],
            sellable_quantity: 0,
            resolution,
        }
    }

    fn saved_snapshot(items: Vec<ResolvedInventoryItem>) -> ResolvedInventorySnapshot {
        ResolvedInventorySnapshot {
            metadata: InventorySnapshotMetadata {
                source: InventorySource::ReadOnlyScan,
                observed_at: Utc.with_ymd_and_hms(2026, 8, 27, 1, 2, 3).unwrap(),
                schema_version: 1,
                item_count: items.len() as u64,
                checksum_sha256: "saved-inventory".into(),
            },
            keep_copies: 1,
            mod_usage_scanned: true,
            credits: Some(123_456),
            syndicates: vec![SyndicateStanding {
                tag: "SteelMeridianSyndicate".into(),
                standing: 42_000,
                title: Some("General".into()),
            }],
            items,
        }
    }

    #[test]
    fn reresolution_revives_unknown_and_unavailable_rows_by_exact_game_ref() {
        let canonical_game_id = "/Lotus/Upgrades/Mods/PrimedFlow";
        let unknown = saved_item(canonical_game_id, Some(5), InventoryResolution::UnknownItem);
        let mut unavailable = saved_item(
            canonical_game_id,
            Some(7),
            InventoryResolution::ExactVariantUnavailable,
        );
        unavailable.key = Some(
            MarketVariantKey::new("primed_flow", Platform::Pc, Some(7), None::<String>)
                .expect("old key"),
        );
        let original = saved_snapshot(vec![unknown, unavailable]);

        let result = reresolve_inventory_snapshot(&original, &catalog(), Platform::Xbox);

        assert_eq!(result.revived_item_count, 2);
        assert_eq!(result.unresolved_item_count, 0);
        assert!(!result.requires_inventory_rescan);
        assert!(result.issues.is_empty());
        assert!(result.snapshot.items.iter().all(|item| {
            item.resolution == InventoryResolution::Resolved
                && item
                    .key
                    .as_ref()
                    .is_some_and(|key| key.platform == Platform::Xbox && key.rank == item.rank)
        }));
        assert_eq!(result.snapshot.items[0].sellable_quantity, 2);
        assert_eq!(result.snapshot.metadata, original.metadata);
        assert_eq!(result.snapshot.keep_copies, original.keep_copies);
        assert_eq!(result.snapshot.credits, original.credits);
        assert_eq!(result.snapshot.syndicates, original.syndicates);
        assert_eq!(
            result.snapshot.items[0].equipped_placements,
            original.items[0].equipped_placements
        );
        assert_eq!(
            result.snapshot.items[0].tradeable_quantity,
            original.items[0].tradeable_quantity
        );
        assert_eq!(
            result.snapshot.items[0].unknown_quantity,
            original.items[0].unknown_quantity
        );
    }

    #[test]
    fn reresolution_keeps_raw_rank_out_of_unranked_market_key() {
        let mut item = saved_item(
            "/Lotus/Test/UnrankedItem",
            Some(30),
            InventoryResolution::Resolved,
        );
        item.key = Some(
            MarketVariantKey::new("unranked_item", Platform::Pc, Some(30), None::<String>)
                .expect("legacy key"),
        );
        let mut item_catalog = catalog();
        item_catalog.items[0].slug = "unranked_item".into();
        item_catalog.items[0].game_ref = Some("/Lotus/Test/UnrankedItem".into());
        item_catalog.items[0].max_rank = None;

        let result = reresolve_inventory_snapshot(
            &saved_snapshot(vec![item]),
            &item_catalog,
            Platform::Switch,
        );
        let resolved = &result.snapshot.items[0];

        assert_eq!(resolved.rank, Some(30));
        assert_eq!(resolved.key.as_ref().and_then(|key| key.rank), None);
        assert_eq!(
            resolved.key.as_ref().map(|key| key.platform),
            Some(Platform::Switch)
        );
        assert_eq!(resolved.resolution, InventoryResolution::Resolved);
    }

    #[test]
    fn reresolution_requires_rescan_for_old_ayatan_without_star_mask() {
        let canonical_game_id = "/Lotus/Types/Items/FusionTreasures/Anasa";
        let original = saved_snapshot(vec![saved_item(
            canonical_game_id,
            None,
            InventoryResolution::UnknownItem,
        )]);
        let mut item_catalog = catalog();
        item_catalog.items[0].slug = "ayatan_anasa_sculpture".into();
        item_catalog.items[0].game_ref = Some(canonical_game_id.into());
        item_catalog.items[0].max_rank = None;
        item_catalog.items[0].max_amber_stars = Some(2);
        item_catalog.items[0].max_cyan_stars = Some(2);
        item_catalog.items[0].tags = vec!["ayatan_sculpture".into()];

        let result = reresolve_inventory_snapshot(&original, &item_catalog, Platform::Playstation);

        assert_eq!(result.revived_item_count, 0);
        assert_eq!(result.unresolved_item_count, 1);
        assert!(result.requires_inventory_rescan);
        assert_eq!(
            result.issues,
            vec![InventoryReresolutionIssue {
                canonical_game_id: canonical_game_id.into(),
                kind: InventoryReresolutionIssueKind::AyatanStarsMissing,
            }]
        );
        assert_eq!(
            result.snapshot.items[0].resolution,
            InventoryResolution::UnknownItem
        );
        assert!(result.snapshot.items[0].key.is_none());
        assert_eq!(result.snapshot.credits, original.credits);
        assert_eq!(result.snapshot.syndicates, original.syndicates);
    }

    #[test]
    fn reresolution_never_turns_built_equipment_into_market_set() {
        let canonical_game_id = "/Lotus/Powersuits/Rhino/RhinoPrime";
        let mut item_catalog = catalog();
        item_catalog.items[0].slug = "rhino_prime_set".into();
        item_catalog.items[0].game_ref = Some(canonical_game_id.into());
        item_catalog.items[0].max_rank = None;
        item_catalog.items[0].tags.clear();
        let original = saved_snapshot(vec![saved_item(
            canonical_game_id,
            Some(30),
            InventoryResolution::UnknownItem,
        )]);

        let result = reresolve_inventory_snapshot(&original, &item_catalog, Platform::Pc);

        assert_eq!(result.revived_item_count, 0);
        assert_eq!(result.unresolved_item_count, 1);
        assert!(!result.requires_inventory_rescan);
        assert_eq!(
            result.issues[0].kind,
            InventoryReresolutionIssueKind::BuiltEquipmentSetAlias
        );
        assert_eq!(
            result.snapshot.items[0].resolution,
            InventoryResolution::UnknownItem
        );
        assert!(result.snapshot.items[0].key.is_none());
        assert_eq!(result.snapshot.items[0].sellable_quantity, 0);
    }

    #[test]
    fn reresolution_does_not_revive_unknown_row_by_slug_only() {
        let mut item_catalog = catalog();
        item_catalog.items[0].game_ref = None;
        let original = saved_snapshot(vec![saved_item(
            "primed_flow",
            Some(5),
            InventoryResolution::UnknownItem,
        )]);

        let result = reresolve_inventory_snapshot(&original, &item_catalog, Platform::Pc);

        assert_eq!(result.revived_item_count, 0);
        assert_eq!(result.unresolved_item_count, 1);
        assert_eq!(
            result.issues[0].kind,
            InventoryReresolutionIssueKind::CatalogItemMissing
        );
        assert_eq!(
            result.snapshot.items[0].resolution,
            InventoryResolution::UnknownItem
        );
    }

    #[test]
    fn resolver_uses_catalog_variant_shape_instead_of_daily_bulk_presence() {
        let source = inventory(vec![InventoryItem {
            canonical_game_id: "/Lotus/Upgrades/Mods/PrimedFlow".into(),
            quantity: 3,
            rank: Some(0),
            charges: None,
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
            market_match_allowed: true,
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

        let without_daily_row = resolve_inventory(
            &source,
            &catalog(),
            &HashSet::<MarketVariantKey>::new(),
            Platform::Pc,
            1,
        );
        assert_eq!(
            without_daily_row.items[0].resolution,
            InventoryResolution::Resolved
        );
        assert_eq!(without_daily_row.items[0].sellable_quantity, 2);

        let mut invalid_source = source;
        invalid_source.items[0].rank = Some(11);
        let invalid = resolve_inventory(
            &invalid_source,
            &catalog(),
            &HashSet::<MarketVariantKey>::new(),
            Platform::Pc,
            0,
        );
        assert_eq!(
            invalid.items[0].resolution,
            InventoryResolution::ExactVariantUnavailable
        );
        assert_eq!(invalid.items[0].sellable_quantity, 0);
    }

    #[test]
    fn resolver_maps_missing_inventory_subtype_to_regular_market_variant() {
        let source = inventory(vec![InventoryItem {
            canonical_game_id: "/Lotus/Upgrades/Mods/PrimedFlow".into(),
            quantity: 2,
            rank: Some(5),
            charges: None,
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
            market_match_allowed: true,
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
            charges: None,
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
            market_match_allowed: true,
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
    fn unknown_copy_does_not_block_confirmed_tradeable_copies() {
        let source = inventory(vec![
            InventoryItem {
                canonical_game_id: "primed_flow".into(),
                quantity: 4,
                rank: Some(0),
                charges: None,
                subtype: None,
                amber_stars: None,
                cyan_stars: None,
                market_match_allowed: true,
                tradeability: Tradeability::Unknown,
                leveled: false,
            },
            InventoryItem {
                canonical_game_id: "primed_flow".into(),
                quantity: 1,
                rank: Some(0),
                charges: None,
                subtype: None,
                amber_stars: None,
                cyan_stars: None,
                market_match_allowed: true,
                tradeability: Tradeability::Tradeable,
                leveled: false,
            },
        ]);
        let variants = HashSet::from([MarketVariantKey::new(
            "primed_flow",
            Platform::Pc,
            Some(0),
            None::<String>,
        )
        .expect("key")]);
        let resolved = resolve_inventory(&source, &catalog(), &variants, Platform::Pc, 0);
        assert_eq!(resolved.items[0].unknown_quantity, 4);
        assert_eq!(resolved.items[0].tradeable_quantity, 1);
        assert_eq!(resolved.items[0].sellable_quantity, 1);
    }

    #[test]
    fn rank_is_omitted_when_catalog_item_has_no_rank_variants() {
        let mut item_catalog = catalog();
        item_catalog.items[0].max_rank = None;
        item_catalog.items[0].max_charges = Some(3);
        let source = inventory(vec![InventoryItem {
            canonical_game_id: "/Lotus/Upgrades/Mods/PrimedFlow".into(),
            quantity: 2,
            rank: Some(5),
            charges: Some(2),
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
            market_match_allowed: true,
            tradeability: Tradeability::Tradeable,
            leveled: true,
        }]);

        let resolved = resolve_inventory(
            &source,
            &item_catalog,
            &HashSet::<MarketVariantKey>::new(),
            Platform::Pc,
            0,
        );

        assert_eq!(resolved.items[0].resolution, InventoryResolution::Resolved);
        assert_eq!(resolved.items[0].rank, Some(5));
        assert_eq!(resolved.items[0].key.as_ref().unwrap().rank, None);
        assert_eq!(resolved.items[0].key.as_ref().unwrap().charges, Some(2));
        assert_eq!(resolved.items[0].sellable_quantity, 2);
    }

    #[test]
    fn built_equipment_is_not_mapped_to_market_set() {
        let raw =
            r#"{"LongGuns":[{"ItemType":"/Lotus/Weapons/ClanTech/Energy/DeraVandal","XP":0}]}"#;
        let parsed = parse_read_only_scan_json(raw).expect("equipment inventory parses");
        assert_eq!(parsed.items[0].tradeability, Tradeability::Unknown);
        assert!(!parsed.items[0].market_match_allowed);

        let mut item_catalog = catalog();
        let item = &mut item_catalog.items[0];
        item.slug = "dera_vandal_set".into();
        item.display_name_en = "Dera Vandal Set".into();
        item.game_ref = Some("/Lotus/Weapons/ClanTech/Energy/DeraVandal".into());
        item.max_rank = None;
        item.tags = vec!["set".into(), "weapon".into()];

        let resolved = resolve_inventory(
            &parsed,
            &item_catalog,
            &HashSet::<MarketVariantKey>::new(),
            Platform::Pc,
            0,
        );
        assert_eq!(
            resolved.items[0].resolution,
            InventoryResolution::UnknownItem
        );
        assert!(resolved.items[0].key.is_none());
        assert_eq!(resolved.items[0].sellable_quantity, 0);

        // Защита resolver не зависит от нового флага сканера: старый или
        // импортированный снимок тоже не оживит подмену собранного предмета сетом.
        let mut legacy = parsed;
        legacy.items[0].market_match_allowed = true;
        legacy.items[0].tradeability = Tradeability::Tradeable;
        let resolved = resolve_inventory(
            &legacy,
            &item_catalog,
            &HashSet::<MarketVariantKey>::new(),
            Platform::Pc,
            0,
        );
        assert!(resolved.items[0].key.is_none());
        assert_eq!(resolved.items[0].sellable_quantity, 0);
    }

    #[test]
    fn failed_mod_usage_scan_blocks_only_mod_sales() {
        let raw = r#"{
            "MiscItems":[{"ItemType":"/Lotus/Test/Part","ItemCount":2}],
            "RawUpgrades":[{"ItemType":"/Lotus/Upgrades/Mods/PrimedFlow","ItemCount":3}],
            "Suits":[{"ItemType":"/Lotus/Powersuits/Volt/VoltPrime","Configs":"unexpected"}]
        }"#;
        let parsed = parse_read_only_scan_json(raw).expect("base inventory remains usable");
        assert!(!parsed.mod_usage_scanned);

        let mut item_catalog = catalog();
        item_catalog.items.push(CatalogItem {
            item_id: "part-id".into(),
            slug: "test_part".into(),
            display_name_en: "Test Part".into(),
            display_name_ru: None,
            thumb: None,
            thumb_ru: None,
            game_ref: Some("/Lotus/Test/Part".into()),
            bulk_tradable: false,
            max_rank: None,
            max_charges: None,
            max_amber_stars: None,
            max_cyan_stars: None,
            subtypes: Vec::new(),
            tags: vec!["component".into()],
        });
        let resolved = resolve_inventory(
            &parsed,
            &item_catalog,
            &HashSet::<MarketVariantKey>::new(),
            Platform::Pc,
            0,
        );
        let mod_item = resolved
            .items
            .iter()
            .find(|item| {
                item.key
                    .as_ref()
                    .is_some_and(|key| key.slug == "primed_flow")
            })
            .expect("mod resolved");
        let part = resolved
            .items
            .iter()
            .find(|item| item.key.as_ref().is_some_and(|key| key.slug == "test_part"))
            .expect("part resolved");
        assert_eq!(mod_item.sellable_quantity, 0);
        assert_eq!(part.sellable_quantity, 2);

        let recalculated = apply_keep_copies(&resolved, 0);
        let mod_item = recalculated
            .items
            .iter()
            .find(|item| {
                item.key
                    .as_ref()
                    .is_some_and(|key| key.slug == "primed_flow")
            })
            .unwrap();
        assert_eq!(mod_item.sellable_quantity, 0);
    }

    #[test]
    fn ayatan_socket_mask_becomes_exact_star_counts() {
        let raw = r#"{"FusionTreasures":[
            {"ItemType":"/Lotus/Types/Items/FusionTreasures/OroFusexF","ItemCount":2,"Sockets":0},
            {"ItemType":"/Lotus/Types/Items/FusionTreasures/OroFusexF","ItemCount":1,"Sockets":15}
        ]}"#;
        let parsed = parse_read_only_scan_json(raw).expect("Ayatan inventory parses");
        assert_eq!(parsed.items.len(), 2);
        assert!(parsed.items.iter().any(|item| {
            item.quantity == 2 && item.amber_stars == Some(0) && item.cyan_stars == Some(0)
        }));
        assert!(parsed.items.iter().any(|item| {
            item.quantity == 1 && item.amber_stars == Some(2) && item.cyan_stars == Some(2)
        }));

        let mut item_catalog = catalog();
        let item = &mut item_catalog.items[0];
        item.slug = "ayatan_anasa_sculpture".into();
        item.display_name_en = "Ayatan Anasa Sculpture".into();
        item.game_ref = Some("/Lotus/Types/Items/FusionTreasures/OroFusexF".into());
        item.max_rank = None;
        item.max_amber_stars = Some(2);
        item.max_cyan_stars = Some(2);
        item.tags = vec!["ayatan_sculpture".into()];

        let resolved = resolve_inventory(
            &parsed,
            &item_catalog,
            &HashSet::<MarketVariantKey>::new(),
            Platform::Pc,
            0,
        );
        assert_eq!(resolved.items.len(), 2);
        assert!(resolved.items.iter().all(|item| {
            item.resolution == InventoryResolution::Resolved && item.sellable_quantity > 0
        }));
        assert!(resolved.items.iter().any(|item| {
            item.key
                .as_ref()
                .is_some_and(|key| key.amber_stars == Some(2) && key.cyan_stars == Some(2))
        }));
    }

    #[test]
    fn read_only_scan_has_a_distinct_trusted_source() {
        let raw = r#"{
            "Inventory": {
                "MiscItems": [{"ItemType":"/Lotus/Test/Part","ItemCount":2}],
                "RawUpgrades": [{"ItemType":"/Lotus/Test/Mod","ItemCount":3}],
                "Upgrades": [
                    {"ItemType":"/Lotus/Test/Mod","UpgradeFingerprint":"{\"lvl\":5}"},
                    {"ItemType":"/Lotus/Test/Arcane","UpgradeFingerprint":"{\"charges\":2}"}
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
        assert_eq!(parsed.items[3].charges, Some(2));
        assert!(!parsed.items[3].leveled);
        assert_eq!(parsed.items[4].tradeability, Tradeability::Unknown);
        assert!(!parsed.items[4].market_match_allowed);
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

    #[test]
    fn veiled_rivens_use_exact_market_subtypes_but_unique_rolls_stay_unpriced() {
        let raw = r#"{
            "RawUpgrades": [
                {"ItemType":"/Lotus/Upgrades/Mods/Randomized/RawRifleRandomMod","ItemCount":2}
            ],
            "Upgrades": [
                {
                    "ItemType":"/Lotus/Upgrades/Mods/Randomized/LotusRifleRandomModRare",
                    "UpgradeFingerprint":"{\"challenge\":{\"Type\":\"/Lotus/Test/Challenge\",\"Progress\":0,\"Required\":3}}"
                },
                {
                    "ItemType":"/Lotus/Upgrades/Mods/Randomized/LotusRifleRandomModRare",
                    "UpgradeFingerprint":"{\"compat\":\"/Lotus/Weapons/Test\",\"lvl\":8,\"buffs\":[{\"Tag\":\"Damage\",\"Value\":1}],\"curses\":[]}"
                }
            ]
        }"#;
        let parsed = parse_read_only_scan_json(raw).expect("Riven inventory parses");
        let resolved = resolve_inventory(
            &parsed,
            &veiled_rifle_riven_catalog(),
            &HashSet::<MarketVariantKey>::new(),
            Platform::Pc,
            0,
        );

        let unrevealed = resolved
            .items
            .iter()
            .find(|item| item.subtype.as_deref() == Some("unrevealed"))
            .expect("stacked unrevealed Riven");
        assert_eq!(unrevealed.owned_quantity, 2);
        assert_eq!(unrevealed.resolution, InventoryResolution::Resolved);
        assert_eq!(unrevealed.sellable_quantity, 2);

        let revealed = resolved
            .items
            .iter()
            .find(|item| item.subtype.as_deref() == Some("revealed"))
            .expect("challenge-revealed Riven");
        assert_eq!(revealed.resolution, InventoryResolution::Resolved);
        assert_eq!(revealed.sellable_quantity, 1);

        let unique = resolved
            .items
            .iter()
            .find(|item| item.rank == Some(8))
            .expect("unique unveiled Riven");
        assert_eq!(unique.subtype, None);
        assert_eq!(
            unique.resolution,
            InventoryResolution::ExactVariantUnavailable
        );
        assert_eq!(unique.sellable_quantity, 0);
    }

    #[test]
    fn old_unresolved_raw_riven_is_recovered_without_another_scan() {
        let canonical_game_id = "/Lotus/Upgrades/Mods/Randomized/RawRifleRandomMod";
        let original = saved_snapshot(vec![saved_item(
            canonical_game_id,
            Some(0),
            InventoryResolution::UnknownItem,
        )]);

        let result =
            reresolve_inventory_snapshot(&original, &veiled_rifle_riven_catalog(), Platform::Pc);

        assert_eq!(result.revived_item_count, 1);
        assert_eq!(result.unresolved_item_count, 0);
        assert_eq!(
            result.snapshot.items[0].subtype.as_deref(),
            Some("unrevealed")
        );
        assert_eq!(
            result.snapshot.items[0]
                .key
                .as_ref()
                .and_then(|key| key.subtype.as_deref()),
            Some("unrevealed")
        );
        assert_eq!(
            result.snapshot.items[0].resolution,
            InventoryResolution::Resolved
        );
    }
}
