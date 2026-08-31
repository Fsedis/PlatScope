use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::Utc;
use futures_util::{StreamExt, stream};
use platscope_domain::{
    ArcaneDissolutionDefinition, ArcanePackComponentDefinition, ArcanePackDefinition, CatalogItem,
    GameItemDefinition, GameItemLocalization, GameMetadataSnapshot, GameMetadataSnapshotMetadata,
    GameMetadataSource, ItemCatalog, NightwaveOfferDefinition, PrimePartMetadata,
    PrimeSetComponentDefinition, PrimeSetDefinition, RelicDefinition, RelicRefinement,
    RelicRewardDefinition, RivenDispositionDefinition, RivenWeaponCategory,
    SyndicateOfferDefinition, VaultStatus,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    BoundedHttpClient, GameMetadataProvider, ProviderError, RawGameMetadataDocument,
    RawGameMetadataDump,
};

const DEFAULT_BASE_URL: &str =
    "https://raw.githubusercontent.com/WFCD/warframe-items/master/data/json";
const WFCD_DOCUMENT_NAMES: [&str; 12] = [
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
    "Mods.json",
    "i18n.json",
];
const ARCANE_DISSOLUTION_URL: &str = "https://raw.githubusercontent.com/calamity-inc/warframe-public-export-plus/senpai/ExportArcanes.json";
const ARCANE_PACKS_URL: &str = "https://raw.githubusercontent.com/calamity-inc/warframe-public-export-plus/senpai/ExportBoosterPacks.json";
const VENDOR_MANIFESTS_URL: &str = "https://raw.githubusercontent.com/calamity-inc/warframe-public-export-plus/senpai/ExportVendors.json";
const MAX_DOCUMENT_COUNT: usize = 16;
const MAX_METADATA_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 128 * 1024 * 1024;
const MAX_CONCURRENT_DOWNLOADS: usize = 4;
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
        let mut sources = WFCD_DOCUMENT_NAMES
            .into_iter()
            .map(|name| {
                (
                    name.to_owned(),
                    format!("{}/{name}", self.base_url.trim_end_matches('/')),
                )
            })
            .collect::<Vec<_>>();
        sources.extend(
            [
                ("ArcaneDissolution.json", ARCANE_DISSOLUTION_URL),
                ("ArcanePacks.json", ARCANE_PACKS_URL),
                ("VendorManifests.json", VENDOR_MANIFESTS_URL),
            ]
            .map(|(name, url)| (name.to_owned(), url.to_owned())),
        );

        let results = stream::iter(sources.into_iter().enumerate())
            .map(|(index, (name, url))| async move {
                self.client
                    .get_json_with_limit(&url, true, MAX_METADATA_DOCUMENT_BYTES)
                    .await
                    .map(|body| (index, RawGameMetadataDocument { name, body }))
            })
            .buffer_unordered(MAX_CONCURRENT_DOWNLOADS)
            .collect::<Vec<_>>()
            .await;

        let mut indexed_documents = Vec::with_capacity(results.len());
        let mut total_bytes = 0usize;
        for result in results {
            let (index, document) = result?;
            total_bytes = total_bytes
                .checked_add(document.body.len())
                .ok_or_else(|| ProviderError::validation("game metadata size overflow"))?;
            if total_bytes > MAX_TOTAL_BYTES {
                return Err(ProviderError::validation(
                    "game metadata documents exceed the aggregate size limit",
                ));
            }
            indexed_documents.push((index, document));
        }
        indexed_documents.sort_by_key(|(index, _)| *index);
        let documents = indexed_documents
            .into_iter()
            .map(|(_, document)| document)
            .collect();

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

#[derive(Debug, Deserialize)]
struct WfcdTranslations {
    ru: Option<WfcdRussianTranslation>,
}

