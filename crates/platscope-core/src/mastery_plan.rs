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
    pub foundry_available: bool,
    pub foundry_issue: bool,
    pub refresh_failed: bool,
    pub observed_at: Option<DateTime<Utc>>,
    pub metadata_at: Option<DateTime<Utc>>,
    pub credits: Option<u64>,
    pub account_rank: Option<u8>,
    pub saved_refs: Vec<String>,
    pub candidates: Vec<MasteryPlanItem>,
    pub queue: Vec<MasteryPlanItem>,
    pub known_plan_credits: u64,
    pub total_plan_credits: Option<u64>,
    pub missing_plan_credits: Option<u64>,
    pub plan_materials: Vec<MasteryPlanMaterial>,
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
    pub blueprint_planned: bool,
    pub materials: Vec<MasteryPlanMaterial>,
    pub missing_types: usize,
    pub total_credits: Option<u64>,
    pub missing_credits: Option<u64>,
    pub rank_blocked: bool,
    pub foundry: Option<MasteryPlanFoundry>,
    pub known_credits: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasteryPlanMaterial {
    pub definition: CraftingIngredientDefinition,
    pub owned_quantity: u32,
    pub available_quantity: u32,
    pub protected_quantity: u32,
    pub missing_quantity: u32,
    pub pending_quantity: u32,
    pub planned_quantity: u32,
    pub foundry: Option<MasteryPlanFoundry>,
    pub component: Option<MasteryPlanComponent>,
    pub component_issue: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasteryPlanFoundry {
    pub quantity: u32,
    pub ready_quantity: u32,
    pub unknown_completion_quantity: u32,
    pub completes_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasteryPlanComponent {
    pub recipe: CraftingRecipeDefinition,
    pub batches: u32,
    pub required_quantity: u32,
    pub produced_quantity: u32,
    pub blueprint_owned: u32,
    pub blueprint_planned: bool,
    pub materials: Vec<MasteryPlanMaterial>,
    pub missing_types: usize,
    pub known_credits: u64,
    pub total_credits: Option<u64>,
    pub missing_credits: Option<u64>,
    pub state: &'static str,
    pub rank_blocked: bool,
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
            if mastery.observed_at.is_none() {
                return Err(CoreError::InventoryData(
                    "account history unavailable".into(),
                ));
            }
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
    let recipes: BTreeMap<&str, Vec<&CraftingRecipeDefinition>> = metadata
        .into_iter()
        .flat_map(|metadata| &metadata.crafting_recipes)
        .fold(BTreeMap::new(), |mut recipes, recipe| {
            recipes
                .entry(recipe.result_game_ref.as_str())
                .or_default()
                .push(recipe);
            recipes
        });
    let now = Utc::now();
    let mut foundry_issue = account.is_some_and(|account| account.foundry_issue);
    let mut pending: BTreeMap<String, Vec<PendingOutput>> = BTreeMap::new();
    if let Some(account) = account {
        for job in &account.foundry_jobs {
            let recipe = metadata
                .into_iter()
                .flat_map(|metadata| &metadata.crafting_recipes)
                .find(|recipe| recipe.blueprint_game_ref == job.recipe_game_ref);
            if let Some(recipe) = recipe {
                pending
                    .entry(recipe.result_game_ref.clone())
                    .or_default()
                    .push(PendingOutput {
                        quantity: recipe.result_quantity,
                        completion_at: job.completion_at,
                    });
            } else {
                // Неизвестный рецепт нельзя подменять похожим названием или чужим чертежом.
                foundry_issue = true;
            }
        }
    }
    for jobs in pending.values_mut() {
        jobs.sort_by_key(|job| (job.completion_at.is_none(), job.completion_at));
    }
    let initial = PlanBudget {
        stock: available,
        pending,
        planned: BTreeMap::new(),
        credits,
        known_cost: 0,
        complete_cost: true,
        demand: BTreeMap::new(),
        acquired_blueprints: BTreeSet::new(),
    };
    let context = PlanContext {
        account,
        owned: &quantities,
        protected: &protected,
        unmastered_equipment: mastery
            .items
            .iter()
            .filter(|item| item.status != "mastered")
            .map(|item| item.game_ref.as_str())
            .collect(),
        recipes: &recipes,
        inventory_available: inventory.is_some(),
        now,
    };
    let saved: Vec<_> = saved
        .into_iter()
        .filter(|game_ref| game_ref.starts_with("/Lotus/") && game_ref.len() <= 256)
        .take(MAX_PLAN_ITEMS)
        .collect();
    let mut candidates: Vec<_> = mastery
        .items
        .iter()
        .filter(|item| item.status != "mastered" || saved.contains(&item.game_ref))
        .map(|item| evaluate(item, &context, &mut initial.clone()))
        .collect();
    let mut budget = initial.clone();
    let queue: Vec<_> = saved
        .iter()
        .map(|game_ref| {
            mastery
                .items
                .iter()
                .find(|item| &item.game_ref == game_ref)
                .map_or_else(
                    || unavailable_item(game_ref),
                    |item| evaluate(item, &context, &mut budget),
                )
        })
        .collect();
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
    let total_plan_credits = (budget.complete_cost
        && queue.iter().all(|row| row.total_credits.is_some()))
    .then_some(budget.known_cost);
    let plan_materials = budget
        .demand
        .into_values()
        .map(|mut material| {
            material.owned_quantity = quantities
                .get(&material.definition.game_ref)
                .copied()
                .unwrap_or(0);
            material.available_quantity = initial
                .stock
                .get(&material.definition.game_ref)
                .copied()
                .unwrap_or(0);
            material.protected_quantity = protected
                .get(&material.definition.game_ref)
                .copied()
                .unwrap_or(0);
            material
        })
        .collect();
    MasteryPlanView {
        inventory_checksum: inventory.map(|inventory| inventory.metadata.checksum_sha256.clone()),
        inventory_available: inventory.is_some(),
        history_available: mastery.observed_at.is_some(),
        recipes_available: metadata.is_some_and(|metadata| !metadata.crafting_recipes.is_empty()),
        foundry_available: account.is_some_and(|account| account.foundry_available),
        foundry_issue,
        refresh_failed: mastery.refresh_failed,
        observed_at: inventory.map(|inventory| inventory.metadata.observed_at),
        metadata_at: metadata.map(|metadata| metadata.metadata.fetched_at),
        account_rank: account.and_then(|account| account.rank),
        credits,
        saved_refs: saved,
        candidates,
        queue,
        known_plan_credits: budget.known_cost,
        total_plan_credits,
        missing_plan_credits: total_plan_credits
            .zip(credits)
            .map(|(cost, balance)| cost.saturating_sub(balance)),
        plan_materials,
    }
}

#[derive(Clone)]
struct PendingOutput {
    quantity: u32,
    completion_at: Option<DateTime<Utc>>,
}

/// Физический запас, уже оплаченное изготовление и будущий выход плана хранятся отдельно.
#[derive(Clone)]
struct PlanBudget {
    stock: BTreeMap<String, u32>,
    pending: BTreeMap<String, Vec<PendingOutput>>,
    planned: BTreeMap<String, u32>,
    credits: Option<u64>,
    known_cost: u64,
    complete_cost: bool,
    demand: BTreeMap<String, MasteryPlanMaterial>,
    acquired_blueprints: BTreeSet<String>,
}

struct PlanContext<'a> {
    account: Option<&'a PlannerAccount>,
    owned: &'a BTreeMap<String, u32>,
    protected: &'a BTreeMap<String, u32>,
    unmastered_equipment: BTreeSet<&'a str>,
    recipes: &'a BTreeMap<&'a str, Vec<&'a CraftingRecipeDefinition>>,
    inventory_available: bool,
    now: DateTime<Utc>,
}

impl PlanBudget {
    fn take_foundry(
        &mut self,
        game_ref: &str,
        quantity: u32,
        now: DateTime<Utc>,
    ) -> Option<MasteryPlanFoundry> {
        let jobs = self.pending.get_mut(game_ref)?;
        let mut result = MasteryPlanFoundry {
            quantity: 0,
            ready_quantity: 0,
            unknown_completion_quantity: 0,
            completes_at: None,
        };
        for job in jobs {
            let take = quantity.saturating_sub(result.quantity).min(job.quantity);
            if take == 0 {
                continue;
            }
            job.quantity -= take;
            result.quantity += take;
            match job.completion_at {
                Some(at) if at <= now => result.ready_quantity += take,
                Some(at) => {
                    result.completes_at =
                        Some(result.completes_at.map_or(at, |previous| previous.max(at)));
                }
                None => result.unknown_completion_quantity += take,
            }
        }
        (result.quantity > 0).then_some(result)
    }

    fn reserve_credits(&mut self, known: u64, complete: bool) {
        self.known_cost = self.known_cost.saturating_add(known);
        self.complete_cost &= complete;
        self.credits = self.credits.map(|balance| balance.saturating_sub(known));
    }

    fn record_demand(&mut self, material: &MasteryPlanMaterial, quantity: u32, missing: u32) {
        if quantity == 0 {
            return;
        }
        let entry = self
            .demand
            .entry(material.definition.game_ref.clone())
            .or_insert_with(|| {
                let mut row = material.clone();
                row.definition.quantity = 0;
                row.missing_quantity = 0;
                row.pending_quantity = 0;
                row.planned_quantity = 0;
                row.foundry = None;
                row.component = None;
                row.component_issue = None;
                row
            });
        entry.definition.quantity = entry.definition.quantity.saturating_add(quantity);
        entry.missing_quantity = entry.missing_quantity.saturating_add(missing);
    }
}

fn choose_recipe<'a>(
    game_ref: &str,
    context: &PlanContext<'a>,
    budget: &PlanBudget,
    component: bool,
) -> Option<&'a CraftingRecipeDefinition> {
    let choices = context.recipes.get(game_ref)?;
    if component && choices.len() > 1 {
        let mut owned = choices.iter().filter(|recipe| {
            budget
                .stock
                .get(&recipe.blueprint_game_ref)
                .copied()
                .unwrap_or(0)
                > 0
        });
        let selected = owned.next().copied()?;
        return owned.next().is_none().then_some(selected);
    }
    choices.iter().copied().min_by_key(|recipe| {
        (
            u8::from(
                budget
                    .stock
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
            recipe.blueprint_game_ref.as_str(),
        )
    })
}

