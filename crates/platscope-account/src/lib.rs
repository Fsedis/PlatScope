#![forbid(unsafe_code)]

use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use reqwest::{Client, Method, Response, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use zeroize::Zeroizing;

const V1_BASE_URL: &str = "https://api.warframe.market/v1";
const V2_BASE_URL: &str = "https://api.warframe.market/v2";
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(350);
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const KEYRING_SERVICE: &str = "PlatScope.WarframeMarket";
const KEYRING_ACCOUNT: &str = "default-session";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountOrderType {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfile {
    pub id: String,
    pub ingame_name: String,
    pub slug: String,
    pub platform: String,
    pub crossplay: bool,
    pub verification: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountOrder {
    pub id: String,
    pub item_id: Option<String>,
    #[serde(rename = "type")]
    pub order_type: AccountOrderType,
    pub platinum: u32,
    pub quantity: u32,
    pub per_trade: Option<u32>,
    pub rank: Option<u16>,
    pub charges: Option<u16>,
    pub subtype: Option<String>,
    pub amber_stars: Option<u16>,
    pub cyan_stars: Option<u16>,
    pub visible: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateListingInput {
    pub item_id: String,
    #[serde(rename = "type")]
    pub order_type: AccountOrderType,
    pub platinum: u32,
    pub quantity: u32,
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_trade: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charges: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amber_stars: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cyan_stars: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateListingInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platinum: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_trade: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charges: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amber_stars: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cyan_stars: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseOrderInput {
    quantity: u32,
}

impl CreateListingInput {
    /// Проверяет documented WFM bounds до любого сетевого write.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError::Validation`] для некорректного item/price/quantity/variant.
    pub fn validate(&self) -> Result<(), AccountError> {
        validate_id("item_id", &self.item_id)?;
        validate_price_quantity(self.platinum, self.quantity, self.per_trade)?;
        validate_subtype(self.subtype.as_deref())
    }
}

impl UpdateListingInput {
    /// Проверяет PATCH и запрещает пустой write.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError::Validation`] для пустого или некорректного PATCH.
    pub fn validate(&self) -> Result<(), AccountError> {
        if self.platinum.is_none()
            && self.quantity.is_none()
            && self.visible.is_none()
            && self.per_trade.is_none()
            && self.rank.is_none()
            && self.charges.is_none()
            && self.subtype.is_none()
            && self.amber_stars.is_none()
            && self.cyan_stars.is_none()
        {
            return Err(AccountError::Validation("empty order update".into()));
        }
        if self
            .platinum
            .is_some_and(|value| !(1..=900_000).contains(&value))
        {
            return Err(AccountError::Validation(
                "platinum must be within 1..=900000".into(),
            ));
        }
        if self
            .quantity
            .is_some_and(|value| !(1..=9_999).contains(&value))
        {
            return Err(AccountError::Validation(
                "quantity must be within 1..=9999".into(),
            ));
        }
        if let (Some(quantity), Some(per_trade)) = (self.quantity, self.per_trade) {
            validate_per_trade(quantity, per_trade)?;
        } else if self
            .per_trade
            .is_some_and(|value| !(1..=6).contains(&value))
        {
            return Err(AccountError::Validation(
                "per_trade must be within 1..=6".into(),
            ));
        }
        validate_subtype(self.subtype.as_deref())
    }
}

pub struct AccountToken(Zeroizing<String>);

impl AccountToken {
    /// Создаёт redacted token после bounded validation.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError::Validation`] для пустого или слишком длинного token.
    pub fn new(value: &str) -> Result<Self, AccountError> {
        let value = value.trim().trim_start_matches("JWT ").trim().to_owned();
        if value.len() < 6 || value.len() > MAX_TOKEN_BYTES {
            return Err(AccountError::Validation(
                "authorization token has invalid length".into(),
            ));
        }
        Ok(Self(Zeroizing::new(value)))
    }

    fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for AccountToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AccountToken([REDACTED])")
    }
}

pub trait CredentialStore: Send + Sync {
    /// Загружает token из secure storage.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] при ошибке OS keychain.
    fn load(&self) -> Result<Option<AccountToken>, AccountError>;

    /// Записывает token только в secure storage.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] при ошибке OS keychain.
    fn save(&self, token: &AccountToken) -> Result<(), AccountError>;

    /// Удаляет credential из secure storage.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] при ошибке OS keychain.
    fn delete(&self) -> Result<(), AccountError>;
}

pub struct OsCredentialStore;

impl OsCredentialStore {
    fn entry() -> Result<keyring::Entry, AccountError> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
            .map_err(|error| AccountError::Credential(error.to_string()))
    }
}

impl CredentialStore for OsCredentialStore {
    fn load(&self) -> Result<Option<AccountToken>, AccountError> {
        match Self::entry()?.get_password() {
            Ok(value) => {
                let value = Zeroizing::new(value);
                AccountToken::new(value.as_str()).map(Some)
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(AccountError::Credential(error.to_string())),
        }
    }

    fn save(&self, token: &AccountToken) -> Result<(), AccountError> {
        Self::entry()?
            .set_password(token.expose())
            .map_err(|error| AccountError::Credential(error.to_string()))
    }

    fn delete(&self) -> Result<(), AccountError> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AccountError::Credential(error.to_string())),
        }
    }
}

pub struct WfmAccountClient {
    client: Client,
    request_lock: tokio::sync::Mutex<()>,
    last_request: Mutex<Option<Instant>>,
    v1_base_url: String,
    v2_base_url: String,
}

impl WfmAccountClient {
    /// Создаёт bounded client для legacy sign-in и актуальных v2 account routes.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] при невозможности построить HTTP client.
    pub fn production() -> Result<Self, AccountError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent("PlatScope/0.1 (account integration)")
            .build()?;
        Ok(Self {
            client,
            request_lock: tokio::sync::Mutex::new(()),
            last_request: Mutex::new(None),
            v1_base_url: V1_BASE_URL.into(),
            v2_base_url: V2_BASE_URL.into(),
        })
    }

    /// Выполняет одноразовый legacy v1 sign-in; password не сохраняется.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] для rejected credentials, rate limit, transport или schema drift.
    pub async fn sign_in(
        &self,
        email: &str,
        password: &str,
        device_id: &str,
    ) -> Result<AccountToken, AccountError> {
        if email.trim().is_empty()
            || email.len() > 128
            || password.is_empty()
            || password.len() > 128
        {
            return Err(AccountError::Validation("invalid sign-in fields".into()));
        }
        validate_id("device_id", device_id)?;
        let response = self
            .send(
                self.client
                    .post(format!("{}/auth/signin", self.v1_base_url))
                    .header("Authorization", "")
                    .json(&serde_json::json!({
                        "auth_type": "header",
                        "email": email.trim(),
                        "password": password,
                        "device-id": device_id,
                    })),
            )
            .await?;
        let header = response
            .headers()
            .get("authorization")
            .ok_or(AccountError::Unauthorized)?
            .to_str()
            .map_err(|_| AccountError::Schema("invalid authorization header".into()))?
            .to_owned();
        let _ = read_bounded(response).await?;
        let header = Zeroizing::new(header);
        AccountToken::new(header.as_str())
    }

    /// Читает private self-profile.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] при отсутствии авторизации, transport/API или schema error.
    pub async fn me(&self, token: &AccountToken) -> Result<AccountProfile, AccountError> {
        self.auth_json(Method::GET, "me", token, None::<&()>).await
    }

    /// Читает visible и hidden orders владельца.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] при отсутствии авторизации, transport/API или schema error.
    pub async fn my_orders(&self, token: &AccountToken) -> Result<Vec<AccountOrder>, AccountError> {
        self.auth_json(Method::GET, "orders/my", token, None::<&()>)
            .await
    }

    /// Завершает текущую server session.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] при отсутствии авторизации или rejected API request.
    pub async fn sign_out(&self, token: &AccountToken) -> Result<(), AccountError> {
        let response = self
            .send(
                self.client
                    .post(format!("{}/auth/signout", self.v2_base_url))
                    .bearer_auth(token.expose()),
            )
            .await?;
        let _ = read_bounded(response).await?;
        Ok(())
    }

    /// Создаёт order только после локальной validation.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] при invalid input, отсутствии авторизации или rejected API write.
    pub async fn create_order(
        &self,
        token: &AccountToken,
        input: &CreateListingInput,
    ) -> Result<AccountOrder, AccountError> {
        input.validate()?;
        self.auth_json(Method::POST, "order", token, Some(input))
            .await
    }

    /// Изменяет существующий order только после локальной validation.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] при invalid input, отсутствии авторизации или rejected API write.
    pub async fn update_order(
        &self,
        token: &AccountToken,
        id: &str,
        input: &UpdateListingInput,
    ) -> Result<AccountOrder, AccountError> {
        validate_id("order_id", id)?;
        input.validate()?;
        self.auth_json(Method::PATCH, &format!("order/{id}"), token, Some(input))
            .await
    }

    /// Удаляет существующий order и возвращает representation сервера.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] при invalid ID, отсутствии авторизации или rejected API write.
    pub async fn delete_order(
        &self,
        token: &AccountToken,
        id: &str,
    ) -> Result<AccountOrder, AccountError> {
        validate_id("order_id", id)?;
        self.auth_json(Method::DELETE, &format!("order/{id}"), token, None::<&()>)
            .await
    }

    /// Отмечает указанное количество ордера проданным и создаёт транзакцию WFM.
    ///
    /// В отличие от удаления ордера, `/close` попадает в историю закрытых сделок
    /// Warframe Market. При частичном закрытии сервер сохраняет ордер с остатком.
    ///
    /// # Errors
    ///
    /// Возвращает [`AccountError`] при некорректном ID/количестве, отсутствии
    /// авторизации или отклонённом API-запросе.
    pub async fn close_order(
        &self,
        token: &AccountToken,
        id: &str,
        quantity: u32,
    ) -> Result<serde_json::Value, AccountError> {
        validate_id("order_id", id)?;
        if !(1..=9_999).contains(&quantity) {
            return Err(AccountError::Validation(
                "closed quantity must be within 1..=9999".into(),
            ));
        }
        self.auth_json(
            Method::POST,
            &format!("order/{id}/close"),
            token,
            Some(&CloseOrderInput { quantity }),
        )
        .await
    }

    async fn auth_json<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        token: &AccountToken,
        body: Option<&B>,
    ) -> Result<T, AccountError> {
        let mut request = self
            .client
            .request(method, format!("{}/{path}", self.v2_base_url))
            .bearer_auth(token.expose());
        if let Some(body) = body {
            request = request.json(body);
        }
        let bytes = read_bounded(self.send(request).await?).await?;
        let envelope: ApiEnvelope<T> = serde_json::from_slice(&bytes)
            .map_err(|error| AccountError::Schema(error.to_string()))?;
        envelope.data.ok_or_else(|| {
            AccountError::Api(
                envelope
                    .error
                    .map_or_else(|| "empty WFM response".into(), |value| value.to_string()),
            )
        })
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Response, AccountError> {
        let _request_guard = self.request_lock.lock().await;
        let wait = {
            let last = self
                .last_request
                .lock()
                .map_err(|_| AccountError::State("request clock unavailable".into()))?;
            last.and_then(|instant| MIN_REQUEST_INTERVAL.checked_sub(instant.elapsed()))
        };
        if let Some(wait) = wait {
            tokio::time::sleep(wait).await;
        }
        let response = request.send().await?;
        *self
            .last_request
            .lock()
            .map_err(|_| AccountError::State("request clock unavailable".into()))? =
            Some(Instant::now());
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(AccountError::Unauthorized),
            StatusCode::TOO_MANY_REQUESTS => Err(AccountError::RateLimited),
            status if status.is_success() => Ok(response),
            status => {
                let bytes = read_bounded(response).await?;
                let detail = account_api_error_detail(&bytes)
                    .map_or_else(String::new, |value| format!(": {value}"));
                Err(AccountError::Api(format!(
                    "WFM returned HTTP {status}{detail}"
                )))
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiEnvelope<T> {
    data: Option<T>,
    error: Option<serde_json::Value>,
}

fn account_api_error_detail(bytes: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let error = value.get("error")?;
    if error.is_null() {
        return None;
    }
    let text = error
        .as_str()
        .map(str::to_owned)
        .or_else(|| serde_json::to_string(error).ok())?;
    let sanitized: String = text
        .chars()
        .filter(|character| !character.is_control())
        .take(500)
        .collect();
    (!sanitized.trim().is_empty()).then(|| sanitized.trim().to_owned())
}

async fn read_bounded(response: Response) -> Result<Vec<u8>, AccountError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(AccountError::ResponseTooLarge);
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(AccountError::ResponseTooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn validate_price_quantity(
    platinum: u32,
    quantity: u32,
    per_trade: Option<u32>,
) -> Result<(), AccountError> {
    if !(1..=900_000).contains(&platinum) {
        return Err(AccountError::Validation(
            "platinum must be within 1..=900000".into(),
        ));
    }
    if !(1..=9_999).contains(&quantity) {
        return Err(AccountError::Validation(
            "quantity must be within 1..=9999".into(),
        ));
    }
    if let Some(per_trade) = per_trade {
        validate_per_trade(quantity, per_trade)?;
    }
    Ok(())
}

fn validate_per_trade(quantity: u32, per_trade: u32) -> Result<(), AccountError> {
    if !(1..=6).contains(&per_trade) || !quantity.is_multiple_of(per_trade) {
        return Err(AccountError::Validation(
            "per_trade must be within 1..=6 and divide quantity".into(),
        ));
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), AccountError> {
    if value.trim().len() < 6 || value.len() > 256 {
        return Err(AccountError::Validation(format!("invalid {field}")));
    }
    Ok(())
}

fn validate_subtype(value: Option<&str>) -> Result<(), AccountError> {
    if value.is_some_and(|subtype| subtype.trim().is_empty() || subtype.len() > 128) {
        return Err(AccountError::Validation("invalid subtype".into()));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("account input validation failed: {0}")]
    Validation(String),
    #[error("WFM account authorization is unavailable or expired")]
    Unauthorized,
    #[error("WFM account API rate limit reached")]
    RateLimited,
    #[error("WFM account response exceeds the safe size limit")]
    ResponseTooLarge,
    #[error("WFM account API error: {0}")]
    Api(String),
    #[error("WFM account response schema changed: {0}")]
    Schema(String),
    #[error("OS secure credential storage failed: {0}")]
    Credential(String),
    #[error("account service state unavailable: {0}")]
    State(String),
    #[error("WFM account transport failed: {0}")]
    Transport(#[from] reqwest::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_debug_is_always_redacted() {
        let token = AccountToken::new("secret-token-value").expect("token");
        let debug = format!("{token:?}");
        assert_eq!(debug, "AccountToken([REDACTED])");
        assert!(!debug.contains("secret-token-value"));
    }

    #[test]
    fn create_order_enforces_documented_bounds() {
        let input = CreateListingInput {
            item_id: "valid-item-id".into(),
            order_type: AccountOrderType::Sell,
            platinum: 10,
            quantity: 12,
            visible: false,
            per_trade: Some(5),
            rank: None,
            charges: None,
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn empty_patch_is_rejected_before_network() {
        let input = UpdateListingInput {
            platinum: None,
            quantity: None,
            visible: None,
            per_trade: None,
            rank: None,
            charges: None,
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn create_order_uses_current_v2_field_names() {
        let input = CreateListingInput {
            item_id: "valid-item-id".into(),
            order_type: AccountOrderType::Sell,
            platinum: 38,
            quantity: 1,
            visible: false,
            per_trade: None,
            rank: Some(5),
            charges: None,
            subtype: Some("blueprint".into()),
            amber_stars: None,
            cyan_stars: None,
        };
        let value = serde_json::to_value(input).expect("serialize current WFM body");
        assert_eq!(value["type"], "sell");
        assert!(value.get("orderType").is_none());
        assert_eq!(value["itemId"], "valid-item-id");
        assert!(value.get("charges").is_none());
    }

    #[test]
    fn close_order_uses_current_v2_quantity_field() {
        let value = serde_json::to_value(CloseOrderInput { quantity: 3 })
            .expect("serialize close-order body");
        assert_eq!(value, serde_json::json!({ "quantity": 3 }));
        assert!(value.get("Quantity").is_none());
    }

    #[test]
    fn api_error_detail_is_bounded_and_ignores_non_json_bodies() {
        let detail = account_api_error_detail(
            br#"{"apiVersion":"0.25.0","data":null,"error":{"perTrade":"field is required"}}"#,
        )
        .expect("structured API error");
        assert!(detail.contains("perTrade"));
        assert!(detail.len() <= 500);
        assert!(account_api_error_detail(b"<html>proxy error</html>").is_none());
    }

    #[test]
    fn current_v2_account_fixtures_deserialize() {
        let profile: ApiEnvelope<AccountProfile> =
            serde_json::from_str(include_str!("../../../fixtures/wfm/account_me_v2.json"))
                .expect("current /v2/me fixture");
        let profile = profile.data.expect("profile data");
        assert_eq!(profile.ingame_name, "FixtureTenno");
        assert!(profile.verification);

        let orders: ApiEnvelope<Vec<AccountOrder>> =
            serde_json::from_str(include_str!("../../../fixtures/wfm/account_orders_v2.json"))
                .expect("current /v2/orders/my fixture");
        let order = orders.data.expect("orders data").remove(0);
        assert_eq!(order.order_type, AccountOrderType::Sell);
        assert_eq!(order.charges, Some(3));
        assert_eq!(order.amber_stars, Some(3));
    }
}
