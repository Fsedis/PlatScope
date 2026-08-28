use async_trait::async_trait;
use chrono::Utc;
use platscope_domain::{ItemCatalog, NormalizedMarketSnapshot, ProviderId};

use crate::{
    BoundedHttpClient, BulkMarketProvider, ProviderError, RawMarketDump, ValidationProfile,
    normalize_market_dump, parser::extract_source_date,
};

const DEFAULT_URL: &str = "https://raw.githubusercontent.com/WyrmStudios/FrameForgePricing/main/price_history_latest.json";

pub struct FrameForgeMirrorProvider {
    http: BoundedHttpClient,
    url: String,
    validation: ValidationProfile,
}

impl FrameForgeMirrorProvider {
    /// Создаёт production mirror provider.
    ///
    /// # Errors
    ///
    /// Возвращает ошибку, если общий HTTP client нельзя инициализировать.
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            http: BoundedHttpClient::new()?,
            url: DEFAULT_URL.to_owned(),
            validation: ValidationProfile::production(),
        })
    }
}

#[async_trait]
impl BulkMarketProvider for FrameForgeMirrorProvider {
    fn id(&self) -> ProviderId {
        ProviderId::FrameForgeMirror
    }

    async fn fetch_latest(&self) -> Result<RawMarketDump, ProviderError> {
        let body = self.http.get_json(&self.url, true).await?;
        let source_date = extract_source_date(&body)?;
        Ok(RawMarketDump {
            provider: ProviderId::FrameForgeMirror,
            source_date,
            fetched_at: Utc::now(),
            body,
        })
    }

    fn normalize(
        &self,
        dump: &RawMarketDump,
        catalog: &ItemCatalog,
    ) -> Result<NormalizedMarketSnapshot, ProviderError> {
        normalize_market_dump(dump, catalog, self.validation)
    }
}