/// Чертёж многократного использования приобретается один раз для всего плана.
fn reserve_recipe_cost(
    recipe: &CraftingRecipeDefinition,
    batches: u32,
    budget: &mut PlanBudget,
) -> (u32, bool, u64, Option<u64>) {
    let quantity = budget
        .stock
        .entry(recipe.blueprint_game_ref.clone())
        .or_default();
    let owned = *quantity;
    let needed = if recipe.blueprint_consumed {
        batches
    } else {
        1
    };
    let planned = !recipe.blueprint_consumed
        && budget
            .acquired_blueprints
            .contains(&recipe.blueprint_game_ref);
    let missing = if planned {
        0
    } else {
        needed.saturating_sub(owned)
    };
    if recipe.blueprint_consumed {
        *quantity = quantity.saturating_sub(needed);
    } else if missing > 0 && recipe.blueprint_price.is_some() {
        // Будущая покупка не становится доказательством физического владения.
        budget
            .acquired_blueprints
            .insert(recipe.blueprint_game_ref.clone());
    }
    let build = recipe.build_price.saturating_mul(u64::from(batches));
    let known = build.saturating_add(
        recipe
            .blueprint_price
            .unwrap_or(0)
            .saturating_mul(u64::from(missing)),
    );
    let total = (missing == 0 || recipe.blueprint_price.is_some()).then_some(known);
    budget.reserve_credits(known, total.is_some());
    (owned, planned, known, total)
}

