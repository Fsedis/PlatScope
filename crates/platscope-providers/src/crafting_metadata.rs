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
    for (blueprint, value) in recipes {
        if !value
            .get("resultType")
            .and_then(Value::as_str)
            .is_some_and(|game_ref| mastery_refs.contains(game_ref))
        {
            continue;
        }
        let recipe: Recipe = serde_json::from_value(value)
            .map_err(|_| ProviderError::schema_changed("invalid crafting recipe fields"))?;
        // Пустой скрытый рецепт, например старый Braton, не означает бесплатное изготовление.
        if !valid_ref(&blueprint)
            || recipe.ingredients.is_empty()
            || recipe.ingredients.len() > 32
            || recipe.num != 1
            || recipe.build_price > 1_000_000_000
            || recipe.build_time > 31_536_000
        {
            continue;
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
                .ok_or_else(|| {
                    ProviderError::validation("crafting ingredient quantity overflow")
                })?;
        }
        let dojo_price = dojo
            .get("research")
            .and_then(|research| research.get(&blueprint))
            .and_then(|research| research.get("replicatePrice"))
            .and_then(Value::as_u64);
        let (source, price) = if let Some(price) = dojo_price {
            (BlueprintSource::Dojo, Some(price))
        } else if !recipe.exclude_from_market && recipe.credits_cost.is_some_and(|price| price > 0)
        {
            (BlueprintSource::Market, recipe.credits_cost)
        } else {
            (BlueprintSource::Unknown, None)
        };
        let requirements = items
            .get(&recipe.result_type)
            .and_then(|item| item.get("masteryReq"))
            .and_then(Value::as_u64)
            .and_then(|rank| u8::try_from(rank).ok())
            .filter(|rank| *rank <= 50);
        let ingredients = quantities
            .into_iter()
            .map(|(game_ref, quantity)| {
                make_ingredient(game_ref, quantity, &items, localizations, &mastery_refs)
            })
            .collect();
        result.push(CraftingRecipeDefinition {
            blueprint_drops: drops(items.get(&blueprint)),
            result_game_ref: recipe.result_type,
            blueprint_game_ref: blueprint,
            blueprint_consumed: recipe.consume_on_use,
            blueprint_source: source,
            blueprint_price: price,
            mastery_requirement: requirements,
            build_price: recipe.build_price,
            build_time_seconds: recipe.build_time,
            ingredients,
        });
    }
    Ok(result)
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
    CraftingIngredientDefinition {
        display_name_en: item
            .and_then(|item| item.get("name"))
            .and_then(Value::as_str)
            .filter(|name| *name != "Blueprint")
            .unwrap_or("Unknown material")
            .to_owned(),
        display_name_ru: localizations
            .get(&game_ref)
            .map(|entry| entry.display_name_ru.clone()),
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
        let mut dump = RawGameMetadataDump { fetched_at: chrono::Utc::now(), documents: vec![
            RawGameMetadataDocument { name:"CraftingRecipes.json".into(), body: serde_json::to_vec(&serde_json::json!({
                "/Lotus/Recipe/Shop":recipe(false),"/Lotus/Recipe/Hidden":recipe(true),"/Lotus/Recipe/Dojo":recipe(false)})).unwrap() },
            RawGameMetadataDocument { name:"DojoRecipes.json".into(), body: br#"{"research":{"/Lotus/Recipe/Dojo":{"replicatePrice":17000}}}"#.to_vec() },
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
        assert_eq!(
            parsed
                .iter()
                .find(|recipe| recipe.blueprint_game_ref.ends_with("Shop"))
                .unwrap()
                .blueprint_source,
            BlueprintSource::Market
        );
        assert_eq!(
            parsed
                .iter()
                .find(|recipe| recipe.blueprint_game_ref.ends_with("Hidden"))
                .unwrap()
                .blueprint_source,
            BlueprintSource::Unknown
        );
        let dojo = parsed
            .iter()
            .find(|recipe| recipe.blueprint_game_ref.ends_with("Dojo"))
            .unwrap();
        assert_eq!(
            (dojo.blueprint_source, dojo.blueprint_price),
            (BlueprintSource::Dojo, Some(17000))
        );
        assert_eq!(dojo.ingredients[0].quantity, 5);
        dump.documents[0].body = br#"{"/Lotus/Recipe/Bad":{"resultType":"/Lotus/Weapon/A","buildPrice":0,"buildTime":0,"consumeOnUse":true,"num":1,"ingredients":[{"ItemType":"/Lotus/Resource/A","ItemCount":0}]}}"#.to_vec();
        assert!(normalize_recipes(&dump, &mastery, &BTreeMap::new()).is_err());
    }
}
