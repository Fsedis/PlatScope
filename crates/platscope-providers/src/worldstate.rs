use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{BoundedHttpClient, ProviderError};

const WORLDSTATE_BASE_URL: &str = "https://api.warframestat.us/pc";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoidTraderItem {
    #[serde(alias = "item")]
    pub name: String,
    pub ducats: u32,
    pub credits: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoidTraderState {
    pub activation: DateTime<Utc>,
    pub expiry: DateTime<Utc>,
    pub location: String,
    pub inventory: Vec<VoidTraderItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NightwaveState {
    pub activation: DateTime<Utc>,
    pub expiry: DateTime<Utc>,
    pub season: u32,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteelPathReward {
    pub name: String,
    pub cost: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteelPathState {
    pub activation: DateTime<Utc>,
    pub expiry: DateTime<Utc>,
    pub current_reward: SteelPathReward,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyMarketState {
    pub fetched_at: DateTime<Utc>,
    pub void_trader: Option<VoidTraderState>,
    pub nightwave: Option<NightwaveState>,
    pub steel_path: Option<SteelPathState>,
    pub unavailable_sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BountyRewardDrop {
    pub item: String,
    pub rarity: String,
    pub chance: f64,
    #[serde(default = "one")]
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BountyJob {
    pub id: String,
    pub expiry: DateTime<Utc>,
    pub unique_name: String,
    #[serde(default)]
    pub reward_pool_drops: Vec<BountyRewardDrop>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub enemy_levels: Vec<u16>,
    #[serde(default)]
    pub standing_stages: Vec<u32>,
    #[serde(default)]
    pub min_mr: u8,
    pub time_bound: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BountyMission {
    pub id: String,
    pub activation: DateTime<Utc>,
    pub expiry: DateTime<Utc>,
    pub syndicate: String,
    pub syndicate_key: String,
    #[serde(default)]
    pub jobs: Vec<BountyJob>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BountyState {
    pub fetched_at: DateTime<Utc>,
    pub missions: Vec<BountyMission>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VoidTraderTransport {
    activation: DateTime<Utc>,
    expiry: DateTime<Utc>,
    location: String,
    #[serde(default)]
    inventory: Vec<VoidTraderItem>,
}

#[derive(Debug, Deserialize)]
struct NightwaveTransport {
    activation: DateTime<Utc>,
    expiry: DateTime<Utc>,
    season: u32,
    tag: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SteelPathTransport {
    activation: DateTime<Utc>,
    expiry: DateTime<Utc>,
    current_reward: SteelPathReward,
}

const fn one() -> u32 {
    1
}

#[derive(Clone)]
pub struct WarframeWorldstateProvider {
    client: BoundedHttpClient,
    base_url: String,
}

impl WarframeWorldstateProvider {
    /// Создаёт ограниченный HTTP-клиент для публичного Warframe worldstate.
    ///
    /// # Errors
    ///
    /// Возвращает [`ProviderError`], если HTTP-клиент нельзя инициализировать.
    pub fn production() -> Result<Self, ProviderError> {
        Ok(Self {
            client: BoundedHttpClient::new()?,
            base_url: WORLDSTATE_BASE_URL.to_owned(),
        })
    }

    /// Загружает независимые ежедневные источники. Ошибка одного источника не скрывает остальные.
    ///
    /// # Errors
    ///
    /// Этот метод возвращает только ошибки построения общего состояния; сетевые ошибки отдельных
    /// маршрутов перечисляются в `unavailable_sources`.
    pub async fn fetch(&self) -> Result<DailyMarketState, ProviderError> {
        let void_trader = self.fetch_void_trader();
        let nightwave = self.fetch_nightwave();
        let steel_path = self.fetch_steel_path();
        let (void_trader, nightwave, steel_path) = tokio::join!(void_trader, nightwave, steel_path);
        let mut unavailable_sources = Vec::new();
        let void_trader = void_trader
            .map_err(|_| unavailable_sources.push("void_trader".to_owned()))
            .ok();
        let nightwave = nightwave
            .map_err(|_| unavailable_sources.push("nightwave".to_owned()))
            .ok();
        let steel_path = steel_path
            .map_err(|_| unavailable_sources.push("steel_path".to_owned()))
            .ok();
        Ok(DailyMarketState {
            fetched_at: Utc::now(),
            void_trader,
            nightwave,
            steel_path,
            unavailable_sources,
        })
    }

    /// Загружает открытые заказы вместе с наградами и шансами по этапам.
    ///
    /// # Errors
    ///
    /// Возвращает [`ProviderError`] при сетевой ошибке или изменении схемы Worldstate.
    pub async fn fetch_bounties(&self) -> Result<BountyState, ProviderError> {
        let mut missions: Vec<BountyMission> =
            self.fetch_json("syndicateMissions?language=en").await?;
        missions.retain(|mission| !mission.jobs.is_empty());
        for mission in &mut missions {
            mission.jobs.retain(|job| {
                !job.reward_pool_drops.is_empty()
                    && job.enemy_levels.len() >= 2
                    && job.reward_pool_drops.iter().all(|drop| {
                        !drop.item.trim().is_empty()
                            && drop.item.len() <= 256
                            && drop.count > 0
                            && drop.count <= 100_000
                            && drop.chance.is_finite()
                            && (0.0..=100.0).contains(&drop.chance)
                    })
            });
        }
        missions.retain(|mission| !mission.jobs.is_empty());
        Ok(BountyState {
            fetched_at: Utc::now(),
            missions,
        })
    }

    async fn fetch_void_trader(&self) -> Result<VoidTraderState, ProviderError> {
        let transport: VoidTraderTransport = self.fetch_json("voidTrader?language=en").await?;
        Ok(VoidTraderState {
            activation: transport.activation,
            expiry: transport.expiry,
            location: transport.location,
            inventory: transport.inventory,
        })
    }

    async fn fetch_nightwave(&self) -> Result<NightwaveState, ProviderError> {
        let transport: NightwaveTransport = self.fetch_json("nightwave?language=en").await?;
        Ok(NightwaveState {
            activation: transport.activation,
            expiry: transport.expiry,
            season: transport.season,
            tag: transport.tag,
        })
    }

    async fn fetch_steel_path(&self) -> Result<SteelPathState, ProviderError> {
        let transport: SteelPathTransport = self.fetch_json("steelPath?language=en").await?;
        Ok(SteelPathState {
            activation: transport.activation,
            expiry: transport.expiry,
            current_reward: transport.current_reward,
        })
    }

    async fn fetch_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
    ) -> Result<T, ProviderError> {
        let url = format!("{}/{path}", self.base_url.trim_end_matches('/'));
        let body = self.client.get_json(&url, false).await?;
        serde_json::from_slice(&body).map_err(|error| {
            ProviderError::schema_changed(format!("invalid worldstate JSON for {path}: {error}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_accepts_real_void_trader_shape() {
        let parsed: VoidTraderTransport = serde_json::from_str(
            r#"{"activation":"2026-09-04T13:00:00Z","expiry":"2026-09-06T13:00:00Z","location":"Strata Relay (Earth)","inventory":[{"item":"Primed Flow","ducats":350,"credits":110000}]}"#,
        )
        .expect("shape is accepted");
        assert_eq!(parsed.inventory[0].name, "Primed Flow");
        assert_eq!(parsed.inventory[0].ducats, 350);
    }

    #[test]
    fn transport_accepts_real_steel_path_shape() {
        let parsed: SteelPathTransport = serde_json::from_str(
            r#"{"currentReward":{"name":"Rifle Riven Mod","cost":75},"activation":"2026-08-31T00:00:00Z","expiry":"2026-09-06T23:59:59Z","rotation":[],"evergreens":[]}"#,
        )
        .expect("shape is accepted");
        assert_eq!(parsed.current_reward.cost, 75);
    }

    #[test]
    fn bounty_transport_accepts_reward_probabilities() {
        let missions: Vec<BountyMission> = serde_json::from_str(
            r#"[{"id":"cetus","activation":"2026-09-01T20:00:00Z","expiry":"2026-09-01T22:00:00Z","syndicate":"Ostrons","syndicateKey":"Ostrons","jobs":[{"id":"tier-a","expiry":"2026-09-01T22:00:00Z","uniqueName":"/Lotus/Test","rewardPoolDrops":[{"item":"Aya","rarity":"Rare","chance":5.88,"count":1}],"type":"Test bounty","enemyLevels":[10,30],"standingStages":[500,500,1000],"minMR":0}]}]"#,
        )
        .expect("shape is accepted");
        assert_eq!(missions[0].jobs[0].reward_pool_drops[0].item, "Aya");
        assert_eq!(missions[0].jobs[0].reward_pool_drops[0].count, 1);
        assert_eq!(missions[0].jobs[0].enemy_levels, [10, 30]);
    }
}