fn recipe_state(
    recipe: &CraftingRecipeDefinition,
    blueprint_owned: u32,
    blueprint_planned: bool,
    blueprint_needed: u32,
    missing_types: usize,
    missing_credits: Option<u64>,
    context: &PlanContext<'_>,
) -> (&'static str, bool) {
    let rank_blocked = recipe
        .mastery_requirement
        .zip(context.account.and_then(|account| account.rank))
        .is_some_and(|(required, current)| required > current);
    let access_unknown = match recipe.mastery_requirement {
        Some(0) => false,
        Some(_) => context.account.and_then(|account| account.rank).is_none(),
        None => true,
    };
    let has_blueprint = blueprint_owned >= blueprint_needed || blueprint_planned;
    let state = if rank_blocked {
        "rank_locked"
    } else if access_unknown {
        "access_unknown"
    } else if missing_types == 0 && missing_credits == Some(0) && has_blueprint {
        if blueprint_planned && blueprint_owned < blueprint_needed {
            "waiting_blueprint"
        } else {
            "craft"
        }
    } else if missing_types == 0
        && missing_credits == Some(0)
        && recipe.blueprint_source == BlueprintSource::Market
    {
        "buy_blueprint"
    } else if missing_types == 1
        && (has_blueprint || recipe.blueprint_source == BlueprintSource::Market)
    {
        "one_short"
    } else {
        "gather"
    };
    (state, rank_blocked)
}

