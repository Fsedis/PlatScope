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
}
