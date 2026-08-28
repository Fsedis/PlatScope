#![forbid(unsafe_code)]

mod frameforge;
mod http;
mod parser;
mod relics_run;
mod warframe_market;
mod wfcd_metadata;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use platscope_domain::{
    GameMetadataSnapshot, ItemCatalog, LiveOrderBook, MarketVariantKey, NormalizedMarketSnapshot,
    PlayerInventory, ProviderId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use frameforge::FrameForgeMirrorProvider;
pub use http::BoundedHttpClient;
pub use parser::{ValidationProfile, normalize_catalog, normalize_market_dump};
pub use relics_run::{RelicsRunCatalogProvider, RelicsRunProvider};
pub use warframe_market::WarframeMarketProvider;
pub use wfcd_metadata::WfcdMetadataProvider;

pub const MAX_BULK_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMarketDump {
    pub provider: ProviderId,
    pub source_date: NaiveDate,
    pub fetched_at: DateTime<Utc>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMetadataCatalog {
    pub provider: ProviderId,
    pub fetched_at: DateTime<Utc>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGameMetadataDocument {
    pub name: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGameMetadataDump {
    pub fetched_at: DateTime<Utc>,
    pub documents: Vec<RawGameMetadataDocument>,
}

#[async_trait]
pub trait BulkMarketProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    async fn fetch_latest(&self) -> Result<RawMarketDump, ProviderError>;

    /// Преобразует transport dump в provider-neutral snapshot.
    ///
    /// # Errors
    ///
    /// Возвращает [`ProviderError`] при invalid JSON, schema drift или semantic validation failure.
    fn normalize(
        &self,
        dump: &RawMarketDump,
        catalog: &ItemCatalog,
    ) -> Result<NormalizedMarketSnapshot, ProviderError>;
}

#[async_trait]
pub trait LiveMarketProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    async fn fetch_orders(
        &self,
        item: &MarketVariantKey,
        language: &str,
        crossplay: bool,
    ) -> Result<LiveOrderBook, ProviderError>;
}

#[async_trait]
pub trait HistoricalMarketProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    async fn fetch_day(&self, date: NaiveDate) -> Result<RawMarketDump, ProviderError>;

    /// Нормализует один immutable daily dump в тот же provider-neutral snapshot.
    ///
    /// # Errors
    ///
    /// Возвращает [`ProviderError`] при schema drift или semantic validation failure.
    fn normalize_history(
        &self,
        dump: &RawMarketDump,
        catalog: &ItemCatalog,
    ) -> Result<NormalizedMarketSnapshot, ProviderError>;
}

#[async_trait]
pub trait InventoryProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    async fn load_inventory(&self) -> Result<PlayerInventory, ProviderError>;
}

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    async fn load_metadata(&self) -> Result<RawMetadataCatalog, ProviderError>;

    /// Нормализует каталог независимо от price snapshot.
    ///
    /// # Errors
    ///
    /// Возвращает [`ProviderError`] при несовместимой схеме или невалидных данных.
    fn normalize_metadata(
        &self,
        catalog: &RawMetadataCatalog,
    ) -> Result<ItemCatalog, ProviderError>;
}

#[async_trait]
pub trait GameMetadataProvider: Send + Sync {
    async fn fetch_latest(&self) -> Result<RawGameMetadataDump, ProviderError>;

    /// Нормализует WFCD-подобные transport documents через exact catalog identities.
    ///
    /// # Errors
    ///
    /// Возвращает [`ProviderError`] при schema drift, невалидных шансах или пустых данных.
    fn normalize(
        &self,
        dump: &RawGameMetadataDump,
        catalog: &ItemCatalog,
    ) -> Result<GameMetadataSnapshot, ProviderError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderErrorCode {
    NotPublished,
    Unavailable,
    Timeout,
    RateLimited,
    ResponseTooLarge,
    InvalidJson,
    UpstreamSchemaChanged,
    ValidationFailed,
    Cancelled,
}

#[derive(Debug, Error)]
#[error("{code:?}: {message}")]
pub struct ProviderError {
    pub code: ProviderErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl ProviderError {
    pub fn new(code: ProviderErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }

    pub fn schema_changed(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::UpstreamSchemaChanged, message, false)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::ValidationFailed, message, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_body_limit_is_32_mib() {
        assert_eq!(MAX_BULK_RESPONSE_BYTES, 33_554_432);
    }

    #[test]
    fn schema_drift_has_stable_diagnostic_code() {
        let error = ProviderError::schema_changed("root is no longer an object");
        assert_eq!(error.code, ProviderErrorCode::UpstreamSchemaChanged);
        assert!(!error.retryable);
    }
}
