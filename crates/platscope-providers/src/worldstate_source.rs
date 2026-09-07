//! Общий источник публичного состояния игры. При сбое WFCD читаем DE напрямую.
//! Формат дат, циклы и сопоставление таблиц сверены с WFCD worldstate-parser.
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::{BoundedHttpClient, ProviderError, ProviderErrorCode};

const SOURCE_TTL: Duration = Duration::from_secs(30);
const PRIMARY_RETRY: Duration = Duration::from_secs(300);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);
const REFERENCE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Default)]
struct SourceCache {
    document: Option<(Instant, Value)>,
    primary_failed_at: Option<Instant>,
    failure: Option<(Instant, ProviderErrorCode)>,
    references: HashMap<&'static str, (Instant, Value)>,
}

static SOURCE_CACHE: OnceLock<Mutex<SourceCache>> = OnceLock::new();

fn timed_out() -> ProviderError {
    ProviderError::new(
        ProviderErrorCode::Timeout,
        "public worldstate request timed out",
        true,
    )
}

fn valid_source_time(
    time: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, ProviderError> {
    time.filter(|time| {
        *time <= now + chrono::Duration::minutes(5) && *time >= now - chrono::Duration::minutes(20)
    })
    .ok_or_else(|| ProviderError::schema_changed("worldstate server timestamp is missing or stale"))
}

fn validate_parsed(document: &Value, now: DateTime<Utc>) -> Result<(), ProviderError> {
    valid_source_time(
        document["timestamp"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc)),
        now,
    )?;
    if !document["syndicateMissions"].is_array() || !document["cetusCycle"].is_object() {
        return Err(ProviderError::schema_changed(
            "worldstate has no missions or cycles",
        ));
    }
    Ok(())
}

/// Один запрос и короткий общий кэш для заказов, циклов и публичных продавцов.
pub(crate) async fn fetch_worldstate(client: &BoundedHttpClient) -> Result<Value, ProviderError> {
    let mut cache = SOURCE_CACHE
        .get_or_init(|| Mutex::new(SourceCache::default()))
        .lock()
        .await;
    if let Some((at, document)) = &cache.document
        && at.elapsed() < SOURCE_TTL
    {
        return Ok(document.clone());
    }
    if let Some((at, code)) = cache.failure
        && at.elapsed() < SOURCE_TTL
    {
        return Err(ProviderError::new(
            code,
            "public worldstate sources unavailable; retry shortly",
            true,
        ));
    }
    if cache
        .primary_failed_at
        .is_none_or(|at| at.elapsed() >= PRIMARY_RETRY)
    {
        let response = tokio::time::timeout(
            REQUEST_TIMEOUT,
            client.get_json_with_limit(
                "https://api.warframestat.us/pc?language=en",
                false,
                4 * 1024 * 1024,
            ),
        )
        .await;
        let parsed = match response {
            Ok(Ok(body)) => serde_json::from_slice::<Value>(&body)
                .map_err(|_| ProviderError::schema_changed("invalid worldstate JSON")),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(timed_out()),
        }
        .and_then(|document| {
            validate_parsed(&document, Utc::now())?;
            Ok(document)
        });
        match parsed {
            Ok(document) => {
                cache.primary_failed_at = None;
                cache.failure = None;
                cache.document = Some((Instant::now(), document.clone()));
                return Ok(document);
            }
            Err(error) => {
                tracing::warn!(event = "worldstate_primary_failed", source = "warframestat.us", code = ?error.code,
                    "public worldstate unavailable; trying Digital Extremes");
                cache.primary_failed_at = Some(Instant::now());
            }
        }
    }
    let response = tokio::time::timeout(REQUEST_TIMEOUT, client.get_game_worldstate()).await;
    let raw = match response {
        Ok(Ok(body)) => serde_json::from_slice::<Value>(&body)
            .map_err(|_| ProviderError::schema_changed("invalid DE worldstate JSON")),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(timed_out()),
    }
    .and_then(|raw| {
        source_time(&raw, Utc::now())?;
        Ok(raw)
    });
    let raw = match raw {
        Ok(raw) => raw,
        Err(error) => {
            tracing::warn!(event = "worldstate_direct_failed", source = "api.warframe.com", code = ?error.code,
                "direct public worldstate unavailable");
            cache.failure = Some((Instant::now(), error.code));
            return Err(error);
        }
    };
    refresh_references(client, &mut cache).await;
    // Не продлеваем срок старых таблиц при неудачной загрузке.
    let references = cache
        .references
        .iter()
        .filter(|(_, (at, _))| at.elapsed() < REFERENCE_TTL)
        .map(|(key, (_, value))| (*key, value.clone()))
        .collect();
    let document = normalize_direct(&raw, &references, Utc::now())?;
    cache.failure = None;
    cache.document = Some((Instant::now(), document.clone()));
    Ok(document)
}

