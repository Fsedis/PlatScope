use async_trait::async_trait;
use chrono::{Duration, NaiveDate, Utc};
use platscope_domain::{ItemCatalog, NormalizedMarketSnapshot, ProviderId};

use crate::{
    BoundedHttpClient, BulkMarketProvider, HistoricalMarketProvider, MetadataProvider,
    ProviderError, ProviderErrorCode, RawMarketDump, RawMetadataCatalog, ValidationProfile,
    normalize_catalog, normalize_market_dump,
};

const DEFAULT_BASE_URL: &str = "https://www.relics.run/history";
const HISTORY_LOOKBACK_DAYS: i64 = 5;

pub struct RelicsRunProvider {
    http: BoundedHttpClient,
    base_url: String,
    validation: ValidationProfile,
}

#[async_trait]
impl HistoricalMarketProvider for RelicsRunProvider {
    fn id(&self) -> ProviderId {
        ProviderId::RelicsRun
    }

    async fn fetch_day(&self, date: NaiveDate) -> Result<RawMarketDump, ProviderError> {
        self.fetch_for_date(date).await
    }

    fn normalize_history(
        &self,
        dump: &RawMarketDump,
        catalog: &ItemCatalog,
    ) -> Result<NormalizedMarketSnapshot, ProviderError> {
        normalize_market_dump(dump, catalog, ValidationProfile::historical())
    }
}

impl RelicsRunProvider {
    /// Создаёт production provider.
    ///
    /// # Errors
    ///
    /// Возвращает ошибку, если общий HTTP client нельзя инициализировать.
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            http: BoundedHttpClient::new()?,
            base_url: DEFAULT_BASE_URL.to_owned(),
            validation: ValidationProfile::production(),
        })
    }

    async fn fetch_for_date(&self, date: NaiveDate) -> Result<RawMarketDump, ProviderError> {
        let url = format!("{}/price_history_{date}.json", self.base_url);
        let body = self.http.get_json(&url, false).await?;
        Ok(RawMarketDump {
            provider: ProviderId::RelicsRun,
            source_date: date,
            fetched_at: Utc::now(),
            body,
        })
    }
}

#[async_trait]
impl BulkMarketProvider for RelicsRunProvider {
    fn id(&self) -> ProviderId {
        ProviderId::RelicsRun
    }

    async fn fetch_latest(&self) -> Result<RawMarketDump, ProviderError> {
        let today = Utc::now().date_naive();
        let mut last_error = None;
        for offset in 0..=HISTORY_LOOKBACK_DAYS {
            let date = today - Duration::days(offset);
            match self.fetch_for_date(date).await {
                Ok(dump) => return Ok(dump),
                Err(error) if error.code == ProviderErrorCode::NotPublished => {
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| ProviderError::validation("no candidate history dates")))
    }

    fn normalize(
        &self,
        dump: &RawMarketDump,
        catalog: &ItemCatalog,
    ) -> Result<NormalizedMarketSnapshot, ProviderError> {
        normalize_market_dump(dump, catalog, self.validation)
    }
}

pub struct RelicsRunCatalogProvider {
    http: BoundedHttpClient,
    base_url: String,
    validation: ValidationProfile,
}

impl RelicsRunCatalogProvider {
    /// Создаёт production catalog provider.
    ///
    /// # Errors
    ///
    /// Возвращает ошибку, если общий HTTP client нельзя инициализировать.
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            http: BoundedHttpClient::new()?,
            base_url: DEFAULT_BASE_URL.to_owned(),
            validation: ValidationProfile::production(),
        })
    }
}

#[async_trait]
impl MetadataProvider for RelicsRunCatalogProvider {
    fn id(&self) -> ProviderId {
        ProviderId::RelicsRun
    }

    async fn load_metadata(&self) -> Result<RawMetadataCatalog, ProviderError> {
        let url = format!("{}/item_data/items.json", self.base_url);
        let body = self.http.get_json(&url, false).await?;
        Ok(RawMetadataCatalog {
            provider: ProviderId::RelicsRun,
            fetched_at: Utc::now(),
            body,
        })
    }

    fn normalize_metadata(
        &self,
        catalog: &RawMetadataCatalog,
    ) -> Result<ItemCatalog, ProviderError> {
        normalize_catalog(catalog, self.validation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "требует доступ к production relics.run"]
    async fn historical_archive_older_than_ten_days_normalizes() {
        let catalog_provider = RelicsRunCatalogProvider::new().expect("catalog provider");
        let raw_catalog = catalog_provider
            .load_metadata()
            .await
            .expect("catalog downloads");
        let catalog = catalog_provider
            .normalize_metadata(&raw_catalog)
            .expect("catalog normalizes");
        let history_provider = RelicsRunProvider::new().expect("history provider");
        let source_date = NaiveDate::from_ymd_opt(2026, 8, 16).expect("valid archive date");
        let dump = history_provider
            .fetch_day(source_date)
            .await
            .expect("old archive downloads");
        let snapshot = history_provider
            .normalize_history(&dump, &catalog)
            .expect("old archive normalizes");

        assert_eq!(snapshot.metadata.source_date, source_date);
        assert!(snapshot.metadata.record_count >= 9_000);
    }
}
