//! План освоения использует текущие копии и бюджет в порядке выбранных предметов.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use platscope_domain::{
    BlueprintSource, CraftingIngredientDefinition, CraftingRecipeDefinition, GameMetadataSnapshot,
};
use platscope_storage::Database;
use serde::Serialize;

use crate::mastery::{MasteryItemView, MasteryView, PlannerAccount};
use crate::{AppSettings, CoreError, InventoryService, InventoryView, MasteryService};

const MAX_PLAN_ITEMS: usize = 24;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)] // Независимые признаки доступности источников для IPC, а не одно состояние.
pub struct MasteryPlanView {
    pub inventory_checksum: Option<String>,
    pub inventory_available: bool,
    pub history_available: bool,
    pub recipes_available: bool,
    pub refresh_failed: bool,
    pub observed_at: Option<DateTime<Utc>>,
    pub metadata_at: Option<DateTime<Utc>>,
    pub credits: Option<u64>,
    pub account_rank: Option<u8>,
    pub saved_refs: Vec<String>,
    pub candidates: Vec<MasteryPlanItem>,
    pub queue: Vec<MasteryPlanItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasteryPlanItem {
    pub game_ref: String,
    pub display_name: String,
    pub display_name_en: String,
    pub image_url: Option<String>,
    pub category: String,
    pub owned_quantity: u32,
    pub mastery_rank: Option<u8>,
    pub max_rank: Option<u8>,
    pub remaining_mastery_points: Option<u32>,
    pub state: &'static str,
    pub recipe: Option<CraftingRecipeDefinition>,
    pub blueprint_owned: u32,
    pub materials: Vec<MasteryPlanMaterial>,
    pub missing_types: usize,
    pub total_credits: Option<u64>,
    pub missing_credits: Option<u64>,
    pub rank_blocked: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasteryPlanMaterial {
    pub definition: CraftingIngredientDefinition,
    pub owned_quantity: u32,
    pub available_quantity: u32,
    pub protected_quantity: u32,
    pub missing_quantity: u32,
}

pub struct MasteryPlanService;

impl MasteryPlanService {
    /// Загружает план только для аккаунта, подтверждённого текущим снимком.
    /// # Errors
    /// Возвращает ошибку хранилища или несовпадения снимков при обновлении.
    pub fn view(
        database: &Mutex<Database>,
        settings: &AppSettings,
    ) -> Result<MasteryPlanView, CoreError> {
        Self::load(database, settings, None)
    }

    /// Сохраняет порядок предметов, без операций в игре и без изменения торговых резервов.
    /// # Errors
    /// Отклоняет неизвестные предметы, чужой снимок и недоступную историю аккаунта.
    pub fn save(
        database: &Mutex<Database>,
        settings: &AppSettings,
        game_refs: Vec<String>,
        expected_inventory_checksum: &str,
    ) -> Result<MasteryPlanView, CoreError> {
        Self::load(
            database,
            settings,
            Some((game_refs, expected_inventory_checksum)),
        )
    }

    fn load(
        database: &Mutex<Database>,
        settings: &AppSettings,
        requested: Option<(Vec<String>, &str)>,
    ) -> Result<MasteryPlanView, CoreError> {
        let inventory = InventoryService::view(database, settings)?;
        let guard = database
            .lock()
            .map_err(|_| CoreError::DatabaseState("unavailable".into()))?;
        let snapshot = guard.current_inventory_snapshot()?;
        if inventory
            .as_ref()
            .map(|view| &view.metadata.checksum_sha256)
            != snapshot.as_ref().map(|view| &view.metadata.checksum_sha256)
        {
            return Err(CoreError::InventoryData(
                "snapshot changed during planning".into(),
            ));
        }
        let metadata = guard.load_current_game_metadata()?;
        let (mastery, account) = MasteryService::planner_context(&guard)?;
        let saved = if let Some((refs, expected_checksum)) = requested {
            if snapshot
                .as_ref()
                .map(|snapshot| snapshot.metadata.checksum_sha256.as_str())
                != Some(expected_checksum)
            {
                return Err(CoreError::InventoryData(
                    "inventory changed before saving plan".into(),
                ));
            }
            let Some(account) = account.as_ref() else {
                return Err(CoreError::InventoryData(
                    "account history unavailable".into(),
                ));
            };
            let catalog: BTreeSet<_> = mastery
                .items
                .iter()
                .map(|item| item.game_ref.as_str())
                .collect();
            let previous = guard
                .get_setting::<Vec<String>>(&plan_key(&account.key))?
                .unwrap_or_default();
            if refs.len() > MAX_PLAN_ITEMS
                || refs.iter().any(|game_ref| {
                    !catalog.contains(game_ref.as_str()) && !previous.contains(game_ref)
                })
                || refs.iter().collect::<BTreeSet<_>>().len() != refs.len()
            {
                return Err(CoreError::MetadataData(
                    "invalid mastery plan selection".into(),
                ));
            }
            guard.set_setting(&plan_key(&account.key), &refs)?;
            refs
        } else {
            account
                .as_ref()
                .map(|account| guard.get_setting::<Vec<String>>(&plan_key(&account.key)))
                .transpose()?
                .flatten()
                .unwrap_or_default()
        };
        let credits = snapshot.as_ref().and_then(|snapshot| snapshot.credits);
        let goal_reservations = inventory
            .as_ref()
            .map(|view| crate::personal_goals::crafting_reservations(&guard, &view.items))
            .transpose()?
            .unwrap_or_default();
        Ok(build_plan(
            &mastery,
            account.as_ref(),
            metadata.as_ref(),
            inventory.as_ref(),
            credits,
            saved,
            &goal_reservations,
        ))
    }
}

fn plan_key(account: &str) -> String {
    format!("mastery.plan.v1.{account}")
}

#[allow(clippy::too_many_lines)] // Распределение одного бюджета и формирование представления выполняются вместе.
fn build_plan(
    mastery: &MasteryView,
    account: Option<&PlannerAccount>,
    metadata: Option<&GameMetadataSnapshot>,
    inventory: Option<&InventoryView>,
    credits: Option<u64>,
    saved: Vec<String>,
    goal_reservations: &BTreeMap<String, u32>,
) -> MasteryPlanView {
    let mut quantities = BTreeMap::<String, u32>::new();
    let mut protected = BTreeMap::<String, u32>::new();
    if let Some(inventory) = inventory {
        for item in &inventory.items {
            let quantity = quantities
                .entry(item.canonical_game_id.clone())
                .or_default();
            *quantity = quantity.saturating_add(item.owned_quantity);
            let reserved = protected.entry(item.canonical_game_id.clone()).or_default();
            *reserved = reserved.saturating_add(item.personal_reserved_quantity);
        }
    }
    for (game_ref, quantity) in goal_reservations {
        let reserved = protected.entry(game_ref.clone()).or_default();
        *reserved = (*reserved).max(*quantity);
    }
    // Не предлагаем потратить ещё не освоенное снаряжение как ингредиент.
    for item in &mastery.items {
        if item.status != "mastered"
            && account
                .and_then(|account| account.owned_equipment.get(&item.game_ref))
                .is_some_and(|count| *count > 0)
        {
            protected.insert(
                item.game_ref.clone(),
                quantities.get(&item.game_ref).copied().unwrap_or(0),
            );
        }
    }
    let available: BTreeMap<_, _> = quantities
        .iter()
        .map(|(key, quantity)| {
            (
                key.clone(),
                quantity.saturating_sub(protected.get(key).copied().unwrap_or(0)),
            )
        })
        .collect();
    let recipes: BTreeMap<_, _> = metadata
        .into_iter()
        .flat_map(|metadata| &metadata.crafting_recipes)
        .fold(
            BTreeMap::<&str, &CraftingRecipeDefinition>::new(),
            |mut recipes, recipe| {
                // При нескольких рецептах сначала используем доступный пользователю чертёж.
                let key = recipe.result_game_ref.as_str();
                let priority = |recipe: &CraftingRecipeDefinition| {
                    (
                        u8::from(
                            available
                                .get(&recipe.blueprint_game_ref)
                                .copied()
                                .unwrap_or(0)
                                == 0,
                        ),
                        match recipe.blueprint_source {
                            BlueprintSource::Market => 0,
                            BlueprintSource::Dojo => 1,
                            BlueprintSource::Unknown => 2,
                        },
                        recipe.blueprint_price.unwrap_or(u64::MAX),
                    )
                };
                if recipes
                    .get(key)
                    .is_none_or(|previous| priority(recipe) < priority(previous))
                {
                    recipes.insert(key, recipe);
                }
                recipes
            },
        );
    let saved: Vec<_> = saved
        .into_iter()
        .filter(|game_ref| game_ref.starts_with("/Lotus/") && game_ref.len() <= 256)
        .take(MAX_PLAN_ITEMS)
        .collect();
    let make_item =
        |item: &MasteryItemView, budget: &BTreeMap<String, u32>, balance: Option<u64>| {
            evaluate(
                item,
                recipes.get(item.game_ref.as_str()).copied(),
                account,
                &quantities,
                &protected,
                budget,
                balance,
                inventory.is_some(),
            )
        };
    let mut candidates: Vec<_> = mastery
        .items
        .iter()
        .filter(|item| item.status != "mastered" || saved.contains(&item.game_ref))
        .map(|item| make_item(item, &available, credits))
        .collect();
    let mut budget = available;
    let mut balance = credits;
    let mut queue = Vec::new();
    for game_ref in &saved {
        let Some(item) = mastery.items.iter().find(|item| &item.game_ref == game_ref) else {
            queue.push(unavailable_item(game_ref));
            continue;
        };
        let row = make_item(item, &budget, balance);
        if row.owned_quantity == 0
            && row.state != "mastered"
            && let Some(recipe) = &row.recipe
        {
            for ingredient in &recipe.ingredients {
                let quantity = budget.entry(ingredient.game_ref.clone()).or_default();
                *quantity = quantity.saturating_sub(ingredient.quantity);
            }
            if recipe.blueprint_consumed {
                let quantity = budget.entry(recipe.blueprint_game_ref.clone()).or_default();
                *quantity = quantity.saturating_sub(1);
            }
            // Даже при неизвестном источнике чертежа известная стоимость кузницы учитывается.
            let cost = row.total_credits.unwrap_or(recipe.build_price);
            balance = balance.map(|balance| balance.saturating_sub(cost));
        }
        queue.push(row);
    }
    // В списке выбранный предмет имеет те же доступные запасы, что и в порядке плана.
    for candidate in &mut candidates {
        if let Some(row) = queue.iter().find(|row| row.game_ref == candidate.game_ref) {
            *candidate = row.clone();
        }
    }
    candidates.sort_by(|a, b| {
        state_priority(a.state)
            .cmp(&state_priority(b.state))
            .then(a.rank_blocked.cmp(&b.rank_blocked))
            .then(a.missing_types.cmp(&b.missing_types))
            .then(
                a.total_credits
                    .unwrap_or(u64::MAX)
                    .cmp(&b.total_credits.unwrap_or(u64::MAX)),
            )
            .then(a.display_name.cmp(&b.display_name))
    });
    MasteryPlanView {
        inventory_checksum: inventory.map(|inventory| inventory.metadata.checksum_sha256.clone()),
        inventory_available: inventory.is_some(),
        history_available: mastery.observed_at.is_some(),
        recipes_available: metadata.is_some_and(|metadata| !metadata.crafting_recipes.is_empty()),
        refresh_failed: mastery.refresh_failed,
        observed_at: inventory.map(|inventory| inventory.metadata.observed_at),
        metadata_at: metadata.map(|metadata| metadata.metadata.fetched_at),
        account_rank: account.and_then(|account| account.rank),
        credits,
        saved_refs: saved,
        candidates,
        queue,
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // Все количества вычисляются на одном согласованном снимке.
fn evaluate(
    item: &MasteryItemView,
    recipe: Option<&CraftingRecipeDefinition>,
    account: Option<&PlannerAccount>,
    owned: &BTreeMap<String, u32>,
    protected: &BTreeMap<String, u32>,
    budget: &BTreeMap<String, u32>,
    credits: Option<u64>,
    inventory_available: bool,
) -> MasteryPlanItem {
    let owned_quantity = account
        .and_then(|account| account.owned_equipment.get(&item.game_ref))
        .copied()
        // Отдельная модульная деталь в материалах не является собранной копией.
        .unwrap_or(0);
    let materials: Vec<_> = recipe
        .into_iter()
        .flat_map(|recipe| &recipe.ingredients)
        .map(|ingredient| {
            let available = budget.get(&ingredient.game_ref).copied().unwrap_or(0);
            MasteryPlanMaterial {
                definition: ingredient.clone(),
                owned_quantity: owned.get(&ingredient.game_ref).copied().unwrap_or(0),
                available_quantity: available,
                protected_quantity: protected.get(&ingredient.game_ref).copied().unwrap_or(0),
                missing_quantity: ingredient.quantity.saturating_sub(available),
            }
        })
        .collect();
    let missing_types = materials
        .iter()
        .filter(|material| material.missing_quantity > 0)
        .count();
    let blueprint_owned = recipe
        .and_then(|recipe| budget.get(&recipe.blueprint_game_ref))
        .copied()
        .unwrap_or(0);
    let total_credits = recipe.and_then(|recipe| {
        if blueprint_owned > 0 {
            Some(recipe.build_price)
        } else {
            recipe
                .blueprint_price
                .and_then(|price| recipe.build_price.checked_add(price))
        }
    });
    let missing_credits = total_credits
        .zip(credits)
        .map(|(cost, balance)| cost.saturating_sub(balance));
    let rank_blocked = recipe
        .and_then(|recipe| recipe.mastery_requirement)
        .zip(account.and_then(|account| account.rank))
        .is_some_and(|(required, current)| required > current);
    let access_unknown = recipe.is_some_and(|recipe| match recipe.mastery_requirement {
        Some(0) => false,
        Some(_) => account.and_then(|account| account.rank).is_none(),
        None => true,
    });
    let state = if item.status == "mastered" {
        "mastered"
    } else if item.status == "unknown" || !inventory_available {
        "unknown"
    } else if owned_quantity > 0 {
        "owned"
    } else if rank_blocked {
        "rank_locked"
    } else if access_unknown {
        "access_unknown"
    } else if recipe.is_some()
        && missing_types == 0
        && missing_credits == Some(0)
        && blueprint_owned > 0
    {
        "craft"
    } else if recipe.is_some_and(|recipe| recipe.blueprint_source == BlueprintSource::Market)
        && missing_types == 0
        && missing_credits == Some(0)
    {
        "buy_blueprint"
    } else if recipe.is_some()
        && missing_types == 1
        && (blueprint_owned > 0
            || recipe.is_some_and(|recipe| recipe.blueprint_source == BlueprintSource::Market))
    {
        "one_short"
    } else if recipe.is_some() {
        "gather"
    } else {
        "no_recipe"
    };
    let remaining_mastery_points = mastery_points_per_rank(&item.category)
        .zip(item.max_rank)
        .filter(|_| item.status != "unknown")
        .map(|(points, max_rank)| {
            u32::from(max_rank.saturating_sub(item.mastery_rank.unwrap_or(0))) * points
        });
    MasteryPlanItem {
        game_ref: item.game_ref.clone(),
        display_name: item.display_name.clone(),
        display_name_en: item.display_name_en.clone(),
        image_url: item.image_url.clone(),
        category: item.category.clone(),
        owned_quantity,
        mastery_rank: item.mastery_rank,
        max_rank: item.max_rank,
        remaining_mastery_points,
        state,
        recipe: recipe.cloned(),
        blueprint_owned,
        materials,
        missing_types,
        total_credits,
        missing_credits,
        rank_blocked,
    }
}

fn mastery_points_per_rank(category: &str) -> Option<u32> {
    match category {
        "warframe" | "archwing" | "companion" | "sentinel" | "necramech" | "kdrive" | "plexus" => {
            Some(200)
        }
        "primary" | "secondary" | "melee" | "sentinel_weapon" | "companion_weapon" | "archgun"
        | "archmelee" | "amp" | "modular" => Some(100),
        _ => None,
    }
}

fn unavailable_item(game_ref: &str) -> MasteryPlanItem {
    MasteryPlanItem {
        game_ref: game_ref.to_owned(),
        display_name: "Предмет отсутствует в текущем каталоге".into(),
        display_name_en: "Item unavailable in current catalog".into(),
        image_url: None,
        category: "other".into(),
        owned_quantity: 0,
        mastery_rank: None,
        max_rank: None,
        remaining_mastery_points: None,
        state: "unknown",
        recipe: None,
        blueprint_owned: 0,
        materials: vec![],
        missing_types: 0,
        total_credits: None,
        missing_credits: None,
        rank_blocked: false,
    }
}

fn state_priority(state: &str) -> u8 {
    match state {
        "owned" => 0,
        "craft" => 1,
        "buy_blueprint" => 2,
        "one_short" => 3,
        "gather" => 4,
        "rank_locked" => 5,
        "access_unknown" => 6,
        "no_recipe" => 7,
        _ => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_does_not_spend_shared_materials_and_credits_twice_or_spend_protected_copies() {
        let mut metadata = crate::tests::empty_game_metadata_fixture(Utc::now());
        let mut inventory = crate::tests::empty_inventory_view(3);
        inventory.items.push(crate::InventoryViewItem {
            canonical_game_id: "/Lotus/Material/A".into(),
            item_id: None,
            bulk_tradable: false,
            display_name: "A".into(),
            image_url: None,
            tags: vec![],
            key: None,
            rank: None,
            subtype: None,
            owned_quantity: 12,
            tradeable_quantity: 12,
            untradeable_quantity: 0,
            unknown_quantity: 0,
            leveled_quantity: 0,
            equipped_quantity: 0,
            equipped_placements: vec![],
            sellable_quantity: 10,
            personal_reserved_quantity: 2,
            keep_copies_override: None,
            resolution: platscope_domain::InventoryResolution::UnknownItem,
            vault_status: platscope_domain::VaultStatus::Unknown,
        });
        let mut items = vec![];
        for name in ["A", "B"] {
            let game_ref = format!("/Lotus/Weapon/{name}");
            items.push(MasteryItemView {
                game_ref: game_ref.clone(),
                display_name: name.into(),
                display_name_en: name.into(),
                category: "primary".into(),
                image_url: None,
                max_rank: Some(30),
                xp: None,
                mastery_rank: None,
                status: "progress",
                reason: "history_absent",
                set_slugs: vec![],
            });
            metadata.crafting_recipes.push(CraftingRecipeDefinition {
                result_game_ref: game_ref,
                blueprint_game_ref: format!("/Lotus/Recipe/{name}"),
                blueprint_consumed: true,
                blueprint_source: BlueprintSource::Market,
                blueprint_price: Some(100),
                mastery_requirement: Some(0),
                build_price: 200,
                build_time_seconds: 3600,
                blueprint_drops: vec![],
                ingredients: vec![CraftingIngredientDefinition {
                    game_ref: "/Lotus/Material/A".into(),
                    display_name_en: "A".into(),
                    display_name_ru: None,
                    image_url: None,
                    quantity: 8,
                    equipment: false,
                    drops: vec![],
                }],
            });
        }
        let saved = items.iter().map(|item| item.game_ref.clone()).collect();
        let mastery = MasteryView {
            observed_at: Some(Utc::now()),
            source: Some("inventory_xp_info"),
            refresh_failed: false,
            catalog_available: true,
            items,
        };
        let account = PlannerAccount {
            key: "account-a".into(),
            owned_equipment: BTreeMap::new(),
            rank: Some(30),
        };
        let view = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(450),
            saved,
            &BTreeMap::new(),
        );
        assert_eq!(view.queue[0].state, "buy_blueprint");
        assert_eq!(view.queue[1].materials[0].missing_quantity, 6);
        assert_eq!(view.queue[1].missing_credits, Some(150));
        assert_eq!(view.queue[0].materials[0].protected_quantity, 2);
        assert_ne!(view.queue[1].state, "buy_blueprint");
        assert_eq!(
            view.candidates
                .iter()
                .find(|item| item.game_ref.ends_with('B'))
                .unwrap()
                .state,
            view.queue[1].state
        );
        let unknown_balance = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            None,
            view.saved_refs.clone(),
            &BTreeMap::new(),
        );
        assert!(
            unknown_balance
                .queue
                .iter()
                .all(|item| item.state != "craft" && item.state != "buy_blueprint")
        );
        let reversed = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(450),
            view.saved_refs.into_iter().rev().collect(),
            &BTreeMap::new(),
        );
        assert!(reversed.queue[0].game_ref.ends_with('B'));
        assert_eq!(reversed.queue[0].materials[0].missing_quantity, 0);
        let reserved_copies = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(1000),
            vec![],
            &BTreeMap::from([("/Lotus/Material/A".into(), 12)]),
        );
        assert!(reserved_copies.candidates.iter().all(|item| {
            item.materials[0].available_quantity == 0
                && item.materials[0].protected_quantity == 12
                && item.materials[0].missing_quantity == 8
        }));
        metadata.crafting_recipes[0].mastery_requirement = Some(5);
        let account = PlannerAccount {
            rank: None,
            ..account
        };
        let unknown_access = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(1000),
            vec![],
            &BTreeMap::new(),
        );
        assert_eq!(
            unknown_access
                .candidates
                .iter()
                .find(|item| item.game_ref.ends_with('A'))
                .unwrap()
                .state,
            "access_unknown"
        );
    }

