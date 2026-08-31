use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use platscope_domain::{
    ItemCatalog, LiveOrder, LiveOrderBook, LiveOrderSide, MarketVariantKey, Platform, ProviderId,
    UserStatus,
};
use reqwest::{Client, StatusCode, header};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};

use crate::{
    LiveMarketProvider, MetadataProvider, ProviderError, ProviderErrorCode, RawMetadataCatalog,
    ValidationProfile, normalize_catalog,
};

const API_BASE: &str = "https://api.warframe.market/v2";
const MAX_LIVE_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_CATALOG_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(350);
const MAX_ATTEMPTS: u32 = 3;

pub struct WarframeMarketProvider {
    client: Client,
    last_request: Mutex<Option<Instant>>,
}

impl WarframeMarketProvider {
    /// Создаёт публичный WFM v2 client с конечными таймаутами и явным User-Agent.
    ///
    /// # Errors
    ///
    /// Возвращает [`ProviderError`], если HTTP client нельзя сконфигурировать.
    pub fn new() -> Result<Self, ProviderError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .user_agent("PlatScope/0.1.0 (+local-desktop-app)")
            .build()
            .map_err(|error| map_reqwest_error(&error))?;
        Ok(Self {
            client,
            last_request: Mutex::new(None),
        })
    }

    async fn wait_for_rate_limit(&self) {
        let mut last_request = self.last_request.lock().await;
        if let Some(previous) = *last_request {
            let elapsed = previous.elapsed();
            if let Some(wait) = MIN_REQUEST_INTERVAL.checked_sub(elapsed) {
                sleep(wait).await;
            }
        }
        *last_request = Some(Instant::now());
    }

    async fn fetch_top_body(
        &self,
        item: &MarketVariantKey,
        language: &str,
        crossplay: bool,
    ) -> Result<Vec<u8>, ProviderError> {
        let url = format!("{API_BASE}/orders/item/{}/top", item.slug);
        let query = exact_variant_query(item);
        for attempt in 0..MAX_ATTEMPTS {
            self.wait_for_rate_limit().await;
            let response = self
                .client
                .get(&url)
                .query(&query)
                .header("Language", language)
                .header("Platform", platform_name(item.platform))
                .header("Crossplay", crossplay.to_string())
                .send()
                .await
                .map_err(|error| map_reqwest_error(&error))?;
            let status = response.status();
            if status == StatusCode::TOO_MANY_REQUESTS || status.as_u16() == 509 {
                if attempt + 1 == MAX_ATTEMPTS {
                    return Err(ProviderError::new(
                        ProviderErrorCode::RateLimited,
                        format!("WFM rate limit reached after {MAX_ATTEMPTS} attempts"),
                        true,
                    ));
                }
                sleep(retry_delay(&response, attempt)).await;
                continue;
            }
            if status.is_server_error() {
                if attempt + 1 == MAX_ATTEMPTS {
                    return Err(ProviderError::new(
                        ProviderErrorCode::Unavailable,
                        format!("WFM returned HTTP {status}"),
                        true,
                    ));
                }
                sleep(backoff_with_jitter(attempt)).await;
                continue;
            }
            if !status.is_success() {
                return Err(ProviderError::new(
                    ProviderErrorCode::Unavailable,
                    format!("WFM returned HTTP {status}"),
                    false,
                ));
            }
            let content_type = response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !content_type.contains("application/json") && !content_type.contains("+json") {
                return Err(ProviderError::schema_changed(format!(
                    "unexpected WFM content type: {content_type}"
                )));
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_LIVE_RESPONSE_BYTES as u64)
            {
                return Err(ProviderError::new(
                    ProviderErrorCode::ResponseTooLarge,
                    "WFM live response exceeds 1 MiB",
                    false,
                ));
            }
            let body = response
                .bytes()
                .await
                .map_err(|error| map_reqwest_error(&error))?;
            if body.len() > MAX_LIVE_RESPONSE_BYTES {
                return Err(ProviderError::new(
                    ProviderErrorCode::ResponseTooLarge,
                    "WFM live response exceeds 1 MiB",
                    false,
                ));
            }
            return Ok(body.to_vec());
        }
        Err(ProviderError::new(
            ProviderErrorCode::Unavailable,
            "WFM retry loop ended unexpectedly",
            true,
        ))
    }

    async fn fetch_catalog_body(&self) -> Result<Vec<u8>, ProviderError> {
        let url = format!("{API_BASE}/items");
        for attempt in 0..MAX_ATTEMPTS {
            self.wait_for_rate_limit().await;
            let response = self
                .client
                .get(&url)
                .header("Language", "ru")
                .header("Platform", "pc")
                .header("Crossplay", "true")
                .send()
                .await
                .map_err(|error| map_reqwest_error(&error))?;
            let status = response.status();
            if status == StatusCode::TOO_MANY_REQUESTS || status.as_u16() == 509 {
                if attempt + 1 == MAX_ATTEMPTS {
                    return Err(ProviderError::new(
                        ProviderErrorCode::RateLimited,
                        format!("WFM rate limit reached after {MAX_ATTEMPTS} attempts"),
                        true,
                    ));
                }
                sleep(retry_delay(&response, attempt)).await;
                continue;
            }
            if status.is_server_error() {
                if attempt + 1 == MAX_ATTEMPTS {
                    return Err(ProviderError::new(
                        ProviderErrorCode::Unavailable,
                        format!("WFM returned HTTP {status}"),
                        true,
                    ));
                }
                sleep(backoff_with_jitter(attempt)).await;
                continue;
            }
            if !status.is_success() {
                return Err(ProviderError::new(
                    ProviderErrorCode::Unavailable,
                    format!("WFM returned HTTP {status}"),
                    false,
                ));
            }
            let content_type = response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !content_type.contains("application/json") && !content_type.contains("+json") {
                return Err(ProviderError::schema_changed(format!(
                    "unexpected WFM content type: {content_type}"
                )));
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_CATALOG_RESPONSE_BYTES as u64)
            {
                return Err(ProviderError::new(
                    ProviderErrorCode::ResponseTooLarge,
                    "WFM catalog response exceeds 8 MiB",
                    false,
                ));
            }
            let body = response
                .bytes()
                .await
                .map_err(|error| map_reqwest_error(&error))?;
            if body.len() > MAX_CATALOG_RESPONSE_BYTES {
                return Err(ProviderError::new(
                    ProviderErrorCode::ResponseTooLarge,
                    "WFM catalog response exceeds 8 MiB",
                    false,
                ));
            }
            return Ok(body.to_vec());
        }
        Err(ProviderError::new(
            ProviderErrorCode::Unavailable,
            "WFM retry loop ended unexpectedly",
            true,
        ))
    }
}

