//! Публичные события игры. Секции проверяются независимо: сбой одного продавца
//! не должен убирать циклы остальных локаций.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{BoundedHttpClient, ProviderError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPeriod {
    pub activation: DateTime<Utc>,
    pub expiry: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityCycle {
    pub key: String,
    pub state: String,
    #[serde(flatten)]
    pub period: ActivityPeriod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityOffer {
    pub game_ref: String,
    pub name: String,
    pub ducats: Option<u64>,
    pub credits: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityTrader {
    #[serde(flatten)]
    pub period: ActivityPeriod,
    pub location: String,
    pub inventory: Vec<ActivityOffer>,
    pub inventory_incomplete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySteelPath {
    #[serde(flatten)]
    pub period: ActivityPeriod,
    pub reward: String,
    pub cost: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub period: ActivityPeriod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldActivitySnapshot {
    pub fetched_at: DateTime<Utc>,
    pub source_at: DateTime<Utc>,
    pub cycles: Vec<ActivityCycle>,
    pub baro: Option<ActivityTrader>,
    pub resurgence: Option<ActivityTrader>,
    pub steel_path: Option<ActivitySteelPath>,
    pub sortie: Option<ActivityPeriod>,
    pub events: Vec<ActivityEvent>,
    pub unavailable_sections: Vec<String>,
}

pub struct WorldActivityProvider {
    client: BoundedHttpClient,
}

impl WorldActivityProvider {
    /// # Errors
    /// Возвращает ошибку создания HTTP-клиента.
    pub fn production() -> Result<Self, ProviderError> {
        Ok(Self {
            client: BoundedHttpClient::new()?,
        })
    }

    /// Один запрос для всего экрана; Warframe Market здесь не вызывается.
    ///
    /// # Errors
    /// Возвращает ошибку сети либо несовместимого корневого документа.
    pub async fn fetch(&self) -> Result<WorldActivitySnapshot, ProviderError> {
        let body = self
            .client
            .get_json_with_limit(
                "https://api.warframestat.us/pc?language=en",
                false,
                4 * 1024 * 1024,
            )
            .await?;
        parse_world_activity(&body, Utc::now())
    }
}

fn string(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 512)
        .map(|s| s.trim().to_owned())
}

fn timestamp(value: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

fn period(value: &Value) -> Option<ActivityPeriod> {
    let activation = timestamp(&value["activation"])?;
    let expiry = timestamp(&value["expiry"])?;
    (expiry > activation && activation.timestamp() > 0)
        .then_some(ActivityPeriod { activation, expiry })
}

fn trader(value: &Value, resurgence: bool) -> Option<ActivityTrader> {
    let raw_inventory = value["inventory"].as_array()?;
    let inventory: Vec<_> = raw_inventory
        .iter()
        .take(500)
        .filter_map(|item| {
            let ducats = item["ducats"]
                .as_u64()
                .filter(|cost| *cost <= 1_000_000_000);
            let credits = item["credits"]
                .as_u64()
                .filter(|cost| *cost <= 1_000_000_000);
            if (!item["ducats"].is_null() && ducats.is_none())
                || (!item["credits"].is_null() && credits.is_none())
                || (!resurgence && (ducats.is_none() || credits.is_none()))
            {
                return None;
            }
            if ducats.is_none() && credits.is_none() {
                return None;
            }
            Some(ActivityOffer {
                game_ref: string(&item["uniqueName"])?.replacen("/Lotus/StoreItems/", "/Lotus/", 1),
                name: string(&item["item"])?,
                ducats,
                credits,
            })
        })
        .collect();
    Some(ActivityTrader {
        period: period(value)?,
        location: string(&value["location"])?,
        inventory_incomplete: inventory.len() != raw_inventory.len(),
        inventory,
    })
}

/// Разбирает известные поля без зависимости от остальных полей большого `WorldState`.
///
/// # Errors
/// Возвращает ошибку схемы, если источник не передал время или ни одной известной секции.
pub fn parse_world_activity(
    body: &[u8],
    fetched_at: DateTime<Utc>,
) -> Result<WorldActivitySnapshot, ProviderError> {
    let root: Value = serde_json::from_slice(body)
        .map_err(|error| ProviderError::schema_changed(error.to_string()))?;
    let source_at = timestamp(&root["timestamp"])
        .filter(|time| *time <= fetched_at + chrono::Duration::minutes(5))
        .ok_or_else(|| {
            ProviderError::schema_changed("worldstate timestamp is missing or invalid")
        })?;
    let mut unavailable = Vec::new();
    let mut cycles = Vec::new();
    for (key, field, states) in [
        ("cetus", "cetusCycle", &["day", "night"][..]),
        ("vallis", "vallisCycle", &["warm", "cold"][..]),
        ("cambion", "cambionCycle", &["fass", "vome"][..]),
        ("zariman", "zarimanCycle", &["corpus", "grineer"][..]),
        (
            "duviri",
            "duviriCycle",
            &["sorrow", "fear", "joy", "anger", "envy"][..],
        ),
    ] {
        let value = &root[field];
        let cycle = period(value)
            .zip(string(&value["state"]))
            .filter(|(period, state)| {
                states.contains(&state.as_str())
                    && (period.expiry - period.activation).num_hours() <= 6
            })
            .map(|(period, state)| ActivityCycle {
                key: key.into(),
                state,
                period,
            });
        if let Some(cycle) = cycle {
            cycles.push(cycle);
        } else {
            unavailable.push(key.into());
        }
    }
    let baro = trader(&root["voidTrader"], false);
    let resurgence = trader(&root["vaultTrader"], true);
    let steel_path = period(&root["steelPath"])
        .zip(string(&root["steelPath"]["currentReward"]["name"]))
        .zip(root["steelPath"]["currentReward"]["cost"].as_u64())
        .map(|((period, reward), cost)| ActivitySteelPath {
            period,
            reward,
            cost,
        });
    let sortie = period(&root["sortie"]);
    for (key, missing) in [
        ("baro", baro.is_none()),
        ("resurgence", resurgence.is_none()),
        ("steel_path", steel_path.is_none()),
        ("sortie", sortie.is_none()),
    ] {
        if missing {
            unavailable.push(key.into());
        }
    }
    let raw_events = root["events"].as_array();
    let events: Vec<_> = raw_events
        .into_iter()
        .flatten()
        .take(100)
        .filter_map(|event| {
            Some(ActivityEvent {
                id: string(&event["id"])?,
                name: string(&event["description"])?,
                period: period(event)?,
            })
        })
        .collect();
    if raw_events.is_none_or(|raw| raw.len() != events.len()) {
        unavailable.push("events".into());
    }
    if cycles.is_empty()
        && baro.is_none()
        && resurgence.is_none()
        && steel_path.is_none()
        && sortie.is_none()
    {
        return Err(ProviderError::schema_changed(
            "worldstate contains no valid activity sections",
        ));
    }
    Ok(WorldActivitySnapshot {
        fetched_at,
        source_at,
        cycles,
        baro,
        resurgence,
        steel_path,
        sortie,
        events,
        unavailable_sections: unavailable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Value {
        json!({"timestamp":"2026-09-05T09:45:00Z", "cetusCycle": {
            "activation":"2026-09-05T09:20:00Z","expiry":"2026-09-05T11:00:00Z","state":"day"
        }, "events":[], "vaultTrader": {"activation":"2026-09-03T18:00:00Z",
            "expiry":"2026-10-01T18:00:00Z","location":"Maroo's Bazaar (Mars)","inventory":[
                {"uniqueName":"/Lotus/StoreItems/Powersuits/Banshee/BansheePrime","item":"Banshee Prime","ducats":3,"credits":null},
                {"uniqueName":"/Lotus/StoreItems/Types/Game/Projections/TestBronze","item":"Test Relic","ducats":null,"credits":1}
            ]}})
    }
    fn parse(value: &Value) -> WorldActivitySnapshot {
        parse_world_activity(
            &serde_json::to_vec(value).unwrap(),
            timestamp(&value["timestamp"]).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn partial_response_keeps_cycles_and_does_not_turn_aya_into_zero_ducats() {
        let result = parse(&fixture());
        assert_eq!(result.cycles.len(), 1);
        assert!(result.unavailable_sections.contains(&"vallis".into()));
        let trader = result.resurgence.unwrap();
        assert_eq!(
            trader.inventory[0].game_ref,
            "/Lotus/Powersuits/Banshee/BansheePrime"
        );
        assert_eq!(trader.inventory[0].credits, None);
        assert_eq!(trader.inventory[1].ducats, None);
        assert!(!trader.inventory_incomplete);
    }
    #[test]
    fn rejects_unknown_state_and_reversed_period() {
        let mut value = fixture();
        value["cetusCycle"]["state"] = json!("new_state");
        assert!(parse(&value).cycles.is_empty());
        value["cetusCycle"]["state"] = json!("day");
        value["cetusCycle"]["expiry"] = json!("2026-09-05T08:00:00Z");
        assert!(parse(&value).cycles.is_empty());
    }
    #[test]
    fn broken_offer_does_not_hide_trader_but_is_reported() {
        let mut value = fixture();
        value["vaultTrader"]["inventory"][0]["ducats"] = json!(-4);
        let trader = parse(&value).resurgence.unwrap();
        assert!(trader.inventory_incomplete);
        assert_eq!(trader.inventory.len(), 1);
    }
    #[test]
    fn rejects_empty_or_unstamped_document() {
        assert!(parse_world_activity(b"{}", Utc::now()).is_err());
        let mut value = fixture();
        value["timestamp"] = json!("not-a-date");
        assert!(parse_world_activity(&serde_json::to_vec(&value).unwrap(), Utc::now()).is_err());
    }
}
