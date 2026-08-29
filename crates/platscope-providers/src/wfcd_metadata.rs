use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::Utc;
use platscope_domain::{
    GameItemDefinition, GameMetadataSnapshot, GameMetadataSnapshotMetadata, GameMetadataSource,
    ItemCatalog, PrimePartMetadata, PrimeSetComponentDefinition, PrimeSetDefinition,
    RelicDefinition, RelicRefinement, RelicRewardDefinition, RivenDispositionDefinition,
    RivenWeaponCategory, VaultStatus,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    BoundedHttpClient, GameMetadataProvider, ProviderError, RawGameMetadataDocument,
    RawGameMetadataDump,
};

const DEFAULT_BASE_URL: &str =
    "https://raw.githubusercontent.com/WFCD/warframe-items/master/data/json";
const DOCUMENT_NAMES: [&str; 10] = [
    "Relics.json",
    "Warframes.json",
    "Primary.json",
    "Secondary.json",
    "Melee.json",
    "Sentinels.json",
    "SentinelWeapons.json",
    "Archwing.json",
    "Arch-Gun.json",
    "Arch-Melee.json",
];
const MAX_DOCUMENT_COUNT: usize = 12;
const MAX_TOTAL_BYTES: usize = 96 * 1024 * 1024;
const WFCD_IMAGE_BASE_URL: &str = "https://cdn.warframestat.us/img";

pub struct WfcdMetadataProvider {
    client: BoundedHttpClient,
    base_url: String,
}

impl WfcdMetadataProvider {
    /// Создаёт production metadata provider с bounded HTTP client.
    ///
    /// # Errors
    ///
    /// Возвращает transport error, если HTTP client нельзя создать.
    pub fn production() -> Result<Self, ProviderError> {
        Ok(Self {
            client: BoundedHttpClient::new()?,
            base_url: DEFAULT_BASE_URL.into(),
        })
    }

    #[cfg(test)]
    fn for_tests() -> Result<Self, ProviderError> {
        Self::production()
    }
}

#[async_trait]
impl GameMetadataProvider for WfcdMetadataProvider {
    async fn fetch_latest(&self) -> Result<RawGameMetadataDump, ProviderError> {
        let mut documents = Vec::with_capacity(DOCUMENT_NAMES.len());
        let mut total_bytes = 0usize;
        for name in DOCUMENT_NAMES {
            let url = format!("{}/{name}", self.base_url.trim_end_matches('/'));
            let body = self.client.get_json(&url, true).await?;
            total_bytes = total_bytes.saturating_add(body.len());
            if total_bytes > MAX_TOTAL_BYTES {
                return Err(ProviderError::validation(
                    "WFCD metadata documents exceed the 96 MiB aggregate limit",
                ));
            }
            documents.push(RawGameMetadataDocument {
                name: name.into(),
                body,
            });
        }
        Ok(RawGameMetadataDump {
            fetched_at: Utc::now(),
            documents,
        })
    }

