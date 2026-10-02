//! Подсказка перед открытием реликвии использует точное улучшение и текущий аккаунт.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use platscope_domain::{
    GameMetadataSnapshot, MarketVariantKey, Platform, PriceConfidence, PriceFreshness,
    RelicRefinement,
};
use platscope_insights::{RelicPricingCoverage, RelicRewardInput, calculate_relic_ev};
use platscope_storage::Database;
use serde::{Deserialize, Serialize};

use crate::{
    AppSettings, CoreError, InventoryService, InventoryView, MasteryPlanService, MasteryService,
    PersonalGoalsService, PriceRecommendation, RelicInsightRow, build_relic_insights,
    lock_database,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicEra {
    Lith,
    Meso,
    Neo,
    Axi,
    Requiem,
    Omnia,
}

impl RelicEra {
    fn from_slug(slug: &str) -> Option<Self> {
        match slug.split('_').next()? {
            "lith" => Some(Self::Lith),
            "meso" => Some(Self::Meso),
            "neo" => Some(Self::Neo),
            "axi" => Some(Self::Axi),
            "requiem" => Some(Self::Requiem),
            _ => None,
        }
    }

    fn accepts(self, era: Self) -> bool {
        self == era
            || (self == Self::Omnia
                && matches!(era, Self::Lith | Self::Meso | Self::Neo | Self::Axi))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelicSelectionRequest {
    pub era: Option<RelicEra>,
    pub mission_name: Option<String>,
    pub selected_key: Option<MarketVariantKey>,
    pub selected_relic_name: Option<String>,
    #[serde(default)]
    pub consumed: Vec<RelicSelectionConsumption>,
    #[serde(default)]
    pub base_consumed: Vec<RelicSelectionBaseConsumption>,
    pub baseline_inventory_checksum: Option<String>,
    pub baseline_reserve_account_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelicSelectionConsumption {
    pub key: MarketVariantKey,
    pub quantity: u32,
}

/// Открытие с известным названием, но без подтверждённого улучшения.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelicSelectionBaseConsumption {
    pub relic_name: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelicSelectionView {
    pub inventory_checksum: String,
    pub reserve_account_key: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub era: Option<RelicEra>,
    pub mission_name: Option<String>,
    pub goals_available: bool,
    pub mastery_available: bool,
    /// Есть остаток названия, но отдельные улучшения могли быть полностью израсходованы.
    pub has_uncertain_stock: bool,
    pub selected: Option<RelicSelectionRow>,
    pub selected_relic_name: Option<String>,
    pub selected_remaining_quantity: Option<u32>,
    pub selected_refinement_known: bool,
    /// Состав просматриваемого названия; без подтверждённого улучшения шансы неизвестны.
    pub selected_rewards: Vec<RelicSelectionReward>,
    pub recommendations: Vec<RelicSelectionRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelicSelectionRow {
    pub key: MarketVariantKey,
    pub relic_slug: String,
    pub refinement: RelicRefinement,
    pub era: RelicEra,
    pub display_name: String,
    pub display_name_en: String,
    pub image_url: Option<String>,
    /// Физические копии выбранного улучшения, включая оставляемые для себя.
    pub owned_quantity: u32,
    pub opened_quantity: u32,
    /// Неизвестно, если одно из открытий этого названия не подтвердило улучшение.
    pub remaining_quantity: Option<u32>,
    pub remaining_total_quantity: u32,
    /// Нижняя граница после открытий, улучшение которых подтвердить не удалось.
    pub remaining_quantity_lower_bound: u32,
    /// Ожидание для одной реликвии; состав публичного отряда не угадывается.
    pub expected_platinum: Option<f64>,
    pub pricing_coverage: RelicPricingCoverage,
    pub priced_chance_percent: f64,
    pub expected_ducats: Option<f64>,
    pub ducat_coverage_percent: f64,
    pub goal_chance_percent: f64,
    pub crafting_chance_percent: f64,
    pub mastery_chance_percent: f64,
    pub rewards: Vec<RelicSelectionReward>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelicSelectionReward {
    pub game_ref: String,
    pub slug: Option<String>,
    pub display_name: String,
    pub display_name_en: String,
    pub image_url: Option<String>,
    pub chance_percent: Option<f64>,
    pub price: Option<f64>,
    pub ducats: Option<u32>,
    pub owned_quantity: u32,
    pub needed_for_goal: bool,
    pub needed_for_craft: bool,
    pub needed_for_mastery: bool,
    /// Одна деталь может одновременно отвечать нескольким целям; количества не складываются.
    pub needed_quantity: u32,
    pub goal_names: Vec<String>,
    pub crafting_names: Vec<String>,
    pub mastery_names: Vec<String>,
}

#[derive(Default)]
struct Need {
    quantity: u32,
    names: BTreeSet<String>,
}

type Needs = BTreeMap<String, Need>;

#[derive(Default)]
struct SelectionNeeds {
    goals: Needs,
    crafting: Needs,
    mastery: Needs,
}

pub struct RelicSelectionService;

impl RelicSelectionService {
    /// Собирает подсказку из локальных цен и согласованного инвентаря.
    ///
    /// # Errors
    /// Возвращает ошибку хранилища или смены снимка/аккаунта во время расчёта.
    pub fn view(
        database: &Mutex<Database>,
        settings: &AppSettings,
        request: &RelicSelectionRequest,
    ) -> Result<Option<RelicSelectionView>, CoreError> {
        let Some(inventory) = InventoryService::view(database, settings)? else {
            return Ok(None);
        };
        let (metadata, mastery, account) = {
            let guard = lock_database(database)?;
            let Some(metadata) = guard.load_current_game_metadata()? else {
                return Ok(None);
            };
            let (mastery, account) = MasteryService::planner_context(&guard)?;
            (metadata, mastery, account)
        };
        if inventory.reserve_account_key.as_ref() != account.as_ref().map(|account| &account.key) {
            return Err(CoreError::InventoryData(
                "account changed during relic selection".into(),
            ));
        }
        let mut needs = SelectionNeeds::default();
        let goals_available = account.is_some();
        if goals_available {
            let goals = PersonalGoalsService::view(database, settings)?;
            for goal in goals
                .goals
                .iter()
                .filter(|goal| goal.completed_at.is_none())
            {
                for part in &goal.parts {
                    add_need(
                        &mut needs.goals,
                        &part.slug,
                        part.required_quantity
                            .saturating_sub(part.allocated_quantity),
                        &goal.set.display_name,
                    );
                }
            }
        }
        let mastery_available =
            account.is_some() && mastery.observed_at.is_some() && !mastery.refresh_failed;
        if let Some(account) = account.as_ref().filter(|_| mastery_available) {
            collect_mastery_needs(
                &mut needs.mastery,
                &metadata,
                &inventory,
                &mastery,
                &account.owned_equipment,
            );
            // Только явная очередь изготовления, без приписывания всего каталога планам игрока.
            let plan = MasteryPlanService::view(database, settings)?;
            if plan.inventory_checksum.as_deref()
                != Some(inventory.metadata.checksum_sha256.as_str())
            {
                return Err(CoreError::InventoryData(
                    "inventory changed during relic selection".into(),
                ));
            }
            for item in &plan.queue {
                if item.owned_quantity > 0 {
                    continue;
                }
                if let Some(recipe) = item.recipe.as_ref() {
                    if item.blueprint_owned == 0 {
                        add_need(
                            &mut needs.crafting,
                            &recipe.blueprint_game_ref,
                            1,
                            &item.display_name,
                        );
                    }
                }
                for material in &item.materials {
                    add_need(
                        &mut needs.crafting,
                        &material.definition.game_ref,
                        material.missing_quantity,
                        &item.display_name,
                    );
                }
            }
        }
        let relics = build_relic_insights(
            database,
            settings,
            &metadata.relics,
            &metadata.prime_sets,
            &inventory.items,
        )?;
        let mut recommendations = rank_rows(
            relics,
            &metadata,
            &inventory,
            settings.platform,
            request,
            &needs,
        );
        let selected = request
            .selected_key
            .as_ref()
            .and_then(|key| recommendations.iter().find(|row| &row.key == key).cloned());
        let selected_relic_name = request
            .selected_relic_name
            .clone()
            .or_else(|| selected.as_ref().map(|row| relic_name(&row.relic_slug)));
        let selected_remaining_quantity = selected_relic_name.as_deref().and_then(|name| {
            recommendations
                .iter()
                .find(|row| relic_name(&row.relic_slug) == normalize_name(name))
                .map(|row| row.remaining_total_quantity)
        });
        let selected_refinement_known = selected.is_some();
        let selected_rewards = selection_reward_details(
            &recommendations,
            selected.as_ref(),
            selected_relic_name.as_deref(),
        );
        recommendations.retain(|row| request.era.is_none_or(|era| era.accepts(row.era)));
        let has_uncertain_stock = retain_available_recommendations(&mut recommendations);
        // Нельзя смешать историю/личные цели предыдущего аккаунта с новым снимком.
        {
            let guard = lock_database(database)?;
            let current = guard.current_inventory_snapshot()?;
            let (_, current_account) = MasteryService::planner_context(&guard)?;
            if current
                .as_ref()
                .map(|snapshot| &snapshot.metadata.checksum_sha256)
                != Some(&inventory.metadata.checksum_sha256)
                || current_account.as_ref().map(|account| &account.key)
                    != inventory.reserve_account_key.as_ref()
            {
                return Err(CoreError::InventoryData(
                    "inventory changed during relic selection".into(),
                ));
            }
        }
        Ok(Some(RelicSelectionView {
            inventory_checksum: inventory.metadata.checksum_sha256,
            reserve_account_key: inventory.reserve_account_key,
            observed_at: inventory.metadata.observed_at,
            era: request.era,
            mission_name: request.mission_name.clone(),
            goals_available,
            mastery_available,
            has_uncertain_stock,
            selected,
            selected_relic_name,
            selected_remaining_quantity,
            selected_refinement_known,
            selected_rewards,
            recommendations,
        }))
    }
}

fn retain_available_recommendations(rows: &mut Vec<RelicSelectionRow>) -> bool {
    let uncertain = rows.iter().any(|row| {
        row.remaining_quantity.is_none()
            && row.remaining_total_quantity > 0
            && row.remaining_quantity_lower_bound == 0
    });
    rows.retain(|row| row.remaining_total_quantity > 0 && row.remaining_quantity_lower_bound > 0);
    rows.truncate(8);
    uncertain
}

fn add_need(needs: &mut Needs, identity: &str, quantity: u32, name: &str) {
    if quantity == 0 || identity.is_empty() {
        return;
    }
    let need = needs.entry(identity.to_owned()).or_default();
    need.quantity = need.quantity.max(quantity);
    need.names.insert(name.into());
}

fn collect_mastery_needs(
    needs: &mut Needs,
    metadata: &GameMetadataSnapshot,
    inventory: &InventoryView,
    mastery: &crate::MasteryView,
    owned_equipment: &BTreeMap<String, u32>,
) {
    let physical = physical_quantities(inventory);
    for item in &mastery.items {
        // Уже имеющийся предмет нужно прокачать, а неизвестный статус нельзя считать неосвоенным.
        if item.status != "progress"
            || owned_equipment.get(&item.game_ref).copied().unwrap_or(0) > 0
        {
            continue;
        }
        for set in metadata
            .prime_sets
            .iter()
            .filter(|set| item.set_slugs.contains(&set.set_slug))
        {
            for part in &set.components {
                let blueprint_owned = physical.get(part.game_ref.as_str()).copied().unwrap_or(0);
                let completed_owned = metadata
                    .crafting_recipes
                    .iter()
                    .filter(|recipe| recipe.blueprint_game_ref == part.game_ref)
                    .map(|recipe| {
                        physical
                            .get(recipe.result_game_ref.as_str())
                            .copied()
                            .unwrap_or(0)
                    })
                    .max()
                    .unwrap_or(0);
                let owned = blueprint_owned.saturating_add(completed_owned);
                add_need(
                    needs,
                    &part.slug,
                    part.required_quantity.saturating_sub(owned),
                    &item.display_name,
                );
            }
        }
    }
}

fn physical_quantities(inventory: &InventoryView) -> HashMap<&str, u32> {
    let mut quantities = HashMap::<&str, u32>::new();
    for item in &inventory.items {
        // Разрешение рынка и передаваемость не определяют физическое наличие материалов.
        let count = quantities.entry(&item.canonical_game_id).or_default();
        *count = count.saturating_add(item.owned_quantity);
    }
    quantities
}

fn known_price(recommendation: Option<&PriceRecommendation>) -> Option<f64> {
    recommendation
        .filter(|price| {
            matches!(
                price.freshness,
                PriceFreshness::Fresh | PriceFreshness::Aging
            )
        })
        .filter(|price| {
            matches!(
                price.confidence,
                PriceConfidence::High | PriceConfidence::Medium
            )
        })
        .and_then(|price| price.fair_price)
        .filter(|price| price.is_finite() && *price > 0.0)
}

fn selection_reward_details(
    rows: &[RelicSelectionRow],
    selected: Option<&RelicSelectionRow>,
    name: Option<&str>,
) -> Vec<RelicSelectionReward> {
    if let Some(selected) = selected {
        return selected.rewards.clone();
    }
    let Some(name) = name else {
        return vec![];
    };
    let Some(row) = rows
        .iter()
        .find(|row| relic_name(&row.relic_slug) == normalize_name(name))
    else {
        return vec![];
    };
    row.rewards
        .iter()
        .cloned()
        .map(|mut reward| {
            reward.chance_percent = None;
            reward
        })
        .collect()
}

fn matched_need<'a>(needs: &'a Needs, slug: Option<&str>, game_ref: &str) -> Option<&'a Need> {
    slug.and_then(|slug| needs.get(slug))
        .or_else(|| needs.get(game_ref))
}

fn reward_ducats(
    metadata: &GameMetadataSnapshot,
    slug: Option<&str>,
    game_ref: &str,
) -> Option<u32> {
    let mut values = metadata
        .prime_parts
        .iter()
        .filter(|part| part.game_ref == game_ref || Some(part.slug.as_str()) == slug)
        .map(|part| part.ducats)
        .chain(
            metadata
                .prime_sets
                .iter()
                .flat_map(|set| &set.components)
                .filter(|part| part.game_ref == game_ref || Some(part.slug.as_str()) == slug)
                .filter_map(|part| part.ducats),
        );
    if let Some(value) = values.next() {
        return values.all(|next| next == value).then_some(value);
    }
    // Чертёж Формы не обменивается на дукаты; прочие отсутствующие детали не угадываем.
    game_ref.ends_with("/FormaBlueprint").then_some(0)
}

fn rank_rows(
    relics: Vec<RelicInsightRow>,
    metadata: &GameMetadataSnapshot,
    inventory: &InventoryView,
    platform: Platform,
    request: &RelicSelectionRequest,
    needs: &SelectionNeeds,
) -> Vec<RelicSelectionRow> {
    let physical = physical_quantities(inventory);
    let consumption_matches = request.baseline_inventory_checksum.as_deref()
        == Some(inventory.metadata.checksum_sha256.as_str())
        && request.baseline_reserve_account_key == inventory.reserve_account_key;
    let consumed = if consumption_matches {
        request.consumed.iter().fold(
            HashMap::<&MarketVariantKey, u32>::new(),
            |mut totals, entry| {
                let count = totals.entry(&entry.key).or_default();
                *count = count.saturating_add(entry.quantity);
                totals
            },
        )
    } else {
        HashMap::new()
    };
    let base_consumed = if consumption_matches {
        request
            .base_consumed
            .iter()
            .fold(HashMap::<String, u32>::new(), |mut totals, entry| {
                let count = totals.entry(normalize_name(&entry.relic_name)).or_default();
                *count = count.saturating_add(entry.quantity);
                totals
            })
    } else {
        HashMap::new()
    };
    let mut base_totals = HashMap::<String, (u32, u32)>::new();
    let mut counted_variants = HashSet::new();
    for relic in &relics {
        if let Ok(key) = MarketVariantKey::new(
            &relic.definition.relic_slug,
            platform,
            None,
            Some(relic.definition.refinement.market_subtype()),
        ) {
            if !counted_variants.insert(key.clone()) {
                continue;
            }
            let total = base_totals
                .entry(relic.definition.relic_slug.clone())
                .or_default();
            total.0 = total.0.saturating_add(relic.owned_quantity);
            total.1 = total.1.saturating_add(
                consumed
                    .get(&key)
                    .copied()
                    .unwrap_or(0)
                    .min(relic.owned_quantity),
            );
        }
    }
    let mut rows = Vec::new();
    let mut emitted_variants = HashSet::new();
    for relic in relics {
        let Some(era) = RelicEra::from_slug(&relic.definition.relic_slug) else {
            continue;
        };
        if request.era.is_some_and(|requested| !requested.accepts(era))
            && !request.selected_relic_name.as_deref().is_some_and(|name| {
                normalize_name(name) == relic_name(&relic.definition.relic_slug)
            })
        {
            continue;
        }
        let Ok(key) = MarketVariantKey::new(
            &relic.definition.relic_slug,
            platform,
            None,
            Some(relic.definition.refinement.market_subtype()),
        ) else {
            continue;
        };
        if !emitted_variants.insert(key.clone()) {
            continue;
        }
        let opened_quantity = consumed
            .get(&key)
            .copied()
            .unwrap_or(0)
            .min(relic.owned_quantity);
        let unknown_opened = base_consumed
            .get(&relic_name(&relic.definition.relic_slug))
            .copied()
            .unwrap_or(0);
        let (base_owned, base_opened) = base_totals
            .get(&relic.definition.relic_slug)
            .copied()
            .unwrap_or((0, 0));
        let remaining_total_quantity = base_owned
            .saturating_sub(base_opened)
            .saturating_sub(unknown_opened);
        let rewards: Vec<_> = relic
            .rewards
            .into_iter()
            .map(|reward| {
                let slug = reward.definition.reward_slug.as_deref();
                let game_ref = &reward.definition.reward_game_ref;
                let goal = matched_need(&needs.goals, slug, game_ref);
                let crafting = matched_need(&needs.crafting, slug, game_ref);
                let mastery = matched_need(&needs.mastery, slug, game_ref);
                RelicSelectionReward {
                    game_ref: game_ref.clone(),
                    slug: reward.definition.reward_slug.clone(),
                    display_name: reward.display_name,
                    display_name_en: reward.definition.display_name_en,
                    image_url: reward.image_url,
                    chance_percent: Some(reward.definition.chance_percent),
                    price: known_price(reward.recommendation.as_ref()),
                    ducats: reward_ducats(metadata, slug, game_ref),
                    owned_quantity: physical.get(game_ref.as_str()).copied().unwrap_or(0),
                    needed_for_goal: goal.is_some(),
                    needed_for_craft: crafting.is_some(),
                    needed_for_mastery: mastery.is_some(),
                    needed_quantity: [goal, crafting, mastery]
                        .into_iter()
                        .flatten()
                        .map(|need| need.quantity)
                        .max()
                        .unwrap_or(0),
                    goal_names: goal
                        .map_or_else(Vec::new, |need| need.names.iter().cloned().collect()),
                    crafting_names: crafting
                        .map_or_else(Vec::new, |need| need.names.iter().cloned().collect()),
                    mastery_names: mastery
                        .map_or_else(Vec::new, |need| need.names.iter().cloned().collect()),
                }
            })
            .collect();
        let reward_inputs: Vec<_> = rewards
            .iter()
            .map(|reward| RelicRewardInput {
                reward_slug: reward.slug.as_deref(),
                chance_percent: reward.chance_percent.unwrap_or(f64::NAN),
                fair_price: reward.price,
                confidence: if reward.price.is_some() {
                    PriceConfidence::High
                } else {
                    PriceConfidence::Unknown
                },
            })
            .collect();
        let ev = calculate_relic_ev(&reward_inputs);
        let valid_chance = |chance: f64| chance.is_finite() && (0.0..=100.0).contains(&chance);
        let ducat_coverage_percent: f64 = rewards
            .iter()
            .filter(|reward| {
                reward.ducats.is_some() && reward.chance_percent.is_some_and(valid_chance)
            })
            .filter_map(|reward| reward.chance_percent)
            .sum();
        let expected_ducats = (rewards.iter().all(|reward| {
            reward.ducats.is_some() && reward.chance_percent.is_some_and(valid_chance)
        }) && (99.0..=101.0).contains(&ev.total_chance_percent))
        .then(|| {
            rewards
                .iter()
                .map(|reward| {
                    reward.chance_percent.unwrap_or(0.0) / 100.0
                        * f64::from(reward.ducats.unwrap_or(0))
                })
                .sum()
        });
        let chance = |predicate: fn(&RelicSelectionReward) -> bool| {
            rewards
                .iter()
                .filter(|reward| {
                    predicate(reward) && reward.chance_percent.is_some_and(valid_chance)
                })
                .filter_map(|reward| reward.chance_percent)
                .sum::<f64>()
                .min(100.0)
        };
        rows.push(RelicSelectionRow {
            key,
            relic_slug: relic.definition.relic_slug,
            refinement: relic.definition.refinement,
            era,
            display_name: relic.display_name,
            display_name_en: relic.definition.display_name_en,
            image_url: relic.image_url,
            owned_quantity: relic.owned_quantity,
            opened_quantity,
            remaining_quantity: (unknown_opened == 0)
                .then_some(relic.owned_quantity.saturating_sub(opened_quantity)),
            remaining_total_quantity,
            remaining_quantity_lower_bound: relic
                .owned_quantity
                .saturating_sub(opened_quantity)
                .saturating_sub(unknown_opened),
            expected_platinum: ev.priced_expected_value,
            pricing_coverage: ev.coverage,
            priced_chance_percent: ev.priced_chance_percent,
            expected_ducats,
            ducat_coverage_percent,
            goal_chance_percent: chance(|reward| reward.needed_for_goal),
            crafting_chance_percent: chance(|reward| reward.needed_for_craft),
            mastery_chance_percent: chance(|reward| reward.needed_for_mastery),
            rewards,
        });
    }
    rows.sort_by(|a, b| {
        b.goal_chance_percent
            .total_cmp(&a.goal_chance_percent)
            .then(
                b.crafting_chance_percent
                    .total_cmp(&a.crafting_chance_percent),
            )
            .then(
                b.mastery_chance_percent
                    .total_cmp(&a.mastery_chance_percent),
            )
            .then(
                b.expected_platinum
                    .unwrap_or(-1.0)
                    .total_cmp(&a.expected_platinum.unwrap_or(-1.0)),
            )
            .then(
                b.expected_ducats
                    .unwrap_or(-1.0)
                    .total_cmp(&a.expected_ducats.unwrap_or(-1.0)),
            )
            .then(b.remaining_total_quantity.cmp(&a.remaining_total_quantity))
            .then(a.display_name.cmp(&b.display_name))
            .then(a.refinement.cmp(&b.refinement))
    });
    rows
}

fn relic_name(slug: &str) -> String {
    slug.strip_suffix("_relic")
        .unwrap_or(slug)
        .replace('_', " ")
}

fn normalize_name(name: &str) -> String {
    name.split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use platscope_domain::{
        InventorySnapshotMetadata, InventorySource, RelicDefinition, RelicRewardDefinition,
        VaultStatus,
    };

    fn inventory() -> InventoryView {
        InventoryView {
            metadata: InventorySnapshotMetadata {
                source: InventorySource::ReadOnlyScan,
                observed_at: Utc::now(),
                schema_version: 3,
                item_count: 0,
                checksum_sha256: "one".into(),
            },
            keep_copies: 1,
            reserve_account_key: Some("account-a".into()),
            mod_usage_scanned: true,
            summary: crate::InventorySummary {
                owned_quantity: 0,
                sellable_quantity: 0,
                resolved_rows: 0,
                attention_rows: 0,
            },
            items: vec![],
        }
    }

    fn relic(slug: &str, refinement: RelicRefinement, quantity: u32) -> RelicInsightRow {
        let chances = if refinement == RelicRefinement::Radiant {
            [16.67, 16.67, 16.67, 20.0, 20.0, 10.0]
        } else {
            [25.33, 25.33, 25.33, 11.0, 11.0, 2.0]
        };
        let prices = [10.0, 20.0, 30.0, 40.0, 50.0, 100.0];
        let rewards: Vec<_> = chances
            .into_iter()
            .enumerate()
            .map(|(index, chance)| {
                let reward_slug = format!("part_{index}");
                crate::RelicRewardInsight {
                    definition: RelicRewardDefinition {
                        reward_slug: Some(reward_slug.clone()),
                        reward_game_ref: format!("/Lotus/Part{index}"),
                        display_name_en: reward_slug.clone(),
                        chance_percent: chance,
                    },
                    display_name: reward_slug.clone(),
                    image_url: None,
                    recommendation: Some(PriceRecommendation {
                        key: MarketVariantKey::new(
                            &reward_slug,
                            Platform::Pc,
                            None,
                            None::<String>,
                        )
                        .unwrap(),
                        provider: platscope_domain::ProviderId::RelicsRun,
                        source_date: Utc::now().date_naive(),
                        fair_price: Some(prices[index]),
                        list_price: None,
                        quick_sell: None,
                        lowest_ask: None,
                        depth_three: None,
                        depth_price: None,
                        closed_volume: Some(100.0),
                        live_sell_order_count: 0,
                        live_buy_order_count: 0,
                        confidence: PriceConfidence::High,
                        freshness: PriceFreshness::Fresh,
                        reasons: vec![],
                    }),
                }
            })
            .collect();
        RelicInsightRow {
            definition: RelicDefinition {
                relic_slug: slug.into(),
                relic_game_ref: "/Lotus/Relic".into(),
                display_name_en: slug.into(),
                refinement,
                vault_status: VaultStatus::Unknown,
                rewards: rewards
                    .iter()
                    .map(|reward| reward.definition.clone())
                    .collect(),
            },
            display_name: slug.into(),
            image_url: None,
            owned_quantity: quantity,
            sellable_quantity: quantity.saturating_sub(1),
            relic_recommendation: None,
            expected_value: calculate_relic_ev(&[]),
            rewards,
        }
    }

    fn metadata() -> GameMetadataSnapshot {
        let mut metadata = crate::tests::empty_game_metadata_fixture(Utc::now());
        metadata.prime_parts = [15, 15, 15, 45, 45, 100]
            .into_iter()
            .enumerate()
            .map(|(index, ducats)| platscope_domain::PrimePartMetadata {
                slug: format!("part_{index}"),
                game_ref: format!("/Lotus/Part{index}"),
                ducats,
                vault_status: VaultStatus::Unknown,
            })
            .collect();
        metadata
    }

    #[test]
    fn relic_selection_consumption_never_crosses_refinement_snapshot_or_account() {
        let relics = vec![
            relic("lith_t14_relic", RelicRefinement::Intact, 2),
            relic("lith_t14_relic", RelicRefinement::Radiant, 1),
            relic("meso_a1_relic", RelicRefinement::Intact, 5),
        ];
        let key =
            MarketVariantKey::new("lith_t14_relic", Platform::Pc, None, Some("radiant")).unwrap();
        let mut request = RelicSelectionRequest {
            era: Some(RelicEra::Lith),
            selected_key: Some(key.clone()),
            consumed: vec![RelicSelectionConsumption { key, quantity: 1 }],
            baseline_inventory_checksum: Some("one".into()),
            baseline_reserve_account_key: Some("account-a".into()),
            ..Default::default()
        };
        let rank = |request: &RelicSelectionRequest| {
            rank_rows(
                relics.clone(),
                &metadata(),
                &inventory(),
                Platform::Pc,
                request,
                &SelectionNeeds::default(),
            )
        };
        let rows = rank(&request);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows.iter()
                .find(|row| row.refinement == RelicRefinement::Radiant)
                .unwrap()
                .remaining_quantity,
            Some(0)
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.refinement == RelicRefinement::Intact)
                .unwrap()
                .remaining_quantity,
            Some(2)
        );
        assert!(rows.iter().all(|row| row.remaining_total_quantity == 2));

        request.base_consumed = vec![RelicSelectionBaseConsumption {
            relic_name: "Lith T14".into(),
            quantity: 1,
        }];
        let rows = rank(&request);
        assert!(
            rows.iter()
                .all(|row| row.remaining_quantity.is_none() && row.remaining_total_quantity == 1)
        );
        let mut ambiguous_request = request.clone();
        ambiguous_request.consumed.clear();
        let mut conservative = rank_rows(
            vec![
                relic("lith_t14_relic", RelicRefinement::Intact, 5),
                relic("lith_t14_relic", RelicRefinement::Radiant, 1),
            ],
            &metadata(),
            &inventory(),
            Platform::Pc,
            &ambiguous_request,
            &SelectionNeeds::default(),
        );
        assert!(retain_available_recommendations(&mut conservative));
        assert_eq!(conservative.len(), 1);
        assert_eq!(conservative[0].refinement, RelicRefinement::Intact);
        assert_eq!(conservative[0].remaining_total_quantity, 5);
        assert_eq!(conservative[0].remaining_quantity_lower_bound, 4);
        request.base_consumed[0].quantity = u32::MAX;
        assert!(
            rank(&request)
                .iter()
                .all(|row| row.remaining_total_quantity == 0)
        );
        request.baseline_reserve_account_key = Some("account-b".into());
        assert!(
            rank(&request)
                .iter()
                .all(|row| row.remaining_quantity == Some(row.owned_quantity)
                    && row.remaining_total_quantity == 3)
        );
        request.baseline_reserve_account_key = Some("account-a".into());
        request.baseline_inventory_checksum = Some("two".into());
        assert!(
            rank(&request)
                .iter()
                .all(|row| row.remaining_quantity == Some(row.owned_quantity))
        );
        request.era = Some(RelicEra::Omnia);
        assert_eq!(rank(&request).len(), 3);
    }

    #[test]
    fn relic_selection_prices_probabilities_and_personal_needs_remain_evidence_bound() {
        let mut relics = vec![
            relic("lith_t14_relic", RelicRefinement::Intact, 2),
            relic("lith_t14_relic", RelicRefinement::Radiant, 1),
        ];
        let rows = rank_rows(
            relics.clone(),
            &metadata(),
            &inventory(),
            Platform::Pc,
            &RelicSelectionRequest::default(),
            &SelectionNeeds::default(),
        );
        assert_eq!(rows[0].refinement, RelicRefinement::Radiant);
        assert!((rows[0].expected_platinum.unwrap() - 38.002).abs() < 0.001);
        assert!((rows[0].expected_ducats.unwrap() - 35.5015).abs() < 0.001);
        assert_eq!(rows[0].pricing_coverage, RelicPricingCoverage::Complete);
        let details = selection_reward_details(&rows, None, Some("Lith T14"));
        assert_eq!(details.len(), 6);
        assert!(details.iter().all(|reward| reward.chance_percent.is_none()));
        assert_eq!(details[5].price, rows[0].rewards[5].price);
        assert_eq!(details[5].ducats, rows[0].rewards[5].ducats);
        assert!(
            selection_reward_details(&rows, Some(&rows[0]), Some("Lith T14"))
                .iter()
                .all(|reward| reward.chance_percent.is_some())
        );
        for reward in &mut relics[1].rewards {
            reward.recommendation.as_mut().unwrap().freshness = PriceFreshness::Stale;
        }
        let mut needs = SelectionNeeds::default();
        add_need(&mut needs.goals, "part_5", 1, "Личная цель");
        add_need(&mut needs.crafting, "/Lotus/Part5", 2, "Изготовление");
        let rows = rank_rows(
            relics,
            &metadata(),
            &inventory(),
            Platform::Pc,
            &RelicSelectionRequest::default(),
            &needs,
        );
        assert_eq!(rows[0].refinement, RelicRefinement::Radiant);
        assert!(rows[0].expected_platinum.is_none());
        assert_eq!(rows[0].goal_chance_percent, 10.0);
        assert_eq!(rows[0].rewards[5].needed_quantity, 2);
        assert!(!rows[0].rewards[5].needed_for_mastery);

        let mut metadata = metadata();
        metadata
            .prime_sets
            .push(platscope_domain::PrimeSetDefinition {
                set_slug: "unmastered_set".into(),
                set_game_ref: "/Lotus/Equipment".into(),
                display_name_en: "Unmastered".into(),
                vault_status: VaultStatus::Unknown,
                components: vec![platscope_domain::PrimeSetComponentDefinition {
                    slug: "part_5".into(),
                    game_ref: "/Lotus/Part5".into(),
                    required_quantity: 1,
                    ducats: Some(100),
                    image_url: None,
                }],
            });
        let mut mastery = crate::MasteryView {
            observed_at: Some(Utc::now()),
            source: None,
            refresh_failed: false,
            catalog_available: true,
            items: vec![crate::mastery::MasteryItemView {
                game_ref: "/Lotus/Equipment".into(),
                display_name: "Unmastered".into(),
                display_name_en: "Unmastered".into(),
                category: "primary".into(),
                image_url: None,
                max_rank: Some(30),
                xp: None,
                mastery_rank: None,
                status: "unknown",
                reason: "no_record",
                set_slugs: vec!["unmastered_set".into()],
            }],
        };
        let mut needs = Needs::new();
        collect_mastery_needs(
            &mut needs,
            &metadata,
            &inventory(),
            &mastery,
            &BTreeMap::new(),
        );
        assert!(needs.is_empty());
        mastery.items[0].status = "progress";
        collect_mastery_needs(
            &mut needs,
            &metadata,
            &inventory(),
            &mastery,
            &BTreeMap::new(),
        );
        assert_eq!(needs["part_5"].quantity, 1);
        needs.clear();
        collect_mastery_needs(
            &mut needs,
            &metadata,
            &inventory(),
            &mastery,
            &BTreeMap::from([("/Lotus/Equipment".into(), 1)]),
        );
        assert!(needs.is_empty());
        metadata
            .prime_parts
            .push(platscope_domain::PrimePartMetadata {
                slug: "part_5".into(),
                game_ref: "/Lotus/Part5".into(),
                ducats: 45,
                vault_status: VaultStatus::Unknown,
            });
        assert!(reward_ducats(&metadata, Some("part_5"), "/Lotus/Part5").is_none());
    }
}