async fn refresh_references(client: &BoundedHttpClient, cache: &mut SourceCache) {
    // Независимые справочники загружаем одновременно с общим пределом ожидания.
    let missing = REFERENCE_URLS
        .iter()
        .filter(|(key, _)| {
            cache
                .references
                .get(key)
                .is_none_or(|(at, _)| at.elapsed() >= REFERENCE_TTL)
        })
        .copied()
        .collect::<Vec<_>>();
    let results = futures_util::future::join_all(missing.iter().map(|(key, url)| async {
        let result = tokio::time::timeout(
            REQUEST_TIMEOUT,
            client.get_json_with_limit(url, true, 2 * 1024 * 1024),
        )
        .await;
        let parsed = match result {
            Ok(Ok(bytes)) => serde_json::from_slice::<Value>(&bytes).ok(),
            _ => None,
        };
        (*key, parsed)
    }))
    .await;
    for (key, value) in results {
        if let Some(value) = value.filter(Value::is_object) {
            cache.references.insert(key, (Instant::now(), value));
        } else {
            tracing::warn!(
                event = "worldstate_reference_failed",
                reference = key,
                "public game reference unavailable"
            );
        }
    }
}

const REFERENCE_URLS: [(&str, &str); 5] = [
    (
        "languages",
        "https://raw.githubusercontent.com/WFCD/warframe-worldstate-data/master/data/languages.json",
    ),
    (
        "steelPath",
        "https://raw.githubusercontent.com/WFCD/warframe-worldstate-data/master/data/steelPath.json",
    ),
    (
        "cetusBountyRewards",
        "https://raw.githubusercontent.com/WFCD/warframe-drop-data/gh-pages/data/cetusBountyRewards.json",
    ),
    (
        "solarisBountyRewards",
        "https://raw.githubusercontent.com/WFCD/warframe-drop-data/gh-pages/data/solarisBountyRewards.json",
    ),
    (
        "deimosRewards",
        "https://raw.githubusercontent.com/WFCD/warframe-drop-data/gh-pages/data/deimosRewards.json",
    ),
];

fn source_time(raw: &Value, now: DateTime<Utc>) -> Result<DateTime<Utc>, ProviderError> {
    valid_source_time(
        raw["Time"]
            .as_i64()
            .and_then(|time| DateTime::from_timestamp(time, 0)),
        now,
    )
}

fn date(value: &Value) -> Option<i64> {
    let date = &value["$date"];
    let millis = date["$numberLong"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| date["$numberLong"].as_i64())
        .or_else(|| date.as_i64())?;
    DateTime::from_timestamp_millis(millis)
        .filter(|time| time.timestamp() > 0)
        .map(|time| time.timestamp_millis())
}

fn iso(millis: i64) -> Value {
    DateTime::from_timestamp_millis(millis).map_or(Value::Null, |time| json!(time))
}