    #[test]
    fn saved_plans_follow_the_current_account_and_survive_missing_catalog_and_rejected_save() {
        let database = Mutex::new(Database::open_in_memory().unwrap());
        let settings = AppSettings::default();
        let mut metadata = crate::tests::empty_game_metadata_fixture(Utc::now());
        for name in ["A", "B"] {
            metadata
                .mastery_items
                .push(platscope_domain::MasteryItemDefinition {
                    game_ref: format!("/Lotus/Weapon/{name}"),
                    display_name_en: name.into(),
                    display_name_ru: None,
                    category: "primary".into(),
                    image_url: None,
                    max_rank: Some(30),
                });
        }
        database
            .lock()
            .unwrap()
            .promote_game_metadata(&metadata)
            .unwrap();
        let activate = |account: &str, checksum: &str| {
            let mut snapshot = platscope_domain::ResolvedInventorySnapshot {
                metadata: crate::tests::empty_inventory_view(3).metadata,
                keep_copies: 1,
                mod_usage_scanned: false,
                credits: Some(50000),
                syndicates: vec![],
                items: vec![],
            };
            snapshot.metadata.checksum_sha256 = checksum.into();
            database
                .lock()
                .unwrap()
                .promote_inventory_snapshot(&snapshot)
                .unwrap();
            MasteryService::capture(&database,r#"{"Inventory":{"PlayerLevel":12,"XPInfo":[{"ItemType":"/Lotus/Weapon/Other","XP":0}],"LongGuns":[]}}"#,account,checksum).unwrap();
        };
        activate("account-a", "a1");
        MasteryPlanService::save(&database, &settings, vec!["/Lotus/Weapon/A".into()], "a1")
            .unwrap();
        activate("account-b", "b1");
        assert!(
            MasteryPlanService::save(&database, &settings, vec!["/Lotus/Weapon/A".into()], "a1")
                .is_err()
        );
        assert!(
            MasteryPlanService::view(&database, &settings)
                .unwrap()
                .saved_refs
                .is_empty()
        );
        MasteryPlanService::save(&database, &settings, vec!["/Lotus/Weapon/B".into()], "b1")
            .unwrap();
        activate("account-a", "a2");
        assert_eq!(
            MasteryPlanService::view(&database, &settings)
                .unwrap()
                .saved_refs,
            vec!["/Lotus/Weapon/A"]
        );
        assert!(
            MasteryPlanService::save(
                &database,
                &settings,
                vec!["/Lotus/Unknown/New".into()],
                "a2"
            )
            .is_err()
        );
        assert_eq!(
            MasteryPlanService::view(&database, &settings)
                .unwrap()
                .saved_refs,
            vec!["/Lotus/Weapon/A"]
        );
        metadata.mastery_items.clear();
        database
            .lock()
            .unwrap()
            .promote_game_metadata(&metadata)
            .unwrap();
        let retained = MasteryPlanService::view(&database, &settings).unwrap();
        assert_eq!(retained.saved_refs, vec!["/Lotus/Weapon/A"]);
        assert_eq!(retained.queue[0].state, "unknown");
        MasteryPlanService::save(&database, &settings, retained.saved_refs, "a2").unwrap();
    }
}
