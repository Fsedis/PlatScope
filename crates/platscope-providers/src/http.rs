use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header};

use crate::{MAX_BULK_RESPONSE_BYTES, ProviderError, ProviderErrorCode};

#[derive(Clone)]
pub struct BoundedHttpClient {
    client: Client,
}

impl BoundedHttpClient {
    /// Создаёт общий HTTP client с конечными таймаутами и идентифицируемым User-Agent.
    ///
    /// # Errors
    ///
    /// Возвращает provider error, если TLS/client configuration нельзя построить.
    pub fn new() -> Result<Self, ProviderError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .user_agent("PlatScope/0.1 desktop")
            .build()
            .map_err(|error| map_reqwest_error(&error))?;
        Ok(Self { client })
    }

    /// Загружает ответ в память с проверкой объявленного и фактического размера.
    ///
    /// # Errors
    ///
    /// Возвращает типизированную transport/HTTP/schema ошибку.
    pub async fn get_json(
        &self,
        url: &str,
        allow_text_plain: bool,
    ) -> Result<Vec<u8>, ProviderError> {
        self.get_json_with_limit(url, allow_text_plain, MAX_BULK_RESPONSE_BYTES)
            .await
    }

    /// Загружает JSON с отдельным ограничением размера для заведомо крупных справочников.
    ///
    /// # Errors
    ///
    /// Возвращает типизированную transport/HTTP/schema ошибку или ошибку превышения лимита.
    pub async fn get_json_with_limit(
        &self,
        url: &str,
        allow_text_plain: bool,
        max_response_bytes: usize,
    ) -> Result<Vec<u8>, ProviderError> {
        if max_response_bytes == 0 {
            return Err(ProviderError::validation(
                "HTTP response size limit must be greater than zero",
            ));
        }
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| map_reqwest_error(&error))?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(ProviderError::new(
                ProviderErrorCode::NotPublished,
                "resource not published (HTTP 404)",
                true,
            ));
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::new(
                ProviderErrorCode::RateLimited,
                "provider rate limit reached",
                true,
            ));
        }
        if !status.is_success() {
            return Err(ProviderError::new(
                ProviderErrorCode::Unavailable,
                format!("provider returned HTTP {status}"),
                status.is_server_error(),
            ));
        }

        if response
            .content_length()
            .is_some_and(|size| size > max_response_bytes as u64)
        {
            let limit_mib = max_response_bytes / (1024 * 1024);
            return Err(ProviderError::new(
                ProviderErrorCode::ResponseTooLarge,
                format!("declared response exceeds {limit_mib} MiB"),
                false,
            ));
        }

        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let valid_type = content_type.contains("application/json")
            || content_type.contains("+json")
            || (allow_text_plain && content_type.contains("text/plain"));
        if !valid_type {
            return Err(ProviderError::schema_changed(format!(
                "unexpected content type: {content_type}"
            )));
        }

        let declared_size = response
            .content_length()
            .unwrap_or_default()
            .min(max_response_bytes as u64);
        let mut body =
            Vec::with_capacity(usize::try_from(declared_size).unwrap_or(max_response_bytes));
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| map_reqwest_error(&error))?;
            if body.len().saturating_add(chunk.len()) > max_response_bytes {
                let limit_mib = max_response_bytes / (1024 * 1024);
                return Err(ProviderError::new(
                    ProviderErrorCode::ResponseTooLarge,
                    format!("response exceeds {limit_mib} MiB"),
                    false,
                ));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

fn map_reqwest_error(error: &reqwest::Error) -> ProviderError {
    let code = if error.is_timeout() {
        ProviderErrorCode::Timeout
    } else {
        ProviderErrorCode::Unavailable
    };
    ProviderError::new(code, error.to_string(), true)
}