fn period(raw: &Value) -> Option<(i64, i64)> {
    let start = date(&raw["Activation"])?;
    let end = date(&raw["Expiry"])?;
    (end > start).then_some((start, end))
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

fn name(languages: &Value, reference: &str) -> Option<String> {
    languages
        .get(reference)
        .or_else(|| languages.get(reference.to_ascii_lowercase()))
        .and_then(|entry| entry["value"].as_str())
        .filter(|name| !name.is_empty() && name.len() <= 512)
        .map(str::to_owned)
}

fn cycle(state: &str, start: i64, end: i64) -> Value {
    json!({"state": state, "activation": iso(start), "expiry": iso(end)})
}

fn normalize_direct(
    raw: &Value,
    references: &HashMap<&str, Value>,
    now: DateTime<Utc>,
) -> Result<Value, ProviderError> {
    let time = source_time(raw, now)?;
    // Ответ CDN может немного отставать и содержать следующую ротацию заранее.
    // Выбираем опубликованный интервал по текущему времени, не продлевая старый.
    let millis = now.timestamp_millis();
    let languages = references.get("languages").unwrap_or(&Value::Null);
    let missions = array(&raw["SyndicateMissions"]);
    let current = missions
        .iter()
        .filter(|mission| {
            period(mission).is_some_and(|(start, end)| start <= millis && millis < end)
        })
        .collect::<Vec<_>>();
    let mut root = json!({"timestamp": time, "events": [], "syndicateMissions": null});
    if let Some(end) = current
        .iter()
        .find(|m| m["Tag"] == "CetusSyndicate")
        .and_then(|m| date(&m["Expiry"]))
    {
        // WFCD использует окончание заказов Цетуса как подтверждение конца ночи.
        let night_end = end / 60_000 * 60_000;
        let day = night_end - millis > 3_000_000;
        let expiry = if day {
            night_end - 3_000_000
        } else {
            night_end
        };
        let start = expiry - if day { 6_000_000 } else { 3_000_000 };
        root["cetusCycle"] = cycle(if day { "day" } else { "night" }, start, expiry);
        root["cambionCycle"] = cycle(if day { "fass" } else { "vome" }, start, expiry);
        let corpus = (end - 5000 - 1_655_182_800_000).rem_euclid(18_000_000) < 9_000_000;
        let zariman_end = ((end - 5000 + 30_000) / 60_000) * 60_000;
        root["zarimanCycle"] = cycle(
            if corpus { "corpus" } else { "grineer" },
            zariman_end - 9_000_000,
            zariman_end,
        );
    }
    // Опорные точки те же, что у WFCD. Считаем только от свежего времени сервера DE.
    let vallis_origin = 1_770_234_408_000_i64; // 2026-02-04 19:46:48 UTC
    let vallis_elapsed = (millis - vallis_origin).rem_euclid(1_600_000);
    let vallis_start = millis - vallis_elapsed;
    root["vallisCycle"] = if vallis_elapsed < 400_000 {
        cycle("warm", vallis_start, vallis_start + 400_000)
    } else {
        cycle("cold", vallis_start + 400_000, vallis_start + 1_600_000)
    };
    let duviri_elapsed = (now.timestamp() - 52).rem_euclid(36_000);
    let mood = usize::try_from(duviri_elapsed / 7200).unwrap_or(0);
    let duviri_end = (now.timestamp() + 7200 - duviri_elapsed % 7200) / 60 * 60_000;
    root["duviriCycle"] = cycle(
        ["sorrow", "fear", "joy", "anger", "envy"][mood],
        duviri_end - 7_200_000,
        duviri_end,
    );
    root["voidTrader"] = direct_trader(array(&raw["VoidTraders"]).first(), languages);
    root["vaultTrader"] = direct_trader(array(&raw["PrimeVaultTraders"]).first(), languages);
    if let Some((start, end)) = period(&raw["SeasonInfo"]) {
        root["nightwave"] = json!({"activation": iso(start), "expiry": iso(end),
            "tag": raw["SeasonInfo"]["AffiliationTag"], "season": raw["SeasonInfo"]["Season"]});
    }
    if let Some(sortie) = array(&raw["Sorties"])
        .iter()
        .find(|item| period(item).is_some_and(|(start, end)| start <= millis && millis < end))
        && let Some((start, end)) = period(sortie)
    {
        root["sortie"] = json!({"activation": iso(start), "expiry": iso(end)});
    }
    if let Some(rotation) = references
        .get("steelPath")
        .and_then(|value| value["rotation"].as_array())
        .filter(|rotation| rotation.len() == 8)
    {
        let weeks = (now.timestamp() - 1_605_484_800).div_euclid(604_800); // 2020-11-16 UTC
        let start = (now.timestamp() - 345_600).div_euclid(604_800) * 604_800 + 345_600;
        root["steelPath"] = json!({"activation": iso(start * 1000), "expiry": iso((start + 604_800) * 1000),
            "currentReward": rotation[usize::try_from(weeks.rem_euclid(8)).unwrap_or(0)]});
    }
    root["events"] = Value::Array(array(&raw["Goals"]).iter().filter_map(|event| {
        let (start, end) = period(event)?;
        let description = name(languages, event["Desc"].as_str()?)?;
        Some(json!({"id": event["_id"]["$oid"], "description": description, "activation": iso(start), "expiry": iso(end)}))
    }).collect());
    let parsed_missions = current
        .iter()
        .filter_map(|mission| direct_mission(mission, languages, references, millis))
        .collect::<Vec<_>>();
    // Не превращаем отсутствие таблиц выпадения в успешный пустой список заказов.
    if !parsed_missions.is_empty() {
        root["syndicateMissions"] = json!(parsed_missions);
    }
    Ok(root)
}

fn direct_trader(raw: Option<&Value>, languages: &Value) -> Value {
    let Some(raw) = raw else { return Value::Null };
    let Some((start, end)) = period(raw) else {
        return Value::Null;
    };
    let location = match raw["Node"].as_str().unwrap_or_default() {
        "MercuryHUB" => "Larunda Relay (Mercury)",
        "EarthHUB" => "Strata Relay (Earth)",
        "SaturnHUB" => "Kronia Relay (Saturn)",
        "PlutoHUB" => "Orcus Relay (Pluto)",
        "TradeHUB1" => "Maroo's Bazaar (Mars)",
        _ => return Value::Null,
    };
    let inventory = array(&raw["Manifest"])
        .iter()
        .map(|offer| {
            let reference = offer["ItemType"].as_str().unwrap_or_default();
            // Слой core определяет новые предметы по game_ref из более полного
            // каталога игры. Отсутствие имени у WFCD не удаляет само предложение.
            json!({"uniqueName": reference, "item": name(languages, reference).unwrap_or_else(|| "Unknown item".into()),
            "ducats": offer["PrimePrice"], "credits": offer["RegularPrice"]})
        })
        .collect::<Vec<_>>();
    json!({"activation": iso(start), "expiry": iso(end), "location": location, "inventory": inventory})
}

fn direct_mission(
    raw: &Value,
    languages: &Value,
    references: &HashMap<&str, Value>,
    now: i64,
) -> Option<Value> {
    let (start, end) = period(raw)?;
    let (syndicate, key, suffix) = match raw["Tag"].as_str()? {
        "CetusSyndicate" => ("Ostrons", "cetusBountyRewards", "Cetus Bounty"),
        "SolarisSyndicate" => (
            "Solaris United",
            "solarisBountyRewards",
            "Orb Vallis Bounty",
        ),
        "EntratiSyndicate" => ("Entrati", "deimosRewards", "Cambion Drift Bounty"),
        _ => return None,
    };
    let tables = references.get(key)?;
    let jobs = array(&raw["Jobs"]).iter().filter_map(|job| {
        let reward_path = job["rewards"].as_str()?;
        let leaf = reward_path.rsplit('/').next()?;
        let is_vault = job["isVault"].as_bool().unwrap_or(false);
        // Точная ротация из пути DE; неизвестный формат не подменяем ротацией A.
        let rotation = leaf.split_once("Table")?.1.strip_suffix("Rewards")?;
        if !matches!(rotation, "A" | "B" | "C") { return None; }
        let low = job["minEnemyLevel"].as_u64()?;
        let high = job["maxEnemyLevel"].as_u64()?;
        let variant = if is_vault { "Isolation Vault" } else if leaf.starts_with("Ghoul") { "Ghoul Bounty" } else { suffix };
        let level = format!("Level {low} - {high} {variant}");
        let rewards = array(&tables[key]).iter().find(|row| row["bountyLevel"] == level)?["rewards"][rotation].as_array()?;
        let drops = rewards.iter().map(direct_drop).collect::<Option<Vec<_>>>()?;
        if drops.is_empty() { return None; }
        let job_type = job["jobType"].as_str().unwrap_or_default();
        let title = if is_vault { format!("Isolation Vault {}", job["locationTag"].as_str().unwrap_or_default().replace("Chamber", "Chamber ")) }
            else { name(languages, job_type)? };
        let narmer = job_type.to_ascii_lowercase().contains("narmer");
        let cetus = raw["Tag"] == "CetusSyndicate";
        let expiry = if narmer && cetus { end - 3_000_000 } else { end };
        if expiry <= now || (narmer && !cetus && now < end - 3_000_000) { return None; }
        // Сохраняем идентичность WFCD: её использует точная локализация заданий.
        Some(json!({"id": format!("{}{end}", job_type.rsplit('/').next().unwrap_or_default()), "expiry": iso(expiry),
            "uniqueName": reward_path, "rewardPoolDrops": drops, "type": title,
            "enemyLevels": [low, high], "standingStages": job["xpAmounts"], "minMR": job["masteryReq"],
            "timeBound": if narmer { Some(if cetus { "day" } else { "night" }) } else { None }}))
    }).collect::<Vec<_>>();
    if jobs.is_empty() {
        return None;
    }
    Some(
        json!({"id": format!("{syndicate}:{end}"), "activation": iso(start), "expiry": iso(end),
        "syndicate": syndicate, "syndicateKey": syndicate, "jobs": jobs}),
    )
}

fn direct_drop(raw: &Value) -> Option<Value> {
    let full_name = raw["itemName"].as_str()?;
    let (count, item) = full_name
        .split_once('X')
        .and_then(|(count, item)| {
            count
                .parse::<u32>()
                .ok()
                .filter(|count| *count > 0 && *count <= 100_000)
                .map(|count| (count, item.trim()))
        })
        .unwrap_or((1, full_name));
    let chance = raw["chance"]
        .as_f64()
        .filter(|chance| chance.is_finite() && (0.0..=100.0).contains(chance))?;
    Some(json!({"item": item, "count": count, "chance": chance, "rarity": raw["rarity"]}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Value, HashMap<&'static str, Value>, DateTime<Utc>) {
        let data: Value = serde_json::from_str(include_str!(
            "../../../fixtures/worldstate/direct-2026-09-07.json"
        ))
        .unwrap();
        let refs = REFERENCE_URLS
            .iter()
            .map(|(key, _)| (*key, data["references"][key].clone()))
            .collect();
        let raw = data["worldstate"].clone();
        let now = DateTime::from_timestamp(raw["Time"].as_i64().unwrap(), 0).unwrap();
        (raw, refs, now)
    }

    #[test]
    fn direct_response_restores_current_cycles_bounties_and_traders() {
        let (raw, refs, now) = fixture();
        let parsed = normalize_direct(&raw, &refs, now).unwrap();
        let activity =
            crate::parse_world_activity(&serde_json::to_vec(&parsed).unwrap(), now).unwrap();
        assert_eq!(activity.cycles.len(), 5);
        assert!(activity.unavailable_sections.is_empty());
        assert_eq!(activity.resurgence.as_ref().unwrap().inventory.len(), 21);
        // Имена новых реликвий могут отсутствовать у WFCD, но game_ref и стоимость не теряются.
        assert!(
            activity
                .resurgence
                .unwrap()
                .inventory
                .iter()
                .any(|offer| offer.game_ref.contains("BansheeMirageVault")
                    && offer.credits == Some(1))
        );
        let missions: Vec<crate::BountyMission> =
            serde_json::from_value(parsed["syndicateMissions"].clone()).unwrap();
        assert_eq!(missions.len(), 3);
        assert!(
            missions
                .iter()
                .all(|mission| mission.activation <= now && mission.expiry > now)
        );
        assert_eq!(
            missions
                .iter()
                .map(|mission| mission.jobs.len())
                .sum::<usize>(),
            22
        );
        let job = &missions
            .iter()
            .find(|m| m.syndicate_key == "Entrati")
            .unwrap()
            .jobs[0];
        assert_eq!(job.kind, "Anomaly Retrieval");
        let expected = &refs["deimosRewards"]["deimosRewards"][0]["rewards"]["C"][0];
        assert!(
            (job.reward_pool_drops[0].chance - expected["chance"].as_f64().unwrap()).abs()
                < f64::EPSILON
        );
        assert_eq!(
            parsed["nightwave"]["tag"],
            "RadioLegionIntermission16Syndicate"
        );
    }

    #[test]
    fn cached_response_selects_published_next_rotation_after_boundary() {
        let (raw, refs, now) = fixture();
        let boundary = array(&raw["SyndicateMissions"])
            .iter()
            .find(|m| m["Tag"] == "CetusSyndicate")
            .and_then(|m| date(&m["Expiry"]))
            .unwrap();
        let current_time = DateTime::from_timestamp_millis(boundary + 1000).unwrap();
        assert!(current_time - now < chrono::Duration::minutes(20));
        let parsed = normalize_direct(&raw, &refs, current_time).unwrap();
        let missions: Vec<crate::BountyMission> =
            serde_json::from_value(parsed["syndicateMissions"].clone()).unwrap();
        assert_eq!(missions.len(), 3);
        assert!(
            missions
                .iter()
                .all(|mission| mission.activation <= current_time && mission.expiry > current_time)
        );
        assert_eq!(parsed["cetusCycle"]["state"], "day");
    }

    #[test]
    fn missing_drop_tables_do_not_become_empty_success_or_hide_cycles() {
        let (raw, _, now) = fixture();
        let parsed = normalize_direct(&raw, &HashMap::new(), now).unwrap();
        assert!(parsed["syndicateMissions"].is_null());
        assert!(parsed["cetusCycle"].is_object());
        assert!(
            serde_json::from_value::<Vec<crate::BountyMission>>(
                parsed["syndicateMissions"].clone()
            )
            .is_err()
        );
    }

    #[test]
    fn stale_documents_and_error_pages_cannot_create_fresh_cycles() {
        let (raw, refs, now) = fixture();
        assert!(normalize_direct(&raw, &refs, now + chrono::Duration::minutes(21)).is_err());
        assert!(
            normalize_direct(
                &json!({"message": "WorldState Not Found", "statusCode":404}),
                &refs,
                now
            )
            .is_err()
        );
        assert!(
            validate_parsed(
                &json!({"message": "WorldState Not Found", "statusCode":404}),
                now
            )
            .is_err()
        );
    }

    #[test]
    fn reward_count_and_probability_preserve_published_values() {
        let drop =
            direct_drop(&json!({"itemName":"2X Aya", "chance":5.88, "rarity":"Rare"})).unwrap();
        assert_eq!(
            drop,
            json!({"item":"Aya", "count":2, "chance":5.88, "rarity":"Rare"})
        );
        assert!(direct_drop(&json!({"itemName":"Aya", "chance":101})).is_none());
    }
}