#[async_trait]
impl MetadataProvider for WarframeMarketProvider {
    fn id(&self) -> ProviderId {
        ProviderId::WarframeMarket
    }

    async fn load_metadata(&self) -> Result<RawMetadataCatalog, ProviderError> {
        Ok(RawMetadataCatalog {
            provider: ProviderId::WarframeMarket,
            fetched_at: Utc::now(),
            body: self.fetch_catalog_body().await?,
        })
    }

    fn normalize_metadata(
        &self,
        catalog: &RawMetadataCatalog,
    ) -> Result<ItemCatalog, ProviderError> {
        normalize_catalog(catalog, ValidationProfile::production())
    }
}

#[async_trait]
impl LiveMarketProvider for WarframeMarketProvider {
    fn id(&self) -> ProviderId {
        ProviderId::WarframeMarket
    }

    async fn fetch_orders(
        &self,
        item: &MarketVariantKey,
        language: &str,
        crossplay: bool,
    ) -> Result<LiveOrderBook, ProviderError> {
        let body = self.fetch_top_body(item, language, crossplay).await?;
        normalize_top_orders(&body, item)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    data: Option<TopOrders>,
    error: Option<Value>,
}

#[derive(Deserialize)]
struct TopOrders {
    sell: Vec<RawOrder>,
    buy: Vec<RawOrder>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawOrder {
    #[serde(rename = "type")]
    order_type: String,
    platinum: u32,
    quantity: u32,
    #[serde(default = "default_per_trade")]
    per_trade: u32,
    visible: bool,
    rank: Option<u16>,
    charges: Option<u16>,
    subtype: Option<String>,
    amber_stars: Option<u16>,
    cyan_stars: Option<u16>,
    user: RawUser,
}

#[derive(Deserialize)]
struct RawUser {
    status: String,
}

fn default_per_trade() -> u32 {
    1
}

fn normalize_top_orders(
    body: &[u8],
    key: &MarketVariantKey,
) -> Result<LiveOrderBook, ProviderError> {
    let envelope: Envelope = serde_json::from_slice(body).map_err(|error| {
        ProviderError::new(
            ProviderErrorCode::InvalidJson,
            format!("invalid WFM top-orders JSON: {error}"),
            false,
        )
    })?;
    if envelope.error.is_some() {
        return Err(ProviderError::new(
            ProviderErrorCode::Unavailable,
            "WFM returned an error envelope",
            true,
        ));
    }
    let data = envelope
        .data
        .ok_or_else(|| ProviderError::schema_changed("WFM response has no data object"))?;
    let orders = data
        .sell
        .into_iter()
        .chain(data.buy)
        .filter_map(|order| normalize_order(&order, key))
        .collect();
    Ok(LiveOrderBook {
        key: key.clone(),
        fetched_at: Utc::now(),
        orders,
    })
}

fn normalize_order(order: &RawOrder, key: &MarketVariantKey) -> Option<LiveOrder> {
    if !order.visible
        || order.platinum == 0
        || order.quantity == 0
        || order.per_trade == 0
        || order.rank != key.rank
        || order.charges != key.charges
        || order.subtype.as_deref() != key.subtype.as_deref()
        || order.amber_stars != key.amber_stars
        || order.cyan_stars != key.cyan_stars
    {
        return None;
    }
    let side = match order.order_type.as_str() {
        "sell" => LiveOrderSide::Sell,
        "buy" => LiveOrderSide::Buy,
        _ => return None,
    };
    let user_status = match order.user.status.as_str() {
        "ingame" => UserStatus::InGame,
        "online" => UserStatus::Online,
        _ => UserStatus::Offline,
    };
    Some(LiveOrder {
        side,
        platinum: order.platinum,
        quantity: order.quantity,
        per_trade: order.per_trade,
        user_status,
    })
}

fn exact_variant_query(key: &MarketVariantKey) -> Vec<(&'static str, String)> {
    let mut query = Vec::new();
    if let Some(rank) = key.rank {
        query.push(("rank", rank.to_string()));
    }
    if let Some(charges) = key.charges {
        query.push(("charges", charges.to_string()));
    }
    if let Some(subtype) = &key.subtype {
        query.push(("subtype", subtype.clone()));
    }
    if let Some(stars) = key.amber_stars {
        query.push(("amberStars", stars.to_string()));
    }
    if let Some(stars) = key.cyan_stars {
        query.push(("cyanStars", stars.to_string()));
    }
    query
}

fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Pc => "pc",
        Platform::Playstation => "ps4",
        Platform::Xbox => "xbox",
        Platform::Switch => "switch",
        Platform::Mobile => "mobile",
    }
}