/// Один уровень изготовления: сырьё компонента дальше не разворачивается.
fn plan_component(
    recipe: &CraftingRecipeDefinition,
    missing: u32,
    context: &PlanContext<'_>,
    budget: &mut PlanBudget,
    parent_ref: &str,
) -> Option<MasteryPlanComponent> {
    // Циклический рецепт и переполнение количества не становятся бесплатным компонентом.
    if recipe.result_quantity == 0 {
        return None;
    }
    let batches = missing.div_ceil(recipe.result_quantity);
    let produced = recipe.result_quantity.checked_mul(batches)?;
    let ingredients: Vec<_> = recipe
        .ingredients
        .iter()
        .map(|ingredient| {
            let mut scaled = ingredient.clone();
            scaled.quantity = ingredient.quantity.checked_mul(batches)?;
            (ingredient.game_ref != recipe.result_game_ref && ingredient.game_ref != parent_ref)
                .then_some(scaled)
        })
        .collect::<Option<_>>()?;
    let before_credits = budget.credits;
    let (blueprint_owned, blueprint_planned, known_credits, total_credits) =
        reserve_recipe_cost(recipe, batches, budget);
    let materials: Vec<_> = ingredients
        .iter()
        .map(|ingredient| {
            reserve_material(ingredient, context, budget, false, &recipe.result_game_ref)
        })
        .collect();
    let missing_types = materials
        .iter()
        .filter(|material| material.missing_quantity > 0)
        .count();
    let missing_credits = total_credits
        .zip(before_credits)
        .map(|(cost, balance)| cost.saturating_sub(balance));
    let bp_needed = if recipe.blueprint_consumed {
        batches
    } else {
        1
    };
    let (mut state, rank_blocked) = recipe_state(
        recipe,
        blueprint_owned,
        blueprint_planned,
        bp_needed,
        missing_types,
        missing_credits,
        context,
    );
    if matches!(state, "craft" | "buy_blueprint") {
        if materials
            .iter()
            .any(|material| material.planned_quantity > 0)
        {
            state = "prepare_components";
        } else if materials
            .iter()
            .any(|material| material.pending_quantity > 0)
        {
            state = "waiting_components";
        }
    }
    let surplus = budget
        .planned
        .entry(recipe.result_game_ref.clone())
        .or_default();
    *surplus = surplus.saturating_add(produced.saturating_sub(missing));
    Some(MasteryPlanComponent {
        recipe: recipe.clone(),
        batches,
        required_quantity: missing,
        produced_quantity: produced,
        blueprint_owned,
        blueprint_planned,
        materials,
        missing_types,
        known_credits,
        total_credits,
        missing_credits,
        state,
        rank_blocked,
    })
}