#[derive(Debug, Deserialize)]
struct WfcdRussianTranslation {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WfcdMod {
    unique_name: String,
    name: String,
    #[serde(default)]
    tradable: bool,
    image_name: Option<String>,
    #[serde(default)]
    drops: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct WfcdDrop {
    location: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArcaneDissolutionTransport {
    distill_point_value: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ArcanePackComponentTransport {
    item: String,
    rarity: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArcanePackTransport {
    #[serde(default)]
    components: Vec<ArcanePackComponentTransport>,
    #[serde(default)]
    rarity_weights_per_roll: Vec<HashMap<String, f64>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VendorManifestTransport {
    #[serde(default)]
    items: Vec<VendorItemTransport>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VendorItemTransport {
    store_item: String,
    #[serde(default)]
    item_prices: Vec<VendorPriceTransport>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct VendorPriceTransport {
    item_count: u32,
    item_type: String,
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

fn validate_metadata_dump(dump: &RawGameMetadataDump) -> Result<(), ProviderError> {
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
    Ok(())
}

fn metadata_checksum(dump: &RawGameMetadataDump) -> String {
    let mut hasher = Sha256::new();
    for document in &dump.documents {
        hasher.update(document.name.as_bytes());
        hasher.update(&document.body);
    }
    hex::encode(hasher.finalize())
}

fn normalize_wfcd_metadata(
    dump: &RawGameMetadataDump,
    catalog: &ItemCatalog,
) -> Result<GameMetadataSnapshot, ProviderError> {
    validate_metadata_dump(dump)?;

    let by_game_ref: HashMap<&str, (&str, &[String])> = catalog
        .items
        .iter()
        .filter_map(|item| {
            item.game_ref
                .as_deref()
                .map(|game_ref| (game_ref, (item.slug.as_str(), item.tags.as_slice())))
        })
        .collect();
    let catalog_by_game_ref: HashMap<&str, &CatalogItem> = catalog
        .items
        .iter()
        .filter_map(|item| item.game_ref.as_deref().map(|game_ref| (game_ref, item)))
        .collect();
    let mut sets = BTreeMap::new();
    let mut relics = BTreeMap::new();
    let mut parts = BTreeMap::new();
    let mut riven_dispositions = BTreeMap::new();
    let mut item_definitions = BTreeMap::new();
    let mut item_localizations = BTreeMap::new();
    let mut syndicate_offers = BTreeMap::new();
    let mut nightwave_offers = BTreeMap::new();
    let mut arcane_dissolutions = BTreeMap::new();
    let mut arcane_packs = BTreeMap::new();
    let mut has_riven_document = false;
    for document in &dump.documents {
        if document.name.eq_ignore_ascii_case("Relics.json") {
            parse_relics(&document.body, &by_game_ref, &mut relics)?;
        } else if document.name.eq_ignore_ascii_case("i18n.json") {
            parse_item_localizations(&document.body, &mut item_localizations)?;
        } else if document.name.eq_ignore_ascii_case("Mods.json") {
            parse_syndicate_offers(&document.body, &catalog_by_game_ref, &mut syndicate_offers)?;
        } else if document.name.eq_ignore_ascii_case("ArcaneDissolution.json") {
            parse_arcane_dissolutions(
                &document.body,
                &catalog_by_game_ref,
                &mut arcane_dissolutions,
            )?;
        } else if document.name.eq_ignore_ascii_case("ArcanePacks.json") {
            parse_arcane_packs(&document.body, &mut arcane_packs)?;
        } else if document.name.eq_ignore_ascii_case("VendorManifests.json") {
            parse_nightwave_offers(&document.body, &catalog_by_game_ref, &mut nightwave_offers)?;
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

    let checksum_sha256 = metadata_checksum(dump);
    let prime_sets: Vec<_> = sets.into_values().collect();
    let relics: Vec<_> = relics.into_values().collect();
    let prime_parts: Vec<_> = parts.into_values().collect();
    let mut riven_dispositions: Vec<_> = riven_dispositions.into_values().collect();
    riven_dispositions.sort_by(|left, right| left.weapon_name_en.cmp(&right.weapon_name_en));
    let item_definitions: Vec<_> = item_definitions.into_values().collect();
    let item_localizations: Vec<_> = item_localizations.into_values().collect();
    let syndicate_offers: Vec<_> = syndicate_offers.into_values().collect();
    let nightwave_offers: Vec<_> = nightwave_offers.into_values().collect();
    let arcane_dissolutions: Vec<_> = arcane_dissolutions.into_values().collect();
    let arcane_packs: Vec<_> = arcane_packs.into_values().collect();
    Ok(GameMetadataSnapshot {
        metadata: GameMetadataSnapshotMetadata {
            source: GameMetadataSource::WfcdWarframeItems,
            fetched_at: dump.fetched_at,
            schema_version: 7,
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
        item_localizations,
        syndicate_offers,
        nightwave_offers,
        arcane_dissolutions,
        arcane_packs,
    })
}

fn catalog_image_url(item: &CatalogItem) -> Option<String> {
    item.thumb_ru
        .as_ref()
        .or(item.thumb.as_ref())
        .map(|thumb| format!("https://warframe.market/static/assets/{thumb}"))
}

fn parse_syndicate_offers(
    body: &[u8],
    catalog_by_game_ref: &HashMap<&str, &CatalogItem>,
    offers: &mut BTreeMap<(String, String), SyndicateOfferDefinition>,
) -> Result<(), ProviderError> {
    let mods: Vec<WfcdMod> = serde_json::from_slice(body).map_err(|error| {
        ProviderError::schema_changed(format!("invalid WFCD Mods JSON: {error}"))
    })?;
    for item in mods.into_iter().filter(|item| item.tradable) {
        let Some(catalog_item) = catalog_by_game_ref.get(item.unique_name.as_str()).copied() else {
            continue;
        };
        let drops = match item.drops {
            serde_json::Value::Array(values) => values,
            serde_json::Value::Object(_) => vec![item.drops],
            _ => Vec::new(),
        };
        for drop in drops {
            let Ok(drop) = serde_json::from_value::<WfcdDrop>(drop) else {
                continue;
            };
            let Some((syndicate, title)) = drop.location.split_once(',') else {
                continue;
            };
            let Some(syndicate) = main_syndicate_name(syndicate.trim()) else {
                continue;
            };
            let title = title.trim();
            if title.is_empty() || title.len() > 64 {
                continue;
            }
            offers.insert(
                (syndicate.to_owned(), catalog_item.slug.clone()),
                SyndicateOfferDefinition {
                    syndicate: syndicate.to_owned(),
                    required_title: title.to_owned(),
                    slug: catalog_item.slug.clone(),
                    game_ref: item.unique_name.clone(),
                    display_name_en: item.name.clone(),
                    display_name_ru: catalog_item.display_name_ru.clone(),
                    image_url: wfcd_component_image_url(item.image_name.as_deref())
                        .or_else(|| catalog_image_url(catalog_item)),
                    standing_cost: 25_000,
                },
            );
        }
    }
    Ok(())
}

fn main_syndicate_name(value: &str) -> Option<&'static str> {
    match value {
        "Steel Meridian" => Some("Steel Meridian"),
        "Arbiters of Hexis" => Some("Arbiters of Hexis"),
        "Cephalon Suda" => Some("Cephalon Suda"),
        "The Perrin Sequence" | "Perrin Sequence" => Some("The Perrin Sequence"),
        "Red Veil" => Some("Red Veil"),
        "New Loka" => Some("New Loka"),
        _ => None,
    }
}

fn parse_arcane_dissolutions(
    body: &[u8],
    catalog_by_game_ref: &HashMap<&str, &CatalogItem>,
    definitions: &mut BTreeMap<String, ArcaneDissolutionDefinition>,
) -> Result<(), ProviderError> {
    let arcanes: HashMap<String, ArcaneDissolutionTransport> = serde_json::from_slice(body)
        .map_err(|error| {
            ProviderError::schema_changed(format!("invalid Arcane dissolution JSON: {error}"))
        })?;
    for (game_ref, arcane) in arcanes {
        let Some(vosfor) = arcane
            .distill_point_value
            .filter(|value| (1..=1_000).contains(value))
        else {
            continue;
        };
        let Some(item) = catalog_by_game_ref.get(game_ref.as_str()).copied() else {
            continue;
        };
        definitions.insert(
            game_ref.clone(),
            ArcaneDissolutionDefinition {
                slug: item.slug.clone(),
                game_ref,
                display_name_en: item.display_name_en.clone(),
                display_name_ru: item.display_name_ru.clone(),
                image_url: catalog_image_url(item),
                vosfor,
            },
        );
    }
    Ok(())
}

fn parse_arcane_packs(
    body: &[u8],
    definitions: &mut BTreeMap<String, ArcanePackDefinition>,
) -> Result<(), ProviderError> {
    let packs: HashMap<String, ArcanePackTransport> =
        serde_json::from_slice(body).map_err(|error| {
            ProviderError::schema_changed(format!("invalid Arcane pack JSON: {error}"))
        })?;
    for (key, pack) in packs {
        let Some(display_name_ru) = arcane_pack_name_ru(&key) else {
            continue;
        };
        if pack.components.is_empty()
            || pack.components.len() > 100
            || pack.rarity_weights_per_roll.is_empty()
            || pack.rarity_weights_per_roll.len() > 6
        {
            continue;
        }
        let valid_rolls = pack.rarity_weights_per_roll.iter().all(|roll| {
            let sum = roll.values().sum::<f64>();
            roll.values()
                .all(|weight| weight.is_finite() && (0.0..=1.0).contains(weight))
                && (sum - 1.0).abs() <= 0.001
        });
        if !valid_rolls {
            continue;
        }
        let components = pack
            .components
            .into_iter()
            .filter(|component| {
                component.item.starts_with("/Lotus/")
                    && matches!(
                        component.rarity.as_str(),
                        "COMMON" | "UNCOMMON" | "RARE" | "LEGENDARY"
                    )
            })
            .map(|component| ArcanePackComponentDefinition {
                game_ref: component.item,
                rarity: component.rarity,
            })
            .collect::<Vec<_>>();
        if components.is_empty() {
            continue;
        }
        definitions.insert(
            key.clone(),
            ArcanePackDefinition {
                key,
                display_name_ru: display_name_ru.to_owned(),
                rolls: pack.rarity_weights_per_roll,
                components,
            },
        );
    }
    Ok(())
}

fn arcane_pack_name_ru(key: &str) -> Option<&'static str> {
    let suffix = key.rsplit('/').next()?;
    match suffix {
        "EntratiLabsArcanePackAlbrechtsLaboratories" => Some("Кавия"),
        "EntratiLabsArcanePackDuviri" => Some("Дувири"),
        "EntratiLabsArcanePackZarimanAndLua" => Some("Зариман и Луа"),
        "EntratiLabsArcanePackMarsAndDeimos" => Some("Некралиск"),
        "EntratiLabsArcanePackFortuna" => Some("Солярис"),
        "EntratiLabsArcanePackSteelPathAndArbitrations" => Some("Стальной путь"),
        "EntratiLabsArcanePackCetus" => Some("Острон"),
        "EntratiLabsArcanePackPlainsOfEidolon" => Some("Эйдолон"),
        "EntratiLabsArcanePackHollvania" => Some("Хёльвания"),
        _ => None,
    }
}

fn parse_nightwave_offers(
    body: &[u8],
    catalog_by_game_ref: &HashMap<&str, &CatalogItem>,
    definitions: &mut BTreeMap<String, NightwaveOfferDefinition>,
) -> Result<(), ProviderError> {
    let manifests: HashMap<String, VendorManifestTransport> = serde_json::from_slice(body)
        .map_err(|error| {
            ProviderError::schema_changed(format!("invalid vendor manifest JSON: {error}"))
        })?;
    let Some((_, manifest)) = manifests
        .iter()
        .filter_map(|(key, manifest)| {
            nightwave_manifest_number(key).map(|number| (number, manifest))
        })
        .max_by_key(|(number, _)| *number)
    else {
        return Ok(());
    };
    for entry in &manifest.items {
        let Some(price) = entry
            .item_prices
            .iter()
            .find(|price| price.item_type.contains("/NoraIntermission") && price.item_count > 0)
        else {
            continue;
        };
        let Some(rest) = entry.store_item.strip_prefix("/Lotus/StoreItems") else {
            continue;
        };
        let game_ref = format!("/Lotus{rest}");
        let Some(item) = catalog_by_game_ref.get(game_ref.as_str()).copied() else {
            continue;
        };
        definitions.insert(
            item.slug.clone(),
            NightwaveOfferDefinition {
                slug: item.slug.clone(),
                game_ref,
                display_name_en: item.display_name_en.clone(),
                display_name_ru: item.display_name_ru.clone(),
                image_url: catalog_image_url(item),
                cred_cost: price.item_count,
            },
        );
    }
    Ok(())
}

fn nightwave_manifest_number(key: &str) -> Option<u16> {
    let suffix = key
        .strip_prefix("/Lotus/Types/Game/VendorManifests/Events/RadioLegionIntermission")?
        .strip_suffix("VendorManifest")?;
    suffix.parse().ok()
}

fn parse_item_localizations(
    body: &[u8],
    localizations: &mut BTreeMap<String, GameItemLocalization>,
) -> Result<(), ProviderError> {
    let entries: HashMap<String, WfcdTranslations> =
        serde_json::from_slice(body).map_err(|error| {
            ProviderError::schema_changed(format!("invalid WFCD i18n JSON: {error}"))
        })?;
    for (game_ref, translations) in entries {
        if !game_ref.starts_with("/Lotus/") || game_ref.len() > 256 {
            continue;
        }
        let Some(display_name_ru) = translations
            .ru
            .and_then(|translation| translation.name)
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty() && name.len() <= 256)
        else {
            continue;
        };
        localizations.insert(
            game_ref.clone(),
            GameItemLocalization {
                game_ref,
                display_name_ru,
            },
        );
    }
    Ok(())
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
    'set_items: for item in items.into_iter().filter(|item| {
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
        let mut resolved_parts = Vec::new();
        let tradable_components = item
            .components
            .into_iter()
            .filter(|component| component.tradable && component.item_count > 0)
            .collect::<Vec<_>>();
        for component in tradable_components {
            let Some((slug, _)) = resolve_component(&component.unique_name, by_game_ref) else {
                // Публикация оставшихся строк превратила бы обрезанный рецепт
                // в якобы полный сет. Пропускаем весь сет, пока каталог и WFCD
                // снова не будут согласованы.
                continue 'set_items;
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
                resolved_parts.push((
                    slug.into(),
                    PrimePartMetadata {
                        slug: slug.into(),
                        game_ref: component.unique_name,
                        ducats,
                        vault_status,
                    },
                ));
            }
        }
        components.sort_by(|left, right| left.slug.cmp(&right.slug));
        if components
            .windows(2)
            .any(|pair| pair[0].slug == pair[1].slug)
        {
            continue;
        }
        if components.len() >= 2 {
            parts.extend(resolved_parts);
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
    use crate::{MetadataProvider, RelicsRunCatalogProvider};

    #[tokio::test]
    #[ignore = "сетевой smoke-тест production-источников игровых данных"]
    async fn production_documents_fit_declared_limits() {
        let catalog_provider = RelicsRunCatalogProvider::new().expect("catalog provider");
        let raw_catalog = catalog_provider
            .load_metadata()
            .await
            .expect("production catalog downloads");
        let catalog = catalog_provider
            .normalize_metadata(&raw_catalog)
            .expect("production catalog normalizes");
        let dump = WfcdMetadataProvider::production()
            .expect("provider")
            .fetch_latest()
            .await
            .expect("production metadata downloads");

        assert_eq!(dump.documents.len(), WFCD_DOCUMENT_NAMES.len() + 3);
        validate_metadata_dump(&dump).expect("production metadata stays within aggregate limit");
        let snapshot = normalize_wfcd_metadata(&dump, &catalog)
            .expect("production metadata normalizes against the current catalog");
        assert_eq!(snapshot.metadata.schema_version, 7);
        assert!(!snapshot.syndicate_offers.is_empty());
        assert!(!snapshot.arcane_dissolutions.is_empty());
    }

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
                RawGameMetadataDocument {
                    name: "i18n.json".into(),
                    body: r#"{
                        "/Lotus/Weapons/Test/Soma":{"ru":{"name":"Сома"}},
                        "SolNode1":{"ru":{"name":"Узел"}},
                        "/Lotus/Weapons/Test/Empty":{"ru":{"name":"  "}}
                    }"#
                    .as_bytes()
                    .to_vec(),
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
        assert_eq!(result.item_localizations.len(), 1);
        assert_eq!(
            result.item_localizations[0].game_ref,
            "/Lotus/Weapons/Test/Soma"
        );
        assert_eq!(result.item_localizations[0].display_name_ru, "Сома");
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

    #[test]
    fn converter_metadata_normalizes_vendor_and_arcane_facts() {
        let items = [
            CatalogItem {
                item_id: "mod-id".into(),
                slug: "scattered_justice".into(),
                display_name_en: "Scattered Justice".into(),
                display_name_ru: Some("Рассеянное Правосудие".into()),
                thumb: None,
                thumb_ru: None,
                game_ref: Some("/Lotus/Upgrades/Mods/Syndicate/HekMod".into()),
                bulk_tradable: true,
                max_rank: Some(3),
                max_charges: None,
                max_amber_stars: None,
                max_cyan_stars: None,
                subtypes: vec![],
                tags: vec!["mod".into()],
            },
            CatalogItem {
                item_id: "arcane-id".into(),
                slug: "primary_merciless".into(),
                display_name_en: "Primary Merciless".into(),
                display_name_ru: Some("Основная Беспощадность".into()),
                thumb: None,
                thumb_ru: None,
                game_ref: Some(
                    "/Lotus/Upgrades/CosmeticEnhancers/Offensive/PrimaryDamageOnKill".into(),
                ),
                bulk_tradable: true,
                max_rank: Some(5),
                max_charges: None,
                max_amber_stars: None,
                max_cyan_stars: None,
                subtypes: vec![],
                tags: vec!["arcane_enhancement".into()],
            },
        ];
        let lookup: HashMap<&str, &CatalogItem> = items
            .iter()
            .filter_map(|item| item.game_ref.as_deref().map(|game_ref| (game_ref, item)))
            .collect();

        let mut syndicate = BTreeMap::new();
        parse_syndicate_offers(
            br#"[{"uniqueName":"/Lotus/Upgrades/Mods/Syndicate/HekMod","name":"Scattered Justice","tradable":true,"imageName":"HekMod.png","drops":{"location":"Steel Meridian, Protector"}}]"#,
            &lookup,
            &mut syndicate,
        )
        .expect("syndicate offer parses");
        assert_eq!(syndicate.len(), 1);
        assert_eq!(syndicate.values().next().unwrap().standing_cost, 25_000);

        let mut dissolution = BTreeMap::new();
        parse_arcane_dissolutions(
            br#"{"/Lotus/Upgrades/CosmeticEnhancers/Offensive/PrimaryDamageOnKill":{"distillPointValue":20}}"#,
            &lookup,
            &mut dissolution,
        )
        .expect("dissolution value parses");
        assert_eq!(dissolution.values().next().unwrap().vosfor, 20);

        let mut packs = BTreeMap::new();
        parse_arcane_packs(
            br#"{"/Lotus/Types/BoosterPacks/EntratiLabsArcanePackSteelPathAndArbitrations":{"components":[{"Item":"/Lotus/Upgrades/CosmeticEnhancers/Offensive/PrimaryDamageOnKill","Rarity":"RARE"}],"rarityWeightsPerRoll":[{"COMMON":0,"UNCOMMON":0,"RARE":1,"LEGENDARY":0}]}}"#,
            &mut packs,
        )
        .expect("pack parses");
        assert_eq!(
            packs.values().next().unwrap().display_name_ru,
            "Стальной путь"
        );
    }

    #[test]
    fn newest_nightwave_manifest_yields_only_market_items() {
        let item = CatalogItem {
            item_id: "aura-id".into(),
            slug: "corrosive_projection".into(),
            display_name_en: "Corrosive Projection".into(),
            display_name_ru: Some("Коррозийный Выброс".into()),
            thumb: None,
            thumb_ru: None,
            game_ref: Some("/Lotus/Upgrades/Mods/Aura/EnemyArmorReductionAuraMod".into()),
            bulk_tradable: true,
            max_rank: Some(5),
            max_charges: None,
            max_amber_stars: None,
            max_cyan_stars: None,
            subtypes: vec![],
            tags: vec!["mod".into()],
        };
        let lookup = HashMap::from([(item.game_ref.as_deref().unwrap(), &item)]);
        let mut offers = BTreeMap::new();
        parse_nightwave_offers(
            br#"{"/Lotus/Types/Game/VendorManifests/Events/RadioLegionIntermission15VendorManifest":{"items":[{"storeItem":"/Lotus/StoreItems/Upgrades/Mods/Aura/EnemyArmorReductionAuraMod","itemPrices":[{"ItemCount":20,"ItemType":"/Lotus/Types/Items/MiscItems/NoraIntermissionFifteenCreds"}]}]}}"#,
            &lookup,
            &mut offers,
        )
        .expect("Nightwave manifest parses");
        assert_eq!(offers["corrosive_projection"].cred_cost, 20);
    }

    #[test]
    fn prime_set_with_unresolved_tradable_component_is_not_published() {
        let item_catalog = catalog();
        let by_game_ref: HashMap<&str, (&str, &[String])> = item_catalog
            .items
            .iter()
            .filter_map(|item| {
                item.game_ref
                    .as_deref()
                    .map(|game_ref| (game_ref, (item.slug.as_str(), item.tags.as_slice())))
            })
            .collect();
        let mut sets = BTreeMap::new();
        let mut parts = BTreeMap::new();
        parse_sets(
            br#"[{
                "uniqueName":"/Lotus/Powersuits/Jade/NyxPrime",
                "name":"Nyx Prime",
                "isPrime":true,
                "components":[
                    {"uniqueName":"/Lotus/Types/Recipes/WarframeRecipes/NyxPrimeBlueprint","itemCount":1,"tradable":true,"ducats":15},
                    {"uniqueName":"/Lotus/Types/Recipes/WarframeRecipes/NyxPrimeChassisBlueprint","itemCount":1,"tradable":true,"ducats":45},
                    {"uniqueName":"/Lotus/Types/Recipes/WarframeRecipes/MissingPrimeSystemsBlueprint","itemCount":1,"tradable":true,"ducats":100}
                ]
            }]"#,
            &by_game_ref,
            &mut sets,
            &mut parts,
        )
        .expect("schema-valid WFCD set document parses");

        assert!(sets.is_empty());
        assert!(parts.is_empty());
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
                    max_charges: None,
                    max_amber_stars: None,
                    max_cyan_stars: None,
                    subtypes: vec![],
                    tags: tags.into_iter().map(str::to_owned).collect(),
                })
                .collect(),
        }
    }
}