fn retry_delay(response: &reqwest::Response, attempt: u32) -> Duration {
    response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map_or_else(|| backoff_with_jitter(attempt), Duration::from_secs)
}

fn backoff_with_jitter(attempt: u32) -> Duration {
    let base = 300_u64.saturating_mul(1_u64 << attempt.min(4));
    let jitter = u64::from(Utc::now().timestamp_subsec_millis() % 101);
    Duration::from_millis(base + jitter)
}

fn map_reqwest_error(error: &reqwest::Error) -> ProviderError {
    let code = if error.is_timeout() {
        ProviderErrorCode::Timeout
    } else {
        ProviderErrorCode::Unavailable
    };
    ProviderError::new(code, error.to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../../fixtures/providers/wfm_top_orders.json");

    #[test]
    fn top_orders_keep_only_exact_variant() {
        let key = MarketVariantKey::new("primed_flow", Platform::Pc, Some(10), None::<String>)
            .expect("valid key");
        let book = normalize_top_orders(FIXTURE, &key).expect("fixture normalizes");

        assert_eq!(book.orders.len(), 2);
        assert_eq!(book.orders[0].side, LiveOrderSide::Sell);
        assert_eq!(book.orders[0].platinum, 30);
        assert_eq!(book.orders[1].side, LiveOrderSide::Buy);
        assert_eq!(book.orders[1].platinum, 20);
    }

    #[test]
    fn exact_dimensions_become_query_parameters() {
        let key = MarketVariantKey::new("axi_s18_relic", Platform::Pc, Some(3), Some("radiant"))
            .expect("valid key")
            .with_charges(Some(4))
            .with_stars(Some(2), Some(1));
        assert_eq!(
            exact_variant_query(&key),
            vec![
                ("rank", "3".to_owned()),
                ("charges", "4".to_owned()),
                ("subtype", "radiant".to_owned()),
                ("amberStars", "2".to_owned()),
                ("cyanStars", "1".to_owned()),
            ]
        );
    }

    #[test]
    fn top_orders_keep_only_exact_charges() {
        let body = br#"{
          "data": {
            "sell": [
              {"type":"sell","platinum":30,"quantity":1,"perTrade":1,"visible":true,"rank":0,"charges":2,"user":{"status":"online"}},
              {"type":"sell","platinum":1,"quantity":1,"perTrade":1,"visible":true,"rank":0,"charges":1,"user":{"status":"online"}},
              {"type":"sell","platinum":2,"quantity":1,"perTrade":1,"visible":true,"rank":0,"user":{"status":"online"}}
            ],
            "buy": []
          },
          "error": null
        }"#;
        let key = MarketVariantKey::new("charged_mod", Platform::Pc, Some(0), None::<String>)
            .expect("valid key")
            .with_charges(Some(2));

        let book = normalize_top_orders(body, &key).expect("orders normalize");

        assert_eq!(book.orders.len(), 1);
        assert_eq!(book.orders[0].platinum, 30);
    }

    #[tokio::test]
    #[ignore = "hits the public WFM API"]
    async fn production_catalog_contains_russian_names_and_mod_images() {
        let provider = WarframeMarketProvider::new().expect("provider");
        let raw = provider.load_metadata().await.expect("WFM catalog");
        let catalog = provider
            .normalize_metadata(&raw)
            .expect("normalized WFM catalog");
        let primed_flow = catalog
            .items
            .iter()
            .find(|item| item.slug == "primed_flow")
            .expect("Primed Flow");

        assert_eq!(primed_flow.display_name_ru.as_deref(), Some("Поток Прайм"));
        assert!(primed_flow.thumb.is_some());
        assert!(primed_flow.thumb_ru.is_some());
    }
}