fn reserve_material(
    ingredient: &CraftingIngredientDefinition,
    context: &PlanContext<'_>,
    budget: &mut PlanBudget,
    allow_component: bool,
    parent_ref: &str,
) -> MasteryPlanMaterial {
    let stock = budget.stock.entry(ingredient.game_ref.clone()).or_default();
    let available = *stock;
    let used = available.min(ingredient.quantity);
    *stock -= used;
    // Неосвоенная копия в кузнице, как и готовая копия, нужна сначала для прокачки.
    let foundry = if context
        .unmastered_equipment
        .contains(ingredient.game_ref.as_str())
    {
        None
    } else {
        budget.take_foundry(
            &ingredient.game_ref,
            ingredient.quantity - used,
            context.now,
        )
    };
    let pending = foundry.as_ref().map_or(0, |foundry| foundry.quantity);
    let planned = budget
        .planned
        .entry(ingredient.game_ref.clone())
        .or_default();
    let used_planned = (*planned).min(ingredient.quantity - used - pending);
    *planned -= used_planned;
    let missing = ingredient.quantity - used - pending - used_planned;
    let mut material = MasteryPlanMaterial {
        definition: ingredient.clone(),
        owned_quantity: context
            .owned
            .get(&ingredient.game_ref)
            .copied()
            .unwrap_or(0),
        available_quantity: available,
        protected_quantity: context
            .protected
            .get(&ingredient.game_ref)
            .copied()
            .unwrap_or(0),
        missing_quantity: missing,
        pending_quantity: pending,
        planned_quantity: used_planned,
        foundry,
        component: None,
        component_issue: None,
    };
    if missing > 0 && allow_component && context.recipes.contains_key(ingredient.game_ref.as_str())
    {
        if ingredient.game_ref == parent_ref {
            material.component_issue = Some("invalid_recipe");
        } else if let Some(recipe) = choose_recipe(&ingredient.game_ref, context, budget, true) {
            material.component = plan_component(recipe, missing, context, budget, parent_ref);
            if material.component.is_none() {
                material.component_issue = Some("invalid_recipe");
            }
        } else {
            material.component_issue = Some("ambiguous_recipe");
        }
        if material.component_issue.is_some() {
            budget.complete_cost = false;
        }
    }
    if material.component.is_some() {
        budget.record_demand(&material, used, 0);
    } else {
        budget.record_demand(&material, used.saturating_add(missing), missing);
    }
    material
}

#[allow(clippy::too_many_lines)] // Одна транзакция бюджета включает предмет и один уровень его компонентов.
fn evaluate(
    item: &MasteryItemView,
    context: &PlanContext<'_>,
    budget: &mut PlanBudget,
) -> MasteryPlanItem {
    let owned_quantity = context
        .account
        .and_then(|account| account.owned_equipment.get(&item.game_ref))
        .copied()
        .unwrap_or(0);
    let recipe = choose_recipe(&item.game_ref, context, budget, false);
    let mut row = unavailable_item(&item.game_ref);
    row.display_name.clone_from(&item.display_name);
    row.display_name_en.clone_from(&item.display_name_en);
    row.image_url.clone_from(&item.image_url);
    row.category.clone_from(&item.category);
    row.owned_quantity = owned_quantity;
    row.mastery_rank = item.mastery_rank;
    row.max_rank = item.max_rank;
    row.recipe = recipe.cloned();
    row.remaining_mastery_points = mastery_points_per_rank(&item.category)
        .zip(item.max_rank)
        .filter(|_| item.status != "unknown")
        .map(|(points, max_rank)| {
            u32::from(max_rank.saturating_sub(item.mastery_rank.unwrap_or(0))) * points
        });
    if item.status == "mastered" {
        row.state = "mastered";
    } else if owned_quantity > 0 {
        row.state = "owned";
    } else if let Some(foundry) = budget.take_foundry(&item.game_ref, 1, context.now) {
        row.state = if foundry.ready_quantity > 0 {
            "ready_to_claim"
        } else {
            "crafting"
        };
        row.foundry = Some(foundry);
    } else if item.status == "unknown" || !context.inventory_available {
        budget.complete_cost = false;
        return row;
    } else if let Some(recipe) = recipe {
        let before_credits = budget.credits;
        let before_cost = budget.known_cost;
        let (blueprint_owned, blueprint_planned, _, own_total) =
            reserve_recipe_cost(recipe, 1, budget);
        row.blueprint_owned = blueprint_owned;
        row.blueprint_planned = blueprint_planned;
        row.materials = recipe
            .ingredients
            .iter()
            .map(|ingredient| {
                reserve_material(ingredient, context, budget, true, &recipe.result_game_ref)
            })
            .collect();
        row.missing_types = row
            .materials
            .iter()
            .filter(|material| material.missing_quantity > 0)
            .count();
        row.known_credits = budget.known_cost.saturating_sub(before_cost);
        let complete = own_total.is_some()
            && row
                .materials
                .iter()
                .all(|material| material.component_issue.is_none())
            && row
                .materials
                .iter()
                .filter_map(|material| material.component.as_ref())
                .all(|component| component.total_credits.is_some());
        row.total_credits = complete.then_some(row.known_credits);
        row.missing_credits = row
            .total_credits
            .zip(before_credits)
            .map(|(cost, balance)| cost.saturating_sub(balance));
        let (state, blocked) = recipe_state(
            recipe,
            blueprint_owned,
            blueprint_planned,
            1,
            row.missing_types,
            row.missing_credits,
            context,
        );
        row.state = state;
        row.rank_blocked = blocked;
        let components_ready = row
            .materials
            .iter()
            .filter(|material| material.missing_quantity > 0)
            .all(|material| {
                material.component.as_ref().is_some_and(|component| {
                    matches!(
                        component.state,
                        "craft"
                            | "buy_blueprint"
                            | "waiting_blueprint"
                            | "prepare_components"
                            | "waiting_components"
                    )
                })
            });
        let bp_available = blueprint_owned > 0
            || blueprint_planned
            || recipe.blueprint_source == BlueprintSource::Market;
        if !matches!(
            state,
            "rank_locked" | "access_unknown" | "waiting_blueprint"
        ) {
            if components_ready && bp_available && row.missing_credits == Some(0) {
                if row
                    .materials
                    .iter()
                    .any(|material| material.component.is_some() || material.planned_quantity > 0)
                {
                    row.state = "prepare_components";
                } else if row
                    .materials
                    .iter()
                    .any(|material| material.pending_quantity > 0)
                {
                    row.state = "waiting_components";
                }
            } else if row
                .materials
                .iter()
                .any(|material| material.component.is_some())
            {
                row.state = "gather";
            }
        }
        return row;
    } else {
        row.state = "no_recipe";
        budget.complete_cost = false;
        return row;
    }
    row.total_credits = Some(0);
    row.missing_credits = Some(0);
    row
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
        blueprint_planned: false,
        materials: vec![],
        missing_types: 0,
        total_credits: None,
        missing_credits: None,
        rank_blocked: false,
        foundry: None,
        known_credits: 0,
    }
}

