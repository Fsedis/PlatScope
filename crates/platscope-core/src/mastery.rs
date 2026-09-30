//! История аккаунта хранится отдельно от текущих копий и торговых резервов.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use platscope_domain::{GameMetadataSnapshot, MasteryItemDefinition, ResolvedInventorySnapshot};
use platscope_storage::Database;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::CoreError;

const CACHE_KEY: &str = "mastery.account_history.v2";
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MasteryCache {
    active_account: String,
    inventory_checksum: String,
    accounts: BTreeMap<String, AccountHistory>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountHistory {
    observed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    equipment_observed_at: Option<DateTime<Utc>>,
    refresh_failed: bool,
    entries: BTreeMap<String, u64>,
    #[serde(default)]
    equipment: BTreeMap<String, EquipmentEvidence>,
    /// Только текущие копии; накопленные доказательства освоения для этого не подходят.
    #[serde(default)]
    owned_equipment: Option<BTreeMap<String, u32>>,
    #[serde(default)]
    account_rank: Option<u8>,
}

pub(crate) struct PlannerAccount {
    pub key: String,
    pub owned_equipment: BTreeMap<String, u32>,
    pub rank: Option<u8>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EquipmentEvidence {
    xp: u64,
    polarized: u64,
    /// Опыт и пять поляризаций должны относиться к одному экземпляру в одном снимке.
    xp_with_five_polarizations: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasteryView {
    pub observed_at: Option<DateTime<Utc>>,
    pub source: Option<&'static str>,
    pub refresh_failed: bool,
    pub catalog_available: bool,
    pub items: Vec<MasteryItemView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasteryItemView {
    pub game_ref: String,
    pub display_name: String,
    pub display_name_en: String,
    pub category: String,
    pub image_url: Option<String>,
    pub max_rank: Option<u8>,
    pub xp: Option<u64>,
    pub mastery_rank: Option<u8>,
    pub status: &'static str,
    pub reason: &'static str,
    pub set_slugs: Vec<String>,
}

pub struct MasteryService;

impl MasteryService {
    /// Сохраняет только нормализованную историю и хеш идентификатора аккаунта.
    /// Отсутствующая или повреждённая история не стирает прежние записи.
    ///
    /// # Errors
    /// Возвращает ошибку локального хранилища. Сессионные данные в ошибку не входят.
    pub fn capture(
        database: &Mutex<Database>,
        raw: &str,
        account_id: &str,
        inventory_checksum: &str,
    ) -> Result<(), CoreError> {
        let mut hasher = Sha256::new();
        hasher.update(b"platscope-mastery-account-v1:");
        hasher.update(account_id.to_ascii_lowercase().as_bytes());
        let account_key = hex::encode(hasher.finalize());
        let parsed = parse_history(raw);
        let equipment = parse_equipment(raw);
        let guard = database
            .lock()
            .map_err(|_| CoreError::DatabaseState("unavailable".into()))?;
        let mut cache = guard
            .get_setting::<MasteryCache>(CACHE_KEY)?
            .unwrap_or_default();
        update_cache(
            &mut cache,
            account_key,
            inventory_checksum,
            parsed,
            Utc::now(),
        );
        if let Some(history) = cache.accounts.get_mut(&cache.active_account) {
            let (owned, rank) = parse_current_equipment(raw);
            history.owned_equipment = Some(owned);
            history.account_rank = rank;
            if !equipment.is_empty() {
                history.equipment_observed_at = Some(Utc::now());
            }
            for (game_ref, evidence) in equipment {
                if history.equipment.len() >= MAX_ENTRIES
                    && !history.equipment.contains_key(&game_ref)
                {
                    continue;
                }
                let saved = history.equipment.entry(game_ref).or_default();
                saved.xp = saved.xp.max(evidence.xp);
                saved.polarized = saved.polarized.max(evidence.polarized);
                saved.xp_with_five_polarizations = saved
                    .xp_with_five_polarizations
                    .max(evidence.xp_with_five_polarizations);
            }
        }
        guard.set_setting(CACHE_KEY, &cache)?;
        Ok(())
    }

    /// Возвращает каталог, а не проекцию продаваемого инвентаря.
    ///
    /// # Errors
    /// Возвращает ошибку чтения локального хранилища.
    pub fn view(database: &Mutex<Database>) -> Result<MasteryView, CoreError> {
        let guard = database
            .lock()
            .map_err(|_| CoreError::DatabaseState("unavailable".into()))?;
        let cache = guard
            .get_setting::<MasteryCache>(CACHE_KEY)?
            .unwrap_or_default();
        let metadata = guard.load_current_game_metadata()?;
        // При импорте другого снимка или ошибке записи истории не показываем
        // освоение предыдущего аккаунта рядом с новым инвентарём.
        let current = guard.current_inventory_snapshot()?;
        let history = current
            .as_ref()
            .filter(|inventory| inventory.metadata.checksum_sha256 == cache.inventory_checksum)
            .and_then(|_| cache.accounts.get(&cache.active_account));
        Ok(build_view(metadata.as_ref(), history))
    }

    pub(crate) fn planner_context(
        database: &Database,
    ) -> Result<(MasteryView, Option<PlannerAccount>), CoreError> {
        let cache = database
            .get_setting::<MasteryCache>(CACHE_KEY)?
            .unwrap_or_default();
        let metadata = database.load_current_game_metadata()?;
        let current = database.current_inventory_snapshot()?;
        let history = current
            .as_ref()
            .filter(|snapshot| snapshot.metadata.checksum_sha256 == cache.inventory_checksum)
            .and_then(|_| cache.accounts.get(&cache.active_account));
        let account = history
            .filter(|history| history.observed_at.is_some())
            .map(|history| PlannerAccount {
                key: cache.active_account.clone(),
                owned_equipment: history
                    .owned_equipment
                    .clone()
                    .unwrap_or_else(|| legacy_owned_equipment(metadata.as_ref(), current.as_ref())),
                rank: history.account_rank,
            });
        Ok((build_view(metadata.as_ref(), history), account))
    }
}

/// Старый кэш не содержал текущие копии. Берём их из согласованного снимка,
/// сохраняя отличие отсутствующего поля от подтверждённого пустого списка.
fn legacy_owned_equipment(
    metadata: Option<&GameMetadataSnapshot>,
    inventory: Option<&ResolvedInventorySnapshot>,
) -> BTreeMap<String, u32> {
    let catalog: BTreeSet<_> = metadata
        .into_iter()
        .flat_map(|metadata| &metadata.mastery_items)
        .filter(|item| {
            !matches!(item.category.as_str(), "modular" | "amp" | "kdrive")
                && !is_modular_mastery_part(&item.game_ref)
        })
        .map(|item| item.game_ref.as_str())
        .collect();
    let mut owned = BTreeMap::<String, u32>::new();
    for item in inventory.into_iter().flat_map(|snapshot| &snapshot.items) {
        let game_ref = canonical_mastery_ref(&item.canonical_game_id);
        if catalog.contains(game_ref) && item.owned_quantity > 0 {
            let quantity = owned.entry(game_ref.to_owned()).or_default();
            *quantity = quantity.saturating_add(item.owned_quantity);
        }
    }
    owned
}

fn parse_current_equipment(raw: &str) -> (BTreeMap<String, u32>, Option<u8>) {
    let mut owned = BTreeMap::<String, u32>::new();
    if raw.len() > MAX_RESPONSE_BYTES {
        return (owned, None);
    }
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return (owned, None);
    };
    let root = value.get("Inventory").unwrap_or(&value);
    let rank = root
        .get("PlayerLevel")
        .and_then(Value::as_u64)
        .and_then(|rank| u8::try_from(rank).ok())
        .filter(|rank| *rank <= 50);
    for category in [
        "Suits",
        "LongGuns",
        "Pistols",
        "Melee",
        "Sentinels",
        "SentinelWeapons",
        "SpaceSuits",
        "SpaceGuns",
        "SpaceMelee",
        "MechSuits",
        "KubrowPets",
        "MoaPets",
        "OperatorAmps",
        "Hoverboards",
    ] {
        let Some(items) = root
            .get(category)
            .and_then(Value::as_array)
            .filter(|items| items.len() <= MAX_ENTRIES)
        else {
            continue;
        };
        for item in items {
            let Some(game_ref) = item
                .get("ItemType")
                .and_then(Value::as_str)
                .filter(|name| name.starts_with("/Lotus/") && name.len() <= 256)
            else {
                continue;
            };
            let quantity = item.get("ItemCount").and_then(Value::as_u64).unwrap_or(1);
            if let Ok(quantity) = u32::try_from(quantity) {
                // Собранная модульная копия принадлежит определяющей детали
                // даже до позолоты и при текущем ранге 0.
                let count = owned
                    .entry(equipment_mastery_ref(item, game_ref).to_owned())
                    .or_default();
                *count = count.saturating_add(quantity);
            }
        }
    }
    (owned, rank)
}

fn parse_equipment(raw: &str) -> BTreeMap<String, EquipmentEvidence> {
    let mut result = BTreeMap::<String, EquipmentEvidence>::new();
    if raw.len() > MAX_RESPONSE_BYTES {
        return result;
    }
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return result;
    };
    let root = value.get("Inventory").unwrap_or(&value);
    // Незолочёная модульная копия не доказывает освоение.
    for category in [
        "Suits",
        "LongGuns",
        "Pistols",
        "Melee",
        "Sentinels",
        "SentinelWeapons",
        "SpaceSuits",
        "SpaceGuns",
        "SpaceMelee",
        "MechSuits",
        "KubrowPets",
        "MoaPets",
        "OperatorAmps",
        "Hoverboards",
    ] {
        let Some(items) = root.get(category).and_then(Value::as_array) else {
            continue;
        };
        if items.len() > MAX_ENTRIES {
            continue;
        }
        for item in items {
            let Some(game_ref) = item
                .get("ItemType")
                .and_then(Value::as_str)
                .filter(|name| name.starts_with("/Lotus/") && name.len() <= 256)
            else {
                continue;
            };
            let parts = item.get("ModularParts").and_then(Value::as_array);
            let needs_gilding = category == "MoaPets"
                || (parts.is_some()
                    && matches!(
                        category,
                        "KubrowPets" | "OperatorAmps" | "LongGuns" | "Pistols" | "Melee"
                    ));
            if needs_gilding && item.get("Features").and_then(Value::as_u64).unwrap_or(0) & 8 == 0 {
                continue;
            }
            let mastery_ref = equipment_mastery_ref(item, game_ref);
            let Some(xp) = item.get("XP").and_then(Value::as_u64) else {
                continue;
            };
            let polarized = item.get("Polarized").and_then(Value::as_u64).unwrap_or(0);
            let evidence = result.entry(mastery_ref.into()).or_default();
            evidence.xp = evidence.xp.max(xp);
            evidence.polarized = evidence.polarized.max(polarized);
            if polarized >= 5 {
                evidence.xp_with_five_polarizations = evidence.xp_with_five_polarizations.max(xp);
            }
        }
    }
    result
}

fn equipment_mastery_ref<'a>(item: &'a Value, game_ref: &'a str) -> &'a str {
    let defining_part = item
        .get("ModularParts")
        .and_then(Value::as_array)
        .and_then(|parts| {
            parts
                .iter()
                .filter_map(Value::as_str)
                .filter(|part| part.starts_with("/Lotus/") && part.len() <= 256)
                .find(|part| is_modular_mastery_part(part))
        })
        .unwrap_or(game_ref);
    canonical_mastery_ref(defining_part)
}

fn is_modular_mastery_part(game_ref: &str) -> bool {
    game_ref.contains("/Barrel/")
        || game_ref.contains("/Barrels/")
        || game_ref.contains("/Tip/")
        || game_ref.contains("/Tips/")
        || game_ref.contains("/MoaPetHead")
        || game_ref.contains("/ZanukaPetPartHead")
        || game_ref.ends_with("Deck")
        || game_ref.ends_with("SentAmpTrainingBarrel")
}

fn parse_history(raw: &str) -> Option<BTreeMap<String, u64>> {
    if raw.len() > MAX_RESPONSE_BYTES {
        return None;
    }
    let value: Value = serde_json::from_str(raw).ok()?;
    let root = value.get("Inventory").unwrap_or(&value);
    let entries = root.get("XPInfo")?.as_array()?;
    if entries.is_empty() || entries.len() > MAX_ENTRIES {
        return None;
    }
    let mut result = BTreeMap::<String, u64>::new();
    for entry in entries {
        let game_ref = entry.get("ItemType")?.as_str()?;
        let xp = entry.get("XP")?.as_u64()?;
        if !game_ref.starts_with("/Lotus/") || game_ref.len() > 256 {
            return None;
        }
        result
            .entry(canonical_mastery_ref(game_ref).into())
            .and_modify(|saved| *saved = (*saved).max(xp))
            .or_insert(xp);
    }
    Some(result)
}

fn canonical_mastery_ref(game_ref: &str) -> &str {
    match game_ref {
        "/Lotus/Types/Game/CrewShip/RailJack/DefaultHarness" => {
            "/Lotus/Types/Game/CrewShip/RailjackHarness"
        }
        "/Lotus/Weapons/Sentients/OperatorAmplifiers/SentTrainingAmplifier/OperatorTrainingAmpWeapon" => {
            "/Lotus/Weapons/Sentients/OperatorAmplifiers/SentTrainingAmplifier/SentAmpTrainingBarrel"
        }
        _ => game_ref,
    }
}

fn update_cache(
    cache: &mut MasteryCache,
    account_key: String,
    inventory_checksum: &str,
    parsed: Option<BTreeMap<String, u64>>,
    observed_at: DateTime<Utc>,
) {
    cache.active_account.clone_from(&account_key);
    cache.inventory_checksum = inventory_checksum.into();
    let history = cache.accounts.entry(account_key).or_default();
    history.refresh_failed = parsed.is_none();
    if let Some(entries) = parsed {
        for (key, xp) in entries {
            if history.entries.len() >= MAX_ENTRIES && !history.entries.contains_key(&key) {
                continue;
            }
            history
                .entries
                .entry(key)
                .and_modify(|saved| *saved = (*saved).max(xp))
                .or_insert(xp);
        }
        history.observed_at = Some(observed_at);
    }
}

fn build_view(
    metadata: Option<&GameMetadataSnapshot>,
    history: Option<&AccountHistory>,
) -> MasteryView {
    let items = metadata.map_or_else(Vec::new, |metadata| {
        metadata
            .mastery_items
            .iter()
            .map(|item| {
                let xp = history
                    .and_then(|history| history.entries.get(&item.game_ref))
                    .copied();
                let (status, reason) = account_mastery_status(item, history);
                MasteryItemView {
                    game_ref: item.game_ref.clone(),
                    display_name: item
                        .display_name_ru
                        .clone()
                        .unwrap_or_else(|| item.display_name_en.clone()),
                    display_name_en: item.display_name_en.clone(),
                    category: item.category.clone(),
                    image_url: item.image_url.clone(),
                    max_rank: item.max_rank,
                    xp,
                    mastery_rank: if status == "mastered" {
                        item.max_rank
                    } else {
                        history_rank(item, xp)
                    },
                    status,
                    reason,
                    set_slugs: metadata
                        .prime_sets
                        .iter()
                        .filter(|set| set.set_game_ref == item.game_ref)
                        .map(|set| set.set_slug.clone())
                        .collect(),
                }
            })
            .collect()
    });
    let observed_at =
        history.and_then(|history| history.observed_at.max(history.equipment_observed_at));
    MasteryView {
        observed_at,
        source: observed_at.map(|_| "inventory_xp_info"),
        refresh_failed: history.is_some_and(|history| history.refresh_failed),
        catalog_available: metadata.is_some_and(|metadata| !metadata.mastery_items.is_empty()),
        items,
    }
}

fn account_mastery_status(
    item: &MasteryItemDefinition,
    history: Option<&AccountHistory>,
) -> (&'static str, &'static str) {
    let xp = history
        .and_then(|history| history.entries.get(&item.game_ref))
        .copied();
    let equipment = history.and_then(|history| history.equipment.get(&item.game_ref));
    let result = mastery_status(item, xp, equipment);
    // Отсутствие предмета в успешно полученной истории аккаунта — неосвоение.
    // Дата отдельного экземпляра не доказывает, что XPInfo был получен.
    // Сохранённое подтверждение освоения имеет приоритет даже без записи XPInfo.
    if result == ("unknown", "no_record")
        && history.is_some_and(|history| history.observed_at.is_some())
        && matches!(item.max_rank, Some(30 | 40))
        && affinity_multiplier(item).is_some()
    {
        return ("progress", "history_absent");
    }
    result
}

fn mastery_status(
    item: &MasteryItemDefinition,
    xp: Option<u64>,
    equipment: Option<&EquipmentEvidence>,
) -> (&'static str, &'static str) {
    // История XPInfo относится к типу снаряжения, не к текущей копии.
    // Не суммируем дубли и не заменяем её текущим опытом после Формы.
    if let (Some(rank), Some(cap)) = (history_rank(item, xp), item.max_rank)
        && rank == cap
    {
        return ("mastered", "history_confirmed");
    }
    let multiplier = affinity_multiplier(item);
    if let (Some(evidence), Some(multiplier)) = (equipment, multiplier) {
        let confirmed = match item.max_rank {
            Some(30) => evidence.xp >= 900 * multiplier || evidence.polarized > 0,
            Some(40) => evidence.xp_with_five_polarizations >= 1_600 * multiplier,
            _ => false,
        };
        if confirmed {
            return ("mastered", "equipment_confirmed");
        }
    }
    match (item.max_rank, xp) {
        (_, None) => ("unknown", "no_record"),
        (None, _) => ("unknown", "unsupported"),
        (_, Some(_)) if multiplier.is_some() => ("progress", "history_partial"),
        _ => ("unknown", "unsupported"),
    }
}

fn affinity_multiplier(item: &MasteryItemDefinition) -> Option<u64> {
    match item.category.as_str() {
        "warframe" | "companion" | "archwing" | "necramech" | "kdrive" | "plexus" => Some(1_000),
        "primary" | "secondary" | "melee" | "companion_weapon" | "archgun" | "archmelee"
        | "modular" | "amp" => Some(500),
        _ => None,
    }
}

fn history_rank(item: &MasteryItemDefinition, xp: Option<u64>) -> Option<u8> {
    let cap = item.max_rank.filter(|cap| matches!(cap, 30 | 40))?;
    let multiplier = affinity_multiplier(item)?;
    let xp = xp?;
    // Целочисленный порог исключает ошибки округления на границе ранга.
    (0..=cap)
        .rev()
        .find(|rank| u64::from(*rank).pow(2) * multiplier <= xp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_empty_and_malformed_history_are_not_zero_mastery() {
        for raw in [
            "{}",
            "{\"XPInfo\":[]}",
            "{\"XPInfo\":[{\"ItemType\":\"/Lotus/Test\",\"XP\":-1}]}",
            "{\"XPInfo\":{}}",
        ] {
            assert!(parse_history(raw).is_none());
        }
    }

    #[test]
    fn accepts_wrapped_history_and_merges_duplicates_by_maximum() {
        let parsed = parse_history(r#"{"Inventory":{"XPInfo":[{"ItemType":"/Lotus/Test","XP":42},{"ItemType":"/Lotus/Test","XP":100}]}}"#).unwrap();
        assert_eq!(parsed["/Lotus/Test"], 100);
    }

    #[test]
    fn historical_plexus_alias_is_normalized_without_summing() {
        let parsed = parse_history(r#"{"XPInfo":[{"ItemType":"/Lotus/Types/Game/CrewShip/RailJack/DefaultHarness","XP":3730342},{"ItemType":"/Lotus/Types/Game/CrewShip/RailjackHarness","XP":1}]}"#).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed["/Lotus/Types/Game/CrewShip/RailjackHarness"],
            3_730_342
        );
    }

    #[test]
    fn history_survives_missing_entries_errors_and_lower_duplicate_xp() {
        let mut cache = MasteryCache::default();
        let now = Utc::now();
        update_cache(
            &mut cache,
            "a".into(),
            "one",
            Some(BTreeMap::from([("/Lotus/Sold".into(), 500_000)])),
            now,
        );
        update_cache(
            &mut cache,
            "a".into(),
            "two",
            Some(BTreeMap::from([("/Lotus/Sold".into(), 1)])),
            now,
        );
        update_cache(&mut cache, "a".into(), "three", None, now);
        assert_eq!(cache.accounts["a"].entries["/Lotus/Sold"], 500_000);
        assert!(cache.accounts["a"].refresh_failed);
        assert_eq!(cache.accounts["a"].observed_at, Some(now));
    }

    #[test]
    fn account_switch_does_not_inherit_another_players_history() {
        let mut cache = MasteryCache::default();
        update_cache(
            &mut cache,
            "a".into(),
            "one",
            Some(BTreeMap::from([("/Lotus/Test".into(), 500_000)])),
            Utc::now(),
        );
        update_cache(&mut cache, "b".into(), "two", None, Utc::now());
        assert!(cache.accounts[&cache.active_account].entries.is_empty());
        assert_eq!(cache.inventory_checksum, "two");
        assert_eq!(cache.accounts["a"].entries.len(), 1);
    }

    fn weapon(max_rank: Option<u8>) -> MasteryItemDefinition {
        MasteryItemDefinition {
            game_ref: "/Lotus/Test".into(),
            display_name_en: "Test".into(),
            display_name_ru: None,
            category: "primary".into(),
            image_url: None,
            max_rank,
        }
    }

    #[test]
    fn absent_item_in_loaded_account_history_is_not_mastered() {
        let history = AccountHistory {
            observed_at: Some(Utc::now()),
            entries: BTreeMap::from([("/Lotus/Other".into(), 450_000)]),
            ..Default::default()
        };
        assert_eq!(
            account_mastery_status(&weapon(Some(30)), Some(&history)),
            ("progress", "history_absent")
        );
        assert_eq!(
            account_mastery_status(&weapon(Some(40)), Some(&history)),
            ("progress", "history_absent")
        );
        // Ошибка обновления не стирает результат последнего успешного снимка.
        let stale = AccountHistory {
            refresh_failed: true,
            ..history
        };
        assert_eq!(
            account_mastery_status(&weapon(Some(30)), Some(&stale)),
            ("progress", "history_absent")
        );
    }

    #[test]
    fn missing_history_or_unsupported_item_does_not_become_unmastered() {
        let item = weapon(Some(30));
        assert_eq!(
            account_mastery_status(&item, None),
            ("unknown", "no_record")
        );
        let equipment_only = AccountHistory {
            equipment_observed_at: Some(Utc::now()),
            ..Default::default()
        };
        assert_eq!(
            account_mastery_status(&item, Some(&equipment_only)),
            ("unknown", "no_record")
        );
        let history = AccountHistory {
            observed_at: Some(Utc::now()),
            ..Default::default()
        };
        for cap in [None, Some(50)] {
            assert_eq!(
                account_mastery_status(&weapon(cap), Some(&history)).0,
                "unknown"
            );
        }
        let unsupported = MasteryItemDefinition {
            category: "other".into(),
            ..item
        };
        assert_eq!(
            account_mastery_status(&unsupported, Some(&history)).0,
            "unknown"
        );
    }

    #[test]
    fn account_history_absence_never_overrides_saved_mastery_or_progress() {
        let mut history = AccountHistory {
            observed_at: Some(Utc::now()),
            equipment: BTreeMap::from([(
                "/Lotus/Test".into(),
                EquipmentEvidence {
                    polarized: 1,
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        assert_eq!(
            account_mastery_status(&weapon(Some(30)), Some(&history)),
            ("mastered", "equipment_confirmed")
        );
        history.equipment.clear();
        history.entries.insert("/Lotus/Test".into(), 450_000);
        assert_eq!(
            account_mastery_status(&weapon(Some(30)), Some(&history)),
            ("mastered", "history_confirmed")
        );
        assert_eq!(
            account_mastery_status(&weapon(Some(40)), Some(&history)),
            ("progress", "history_partial")
        );
    }

    #[test]
    fn sold_equipment_is_mastered_from_account_history_without_owned_copy() {
        assert_eq!(
            mastery_status(&weapon(Some(30)), Some(50_000_000), None),
            ("mastered", "history_confirmed")
        );
        assert_eq!(
            mastery_status(&weapon(Some(40)), Some(50_000_000), None),
            ("mastered", "history_confirmed")
        );
        assert_eq!(
            mastery_status(&weapon(Some(30)), None, None),
            ("unknown", "no_record")
        );
    }

    #[test]
    fn normal_equipment_confirms_mastery_and_keeps_it_after_forma() {
        let evidence = EquipmentEvidence {
            xp: 450_000,
            ..Default::default()
        };
        assert_eq!(
            mastery_status(&weapon(Some(30)), None, Some(&evidence)).0,
            "mastered"
        );
        let polarized = EquipmentEvidence {
            polarized: 1,
            ..Default::default()
        };
        assert_eq!(
            mastery_status(&weapon(Some(30)), Some(1), Some(&polarized)).0,
            "mastered"
        );
        assert_ne!(
            mastery_status(&weapon(None), Some(1), Some(&polarized)).0,
            "mastered"
        );
    }

    #[test]
    fn rank_forty_does_not_combine_experience_from_different_copies() {
        let raw = r#"{"Melee":[{"ItemType":"/Lotus/Test","XP":800000,"Polarized":0},{"ItemType":"/Lotus/Test","XP":1,"Polarized":5}]}"#;
        let parsed = parse_equipment(raw);
        assert_ne!(
            mastery_status(&weapon(Some(40)), Some(450_000), parsed.get("/Lotus/Test")).0,
            "mastered"
        );
        let confirmed = EquipmentEvidence {
            xp_with_five_polarizations: 800_000,
            ..Default::default()
        };
        assert_eq!(
            mastery_status(&weapon(Some(40)), None, Some(&confirmed)).0,
            "mastered"
        );
    }

    fn publish_empty_inventory(database: &Mutex<Database>, checksum: &str) {
        use platscope_domain::{
            InventorySnapshotMetadata, InventorySource, ResolvedInventorySnapshot,
        };
        database
            .lock()
            .unwrap()
            .promote_inventory_snapshot(&ResolvedInventorySnapshot {
                metadata: InventorySnapshotMetadata {
                    source: InventorySource::ReadOnlyScan,
                    observed_at: Utc::now(),
                    schema_version: 3,
                    item_count: 0,
                    checksum_sha256: checksum.into(),
                },
                keep_copies: 1,
                mod_usage_scanned: false,
                credits: None,
                syndicates: vec![],
                items: vec![],
            })
            .unwrap();
    }

    #[test]
    fn database_does_not_show_old_account_when_history_write_is_missing() {
        let database = Mutex::new(Database::open_in_memory().unwrap());
        let detron = "/Lotus/Weapons/Corpus/BoardExec/Secondary/CrpBEDetron/CrpBEDetron";
        let prism = "/Lotus/Test/Barrel/LoosePrism";
        let mut metadata = crate::tests::empty_game_metadata_fixture(Utc::now());
        metadata.mastery_items = vec![
            MasteryItemDefinition {
                game_ref: detron.into(),
                display_name_en: "Tenet Detron".into(),
                category: "secondary".into(),
                ..weapon(Some(40))
            },
            MasteryItemDefinition {
                game_ref: prism.into(),
                category: "amp".into(),
                ..weapon(Some(30))
            },
            MasteryItemDefinition {
                game_ref: "/Lotus/Sold".into(),
                ..weapon(Some(40))
            },
        ];
        database
            .lock()
            .unwrap()
            .promote_game_metadata(&metadata)
            .unwrap();
        let raw = serde_json::json!({"XPInfo":[
            {"ItemType":detron,"XP":450000},
            {"ItemType":prism,"XP":0},
            {"ItemType":"/Lotus/Sold","XP":450000}
        ]})
        .to_string();
        publish_empty_inventory(&database, "a");
        MasteryService::capture(&database, &raw, "account-a", "a").unwrap();
        {
            let mut guard = database.lock().unwrap();
            let mut snapshot = guard.current_inventory_snapshot().unwrap().unwrap();
            let gear = platscope_domain::ResolvedInventoryItem {
                canonical_game_id: detron.into(),
                display_name_en: Some("Tenet Detron".into()),
                display_name_ru: None,
                tags: vec![],
                key: None,
                rank: None,
                subtype: None,
                owned_quantity: 1,
                tradeable_quantity: 0,
                untradeable_quantity: 1,
                unknown_quantity: 0,
                leveled_quantity: 1,
                equipped_quantity: 0,
                equipped_tradeable_quantity: 0,
                equipped_placements: vec![],
                sellable_quantity: 0,
                resolution: platscope_domain::InventoryResolution::UnknownItem,
            };
            snapshot.items = vec![
                gear.clone(),
                platscope_domain::ResolvedInventoryItem {
                    canonical_game_id: prism.into(),
                    ..gear
                },
            ];
            snapshot.metadata.item_count = 2;
            guard.promote_inventory_snapshot(&snapshot).unwrap();
        }
        // Подтверждённый пустой список копий не заменяется торговым снимком.
        let known_empty =
            crate::MasteryPlanService::view(&database, &crate::AppSettings::default()).unwrap();
        assert!(
            known_empty
                .candidates
                .iter()
                .all(|item| item.owned_quantity == 0)
        );
        {
            let guard = database.lock().unwrap();
            let mut legacy = guard.get_setting::<Value>(CACHE_KEY).unwrap().unwrap();
            for account in legacy["accounts"].as_object_mut().unwrap().values_mut() {
                account.as_object_mut().unwrap().remove("ownedEquipment");
            }
            guard.set_setting(CACHE_KEY, &legacy).unwrap();
        }
        let migrated =
            crate::MasteryPlanService::view(&database, &crate::AppSettings::default()).unwrap();
        let owned_detron = migrated
            .candidates
            .iter()
            .find(|item| item.game_ref == detron)
            .unwrap();
        assert_eq!(
            (
                owned_detron.owned_quantity,
                owned_detron.state,
                owned_detron.mastery_rank
            ),
            (1, "owned", Some(30))
        );
        assert!(
            migrated
                .candidates
                .iter()
                .filter(|item| item.game_ref != detron)
                .all(|item| item.owned_quantity == 0)
        );
        assert!(
            MasteryService::view(&database)
                .unwrap()
                .observed_at
                .is_some()
        );
        publish_empty_inventory(&database, "b");
        let next = MasteryService::view(&database).unwrap();
        assert!(next.observed_at.is_none());
        assert!(next.source.is_none());
        let switched =
            crate::MasteryPlanService::view(&database, &crate::AppSettings::default()).unwrap();
        assert!(!switched.history_available);
        assert!(
            switched
                .candidates
                .iter()
                .all(|item| item.owned_quantity == 0)
        );
    }

    #[test]
    fn confirmed_equipment_is_dated_and_survives_disappearance_and_reload() {
        let database = Mutex::new(Database::open_in_memory().unwrap());
        publish_empty_inventory(&database, "first");
        // XPInfo недоступен, но отдельный экземпляр действительно достиг ранга 30.
        MasteryService::capture(
            &database,
            r#"{"LongGuns":[{"ItemType":"/Lotus/Test","XP":450000}]}"#,
            "account-a",
            "first",
        )
        .unwrap();
        let first = MasteryService::view(&database).unwrap();
        assert!(first.observed_at.is_some());
        assert!(first.refresh_failed);
        assert!(first.source.is_some());
        publish_empty_inventory(&database, "second");
        MasteryService::capture(
            &database,
            r#"{"XPInfo":[{"ItemType":"/Lotus/Other","XP":1}]}"#,
            "account-a",
            "second",
        )
        .unwrap();
        let guard = database.lock().unwrap();
        let cache = guard
            .get_setting::<MasteryCache>(CACHE_KEY)
            .unwrap()
            .unwrap();
        let evidence = cache.accounts[&cache.active_account]
            .equipment
            .get("/Lotus/Test");
        assert_eq!(
            mastery_status(&weapon(Some(30)), None, evidence).0,
            "mastered"
        );
        let serialized = serde_json::to_string(&cache).unwrap();
        assert!(!serialized.contains("account-a"));
    }

    #[test]
    fn warframe_and_necramech_thresholds_are_not_weapon_thresholds() {
        let mut item = weapon(Some(30));
        item.category = "warframe".into();
        let partial = EquipmentEvidence {
            xp: 450_000,
            ..Default::default()
        };
        assert_ne!(
            mastery_status(&item, Some(450_000), Some(&partial)).0,
            "mastered"
        );
        item.category = "necramech".into();
        item.max_rank = Some(40);
        let partial = EquipmentEvidence {
            xp_with_five_polarizations: 800_000,
            ..Default::default()
        };
        assert_ne!(
            mastery_status(&item, Some(800_000), Some(&partial)).0,
            "mastered"
        );
        let complete = EquipmentEvidence {
            xp_with_five_polarizations: 1_600_000,
            ..Default::default()
        };
        assert_eq!(mastery_status(&item, None, Some(&complete)).0, "mastered");
    }

    #[test]
    fn history_rank_uses_exact_boundaries_and_survives_forma() {
        for (cap, multiplier, category) in [
            (30, 500, "primary"),
            (30, 1000, "warframe"),
            (40, 500, "primary"),
            (40, 1000, "necramech"),
        ] {
            let mut item = weapon(Some(cap));
            item.category = category.into();
            let threshold = u64::from(cap).pow(2) * multiplier;
            assert_eq!(history_rank(&item, Some(threshold - 1)), Some(cap - 1));
            assert_eq!(
                mastery_status(&item, Some(threshold - 1), None).0,
                "progress"
            );
            assert_eq!(history_rank(&item, Some(threshold)), Some(cap));
            assert_eq!(
                mastery_status(&item, Some(threshold), Some(&EquipmentEvidence::default())).0,
                "mastered"
            );
        }
    }

    #[test]
    fn real_coda_shape_does_not_mistake_uncapped_owned_xp_for_rank_forty() {
        // Реальная форма ответа: XP копии >1 млн, история 450000, без Форм.
        let evidence = EquipmentEvidence {
            xp: 1_115_969,
            ..Default::default()
        };
        let item = weapon(Some(40));
        assert_eq!(
            mastery_status(&item, Some(450_000), Some(&evidence)),
            ("progress", "history_partial")
        );
        assert_eq!(history_rank(&item, Some(450_000)), Some(30));
    }

    #[test]
    fn modular_copies_need_gilding_and_share_one_mastery_identity() {
        let raw = r#"{"OperatorAmps":[{"ItemType":"/Lotus/Amp","XP":260912,"Features":8,"ModularParts":["/Lotus/Test/Barrel/Prism"]},{"ItemType":"/Lotus/Amp","XP":406864,"Features":8,"ModularParts":["/Lotus/Test/Barrel/Prism"]},{"ItemType":"/Lotus/Amp","XP":9000000,"Features":1,"ModularParts":["/Lotus/Test/Barrel/Prism"]}]}"#;
        let parsed = parse_equipment(raw);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed["/Lotus/Test/Barrel/Prism"].xp, 406_864);
        let mut item = weapon(Some(30));
        item.category = "amp".into();
        assert_eq!(history_rank(&item, Some(406_864)), Some(28));
        assert_eq!(
            mastery_status(&item, Some(406_864), parsed.get("/Lotus/Test/Barrel/Prism")).0,
            "progress"
        );

        // Ранг 0 и отсутствие позолоты не отменяют владение собранным усилителем.
        // Отдельная призма из материалов при этом не должна стать готовой копией.
        let raw = r#"{"PlayerLevel":12,"XPInfo":[{"ItemType":"/Lotus/Other","XP":1}],"OperatorAmps":[{"ItemType":"/Lotus/Amp","XP":0,"Features":1,"ModularParts":["/Lotus/Test/Barrel/Prism"]}],"MiscItems":[{"ItemType":"/Lotus/Test/Barrel/LoosePrism","ItemCount":1}]}"#;
        let (owned, _) = parse_current_equipment(raw);
        assert_eq!(owned["/Lotus/Test/Barrel/Prism"], 1);
        assert!(!owned.contains_key("/Lotus/Test/Barrel/LoosePrism"));
        assert!(parse_equipment(raw).is_empty());

        let database = Mutex::new(Database::open_in_memory().unwrap());
        let mut metadata = crate::tests::empty_game_metadata_fixture(Utc::now());
        metadata.mastery_items = ["Prism", "LoosePrism"]
            .into_iter()
            .map(|name| MasteryItemDefinition {
                game_ref: format!("/Lotus/Test/Barrel/{name}"),
                display_name_en: name.into(),
                category: "amp".into(),
                ..weapon(Some(30))
            })
            .collect();
        database
            .lock()
            .unwrap()
            .promote_game_metadata(&metadata)
            .unwrap();
        let mut snapshot = platscope_domain::ResolvedInventorySnapshot {
            metadata: crate::tests::empty_inventory_view(3).metadata,
            keep_copies: 1,
            mod_usage_scanned: false,
            credits: Some(50000),
            syndicates: vec![],
            items: vec![platscope_domain::ResolvedInventoryItem {
                canonical_game_id: "/Lotus/Test/Barrel/LoosePrism".into(),
                display_name_en: Some("LoosePrism".into()),
                display_name_ru: None,
                tags: vec![],
                key: None,
                rank: None,
                subtype: None,
                owned_quantity: 1,
                tradeable_quantity: 0,
                untradeable_quantity: 1,
                unknown_quantity: 0,
                leveled_quantity: 0,
                equipped_quantity: 0,
                equipped_tradeable_quantity: 0,
                equipped_placements: vec![],
                sellable_quantity: 0,
                resolution: platscope_domain::InventoryResolution::UnknownItem,
            }],
        };
        snapshot.metadata.item_count = 1;
        database
            .lock()
            .unwrap()
            .promote_inventory_snapshot(&snapshot)
            .unwrap();
        MasteryService::capture(
            &database,
            raw,
            "account-a",
            &snapshot.metadata.checksum_sha256,
        )
        .unwrap();
        let plan =
            crate::MasteryPlanService::view(&database, &crate::AppSettings::default()).unwrap();
        let assembled = plan
            .candidates
            .iter()
            .find(|item| item.game_ref == "/Lotus/Test/Barrel/Prism")
            .unwrap();
        assert_eq!((assembled.owned_quantity, assembled.state), (1, "owned"));
        let component = plan
            .candidates
            .iter()
            .find(|item| item.game_ref == "/Lotus/Test/Barrel/LoosePrism")
            .unwrap();
        assert_eq!(
            (component.owned_quantity, component.state),
            (0, "no_recipe")
        );
    }

    #[test]
    fn ungilded_pet_and_training_amp_do_not_gain_mastery_from_owned_xp() {
        let raw = r#"{"MoaPets":[{"ItemType":"/Lotus/Pet","XP":3983317,"ModularParts":["/Lotus/Test/ZanukaPetPartHeadC"]}],"OperatorAmps":[{"ItemType":"/Lotus/Amp","XP":880794,"Features":1,"ModularParts":["/Lotus/Test/Barrel/Prism"]},{"ItemType":"/Lotus/Sirocco","XP":5690335,"Features":1}],"KubrowPets":[{"ItemType":"/Lotus/Panzer","XP":105025710,"Features":8,"ModularParts":["/Lotus/Antigen","/Lotus/Mutagen"]}]}"#;
        let parsed = parse_equipment(raw);
        assert_eq!(parsed.len(), 2);
        assert!(parsed.contains_key("/Lotus/Sirocco"));
        assert!(parsed.contains_key("/Lotus/Panzer"));
        assert!(!parsed.contains_key("/Lotus/Test/Barrel/Prism"));
        assert!(!parsed.contains_key("/Lotus/Test/ZanukaPetPartHeadC"));
    }
}
