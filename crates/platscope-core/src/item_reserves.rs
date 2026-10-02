//! Индивидуальный минимум копий относится к подтверждённому аккаунту и точному варианту.
use std::collections::HashMap;
use std::sync::Mutex;

use platscope_domain::{InventoryResolution, MarketVariantKey, ResolvedInventorySnapshot};
use platscope_inventory::apply_keep_copies;
use platscope_storage::Database;
use serde::{Deserialize, Serialize};

use crate::{
    AppSettings, CoreError, InventoryService, InventoryView, MasteryService, lock_database,
};

pub const MAX_ITEM_RESERVE_COPIES: u32 = 9_999;
pub struct ItemReserveService;

pub(super) type ItemReserves = HashMap<MarketVariantKey, u32>;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedReserve {
    key: MarketVariantKey,
    keep_copies: u32,
}

impl ItemReserveService {
    /// Заменяет общий минимум для одного текущего варианта; `None` возвращает общий минимум.
    ///
    /// # Errors
    /// Отклоняет чужой или изменившийся снимок, нераспознанный вариант и неверное количество.
    pub fn set_reserve(
        database: &Mutex<Database>,
        settings: &AppSettings,
        key: &MarketVariantKey,
        keep_copies: Option<u32>,
        expected_inventory_checksum: &str,
        expected_reserve_account_key: &str,
    ) -> Result<(), CoreError> {
        if keep_copies.is_some_and(|quantity| quantity > MAX_ITEM_RESERVE_COPIES) {
            return Err(CoreError::InventoryData(
                "invalid individual reserve quantity".into(),
            ));
        }
        let view = InventoryService::view(database, settings)?
            .ok_or_else(|| CoreError::InventoryData("current inventory unavailable".into()))?;
        let guard = lock_database(database)?;
        let snapshot = guard
            .current_inventory_snapshot()?
            .ok_or_else(|| CoreError::InventoryData("current inventory unavailable".into()))?;
        if snapshot.metadata.checksum_sha256 != expected_inventory_checksum
            || view.metadata.checksum_sha256 != expected_inventory_checksum
        {
            return Err(CoreError::InventoryData(
                "inventory changed before saving reserve".into(),
            ));
        }
        if key.platform != settings.platform
            || !view.items.iter().any(|item| {
                item.owned_quantity > 0
                    && item.resolution == InventoryResolution::Resolved
                    && item.key.as_ref() == Some(key)
            })
        {
            return Err(CoreError::InventoryData(
                "reserve requires a resolved current inventory variant".into(),
            ));
        }
        let (_, account) = MasteryService::planner_context(&guard)?;
        let account = account
            .ok_or_else(|| CoreError::InventoryData("current game account unavailable".into()))?;
        if account.key != expected_reserve_account_key {
            return Err(CoreError::InventoryData(
                "game account changed before saving reserve".into(),
            ));
        }
        let setting_key = reserve_key(&account.key);
        // Чтение, изменение и сохранение выполняются под одним mutex, без потери соседних правок.
        let mut saved: Vec<SavedReserve> = guard.get_setting(&setting_key)?.unwrap_or_default();
        saved.retain(|entry| entry.key != *key);
        if let Some(keep_copies) = keep_copies {
            saved.push(SavedReserve {
                key: key.clone(),
                keep_copies,
            });
        }
        guard.set_setting(&setting_key, &saved)?;
        Ok(())
    }
}

fn reserve_key(account: &str) -> String {
    format!("inventory.item_reserves.v1.{account}")
}

pub(super) fn load(
    database: &Database,
    expected_inventory_checksum: &str,
) -> Result<(Option<String>, ItemReserves), CoreError> {
    let current = database.current_inventory_snapshot()?;
    if current
        .as_ref()
        .is_some_and(|snapshot| snapshot.metadata.checksum_sha256 != expected_inventory_checksum)
    {
        return Err(CoreError::InventoryData(
            "inventory changed while reading reserves".into(),
        ));
    }
    let (_, account) = MasteryService::planner_context(database)?;
    let Some(account) = account else {
        return Ok((None, ItemReserves::new()));
    };
    let saved: Vec<SavedReserve> = database
        .get_setting(&reserve_key(&account.key))?
        .unwrap_or_default();
    if saved
        .iter()
        .any(|entry| entry.keep_copies > MAX_ITEM_RESERVE_COPIES)
    {
        return Err(CoreError::InventoryData(
            "invalid saved individual reserve".into(),
        ));
    }
    Ok((
        Some(account.key),
        saved
            .into_iter()
            .map(|entry| (entry.key, entry.keep_copies))
            .collect(),
    ))
}

