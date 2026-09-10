use super::{
    AppSettings, CoreError, Database, GameMetadataSnapshot, HashMap, HashSet, InventoryResolution,
    InventoryService, InventoryView, InventoryViewItem, Mutex, PrimeSetDefinition, RelicDefinition,
    Utc, lock_database,
};
use serde::{Deserialize, Serialize};

const GOALS_KEY: &str = "inventory.personal_set_goals.v1";

/// Рецепт сохраняется с целью: временно недоступный каталог не снимает резерв.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedGoal {
    definition: PrimeSetDefinition,
    #[serde(default)]
    completed_at: Option<chrono::DateTime<Utc>>,
    #[serde(default)]
    completion_seen: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use platscope_domain::{MarketVariantKey, Platform, PrimeSetComponentDefinition, VaultStatus};

    fn item() -> InventoryViewItem {
        InventoryViewItem {
            canonical_game_id: "/part".into(),
            item_id: None,
            bulk_tradable: false,
            display_name: "Деталь".into(),
            image_url: None,
            tags: vec!["component".into()],
            key: Some(MarketVariantKey::new("part", Platform::Pc, None, None::<String>).unwrap()),
            rank: None,
            subtype: None,
            owned_quantity: 6,
            tradeable_quantity: 3,
            untradeable_quantity: 2,
            unknown_quantity: 1,
            leveled_quantity: 0,
            equipped_quantity: 0,
            equipped_placements: vec![],
            sellable_quantity: 2,
            personal_reserved_quantity: 0,
            resolution: InventoryResolution::Resolved,
            vault_status: VaultStatus::Unknown,
        }
    }

    fn goal(quantity: u32) -> SavedGoal {
        SavedGoal {
            completed_at: None,
            completion_seen: false,
            definition: PrimeSetDefinition {
                set_slug: "set".into(),
                set_game_ref: "/set".into(),
                display_name_en: "Set".into(),
                vault_status: VaultStatus::Unknown,
                components: vec![PrimeSetComponentDefinition {
                    slug: "part".into(),
                    game_ref: "/part".into(),
                    required_quantity: quantity,
                    ducats: Some(15),
                    image_url: None,
                }],
            },
        }
    }

    #[test]
    fn personal_copies_prefer_untradeable_and_share_stock_in_goal_order() {
        let result = allocate(&[goal(3), goal(3)], &[item()]);
        assert_eq!(result.parts, vec![vec![3], vec![2]]);
        assert_eq!(result.tradeable, vec![3]);
        assert_eq!(allocate(&[goal(2)], &[item()]).tradeable, vec![0]);
    }

    #[test]
    fn every_variant_dimension_and_unresolved_or_equipped_copies_are_excluded() {
        for dimension in 0..9 {
            let mut item = item();
            match dimension {
                0 => item.key.as_mut().unwrap().rank = Some(0),
                1 => item.key.as_mut().unwrap().charges = Some(1),
                2 => item.key.as_mut().unwrap().subtype = Some("variant".into()),
                3 => item.key.as_mut().unwrap().amber_stars = Some(1),
                4 => item.key.as_mut().unwrap().cyan_stars = Some(1),
                5 => item.resolution = InventoryResolution::ExactVariantUnavailable,
                6 => item.equipped_quantity = 1,
                7 => item.leveled_quantity = 1,
                _ => item.key = None,
            }
            assert_eq!(
                allocate(&[goal(1)], &[item]).parts,
                vec![vec![0]],
                "dimension {dimension}"
            );
        }
    }

    #[test]
    fn duplicate_recipe_entries_and_duplicate_inventory_rows_do_not_reuse_copies() {
        let mut goal = goal(4);
        goal.definition
            .components
            .push(goal.definition.components[0].clone());
        let mut first = item();
        first.untradeable_quantity = 0;
        let result = allocate(&[goal], &[first.clone(), first]);
        assert_eq!(result.parts, vec![vec![4, 2]]);
        assert_eq!(result.tradeable, vec![3, 3]);
    }

    #[test]
    fn legacy_goals_complete_once_and_invalid_recipes_never_complete() {
        let original = goal(2);
        let legacy = serde_json::json!({"definition": original.definition});
        let mut goals = vec![serde_json::from_value::<SavedGoal>(legacy).unwrap()];
        assert!(goals[0].completed_at.is_none());
        let allocation = allocate(&goals, &[item()]);
        let now = Utc::now();
        assert!(complete_goals(&mut goals, &allocation, now));
        assert!(!complete_goals(&mut goals, &allocation, now));
        let empty = allocate(&goals, &[]);
        assert!(!complete_goals(&mut goals, &empty, now));
        assert_eq!(goals[0].completed_at, Some(now));
        for invalid in 0..4 {
            let mut invalid_goal = goal(2);
            match invalid {
                0 => invalid_goal.definition.components.clear(),
                1 => invalid_goal.definition.components[0].required_quantity = 0,
                2 => invalid_goal.definition.components[0].slug.clear(),
                _ => invalid_goal.definition.components[0].game_ref.clear(),
            }
            let mut goals = vec![invalid_goal];
            let allocation = allocate(&goals, &[item()]);
            assert!(!complete_goals(&mut goals, &allocation, now));
            assert!(goals[0].completed_at.is_none());
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSetChoice {
    pub set_slug: String,
    pub display_name: String,
    pub display_name_en: String,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalGoalPart {
    pub slug: String,
    pub display_name: String,
    pub display_name_en: String,
    pub image_url: Option<String>,
    pub required_quantity: u32,
    pub allocated_quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSetGoal {
    #[serde(flatten)]
    pub set: PersonalSetChoice,
    pub parts: Vec<PersonalGoalPart>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub completion_pending: bool,
}

/// Подтверждение показанного уведомления относится к конкретному выполнению цели.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalGoalCompletion {
    pub set_slug: String,
    pub completed_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalGoalsView {
    pub inventory_available: bool,
    pub metadata_available: bool,
    pub observed_at: Option<chrono::DateTime<Utc>>,
    pub catalog: Vec<PersonalSetChoice>,
    pub goals: Vec<PersonalSetGoal>,
    pub relics: Vec<PersonalGoalRelic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalGoalRelic {
    pub definition: RelicDefinition,
    pub display_name: String,
    pub owned_quantity: u32,
}

pub struct PersonalGoalsService;

fn saved_goals(database: &Mutex<Database>) -> Result<Vec<SavedGoal>, CoreError> {
    Ok(lock_database(database)?
        .get_setting(GOALS_KEY)?
        .unwrap_or_default())
}

fn exact_part(item: &InventoryViewItem, slug: &str) -> bool {
    item.resolution == InventoryResolution::Resolved
        && item.rank.is_none()
        && item.subtype.is_none()
        && item.leveled_quantity == 0
        && item.equipped_quantity == 0
        && item.key.as_ref().is_some_and(|key| {
            key.slug == slug
                && key.rank.is_none()
                && key.charges.is_none()
                && key.subtype.is_none()
                && key.amber_stars.is_none()
                && key.cyan_stars.is_none()
        })
}

struct Allocation {
    parts: Vec<Vec<u32>>,
    tradeable: Vec<u32>,
}

/// Каждая физическая копия учитывается один раз, сначала для ранее добавленных целей.
/// Непередаваемые известные детали идут первыми; неизвестные варианты не учитываются.
fn allocate(goals: &[SavedGoal], items: &[InventoryViewItem]) -> Allocation {
    let mut tradeable = vec![0_u32; items.len()];
    let mut untradeable = vec![0_u32; items.len()];
    let parts = goals
        .iter()
        .map(|goal| {
            goal.definition
                .components
                .iter()
                .map(|part| {
                    let mut remaining = part.required_quantity;
                    for transferable in [false, true] {
                        for (index, item) in items
                            .iter()
                            .enumerate()
                            .filter(|(_, item)| exact_part(item, &part.slug))
                        {
                            let (stock, used) = if transferable {
                                (item.tradeable_quantity, &mut tradeable[index])
                            } else {
                                (item.untradeable_quantity, &mut untradeable[index])
                            };
                            let take = stock.saturating_sub(*used).min(remaining);
                            *used += take;
                            remaining -= take;
                        }
                    }
                    part.required_quantity - remaining
                })
                .collect()
        })
        .collect();
    Allocation { parts, tradeable }
}

pub(super) fn apply_reservations(
    database: &Mutex<Database>,
    view: &mut InventoryView,
) -> Result<(), CoreError> {
    let database = lock_database(database)?;
    let mut goals: Vec<SavedGoal> = database.get_setting(GOALS_KEY)?.unwrap_or_default();
    let allocation = allocate(&goals, &view.items);
    if complete_goals(&mut goals, &allocation, Utc::now()) {
        database.set_setting(GOALS_KEY, &goals)?;
    }
    for (item, reserved) in view.items.iter_mut().zip(allocation.tradeable) {
        item.personal_reserved_quantity = reserved;
        // Личная цель и общий минимум могут защищать одни и те же копии.
        // Уже действовавшие ограничения никогда не ослабляются.
        item.sellable_quantity = item
            .sellable_quantity
            .min(item.tradeable_quantity.saturating_sub(reserved));
    }
    view.summary.sellable_quantity = view
        .items
        .iter()
        .map(|item| u64::from(item.sellable_quantity))
        .sum();
    Ok(())
}

fn valid_recipe(definition: &PrimeSetDefinition) -> bool {
    !definition.components.is_empty()
        && definition.components.iter().all(|part| {
            part.required_quantity > 0 && !part.slug.is_empty() && !part.game_ref.is_empty()
        })
}

/// Выполнение необратимо до удаления цели. Текущий резерв всегда считается заново.
fn complete_goals(
    goals: &mut [SavedGoal],
    allocation: &Allocation,
    now: chrono::DateTime<Utc>,
) -> bool {
    let mut changed = false;
    for (goal, parts) in goals.iter_mut().zip(&allocation.parts) {
        if goal.completed_at.is_none()
            && valid_recipe(&goal.definition)
            && parts.len() == goal.definition.components.len()
            && goal
                .definition
                .components
                .iter()
                .zip(parts)
                .all(|(part, owned)| *owned == part.required_quantity)
        {
            goal.completed_at = Some(now);
            goal.completion_seen = false;
            changed = true;
        }
    }
    changed
}

fn goal_relics(
    goals: &[SavedGoal],
    metadata: Option<&GameMetadataSnapshot>,
    inventory: Option<&InventoryView>,
) -> Vec<PersonalGoalRelic> {
    let wanted: HashSet<_> = goals
        .iter()
        .flat_map(|goal| &goal.definition.components)
        .map(|part| part.slug.as_str())
        .collect();
    metadata
        .map(|metadata| {
            metadata
                .relics
                .iter()
                .filter(|relic| {
                    relic.rewards.iter().any(|reward| {
                        reward
                            .reward_slug
                            .as_deref()
                            .is_some_and(|slug| wanted.contains(slug))
                    })
                })
                .map(|definition| PersonalGoalRelic {
                    display_name: definition.display_name_en.clone(),
                    definition: definition.clone(),
                    owned_quantity: inventory.map_or(0, |inventory| {
                        inventory
                            .items
                            .iter()
                            .filter(|item| {
                                matches!(
                                    item.resolution,
                                    InventoryResolution::Resolved
                                        | InventoryResolution::ExactVariantUnavailable
                                ) && item.key.as_ref().is_some_and(|key| {
                                    key.slug == definition.relic_slug
                                        && key.subtype.as_deref()
                                            == Some(definition.refinement.market_subtype())
                                })
                            })
                            .fold(0_u32, |sum, item| sum.saturating_add(item.owned_quantity))
                    }),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn set_image(
    definition: &PrimeSetDefinition,
    metadata: Option<&GameMetadataSnapshot>,
) -> Option<String> {
    metadata?
        .mastery_items
        .iter()
        .find(|item| item.game_ref == definition.set_game_ref)
        .and_then(|item| item.image_url.clone())
}

impl PersonalGoalsService {
    /// # Errors
    /// Возвращает ошибку при недоступной БД или повреждённых сохранённых целях.
    pub fn view(
        database: &Mutex<Database>,
        settings: &AppSettings,
    ) -> Result<PersonalGoalsView, CoreError> {
        let inventory = InventoryService::view(database, settings)?;
        let goals = saved_goals(database)?;
        let allocation = allocate(
            &goals,
            inventory.as_ref().map_or(&[], |view| view.items.as_slice()),
        );
        let database = lock_database(database)?;
        let metadata = database.load_current_game_metadata()?;
        let catalog = database.load_current_catalog()?;
        let names: HashMap<_, _> = catalog
            .as_ref()
            .map(|catalog| {
                catalog
                    .items
                    .iter()
                    .map(|item| (item.slug.as_str(), item))
                    .collect()
            })
            .unwrap_or_default();
        let choice = |definition: &PrimeSetDefinition| PersonalSetChoice {
            set_slug: definition.set_slug.clone(),
            display_name: names
                .get(definition.set_slug.as_str())
                .and_then(|item| item.display_name_ru.clone())
                .unwrap_or_else(|| definition.display_name_en.clone()),
            display_name_en: definition.display_name_en.clone(),
            image_url: set_image(definition, metadata.as_ref()).or_else(|| {
                names
                    .get(definition.set_slug.as_str())
                    .and_then(|item| super::catalog_item_image(item, settings.language))
            }),
        };
        let mut choices: Vec<_> = metadata
            .as_ref()
            .map(|metadata| {
                metadata
                    .prime_sets
                    .iter()
                    .filter(|set| valid_recipe(set))
                    .map(choice)
                    .collect()
            })
            .unwrap_or_default();
        choices.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        let mut relics = goal_relics(&goals, metadata.as_ref(), inventory.as_ref());
        for relic in &mut relics {
            if let Some(name) = names
                .get(relic.definition.relic_slug.as_str())
                .and_then(|item| item.display_name_ru.as_ref())
            {
                relic.display_name.clone_from(name);
            }
        }
        let goals = goals
            .iter()
            .zip(allocation.parts)
            .map(|(goal, allocated)| PersonalSetGoal {
                set: choice(&goal.definition),
                completed_at: goal.completed_at,
                completion_pending: goal.completed_at.is_some() && !goal.completion_seen,
                parts: goal
                    .definition
                    .components
                    .iter()
                    .zip(allocated)
                    .map(|(part, allocated_quantity)| {
                        let item = names.get(part.slug.as_str());
                        let display_name_en = item.map_or_else(
                            || part.slug.replace('_', " "),
                            |item| item.display_name_en.clone(),
                        );
                        PersonalGoalPart {
                            slug: part.slug.clone(),
                            display_name: item
                                .and_then(|item| item.display_name_ru.clone())
                                .unwrap_or_else(|| display_name_en.clone()),
                            display_name_en,
                            image_url: part.image_url.clone().or_else(|| {
                                item.and_then(|item| {
                                    super::catalog_item_image(item, settings.language)
                                })
                            }),
                            required_quantity: part.required_quantity,
                            allocated_quantity,
                        }
                    })
                    .collect(),
            })
            .collect();
        Ok(PersonalGoalsView {
            inventory_available: inventory.is_some(),
            metadata_available: metadata.is_some(),
            observed_at: inventory.map(|view| view.metadata.observed_at),
            catalog: choices,
            goals,
            relics,
        })
    }

    /// # Errors
    /// Неизвестный комплект, неверный рецепт или ошибка сохранения оставляют цели прежними.
    pub fn set_goal(
        database: &Mutex<Database>,
        slug: &str,
        enabled: bool,
    ) -> Result<(), CoreError> {
        let database = lock_database(database)?;
        let mut goals: Vec<SavedGoal> = database.get_setting(GOALS_KEY)?.unwrap_or_default();
        if !enabled {
            goals.retain(|goal| goal.definition.set_slug != slug);
        } else if !goals.iter().any(|goal| goal.definition.set_slug == slug) {
            let definition = database
                .load_current_game_metadata()?
                .and_then(|metadata| {
                    metadata
                        .prime_sets
                        .into_iter()
                        .find(|set| set.set_slug == slug)
                })
                .ok_or_else(|| {
                    CoreError::InventoryData(
                        "Комплект не найден. Обновите данные предметов.".into(),
                    )
                })?;
            if goals.len() >= 100 || !valid_recipe(&definition) {
                return Err(CoreError::InventoryData(
                    "Не удалось добавить цель: проверьте состав комплекта и число целей.".into(),
                ));
            }
            goals.push(SavedGoal {
                definition,
                completed_at: None,
                completion_seen: false,
            });
        }
        database.set_setting(GOALS_KEY, &goals)?;
        Ok(())
    }

    /// # Errors
    /// Возвращает ошибку при недоступной БД или ошибке сохранения подтверждения.
    pub fn acknowledge_completions(
        database: &Mutex<Database>,
        completions: &[PersonalGoalCompletion],
    ) -> Result<(), CoreError> {
        let database = lock_database(database)?;
        let mut goals: Vec<SavedGoal> = database.get_setting(GOALS_KEY)?.unwrap_or_default();
        let mut changed = false;
        for goal in &mut goals {
            if !goal.completion_seen
                && completions.iter().any(|completion| {
                    completion.set_slug == goal.definition.set_slug
                        && Some(completion.completed_at) == goal.completed_at
                })
            {
                goal.completion_seen = true;
                changed = true;
            }
        }
        if changed {
            database.set_setting(GOALS_KEY, &goals)?;
        }
        Ok(())
    }
}
