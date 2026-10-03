//! Рецепты берутся из экспорта игры. Магазин и додзё определяются независимо от WFCD bpCost.
use std::collections::{BTreeMap, BTreeSet};

use platscope_domain::{
    BlueprintSource, CraftingDropSource, CraftingIngredientDefinition, CraftingRecipeDefinition,
    GameItemLocalization, MasteryItemDefinition,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{ProviderError, RawGameMetadataDump};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Recipe {
    result_type: String,
    build_price: u64,
    build_time: u64,
    consume_on_use: bool,
    num: u32,
    #[serde(default)]
    exclude_from_market: bool,
    credits_cost: Option<u64>,
    ingredients: Vec<Ingredient>,
}

#[derive(Deserialize)]
struct Ingredient {
    #[serde(rename = "ItemType")]
    game_ref: String,
    #[serde(rename = "ItemCount")]
    quantity: u32,
}

fn drops(item: Option<&Value>) -> Vec<CraftingDropSource> {
    let mut result: Vec<_> = item
        .and_then(|item| item.get("drops"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|drop| {
            let location = drop.get("location")?.as_str()?;
            let chance = drop.get("chance")?.as_f64()?;
            (location.len() <= 256
                && !location.is_empty()
                && chance.is_finite()
                && (0.0..=100.0).contains(&chance))
            .then(|| CraftingDropSource {
                location: location.to_owned(),
                chance_percent: chance,
            })
        })
        .collect();
    result.sort_by(|a, b| {
        b.chance_percent
            .total_cmp(&a.chance_percent)
            .then(a.location.cmp(&b.location))
    });
    result.dedup_by(|a, b| {
        a.location == b.location && a.chance_percent.total_cmp(&b.chance_percent).is_eq()
    });
    result.truncate(8);
    result
}

pub(crate) fn normalize_recipes(
    dump: &RawGameMetadataDump,
    mastery: &[MasteryItemDefinition],
    localizations: &BTreeMap<String, GameItemLocalization>,
) -> Result<Vec<CraftingRecipeDefinition>, ProviderError> {
    let items = collect_items(dump);
    let Some(recipes_document) = dump
        .documents
        .iter()
        .find(|doc| doc.name == "CraftingRecipes.json")
    else {
        return Ok(Vec::new());
    };
    let recipes: BTreeMap<String, Value> = serde_json::from_slice(&recipes_document.body)
        .map_err(|_| ProviderError::schema_changed("invalid crafting recipes document"))?;
    let dojo: Value = dump
        .documents
        .iter()
        .find(|doc| doc.name == "DojoRecipes.json")
        .map(|doc| serde_json::from_slice(&doc.body))
        .transpose()
        .map_err(|_| ProviderError::schema_changed("invalid dojo recipes document"))?
        .unwrap_or(Value::Null);
    let mastery_refs: BTreeSet<_> = mastery.iter().map(|item| item.game_ref.as_str()).collect();
    let mut result = Vec::new();
    for (blueprint, value) in &recipes {
        if !value
            .get("resultType")
            .and_then(Value::as_str)
            .is_some_and(|game_ref| mastery_refs.contains(game_ref))
        {
            continue;
        }
        if let Some(recipe) = normalize_recipe(
            blueprint,
            value,
            &dojo,
            &items,
            localizations,
            &mastery_refs,
        )? {
            result.push(recipe);
        }
    }

    // Только непосредственные ингредиенты рецептов освоения. Следующий уровень не расширяем,
    // поэтому циклы в экспорте не запускают рекурсию, а альтернативные чертежи сохраняются отдельно.
    let component_refs: BTreeSet<_> = result
        .iter()
        .flat_map(|recipe| {
            recipe
                .ingredients
                .iter()
                .map(|ingredient| ingredient.game_ref.clone())
        })
        .collect();
    for (blueprint, value) in &recipes {
        if !value
            .get("resultType")
            .and_then(Value::as_str)
            .is_some_and(|game_ref| {
                component_refs.contains(game_ref) && !mastery_refs.contains(game_ref)
            })
        {
            continue;
        }
        if let Some(recipe) = normalize_recipe(
            blueprint,
            value,
            &dojo,
            &items,
            localizations,
            &mastery_refs,
        )? {
            result.push(recipe);
        }
    }
    result.sort_by(|left, right| left.blueprint_game_ref.cmp(&right.blueprint_game_ref));
    Ok(result)
}

fn normalize_recipe(
    blueprint: &str,
    value: &Value,
    dojo: &Value,
    items: &BTreeMap<String, Value>,
    localizations: &BTreeMap<String, GameItemLocalization>,
    mastery_refs: &BTreeSet<&str>,
) -> Result<Option<CraftingRecipeDefinition>, ProviderError> {
    let recipe: Recipe = serde_json::from_value(value.clone())
        .map_err(|_| ProviderError::schema_changed("invalid crafting recipe fields"))?;
    // Пустой скрытый рецепт, например старый Braton, не означает бесплатное изготовление.
    if !valid_ref(blueprint)
        || !valid_ref(&recipe.result_type)
        || recipe.ingredients.is_empty()
        || recipe.ingredients.len() > 32
        || recipe.build_price > 1_000_000_000
        || recipe.build_time > 31_536_000
    {
        return Ok(None);
    }
    if recipe.num == 0 || recipe.num > 100_000_000 {
        return Err(ProviderError::validation(
            "invalid crafting result quantity",
        ));
    }
    let mut quantities = BTreeMap::<String, u32>::new();
    for ingredient in recipe.ingredients {
        if !valid_ref(&ingredient.game_ref)
            || ingredient.quantity == 0
            || ingredient.quantity > 100_000_000
        {
            return Err(ProviderError::validation("invalid crafting ingredient"));
        }
        let count = quantities.entry(ingredient.game_ref).or_default();
        *count = count
            .checked_add(ingredient.quantity)
            .filter(|count| *count <= 100_000_000)
            .ok_or_else(|| ProviderError::validation("crafting ingredient quantity overflow"))?;
    }
    let dojo_price = dojo
        .get("research")
        .and_then(|research| research.get(blueprint))
        .and_then(|research| research.get("replicatePrice"))
        .and_then(Value::as_u64);
    let (source, price) = if let Some(price) = dojo_price {
        (BlueprintSource::Dojo, Some(price))
    } else if !recipe.exclude_from_market && recipe.credits_cost.is_some_and(|price| price > 0) {
        (BlueprintSource::Market, recipe.credits_cost)
    } else {
        (BlueprintSource::Unknown, None)
    };
    let requirements = items
        .get(&recipe.result_type)
        .and_then(|item| item.get("masteryReq"))
        .and_then(Value::as_u64)
        .and_then(|rank| u8::try_from(rank).ok())
        .filter(|rank| *rank <= 50)
        .or_else(|| {
            // В WFCD у неосваиваемых деталей и ресурсов MR отсутствует. Не подменяем
            // нулём неизвестный результат, осваиваемое оборудование или повреждённый явный MR.
            let item = items.get(&recipe.result_type)?;
            (!mastery_refs.contains(recipe.result_type.as_str())
                && item.get("masterable").and_then(Value::as_bool) == Some(false)
                && matches!(
                    item.get("category").and_then(Value::as_str),
                    Some("Components" | "Resources" | "Misc")
                )
                && item.get("masteryReq").is_none())
            .then_some(0)
        });
    let ingredients = quantities
        .into_iter()
        .map(|(game_ref, quantity)| {
            make_ingredient(game_ref, quantity, items, localizations, mastery_refs)
        })
        .collect();
    Ok(Some(CraftingRecipeDefinition {
        blueprint_drops: drops(items.get(blueprint)),
        result_game_ref: recipe.result_type,
        result_quantity: recipe.num,
        blueprint_game_ref: blueprint.to_owned(),
        blueprint_consumed: recipe.consume_on_use,
        blueprint_source: source,
        blueprint_price: price,
        mastery_requirement: requirements,
        build_price: recipe.build_price,
        build_time_seconds: recipe.build_time,
        ingredients,
    }))
}

fn valid_ref(game_ref: &str) -> bool {
    game_ref.starts_with("/Lotus/") && game_ref.len() <= 256
}

fn collect_items(dump: &RawGameMetadataDump) -> BTreeMap<String, Value> {
    let mut items = BTreeMap::new();
    for document in &dump.documents {
        if let Ok(Value::Array(entries)) = serde_json::from_slice::<Value>(&document.body) {
            for item in entries {
                if let Some(game_ref) = item
                    .get("uniqueName")
                    .and_then(Value::as_str)
                    .filter(|name| valid_ref(name))
                {
                    items.insert(game_ref.to_owned(), item);
                }
            }
        }
    }
    items
}

fn make_ingredient(
    game_ref: String,
    quantity: u32,
    items: &BTreeMap<String, Value>,
    localizations: &BTreeMap<String, GameItemLocalization>,
    mastery_refs: &BTreeSet<&str>,
) -> CraftingIngredientDefinition {
    let item = items.get(&game_ref);
    let parent_ref = item
        .filter(|item| item.get("category").and_then(Value::as_str) == Some("Components"))
        .and_then(|item| item.get("parentUniqueNames"))
        .and_then(Value::as_array)
        .filter(|parents| parents.len() == 1)
        .and_then(|parents| parents[0].as_str());
    let name_en = item
        .and_then(|item| item.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty() && name.len() <= 256);
    let parent_en = parent_ref
        .and_then(|game_ref| items.get(game_ref))
        .and_then(|item| item.get("name"))
        .and_then(Value::as_str);
    let display_name_en = match (name_en, parent_en) {
        (Some(name), Some(parent)) if !name.contains(parent) => format!("{parent} {name}"),
        (Some(name), _) if name != "Blueprint" => name.to_owned(),
        _ => readable_ref_name(&game_ref),
    };
    let display_name_ru = localizations.get(&game_ref).map(|entry| {
        let name = &entry.display_name_ru;
        match parent_ref.and_then(|parent| localizations.get(parent)) {
            Some(parent) if !name.contains(&parent.display_name_ru) => {
                format!("{}: {name}", parent.display_name_ru)
            }
            _ => name.clone(),
        }
    });
    CraftingIngredientDefinition {
        display_name_en,
        display_name_ru,
        image_url: item
            .and_then(|item| item.get("imageName"))
            .and_then(Value::as_str)
            .filter(|name| {
                name.bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
            })
            .map(|name| format!("https://cdn.warframestat.us/img/{name}")),
        equipment: mastery_refs.contains(game_ref.as_str()),
        drops: drops(item),
        game_ref,
        quantity,
    }
}

fn readable_ref_name(game_ref: &str) -> String {
    let leaf = game_ref.rsplit('/').next().unwrap_or(game_ref);
    let leaf = leaf
        .strip_suffix("Component")
        .or_else(|| leaf.strip_suffix("Item"))
        .unwrap_or(leaf);
    let characters: Vec<_> = leaf.chars().collect();
    let mut name = String::new();
    for (index, &character) in characters.iter().enumerate() {
        if index > 0
            && character.is_uppercase()
            && (characters[index - 1].is_lowercase()
                || characters[index - 1].is_ascii_digit()
                || characters
                    .get(index + 1)
                    .is_some_and(|next| next.is_lowercase()))
        {
            name.push(' ');
        }
        name.push(character);
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RawGameMetadataDocument;

    #[test]
    fn shop_price_does_not_turn_excluded_or_dojo_recipes_into_market_purchases() {
        let recipe = |excluded: bool| {
            serde_json::json!({"resultType":"/Lotus/Weapon/A","buildPrice":25000,
            "buildTime":86400,"consumeOnUse":true,"num":1,"creditsCost":15000,"excludeFromMarket":excluded,
            "ingredients":[{"ItemType":"/Lotus/Resource/A","ItemCount":2},{"ItemType":"/Lotus/Resource/A","ItemCount":3}]})
        };
        let component_recipe = |result: &str, quantity: u32, ingredient: &str| {
            serde_json::json!({"resultType":result,"buildPrice":5000,
            "buildTime":60,"consumeOnUse":false,"num":quantity,
            "ingredients":[{"ItemType":ingredient,"ItemCount":7}]})
        };
        let mut dump = RawGameMetadataDump { fetched_at: chrono::Utc::now(), documents: vec![
            RawGameMetadataDocument { name:"CraftingRecipes.json".into(), body: serde_json::to_vec(&serde_json::json!({
                "/Lotus/Recipe/Shop":recipe(false),"/Lotus/Recipe/Hidden":recipe(true),"/Lotus/Recipe/Dojo":recipe(false),
                "/Lotus/Recipe/Component":component_recipe("/Lotus/Resource/A",20,"/Lotus/Resource/B"),
                "/Lotus/Recipe/ComponentAlternative":component_recipe("/Lotus/Resource/A",10,"/Lotus/Resource/C"),
                "/Lotus/Recipe/Deeper":component_recipe("/Lotus/Resource/B",1,"/Lotus/Resource/A"),
                "/Lotus/Recipe/Unrelated":component_recipe("/Lotus/Resource/D",0,"/Lotus/Resource/D")
            })).unwrap() },
            RawGameMetadataDocument { name:"DojoRecipes.json".into(), body: br#"{"research":{"/Lotus/Recipe/Dojo":{"replicatePrice":17000}}}"#.to_vec() },
            RawGameMetadataDocument { name:"Resources.json".into(), body: br#"[{"uniqueName":"/Lotus/Resource/A","name":"A","category":"Resources","masterable":false}]"#.to_vec() },
        ] };
        let mastery = vec![MasteryItemDefinition {
            game_ref: "/Lotus/Weapon/A".into(),
            display_name_en: "A".into(),
            display_name_ru: None,
            category: "primary".into(),
            image_url: None,
            max_rank: Some(30),
        }];
        let parsed = normalize_recipes(&dump, &mastery, &BTreeMap::new()).unwrap();
        let source = |suffix| {
            parsed
                .iter()
                .find(|recipe| recipe.blueprint_game_ref.ends_with(suffix))
                .unwrap()
                .blueprint_source
        };
        assert_eq!(source("Shop"), BlueprintSource::Market);
        assert_eq!(source("Hidden"), BlueprintSource::Unknown);
        let dojo = parsed
            .iter()
            .find(|recipe| recipe.blueprint_game_ref.ends_with("Dojo"))
            .unwrap();
        assert_eq!(
            (dojo.blueprint_source, dojo.blueprint_price),
            (BlueprintSource::Dojo, Some(17000))
        );
        assert_eq!(dojo.ingredients[0].quantity, 5);
        assert_eq!(parsed.len(), 5, "only one component level is included");
        let component = parsed
            .iter()
            .find(|recipe| recipe.blueprint_game_ref == "/Lotus/Recipe/Component")
            .unwrap();
        assert_eq!(component.result_quantity, 20);
        assert_eq!(component.ingredients[0].quantity, 7);
        assert_eq!(component.build_price, 5000);
        assert_eq!(component.mastery_requirement, Some(0));
        assert_eq!(
            parsed
                .iter()
                .filter(|recipe| recipe.result_game_ref == "/Lotus/Resource/A")
                .count(),
            2,
            "alternative blueprints must not be discarded"
        );
        let mut legacy = serde_json::to_value(component).unwrap();
        legacy.as_object_mut().unwrap().remove("resultQuantity");
        assert_eq!(
            serde_json::from_value::<CraftingRecipeDefinition>(legacy)
                .unwrap()
                .result_quantity,
            1,
            "existing recipe snapshots remain readable"
        );
        dump.documents[2].body = br#"[{"uniqueName":"/Lotus/Resource/A","category":"Resources","masterable":false,"masteryReq":60}]"#.to_vec();
        let invalid_requirement = normalize_recipes(&dump, &mastery, &BTreeMap::new()).unwrap();
        assert!(
            invalid_requirement
                .iter()
                .filter(|recipe| recipe.result_game_ref == "/Lotus/Resource/A")
                .all(|recipe| recipe.mastery_requirement.is_none())
        );
        dump.documents[0].body = br#"{"/Lotus/Recipe/Bad":{"resultType":"/Lotus/Weapon/A","buildPrice":0,"buildTime":0,"consumeOnUse":true,"num":1,"ingredients":[{"ItemType":"/Lotus/Resource/A","ItemCount":0}]}}"#.to_vec();
        assert!(normalize_recipes(&dump, &mastery, &BTreeMap::new()).is_err());
        dump.documents[0].body = serde_json::to_vec(&serde_json::json!({
            "/Lotus/Recipe/Bad":component_recipe("/Lotus/Weapon/A",0,"/Lotus/Resource/A")
        }))
        .unwrap();
        assert!(normalize_recipes(&dump, &mastery, &BTreeMap::new()).is_err());
        dump.documents[0].body = br#"{"/Lotus/Recipe/Bad":{"resultType":"/Lotus/Weapon/A","buildPrice":0,"buildTime":0,"consumeOnUse":true,"num":1,"ingredients":[{"ItemType":"/Lotus/Resource/A","ItemCount":100000000},{"ItemType":"/Lotus/Resource/A","ItemCount":1}]}}"#.to_vec();
        assert!(normalize_recipes(&dump, &mastery, &BTreeMap::new()).is_err());
    }
}