/// Резерв задаётся для всего варианта: игровые псевдонимы не умножают число оставленных копий.
/// Возвращаемый минимум каждой строки нужен также для безопасного растворения мистификаторов.
pub(super) fn apply_to_snapshot(
    snapshot: &ResolvedInventorySnapshot,
    keep_copies: u32,
    reserves: &ItemReserves,
) -> (ResolvedInventorySnapshot, Vec<u32>) {
    let mut updated = apply_keep_copies(snapshot, keep_copies);
    let unrestricted = apply_keep_copies(snapshot, 0);
    let mut allocated = vec![keep_copies; snapshot.items.len()];
    for (key, minimum) in reserves {
        let indices: Vec<usize> = snapshot
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                (item.resolution == InventoryResolution::Resolved && item.key.as_ref() == Some(key))
                    .then_some(index)
            })
            .collect();
        let hard_protected: u64 = indices
            .iter()
            .map(|index| {
                let item = &unrestricted.items[*index];
                u64::from(item.owned_quantity.saturating_sub(item.sellable_quantity))
            })
            .sum();
        let mut remaining = u64::from(*minimum).saturating_sub(hard_protected);
        for index in &indices {
            let item = &unrestricted.items[*index];
            let additionally_reserved =
                u32::try_from(remaining.min(u64::from(item.sellable_quantity)))
                    .expect("reserved quantity is bounded by a row quantity");
            remaining -= u64::from(additionally_reserved);
            updated.items[*index].sellable_quantity =
                item.sellable_quantity - additionally_reserved;
            allocated[*index] = item.equipped_quantity.min(item.owned_quantity);
        }
        // Непередаваемые копии запрещено продавать, но лишние можно растворять.
        // Для растворения защищаем только минимум и оснащённые экземпляры.
        let equipped: u64 = indices
            .iter()
            .map(|index| u64::from(allocated[*index]))
            .sum();
        let mut remaining = u64::from(*minimum).saturating_sub(equipped);
        for protect_unavailable in [true, false] {
            for index in &indices {
                let item = &unrestricted.items[*index];
                let capacity = if protect_unavailable {
                    item.owned_quantity.saturating_sub(item.sellable_quantity)
                } else {
                    item.owned_quantity
                };
                let take = u32::try_from(
                    remaining.min(u64::from(capacity.saturating_sub(allocated[*index]))),
                )
                .expect("reserve is bounded by row quantity");
                allocated[*index] += take;
                remaining -= u64::from(take);
            }
        }
    }
    (updated, allocated)
}