    fn normalize(
        &self,
        dump: &RawGameMetadataDump,
        catalog: &ItemCatalog,
    ) -> Result<GameMetadataSnapshot, ProviderError> {
        normalize_wfcd_metadata(dump, catalog)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WfcdSetItem {
    unique_name: String,
    name: String,
    #[serde(default)]
    is_prime: bool,
    vaulted: Option<bool>,
    #[serde(default)]
    components: Vec<WfcdComponent>,
    disposition: Option<u8>,
    omega_attenuation: Option<f64>,
    mastery_req: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WfcdComponent {
    unique_name: String,
    #[serde(default = "one")]
    item_count: u32,
    #[serde(default)]
    tradable: bool,
    ducats: Option<u32>,
    prime_selling_price: Option<u32>,
    image_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WfcdRelic {
    unique_name: String,
    name: String,
    vaulted: Option<bool>,
    market_info: Option<WfcdMarketInfo>,
    #[serde(default)]
    rewards: Vec<WfcdReward>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WfcdReward {
    chance: f64,
    item: WfcdRewardItem,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WfcdRewardItem {
    unique_name: String,
    name: String,
    warframe_market: Option<WfcdMarketInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WfcdMarketInfo {
    url_name: String,
}

const fn one() -> u32 {
    1
}

fn wfcd_component_image_url(image_name: Option<&str>) -> Option<String> {
    let image_name = image_name?.trim();
    if image_name.is_empty()
        || image_name.len() > 128
        || !image_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return None;
    }
    Some(format!("{WFCD_IMAGE_BASE_URL}/{image_name}"))
}

fn normalize_wfcd_metadata(
    dump: &RawGameMetadataDump,
    catalog: &ItemCatalog,
) -> Result<GameMetadataSnapshot, ProviderError> {
    if dump.documents.is_empty() || dump.documents.len() > MAX_DOCUMENT_COUNT {
        return Err(ProviderError::validation(
            "WFCD metadata document count is outside the bounded range",
        ));
    }
    let total_bytes = dump
        .documents
        .iter()
        .try_fold(0usize, |total, document| {
            total.checked_add(document.body.len())
        })
        .ok_or_else(|| ProviderError::validation("WFCD metadata size overflow"))?;
    if total_bytes == 0 || total_bytes > MAX_TOTAL_BYTES {
        return Err(ProviderError::validation(
            "WFCD metadata aggregate body is empty or too large",
        ));
    }

    let by_game_ref: HashMap<&str, (&str, &[String])> = catalog
        .items
        .iter()
        .filter_map(|item| {
            item.game_ref
                .as_deref()
                .map(|game_ref| (game_ref, (item.slug.as_str(), item.tags.as_slice())))
        })
        .collect();
    let mut sets = BTreeMap::new();
    let mut relics = BTreeMap::new();
    let mut parts = BTreeMap::new();
    let mut riven_dispositions = BTreeMap::new();
    let mut item_definitions = BTreeMap::new();
    let mut has_riven_document = false;
    for document in &dump.documents {
        if document.name.eq_ignore_ascii_case("Relics.json") {
            parse_relics(&document.body, &by_game_ref, &mut relics)?;
        } else {
            parse_item_definitions(&document.body, &by_game_ref, &mut item_definitions)?;
            parse_sets(&document.body, &by_game_ref, &mut sets, &mut parts)?;
            if let Some(category) = riven_category(&document.name) {
                has_riven_document = true;
                parse_riven_dispositions(&document.body, category, &mut riven_dispositions)?;
            }
        }
    }
    if sets.is_empty() || relics.is_empty() || parts.is_empty() || item_definitions.is_empty() {
        return Err(ProviderError::validation(
            "WFCD metadata produced no sets, relics, prime parts, or item definitions",
        ));
    }
    if has_riven_document && riven_dispositions.is_empty() {
        return Err(ProviderError::validation(
            "WFCD weapon metadata produced no Riven dispositions",
        ));
    }

    let checksum_sha256 = {
        let mut hasher = Sha256::new();
        for document in &dump.documents {
            hasher.update(document.name.as_bytes());
            hasher.update(&document.body);
        }
        hex::encode(hasher.finalize())
    };
    let prime_sets: Vec<_> = sets.into_values().collect();
    let relics: Vec<_> = relics.into_values().collect();
    let prime_parts: Vec<_> = parts.into_values().collect();
    let mut riven_dispositions: Vec<_> = riven_dispositions.into_values().collect();
    riven_dispositions.sort_by(|left, right| left.weapon_name_en.cmp(&right.weapon_name_en));
    let item_definitions: Vec<_> = item_definitions.into_values().collect();
    Ok(GameMetadataSnapshot {
        metadata: GameMetadataSnapshotMetadata {
            source: GameMetadataSource::WfcdWarframeItems,
            fetched_at: dump.fetched_at,
            schema_version: 4,
            set_count: u64::try_from(prime_sets.len()).unwrap_or(u64::MAX),
            relic_count: u64::try_from(relics.len()).unwrap_or(u64::MAX),
            prime_part_count: u64::try_from(prime_parts.len()).unwrap_or(u64::MAX),
            riven_disposition_count: u64::try_from(riven_dispositions.len()).unwrap_or(u64::MAX),
            item_definition_count: u64::try_from(item_definitions.len()).unwrap_or(u64::MAX),
            checksum_sha256,
        },
        prime_sets,
        relics,
        prime_parts,
        riven_dispositions,
        item_definitions,
    })
}

fn parse_item_definitions(
    body: &[u8],
    by_game_ref: &HashMap<&str, (&str, &[String])>,
    definitions: &mut BTreeMap<String, GameItemDefinition>,
) -> Result<(), ProviderError> {
    let items: Vec<WfcdSetItem> = serde_json::from_slice(body).map_err(|error| {
        ProviderError::schema_changed(format!("invalid WFCD item JSON: {error}"))
    })?;
    for item in items {
        let Some(mastery_requirement) = item.mastery_req else {
            continue;
        };
        if mastery_requirement > 50 {
            return Err(ProviderError::validation(format!(
                "invalid mastery requirement for {}",
                item.name
            )));
        }
        let Some((slug, _)) = by_game_ref.get(item.unique_name.as_str()).copied() else {
            continue;
        };
        definitions.insert(
            slug.to_owned(),
            GameItemDefinition {
                slug: slug.to_owned(),
                game_ref: item.unique_name,
                mastery_requirement,
            },
        );
    }
    Ok(())
}

fn parse_riven_dispositions(
    body: &[u8],
    category: RivenWeaponCategory,
    definitions: &mut BTreeMap<String, RivenDispositionDefinition>,
) -> Result<(), ProviderError> {
    let items: Vec<WfcdSetItem> = serde_json::from_slice(body).map_err(|error| {
        ProviderError::schema_changed(format!("invalid WFCD weapon JSON: {error}"))
    })?;
    for item in items {
        let (Some(disposition), Some(multiplier)) = (item.disposition, item.omega_attenuation)
        else {
            continue;
        };
        if !(1..=5).contains(&disposition)
            || !multiplier.is_finite()
            || !(0.1..=2.0).contains(&multiplier)
        {
            return Err(ProviderError::validation(format!(
                "invalid Riven disposition for {}",
                item.name
            )));
        }
        definitions.insert(
            item.unique_name.clone(),
            RivenDispositionDefinition {
                weapon_name_en: item.name,
                weapon_game_ref: item.unique_name,
                category,
                disposition,
                multiplier,
            },
        );
    }
    Ok(())
}

fn riven_category(document_name: &str) -> Option<RivenWeaponCategory> {
    match document_name.to_ascii_lowercase().as_str() {
        "primary.json" => Some(RivenWeaponCategory::Primary),
        "secondary.json" => Some(RivenWeaponCategory::Secondary),
        "melee.json" => Some(RivenWeaponCategory::Melee),
        "sentinelweapons.json" => Some(RivenWeaponCategory::SentinelWeapon),
        "arch-gun.json" => Some(RivenWeaponCategory::ArchGun),
        "arch-melee.json" => Some(RivenWeaponCategory::ArchMelee),
        _ => None,
    }
}

fn parse_sets(
    body: &[u8],
    by_game_ref: &HashMap<&str, (&str, &[String])>,
    sets: &mut BTreeMap<String, PrimeSetDefinition>,
    parts: &mut BTreeMap<String, PrimePartMetadata>,
) -> Result<(), ProviderError> {
    let items: Vec<WfcdSetItem> = serde_json::from_slice(body).map_err(|error| {
        ProviderError::schema_changed(format!("invalid WFCD set JSON: {error}"))
    })?;
    for item in items.into_iter().filter(|item| {
        (item.is_prime || item.name.ends_with(" Prime")) && !item.components.is_empty()
    }) {
        let Some((set_slug, tags)) = by_game_ref.get(item.unique_name.as_str()).copied() else {
            continue;
        };
        if !tags.iter().any(|tag| tag == "set") {
            continue;
        }
        let vault_status = vault_status(item.vaulted);
        let mut components = Vec::new();
        for component in item
            .components
            .into_iter()
            .filter(|component| component.tradable && component.item_count > 0)
        {
            let Some((slug, _)) = resolve_component(&component.unique_name, by_game_ref) else {
                continue;
            };
            let ducats = component.ducats.or(component.prime_selling_price);
            let image_url = wfcd_component_image_url(component.image_name.as_deref());
            components.push(PrimeSetComponentDefinition {
                slug: slug.into(),
                game_ref: component.unique_name.clone(),
                required_quantity: component.item_count,
                ducats,
                image_url,
            });
            if let Some(ducats) = ducats.filter(|ducats| *ducats > 0) {
                parts.insert(
                    slug.into(),
                    PrimePartMetadata {
                        slug: slug.into(),
                        game_ref: component.unique_name,
                        ducats,
                        vault_status,
                    },
                );
            }
        }
        components.sort_by(|left, right| left.slug.cmp(&right.slug));
        components.dedup_by(|left, right| left.slug == right.slug);
        if components.len() >= 2 {
            sets.insert(
                set_slug.into(),
                PrimeSetDefinition {
                    set_slug: set_slug.into(),
                    set_game_ref: item.unique_name,
                    display_name_en: format!("{} Set", item.name),
                    vault_status,
                    components,
                },
            );
        }
    }
    Ok(())
}

fn parse_relics(
    body: &[u8],
    by_game_ref: &HashMap<&str, (&str, &[String])>,
    relics: &mut BTreeMap<(String, RelicRefinement), RelicDefinition>,
) -> Result<(), ProviderError> {
    let items: Vec<WfcdRelic> = serde_json::from_slice(body).map_err(|error| {
        ProviderError::schema_changed(format!("invalid WFCD relic JSON: {error}"))
    })?;
    for item in items {
        let Some((display_name, refinement)) = split_refinement(&item.name) else {
            continue;
        };
        let Some(relic_slug) = item.market_info.map(|market| market.url_name) else {
            continue;
        };
        let mut rewards = Vec::new();
        let mut total_chance = 0.0;
        for reward in item.rewards {
            if !reward.chance.is_finite() || !(0.0..=100.0).contains(&reward.chance) {
                return Err(ProviderError::validation(format!(
                    "invalid relic reward chance for {}",
                    item.name
                )));
            }
            total_chance += reward.chance;
            let resolved_slug = by_game_ref
                .get(reward.item.unique_name.as_str())
                .map(|(slug, _)| (*slug).to_owned())
                .or_else(|| reward.item.warframe_market.map(|market| market.url_name));
            rewards.push(RelicRewardDefinition {
                reward_slug: resolved_slug,
                reward_game_ref: reward.item.unique_name,
                display_name_en: reward.item.name,
                chance_percent: reward.chance,
            });
        }
        if rewards.is_empty() || !(99.0..=101.0).contains(&total_chance) {
            return Err(ProviderError::validation(format!(
                "relic reward chances do not sum to 100% for {}: {total_chance}",
                item.name
            )));
        }
        rewards.sort_by(|left, right| left.display_name_en.cmp(&right.display_name_en));
        relics.insert(
            (relic_slug.clone(), refinement),
            RelicDefinition {
                relic_slug,
                relic_game_ref: item.unique_name,
                display_name_en: display_name.into(),
                refinement,
                vault_status: vault_status(item.vaulted),
                rewards,
            },
        );
    }
    Ok(())
}

fn resolve_component<'a>(
    game_ref: &str,
    by_game_ref: &'a HashMap<&str, (&str, &[String])>,
) -> Option<(&'a str, &'a [String])> {
    if let Some(resolved) = by_game_ref.get(game_ref).copied() {
        return Some(resolved);
    }
    let blueprint_alias = game_ref
        .strip_suffix("Component")
        .map(|prefix| format!("{prefix}Blueprint"));
    blueprint_alias
        .as_deref()
        .and_then(|alias| by_game_ref.get(alias).copied())
}

fn split_refinement(name: &str) -> Option<(&str, RelicRefinement)> {
    let (base, refinement) = name.rsplit_once(' ')?;
    let refinement = match refinement.to_ascii_lowercase().as_str() {
        "intact" => RelicRefinement::Intact,
        "exceptional" => RelicRefinement::Exceptional,
        "flawless" => RelicRefinement::Flawless,
        "radiant" => RelicRefinement::Radiant,
        _ => return None,
    };
    Some((base, refinement))
}

const fn vault_status(value: Option<bool>) -> VaultStatus {
    match value {
        Some(true) => VaultStatus::Vaulted,
        Some(false) => VaultStatus::Available,
        None => VaultStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use platscope_domain::{CatalogItem, CatalogMetadata, ProviderId};

    use super::*;

    #[test]
    fn normalizes_exact_sets_relics_ducats_and_vault_status() {
        let dump = RawGameMetadataDump {
            fetched_at: Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).unwrap(),
            documents: vec![
                RawGameMetadataDocument {
                    name: "Warframes.json".into(),
                    body: include_bytes!("../../../fixtures/metadata/wfcd_sets.json").to_vec(),
                },
                RawGameMetadataDocument {
                    name: "Relics.json".into(),
                    body: include_bytes!("../../../fixtures/metadata/wfcd_relics.json").to_vec(),
                },
                RawGameMetadataDocument {
                    name: "Primary.json".into(),
                    body: br#"[{"uniqueName":"/Lotus/Weapons/Test/Soma","name":"Soma","disposition":4,"omegaAttenuation":1.2}]"#.to_vec(),
                },
            ],
        };
        let result = WfcdMetadataProvider::for_tests()
            .expect("provider")
            .normalize(&dump, &catalog())
            .expect("metadata normalizes");
        assert_eq!(result.prime_sets.len(), 1);
        assert_eq!(result.prime_sets[0].set_slug, "nyx_prime_set");
        assert_eq!(result.prime_sets[0].components.len(), 4);
        assert_eq!(
            result.prime_sets[0].components[0].image_url.as_deref(),
            Some("https://cdn.warframestat.us/img/blueprint.png")
        );
        assert_eq!(result.prime_sets[0].vault_status, VaultStatus::Vaulted);
        assert_eq!(result.prime_parts.len(), 4);
        assert_eq!(result.relics.len(), 2);
        assert_eq!(result.relics[1].refinement, RelicRefinement::Radiant);
        assert_eq!(result.relics[1].rewards.len(), 6);
        assert_eq!(result.riven_dispositions.len(), 1);
        assert_eq!(result.metadata.riven_disposition_count, 1);
        assert_eq!(result.riven_dispositions[0].weapon_name_en, "Soma");
        assert_eq!(result.riven_dispositions[0].disposition, 4);
        assert!((result.riven_dispositions[0].multiplier - 1.2).abs() < f64::EPSILON);
        assert_eq!(result.item_definitions.len(), 1);
        assert_eq!(result.item_definitions[0].slug, "nyx_prime_set");
        assert_eq!(result.item_definitions[0].mastery_requirement, 6);
        assert_eq!(result.metadata.item_definition_count, 1);
    }

    #[test]
    fn rejects_incoherent_relic_probability() {
        let body = br#"[{"uniqueName":"r","name":"Axi T1 Intact","vaulted":false,"marketInfo":{"urlName":"axi_t1_relic"},"rewards":[{"chance":10,"item":{"uniqueName":"x","name":"X"}}]}]"#;
        let dump = RawGameMetadataDump {
            fetched_at: Utc::now(),
            documents: vec![
                RawGameMetadataDocument {
                    name: "Warframes.json".into(),
                    body: include_bytes!("../../../fixtures/metadata/wfcd_sets.json").to_vec(),
                },
                RawGameMetadataDocument {
                    name: "Relics.json".into(),
                    body: body.to_vec(),
                },
            ],
        };
        let error = WfcdMetadataProvider::for_tests()
            .expect("provider")
            .normalize(&dump, &catalog())
            .expect_err("invalid probability must fail");
        assert!(error.message.contains("sum to 100%"));
    }

    #[test]
    fn rejects_invalid_mastery_requirement() {
        let invalid_sets =
            String::from_utf8(include_bytes!("../../../fixtures/metadata/wfcd_sets.json").to_vec())
                .expect("fixture is UTF-8")
                .replace("\"masteryReq\": 6", "\"masteryReq\": 51");
        let dump = RawGameMetadataDump {
            fetched_at: Utc::now(),
            documents: vec![
                RawGameMetadataDocument {
                    name: "Warframes.json".into(),
                    body: invalid_sets.into_bytes(),
                },
                RawGameMetadataDocument {
                    name: "Relics.json".into(),
                    body: include_bytes!("../../../fixtures/metadata/wfcd_relics.json").to_vec(),
                },
            ],
        };
        let error = WfcdMetadataProvider::for_tests()
            .expect("provider")
            .normalize(&dump, &catalog())
            .expect_err("invalid mastery requirement must fail");
        assert!(error.message.contains("invalid mastery requirement"));
    }

    #[test]
    fn component_images_accept_only_safe_cdn_file_names() {
        assert_eq!(
            wfcd_component_image_url(Some("GenericGunPrimeBarrel.png")).as_deref(),
            Some("https://cdn.warframestat.us/img/GenericGunPrimeBarrel.png")
        );
        assert!(wfcd_component_image_url(Some("../secret.png")).is_none());
        assert!(wfcd_component_image_url(Some("https://example.com/a.png")).is_none());
    }

    fn catalog() -> ItemCatalog {
        let definitions = [
            (
                "nyx_prime_set",
                "/Lotus/Powersuits/Jade/NyxPrime",
                vec!["set", "prime"],
            ),
            (
                "nyx_prime_blueprint",
                "/Lotus/Types/Recipes/WarframeRecipes/NyxPrimeBlueprint",
                vec!["blueprint", "prime"],
            ),
            (
                "nyx_prime_chassis_blueprint",
                "/Lotus/Types/Recipes/WarframeRecipes/NyxPrimeChassisBlueprint",
                vec!["component", "prime"],
            ),
            (
                "nyx_prime_neuroptics_blueprint",
                "/Lotus/Types/Recipes/WarframeRecipes/NyxPrimeHelmetBlueprint",
                vec!["component", "prime"],
            ),
            (
                "nyx_prime_systems_blueprint",
                "/Lotus/Types/Recipes/WarframeRecipes/NyxPrimeSystemsBlueprint",
                vec!["component", "prime"],
            ),
        ];
        ItemCatalog {
            metadata: CatalogMetadata {
                provider: ProviderId::RelicsRun,
                fetched_at: Utc::now(),
                schema_version: 1,
                item_count: u64::try_from(definitions.len()).unwrap(),
                checksum_sha256: "fixture".into(),
            },
            items: definitions
                .into_iter()
                .enumerate()
                .map(|(index, (slug, game_ref, tags))| CatalogItem {
                    item_id: format!("fixture-{index}"),
                    slug: slug.into(),
                    display_name_en: slug.into(),
                    display_name_ru: None,
                    thumb: None,
                    thumb_ru: None,
                    game_ref: Some(game_ref.into()),
                    bulk_tradable: false,
                    max_rank: None,
                    subtypes: vec![],
                    tags: tags.into_iter().map(str::to_owned).collect(),
                })
                .collect(),
        }
    }
}
