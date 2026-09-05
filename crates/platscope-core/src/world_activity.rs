//! Общий кэш экрана и уведомлений. Последний хороший ответ переживает перезапуск.
use std::sync::Mutex;
use std::time::{Duration, Instant};

use platscope_domain::{GameMetadataSnapshot, ItemCatalog, MasteryItemDefinition};
use platscope_providers::{
    ActivityOffer, ActivityTrader, ProviderError, ProviderErrorCode, WorldActivityProvider,
    WorldActivitySnapshot,
};
use platscope_storage::Database;
use serde::Serialize;

use crate::{CoreError, lock_database};

const CACHE_KEY: &str = "world_activity.snapshot.v1";
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(15);
const RETRY_INTERVAL: Duration = Duration::from_secs(45);
const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Default)]
struct ActivityCache {
    loaded: bool,
    snapshot: Option<WorldActivitySnapshot>,
    attempted_at: Option<Instant>,
    failed: bool,
}

pub struct WorldActivityService {
    provider: WorldActivityProvider,
    cache: tokio::sync::Mutex<ActivityCache>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldActivityView {
    #[serde(flatten)]
    pub snapshot: WorldActivitySnapshot,
    pub refresh_failed: bool,
    pub catalog_available: bool,
    pub baro_offers: Vec<WorldActivityOfferView>,
    pub resurgence_offers: Vec<WorldActivityOfferView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldActivityOfferView {
    pub game_ref: String,
    pub display_name: String,
    pub display_name_en: String,
    pub kind: &'static str,
    pub ducats: Option<u64>,
    pub credits: Option<u64>,
    pub mastery_ref: Option<String>,
    pub set_slug: Option<String>,
    pub relic_slug: Option<String>,
    pub equipment_category: Option<String>,
    pub image_url: Option<String>,
    pub rewards: Vec<WorldActivityRelicRewardView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldActivityRelicRewardView {
    pub game_ref: String,
    pub display_name: String,
    pub chance_percent: f64,
    pub equipment_refs: Vec<String>,
}

impl WorldActivityService {
    /// # Errors
    /// Возвращает ошибку инициализации HTTP-клиента.
    pub fn production() -> Result<Self, CoreError> {
        Ok(Self {
            provider: WorldActivityProvider::production()?,
            cache: tokio::sync::Mutex::new(ActivityCache::default()),
        })
    }

    /// Не требует привязки Warframe Market, сканирования инвентаря или запуска игры.
    ///
    /// # Errors
    /// Возвращает ошибку сети, если ещё нет сохранённых данных, или ошибку чтения справочников.
    pub async fn view(
        &self,
        database: &Mutex<Database>,
        force_refresh: bool,
    ) -> Result<WorldActivityView, CoreError> {
        let mut cache = self.cache.lock().await;
        if !cache.loaded
            && let Ok(database) = database.try_lock()
        {
            // Повреждённый кэш можно восстановить из публичного источника.
            cache.snapshot = database.get_setting(CACHE_KEY).ok().flatten();
            cache.loaded = true;
        }
        if should_refresh(&cache, force_refresh) {
            cache.attempted_at = Some(Instant::now());
            let result = tokio::time::timeout(Duration::from_secs(15), self.provider.fetch()).await;
            match result {
                Ok(Ok(mut snapshot))
                    if cache
                        .snapshot
                        .as_ref()
                        .is_none_or(|previous| snapshot.source_at >= previous.source_at) =>
                {
                    if let Some(previous) = &cache.snapshot {
                        preserve_unavailable_sections(&mut snapshot, previous);
                    }
                    if let Ok(database) = database.try_lock() {
                        let _ = database.set_setting(CACHE_KEY, &snapshot);
                    }
                    cache.snapshot = Some(snapshot);
                    cache.failed = false;
                    cache.loaded = true;
                }
                _ => {
                    cache.failed = true;
                }
            }
        }
        let snapshot = cache.snapshot.clone().ok_or_else(|| {
            ProviderError::new(
                ProviderErrorCode::Unavailable,
                "public worldstate is unavailable; retry shortly",
                true,
            )
        })?;
        let refresh_failed = cache.failed;
        drop(cache);
        let (metadata, catalog) = load_local_catalogs(database)?;
        Ok(build_view(
            snapshot,
            refresh_failed,
            metadata.as_ref(),
            catalog.as_ref(),
        ))
    }
}

fn load_local_catalogs(
    database: &Mutex<Database>,
) -> Result<(Option<GameMetadataSnapshot>, Option<ItemCatalog>), CoreError> {
    // Занятое соединение — не отсутствие справочника. Ошибку чтения
    // возвращаем вызывающему коду, чтобы он сохранил последний хороший экран.
    let database = lock_database(database)?;
    Ok((
        database.load_current_game_metadata()?,
        database.load_current_catalog()?,
    ))
}

fn should_refresh(cache: &ActivityCache, force: bool) -> bool {
    if let Some(attempted_at) = cache.attempted_at {
        let interval = if cache.failed {
            RETRY_INTERVAL
        } else {
            MIN_REQUEST_INTERVAL
        };
        if attempted_at.elapsed() < interval {
            return false;
        }
    }
    force
        || cache.failed
        || cache.snapshot.as_ref().is_none_or(|snapshot| {
            let age = chrono::Utc::now().signed_duration_since(snapshot.fetched_at);
            age.to_std().map_or(true, |age| age >= CACHE_TTL)
                || snapshot
                    .cycles
                    .iter()
                    .any(|cycle| cycle.period.expiry <= chrono::Utc::now())
        })
}

fn preserve_unavailable_sections(
    next: &mut WorldActivitySnapshot,
    previous: &WorldActivitySnapshot,
) {
    for cycle in &previous.cycles {
        if next.unavailable_sections.contains(&cycle.key) {
            next.cycles.push(cycle.clone());
        }
    }
    if next.baro.is_none() {
        next.baro.clone_from(&previous.baro);
    }
    if next.resurgence.is_none() {
        next.resurgence.clone_from(&previous.resurgence);
    }
    if next.steel_path.is_none() {
        next.steel_path.clone_from(&previous.steel_path);
    }
    if next.sortie.is_none() {
        next.sortie.clone_from(&previous.sortie);
    }
    if next.unavailable_sections.contains(&"events".into()) {
        next.events.clone_from(&previous.events);
    }
}

fn build_view(
    snapshot: WorldActivitySnapshot,
    refresh_failed: bool,
    metadata: Option<&GameMetadataSnapshot>,
    catalog: Option<&ItemCatalog>,
) -> WorldActivityView {
    let offers = |trader: Option<&ActivityTrader>| {
        trader.map_or_else(Vec::new, |trader| {
            trader
                .inventory
                .iter()
                .map(|offer| localize_offer(offer, metadata, catalog))
                .collect()
        })
    };
    WorldActivityView {
        baro_offers: offers(snapshot.baro.as_ref()),
        resurgence_offers: offers(snapshot.resurgence.as_ref()),
        catalog_available: metadata.is_some(),
        snapshot,
        refresh_failed,
    }
}

fn localize_offer(
    offer: &ActivityOffer,
    metadata: Option<&GameMetadataSnapshot>,
    catalog: Option<&ItemCatalog>,
) -> WorldActivityOfferView {
    let mastery = metadata.and_then(|metadata| {
        metadata
            .mastery_items
            .iter()
            .find(|item| item.game_ref == offer.game_ref)
    });
    let relic = metadata.and_then(|metadata| {
        metadata
            .relics
            .iter()
            .find(|item| item.relic_game_ref == offer.game_ref)
    });
    let item = catalog.and_then(|catalog| {
        catalog.items.iter().find(|item| {
            item.game_ref.as_ref() == Some(&offer.game_ref)
                || relic.is_some_and(|relic| item.slug == relic.relic_slug)
        })
    });
    let name_en = mastery
        .map(|item| &item.display_name_en)
        .or_else(|| relic.map(|item| &item.display_name_en))
        .or_else(|| item.map(|item| &item.display_name_en))
        .unwrap_or(&offer.name)
        .clone();
    let name_ru = metadata
        .and_then(|metadata| {
            metadata
                .item_localizations
                .iter()
                .find(|item| item.game_ref == offer.game_ref)
        })
        .map(|item| &item.display_name_ru)
        .or_else(|| mastery.and_then(|item| item.display_name_ru.as_ref()))
        .or_else(|| item.and_then(|item| item.display_name_ru.as_ref()));
    let set_slug = metadata
        .and_then(|metadata| {
            metadata
                .prime_sets
                .iter()
                .find(|set| set.set_game_ref == offer.game_ref)
        })
        .map(|set| set.set_slug.clone());
    let bundle_name = metadata
        .and_then(|metadata| localized_bundle_name(&offer.game_ref, &metadata.mastery_items));
    WorldActivityOfferView {
        game_ref: offer.game_ref.clone(),
        display_name: name_ru.or(bundle_name.as_ref()).unwrap_or(&name_en).clone(),
        display_name_en: name_en,
        kind: if mastery.is_some() {
            "equipment"
        } else if relic.is_some() || offer.game_ref.contains("/Projections/") {
            "relic"
        } else {
            "other"
        },
        ducats: offer.ducats,
        credits: offer.credits,
        mastery_ref: mastery.map(|item| item.game_ref.clone()),
        set_slug,
        relic_slug: relic.map(|item| item.relic_slug.clone()),
        equipment_category: mastery.map(|item| item.category.clone()),
        image_url: mastery.and_then(|item| item.image_url.clone()),
        rewards: relic.map_or_else(Vec::new, |relic| {
            relic
                .rewards
                .iter()
                .map(|reward| localize_relic_reward(reward, metadata, catalog))
                .collect()
        }),
    }
}

fn localize_relic_reward(
    reward: &platscope_domain::RelicRewardDefinition,
    metadata: Option<&GameMetadataSnapshot>,
    catalog: Option<&ItemCatalog>,
) -> WorldActivityRelicRewardView {
    let localized = metadata.and_then(|metadata| {
        metadata
            .item_localizations
            .iter()
            .find(|item| item.game_ref == reward.reward_game_ref)
    });
    let market = catalog.and_then(|catalog| {
        catalog.items.iter().find(|item| {
            item.game_ref.as_ref() == Some(&reward.reward_game_ref)
                || reward.reward_slug.as_ref() == Some(&item.slug)
        })
    });
    let display_name = if reward.reward_game_ref == "/Lotus/Types/Recipes/Components/FormaBlueprint"
    {
        if reward.display_name_en.starts_with("2X ") {
            "Чертёж: Форма ×2".into()
        } else {
            "Чертёж: Форма".into()
        }
    } else {
        localized
            .map(|item| &item.display_name_ru)
            .or_else(|| market.and_then(|item| item.display_name_ru.as_ref()))
            .unwrap_or(&reward.display_name_en)
            .replace("(Чертеж)", "· чертёж")
            .replace("(Чертёж)", "· чертёж")
    };
    WorldActivityRelicRewardView {
        game_ref: reward.reward_game_ref.clone(),
        display_name,
        chance_percent: reward.chance_percent,
        equipment_refs: metadata.map_or_else(Vec::new, |metadata| {
            metadata
                .prime_sets
                .iter()
                .filter(|set| {
                    set.components.iter().any(|part| {
                        part.game_ref == reward.reward_game_ref
                            || reward.reward_slug.as_ref() == Some(&part.slug)
                    })
                })
                .map(|set| set.set_game_ref.clone())
                .collect()
        }),
    }
}

// Некоторые наборы не входят в ExportItems. Распознаём их точные идентификаторы,
// а имя варфрейма берём из того же каталога, что и отдельное снаряжение.
fn localized_bundle_name(game_ref: &str, items: &[MasteryItemDefinition]) -> Option<String> {
    let id = game_ref.strip_prefix("/Lotus/Types/StoreItems/Packages/MegaPrimeVault/MPV")?;
    match id {
        "IctusPrimeSentAccessories" => return Some("Аксессуары стража: Иктус Прайм".into()),
        "AtavistPrimeArmorSet" => return Some("Набор брони: Атавист Прайм".into()),
        _ => {}
    }
    let primes: Vec<_> = items
        .iter()
        .filter(|item| item.category == "warframe")
        .filter_map(|item| {
            Some((
                item.display_name_en
                    .strip_suffix(" Prime")?
                    .replace(' ', ""),
                item.display_name_ru
                    .as_ref()
                    .unwrap_or(&item.display_name_en),
            ))
        })
        .collect();
    if let Some(name) = id.strip_suffix("PrimeSinglePack") {
        return primes
            .iter()
            .find(|(base, _)| base == name)
            .map(|(_, label)| format!("Набор «{label}»"));
    }
    let pair = id.strip_suffix("PrimeDualPack")?;
    for (first, first_label) in &primes {
        if let Some(rest) = pair.strip_prefix(first)
            && let Some((_, second_label)) = primes.iter().find(|(base, _)| base == rest)
        {
            return Some(format!("Двойной набор: {first_label} и {second_label}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use platscope_providers::parse_world_activity;

    fn resurgence_fixture() -> (WorldActivitySnapshot, GameMetadataSnapshot, ItemCatalog) {
        let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../fixtures/world-activity/resurgence.json"
        ))
        .unwrap();
        (
            parse_world_activity(
                &serde_json::to_vec(&fixture["worldstate"]).unwrap(),
                chrono::Utc::now(),
            )
            .unwrap(),
            serde_json::from_value(fixture["metadata"].clone()).unwrap(),
            serde_json::from_value(fixture["catalog"].clone()).unwrap(),
        )
    }

    #[test]
    fn resurgence_maps_real_rotation_to_warframes_and_exact_relic_rewards() {
        let (snapshot, metadata, catalog) = resurgence_fixture();
        let view = build_view(snapshot, false, Some(&metadata), Some(&catalog));
        let frames: Vec<_> = view
            .resurgence_offers
            .iter()
            .filter(|offer| offer.equipment_category.as_deref() == Some("warframe"))
            .collect();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].display_name, "Банши Прайм");
        assert_eq!(frames[1].display_name, "Мираж Прайм");
        assert!(frames.iter().all(|offer| offer.image_url.is_some()));
        let relics: Vec<_> = view
            .resurgence_offers
            .iter()
            .filter(|offer| offer.kind == "relic")
            .collect();
        assert_eq!(relics.len(), 6);
        for relic in &relics {
            assert_eq!(relic.rewards.len(), 6);
            for frame in &frames {
                assert!(
                    relic
                        .rewards
                        .iter()
                        .any(|reward| reward.equipment_refs.contains(&frame.game_ref)),
                    "{}: {}",
                    relic.display_name,
                    frame.display_name
                );
            }
            assert!(
                relic
                    .rewards
                    .iter()
                    .filter(|reward| reward.display_name.contains("Форма"))
                    .all(|reward| reward.equipment_refs.is_empty())
            );
        }
        let lith = relics
            .iter()
            .find(|relic| relic.display_name == "Реликвия Лит M7")
            .unwrap();
        assert!(
            lith.rewards
                .iter()
                .any(|reward| reward.display_name.contains("Мираж")
                    && reward.display_name.contains("чертёж"))
        );
        assert!(
            lith.rewards
                .iter()
                .any(|reward| reward.display_name == "Чертёж: Форма ×2")
        );
        let blueprint = lith
            .rewards
            .iter()
            .find(|reward| reward.equipment_refs.contains(&frames[1].game_ref))
            .unwrap();
        assert!((blueprint.chance_percent - 2.0).abs() < f64::EPSILON);
        // Фоновый браузер получает тот же результат сопоставления, что и приложение.
        let browser_offers: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../fixtures/world-activity/resurgence-offers.json"
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(&view.resurgence_offers).unwrap(),
            browser_offers
        );
    }

    #[test]
    fn busy_database_does_not_become_a_missing_catalog() {
        let (_, metadata, catalog) = resurgence_fixture();
        let database = Mutex::new(Database::open_in_memory().unwrap());
        let mut guard = database.lock().unwrap();
        guard.promote_game_metadata(&metadata).unwrap();
        guard.promote_catalog(&catalog).unwrap();
        std::thread::scope(|scope| {
            let reader = scope.spawn(|| load_local_catalogs(&database).unwrap());
            std::thread::sleep(Duration::from_millis(30));
            drop(guard);
            let (metadata, catalog) = reader.join().unwrap();
            assert!(metadata.is_some());
            assert!(catalog.is_some());
        });
    }

    fn snapshot() -> WorldActivitySnapshot {
        parse_world_activity(br#"{"timestamp":"2026-09-05T09:45:00Z","events":[],"cetusCycle":{"state":"day","activation":"2026-09-05T09:20:00Z","expiry":"2026-09-05T11:00:00Z"}}"#,
            chrono::Utc::now()).unwrap()
    }
    #[test]
    fn failed_section_keeps_last_good_value_and_stays_marked_unavailable() {
        let previous = snapshot();
        let mut next = previous.clone();
        next.cycles.clear();
        next.unavailable_sections.push("cetus".into());
        preserve_unavailable_sections(&mut next, &previous);
        assert_eq!(next.cycles[0].state, "day");
        assert!(next.unavailable_sections.contains(&"cetus".into()));
    }
    #[test]
    fn manual_refresh_cannot_bypass_rate_limit_or_failure_backoff() {
        let mut cache = ActivityCache {
            attempted_at: Some(Instant::now()),
            ..ActivityCache::default()
        };
        assert!(!should_refresh(&cache, true));
        cache.failed = true;
        cache.attempted_at = Instant::now().checked_sub(Duration::from_secs(20));
        assert!(!should_refresh(&cache, true));
        cache.attempted_at = Instant::now().checked_sub(Duration::from_secs(46));
        assert!(should_refresh(&cache, false));
    }
    #[test]
    fn no_catalog_does_not_prevent_showing_cycles_or_invent_mastery() {
        assert_eq!(
            build_view(snapshot(), false, None, None)
                .snapshot
                .cycles
                .len(),
            1
        );
        let offer = localize_offer(
            &ActivityOffer {
                game_ref: "/Lotus/Weapons/Test".into(),
                name: "Test".into(),
                ducats: Some(0),
                credits: Some(50000),
            },
            None,
            None,
        );
        assert!(offer.mastery_ref.is_none());
        assert_eq!(offer.ducats, Some(0));
    }

    #[test]
    fn localizes_exact_prime_bundles_without_assigning_equipment_identity() {
        let items: Vec<_> = [
            ("Banshee Prime", "Банши Прайм"),
            ("Mirage Prime", "Мираж Прайм"),
        ]
        .into_iter()
        .map(|(en, ru)| MasteryItemDefinition {
            game_ref: en.into(),
            display_name_en: en.into(),
            display_name_ru: Some(ru.into()),
            category: "warframe".into(),
            image_url: None,
            max_rank: Some(30),
        })
        .collect();
        let prefix = "/Lotus/Types/StoreItems/Packages/MegaPrimeVault/MPV";
        assert_eq!(
            localized_bundle_name(&format!("{prefix}BansheePrimeSinglePack"), &items).as_deref(),
            Some("Набор «Банши Прайм»")
        );
        assert_eq!(
            localized_bundle_name(&format!("{prefix}BansheeMiragePrimeDualPack"), &items)
                .as_deref(),
            Some("Двойной набор: Банши Прайм и Мираж Прайм")
        );
        assert!(
            localized_bundle_name(&format!("{prefix}UnknownPrimeSinglePack"), &items).is_none()
        );
    }

    #[tokio::test]
    #[ignore = "Проверка текущего публичного WorldState; требует сети"]
    async fn live_world_activity_works_without_game_or_account() {
        let database = Mutex::new(Database::open_in_memory().unwrap());
        let service = WorldActivityService::production().unwrap();
        let view = service.view(&database, true).await.unwrap();
        assert_eq!(view.snapshot.cycles.len(), 5);
        assert!(!view.refresh_failed);
        assert!(view.snapshot.baro.is_some());
        assert!(view.snapshot.resurgence.is_some());
        assert!(view.snapshot.steel_path.is_some());
        assert!(view.snapshot.sortie.is_some());
        assert!(
            database
                .lock()
                .unwrap()
                .get_setting::<WorldActivitySnapshot>(CACHE_KEY)
                .unwrap()
                .is_some()
        );
        println!(
            "WorldState: 5 циклов, Баро, Варзия, Тешин, вылазка; кэш сохранён. sourceAt={}",
            view.snapshot.source_at
        );
    }
}