/// Личные цели уже выделили копии; индивидуальный минимум дополняет их только до нужного числа.
pub(super) fn apply_to_view(view: &mut InventoryView) {
    let mut groups = HashMap::<MarketVariantKey, (u32, Vec<usize>)>::new();
    for (index, item) in view.items.iter().enumerate() {
        if let (Some(key), Some(minimum)) = (&item.key, item.keep_copies_override) {
            groups
                .entry(key.clone())
                .or_insert_with(|| (minimum, vec![]))
                .1
                .push(index);
        }
    }
    for (minimum, indices) in groups.values() {
        let protected: u64 = indices
            .iter()
            .map(|index| {
                let item = &view.items[*index];
                u64::from(item.owned_quantity.saturating_sub(item.sellable_quantity))
            })
            .sum();
        let mut remaining = u64::from(*minimum).saturating_sub(protected);
        for index in indices {
            let item = &mut view.items[*index];
            let take = u32::try_from(remaining.min(u64::from(item.sellable_quantity)))
                .expect("reserve is bounded by row quantity");
            item.sellable_quantity -= take;
            remaining -= u64::from(take);
        }
    }
    view.summary.sellable_quantity = view
        .items
        .iter()
        .map(|item| u64::from(item.sellable_quantity))
        .sum();
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use platscope_domain::{
        InventorySnapshotMetadata, InventorySource, Platform, ResolvedInventoryItem,
    };

    fn snapshot(checksum: &str) -> ResolvedInventorySnapshot {
        let key =
            MarketVariantKey::new("arcane_test", Platform::Pc, Some(0), None::<String>).unwrap();
        let item = ResolvedInventoryItem {
            canonical_game_id: "/Lotus/Test/Arcane".into(),
            display_name_en: Some("Test Arcane".into()),
            display_name_ru: None,
            tags: vec!["arcane_enhancement".into()],
            key: Some(key),
            rank: Some(0),
            subtype: None,
            owned_quantity: 10,
            tradeable_quantity: 10,
            untradeable_quantity: 0,
            unknown_quantity: 0,
            leveled_quantity: 0,
            equipped_quantity: 0,
            equipped_tradeable_quantity: 0,
            equipped_placements: vec![],
            sellable_quantity: 9,
            resolution: InventoryResolution::Resolved,
        };
        let mut other_rank = item.clone();
        other_rank.rank = Some(1);
        other_rank.key.as_mut().unwrap().rank = Some(1);
        ResolvedInventorySnapshot {
            metadata: InventorySnapshotMetadata {
                source: InventorySource::ReadOnlyScan,
                observed_at: Utc::now(),
                schema_version: 3,
                item_count: 2,
                checksum_sha256: checksum.into(),
            },
            keep_copies: 1,
            mod_usage_scanned: true,
            credits: None,
            syndicates: vec![],
            items: vec![item, other_rank],
        }
    }

    #[test]
    fn individual_reserve_is_exact_persistent_account_bound_and_never_weakens_hard_protection() {
        let database = Mutex::new(Database::open_in_memory().unwrap());
        let settings = AppSettings::default();
        let original = snapshot("one");
        database
            .lock()
            .unwrap()
            .promote_inventory_snapshot(&original)
            .unwrap();
        MasteryService::capture(&database, "{\"XPInfo\":[]}", "account-one", "one").unwrap();
        let view = InventoryService::view(&database, &settings)
            .unwrap()
            .unwrap();
        let account = view.reserve_account_key.unwrap();
        let key = original.items[0].key.clone().unwrap();
        ItemReserveService::set_reserve(&database, &settings, &key, Some(5), "one", &account)
            .unwrap();
        let view = InventoryService::view(&database, &settings)
            .unwrap()
            .unwrap();
        assert_eq!(view.items[0].keep_copies_override, Some(5));
        assert_eq!(view.items[0].sellable_quantity, 5);
        assert_eq!(view.items[1].keep_copies_override, None);
        assert_eq!(view.items[1].sellable_quantity, 9);

        // Временное отсутствие предмета не удаляет сохранённый минимум.
        let mut absent = snapshot("absent");
        absent.items.clear();
        absent.metadata.item_count = 0;
        database
            .lock()
            .unwrap()
            .promote_inventory_snapshot(&absent)
            .unwrap();
        MasteryService::capture(&database, "{\"XPInfo\":[]}", "account-one", "absent").unwrap();
        assert!(
            ItemReserveService::set_reserve(&database, &settings, &key, None, "one", &account)
                .is_err()
        );
        database
            .lock()
            .unwrap()
            .promote_inventory_snapshot(&original)
            .unwrap();
        MasteryService::capture(&database, "{\"XPInfo\":[]}", "account-one", "one").unwrap();
        assert_eq!(
            InventoryService::view(&database, &settings)
                .unwrap()
                .unwrap()
                .items[0]
                .sellable_quantity,
            5
        );

        // Даже одинаковый снимок двух аккаунтов не разрешает сохранить старую карточку.
        MasteryService::capture(&database, "{\"XPInfo\":[]}", "account-two", "one").unwrap();
        assert!(
            ItemReserveService::set_reserve(&database, &settings, &key, Some(7), "one", &account)
                .is_err()
        );
        assert_eq!(
            InventoryService::view(&database, &settings)
                .unwrap()
                .unwrap()
                .items[0]
                .keep_copies_override,
            None
        );
        MasteryService::capture(&database, "{\"XPInfo\":[]}", "account-one", "one").unwrap();
        assert!(
            ItemReserveService::set_reserve(
                &database,
                &settings,
                &key,
                Some(10_000),
                "one",
                &account
            )
            .is_err()
        );
        ItemReserveService::set_reserve(&database, &settings, &key, Some(0), "one", &account)
            .unwrap();
        assert_eq!(
            InventoryService::view(&database, &settings)
                .unwrap()
                .unwrap()
                .items[0]
                .sellable_quantity,
            10
        );
        ItemReserveService::set_reserve(&database, &settings, &key, None, "one", &account).unwrap();
        assert_eq!(
            InventoryService::view(&database, &settings)
                .unwrap()
                .unwrap()
                .items[0]
                .sellable_quantity,
            9
        );

        let mut protected = original.clone();
        protected.items[0].tradeable_quantity = 7;
        protected.items[0].untradeable_quantity = 2;
        protected.items[0].unknown_quantity = 1;
        protected.items[0].equipped_quantity = 1;
        protected.items[0].equipped_tradeable_quantity = 1;
        let (protected, allocated) =
            apply_to_snapshot(&protected, 1, &HashMap::from([(key.clone(), 0)]));
        assert_eq!(protected.items[0].sellable_quantity, 6);
        assert_eq!(allocated[0], 1);

        let mut aliases = original;
        aliases.items[1] = aliases.items[0].clone();
        aliases.items[1].canonical_game_id = "/Lotus/Test/Alias".into();
        let (aliases, allocated) =
            apply_to_snapshot(&aliases, 1, &HashMap::from([(key.clone(), 5)]));
        assert_eq!(
            aliases
                .items
                .iter()
                .map(|item| item.sellable_quantity)
                .sum::<u32>(),
            15
        );
        assert_eq!(allocated.iter().sum::<u32>(), 5);
        let mut view = crate::inventory_view_from_snapshot_with_reserves(
            &aliases,
            settings.language,
            settings.platform,
            1,
            Some(account),
            &HashMap::from([(key, 5)]),
        );
        // Цель выделила три копии на другой строке того же варианта.
        view.items[1].personal_reserved_quantity = 3;
        view.items[1].sellable_quantity -= 3;
        apply_to_view(&mut view);
        assert_eq!(view.summary.sellable_quantity, 15);
    }
}