fn state_priority(state: &str) -> u8 {
    match state {
        "owned" | "ready_to_claim" => 0,
        "crafting" | "craft" => 1,
        "buy_blueprint" => 2,
        "waiting_blueprint" | "prepare_components" | "waiting_components" => 3,
        "one_short" => 4,
        "gather" => 5,
        "rank_locked" => 6,
        "access_unknown" => 7,
        "no_recipe" => 8,
        _ => 9,
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
                result_quantity: 1,
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
        let mut mastery = MasteryView {
            observed_at: Some(Utc::now()),
            source: Some("inventory_xp_info"),
            refresh_failed: false,
            catalog_available: true,
            items,
        };
        let account = PlannerAccount {
            key: "account-a".into(),
            owned_equipment: BTreeMap::new(),
            foundry_available: true,
            foundry_issue: false,
            foundry_jobs: vec![],
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
        // Кузница уже оплатила сырьё и кредиты, включая неизвестную дату готовности.
        let mut account = PlannerAccount {
            rank: Some(30),
            ..account
        };
        account.foundry_jobs = vec![
            crate::mastery::FoundryJob {
                recipe_game_ref: metadata.crafting_recipes[0].blueprint_game_ref.clone(),
                completion_at: Some(Utc::now() - chrono::Duration::hours(1)),
            },
            crate::mastery::FoundryJob {
                recipe_game_ref: metadata.crafting_recipes[1].blueprint_game_ref.clone(),
                completion_at: None,
            },
        ];
        let refs: Vec<_> = mastery
            .items
            .iter()
            .map(|item| item.game_ref.clone())
            .collect();
        mastery.items[0].status = "unknown";
        let foundry = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(0),
            refs.clone(),
            &BTreeMap::new(),
        );
        assert_eq!(foundry.queue[0].state, "ready_to_claim");
        assert_eq!(foundry.queue[1].state, "crafting");
        assert_eq!(
            foundry.queue[1]
                .foundry
                .as_ref()
                .unwrap()
                .unknown_completion_quantity,
            1
        );
        assert!(
            foundry
                .queue
                .iter()
                .all(|item| item.materials.is_empty() && item.total_credits == Some(0))
        );
        assert!(foundry.plan_materials.is_empty());
        assert_eq!(foundry.total_plan_credits, Some(0));
        mastery.items[0].status = "progress";
        account.foundry_jobs.clear();

        // Готовая деталь используется первой; из партии остаются две штуки для второго предмета.
        inventory.items[0].owned_quantity = 3;
        for recipe in &mut metadata.crafting_recipes {
            recipe.ingredients[0].quantity = 2;
        }
        let mut raw = inventory.items[0].clone();
        raw.canonical_game_id = "/Lotus/Material/Raw".into();
        raw.display_name = "Raw".into();
        raw.owned_quantity = 4;
        raw.personal_reserved_quantity = 0;
        inventory.items.push(raw);
        let mut component = metadata.crafting_recipes[0].clone();
        component.result_game_ref = "/Lotus/Material/A".into();
        component.blueprint_game_ref = "/Lotus/Recipe/Component".into();
        component.result_quantity = 3;
        component.blueprint_consumed = false;
        component.blueprint_price = Some(10);
        component.build_price = 20;
        component.mastery_requirement = Some(0);
        component.ingredients[0].game_ref = "/Lotus/Material/Raw".into();
        component.ingredients[0].quantity = 4;
        metadata.crafting_recipes.push(component.clone());
        let parts = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(1000),
            refs.clone(),
            &BTreeMap::new(),
        );
        assert_eq!(parts.queue[0].state, "prepare_components");
        let fabrication = parts.queue[0].materials[0].component.as_ref().unwrap();
        assert!(!fabrication.blueprint_planned);
        assert_eq!(
            (
                fabrication.batches,
                fabrication.required_quantity,
                fabrication.produced_quantity
            ),
            (1, 1, 3)
        );
        assert_eq!(fabrication.total_credits, Some(30));
        assert!(fabrication.materials[0].component.is_none());
        assert_eq!(parts.queue[1].materials[0].planned_quantity, 2);
        assert!(parts.queue[1].materials[0].component.is_none());
        assert_eq!(parts.queue[1].state, "prepare_components");
        assert_eq!(parts.total_plan_credits, Some(630));
        let raw_budget = parts
            .plan_materials
            .iter()
            .find(|material| material.definition.game_ref.ends_with("Raw"))
            .unwrap();
        assert_eq!(
            (raw_budget.definition.quantity, raw_budget.missing_quantity),
            (4, 0)
        );

        // Единственная активная партия обеспечивает оба предмета: нового сырья и платы нет.
        account.foundry_jobs.push(crate::mastery::FoundryJob {
            recipe_game_ref: component.blueprint_game_ref.clone(),
            completion_at: Some(Utc::now() + chrono::Duration::hours(1)),
        });
        let pending = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(1000),
            refs.clone(),
            &BTreeMap::new(),
        );
        assert_eq!(pending.queue[0].materials[0].pending_quantity, 1);
        assert_eq!(pending.queue[1].materials[0].pending_quantity, 2);
        assert!(pending.queue.iter().all(
            |item| item.state == "waiting_components" && item.materials[0].component.is_none()
        ));
        assert_eq!(pending.total_plan_credits, Some(600));
        assert!(
            !pending
                .plan_materials
                .iter()
                .any(|material| material.definition.game_ref.ends_with("Raw"))
        );

        // Частично оплаченная партия не дублируется; сырьё и многоразовый чертёж общие.
        metadata.crafting_recipes[0].ingredients[0].quantity = 5;
        metadata.crafting_recipes[1].ingredients[0].quantity = 5;
        let partial = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(640),
            refs.clone(),
            &BTreeMap::new(),
        );
        assert_eq!(partial.queue[0].materials[0].pending_quantity, 3);
        assert_eq!(partial.queue[1].materials[0].pending_quantity, 0);
        assert_eq!(partial.queue[1].materials[0].planned_quantity, 2);
        assert_eq!(
            partial.queue[0].materials[0]
                .component
                .as_ref()
                .unwrap()
                .total_credits,
            Some(30)
        );
        let second = partial.queue[1].materials[0].component.as_ref().unwrap();
        assert_eq!(second.total_credits, Some(20));
        assert_eq!(second.blueprint_owned, 0); // Будущая покупка не выдаётся за уже имеющийся чертёж.
        assert!(second.blueprint_planned);
        assert_eq!(second.materials[0].missing_quantity, 4);
        assert_eq!(partial.total_plan_credits, Some(650));
        assert_eq!(partial.missing_plan_credits, Some(10));
        let raw_budget = partial
            .plan_materials
            .iter()
            .find(|material| material.definition.game_ref.ends_with("Raw"))
            .unwrap();
        assert_eq!(
            (raw_budget.definition.quantity, raw_budget.missing_quantity),
            (8, 4)
        );

        // Следующая партия не предлагает купить тот же многоразовый чертёж ещё раз.
        inventory.items[1].owned_quantity = 8;
        let sufficient_raw = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(1000),
            refs.clone(),
            &BTreeMap::new(),
        );
        let first = sufficient_raw.queue[0].materials[0]
            .component
            .as_ref()
            .unwrap();
        assert!(!first.blueprint_planned);
        assert_eq!(first.state, "buy_blueprint");
        let second = sufficient_raw.queue[1].materials[0]
            .component
            .as_ref()
            .unwrap();
        assert!(second.blueprint_planned);
        assert_eq!(second.blueprint_owned, 0);
        assert_eq!(second.state, "waiting_blueprint");
        inventory.items[1].owned_quantity = 4;

        // Не выбираем произвольный альтернативный рецепт без подтверждённого чертежа.
        component.blueprint_game_ref = "/Lotus/Recipe/Alternative".into();
        metadata.crafting_recipes.push(component);
        let ambiguous = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(1000),
            refs,
            &BTreeMap::new(),
        );
        assert!(ambiguous.queue[0].materials[0].component.is_none());
        assert!(ambiguous.queue[0].materials[0].missing_quantity > 0);
        assert_eq!(
            ambiguous.queue[0].materials[0].component_issue,
            Some("ambiguous_recipe")
        );
        assert!(ambiguous.queue[0].total_credits.is_none());
        assert!(ambiguous.total_plan_credits.is_none());

        // Неосвоенная копия в кузнице сохраняется для прокачки, а не уходит в другое оружие.
        let equipment_ref = mastery.items[0].game_ref.clone();
        let parent_ref = mastery.items[1].game_ref.clone();
        metadata.crafting_recipes[1].ingredients[0].game_ref = equipment_ref.clone();
        metadata.crafting_recipes[1].ingredients[0].quantity = 1;
        metadata.crafting_recipes[1].ingredients[0].equipment = true;
        account.foundry_jobs = vec![crate::mastery::FoundryJob {
            recipe_game_ref: metadata.crafting_recipes[0].blueprint_game_ref.clone(),
            completion_at: Some(Utc::now() - chrono::Duration::hours(1)),
        }];
        let protected_foundry = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(1000),
            vec![parent_ref.clone(), equipment_ref],
            &BTreeMap::new(),
        );
        assert_eq!(protected_foundry.queue[0].materials[0].pending_quantity, 0);
        assert!(protected_foundry.queue[0].materials[0].foundry.is_none());
        assert_eq!(protected_foundry.queue[1].state, "ready_to_claim");
        assert_eq!(
            protected_foundry.queue[1]
                .foundry
                .as_ref()
                .unwrap()
                .quantity,
            1
        );

        // После освоения ожидаемая дополнительная копия пригодна как ингредиент.
        mastery.items[0].status = "mastered";
        let mastered_foundry = build_plan(
            &mastery,
            Some(&account),
            Some(&metadata),
            Some(&inventory),
            Some(1000),
            vec![parent_ref],
            &BTreeMap::new(),
        );
        assert_eq!(mastered_foundry.queue[0].materials[0].pending_quantity, 1);
        assert!(mastered_foundry.queue[0].materials[0].component.is_none());
        assert_eq!(mastered_foundry.total_plan_credits, Some(300));
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
