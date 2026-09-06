//! Публичные профили WFM: игровой ник и адрес профиля могут различаться.
//! Переход разрешается только после проверки `ingameName` в ответе API.

use std::time::Duration;

use platscope_storage::TradeEvent;
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::AppState;

const MAX_PROFILE_BODY_BYTES: usize = 128 * 1024;

#[derive(Debug, Deserialize)]
struct ProfileResponse {
    data: PublicProfile,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicProfile {
    ingame_name: String,
    slug: String,
}

/// API документирует поиск по slug, но не поиск по игровому нику. Ник здесь
/// используется только как кандидат адреса. Если владелец адреса носит другой
/// игровой ник, профиль не открывается; произвольные похожие имена не выбираются.
fn verified_profile_url(ingame_name: &str, profile: &PublicProfile) -> Result<String, String> {
    if !ingame_name
        .trim()
        .eq_ignore_ascii_case(&profile.ingame_name)
    {
        return Err("profile_name_mismatch".to_owned());
    }
    profile_url(&profile.slug)
}

fn profile_url(slug: &str) -> Result<String, String> {
    if !valid_profile_slug(slug) {
        return Err("invalid_profile_slug".to_owned());
    }
    Ok(format!("https://warframe.market/profile/{slug}"))
}

fn valid_profile_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|character| character.is_ascii_alphanumeric() || b"._-".contains(&character))
}

async fn resolve_profile(ingame_name: &str) -> Result<String, String> {
    let candidate = ingame_name.trim();
    if candidate.is_empty() || candidate.len() > 100 || candidate.chars().any(char::is_control) {
        return Err("invalid_ingame_name".to_owned());
    }
    let mut url = Url::parse("https://api.warframe.market/v2/user/")
        .map_err(|_| "invalid_profile_endpoint".to_owned())?;
    url.path_segments_mut()
        .map_err(|()| "invalid_profile_endpoint".to_owned())?
        .pop_if_empty()
        .push(&candidate.to_ascii_lowercase());
    let client = Client::builder()
        .timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("PlatScope/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "profile_request_failed".to_owned())?;
    platscope_wfm::wait_for_request_slot().await;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "profile_request_failed".to_owned())?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err("profile_not_found".to_owned());
    }
    if !response.status().is_success() {
        return Err("profile_request_failed".to_owned());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "profile_request_failed".to_owned())?
    {
        if body.len().saturating_add(chunk.len()) > MAX_PROFILE_BODY_BYTES {
            return Err("profile_response_too_large".to_owned());
        }
        body.extend_from_slice(&chunk);
    }
    let profile: ProfileResponse =
        serde_json::from_slice(&body).map_err(|_| "invalid_profile_response".to_owned())?;
    verified_profile_url(candidate, &profile.data)
}

#[tauri::command]
pub(crate) async fn open_trade_partner_profile(
    ingame_name: String,
    app: AppHandle,
) -> Result<(), String> {
    let url = resolve_profile(&ingame_name).await?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "profile_open_failed".to_owned())
}

/// Для предложений игроков API уже возвращает canonical slug владельца.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Аргументами владеет извлекатель Tauri.
pub(crate) fn open_market_user_profile(user_slug: String, app: AppHandle) -> Result<(), String> {
    let url = profile_url(&user_slug)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "profile_open_failed".to_owned())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // State принадлежит извлекателю команды Tauri.
pub(crate) fn market_trade_events(state: State<'_, AppState>) -> Result<Vec<TradeEvent>, String> {
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .recent_trade_events(1000)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{PublicProfile, verified_profile_url};

    #[test]
    fn opens_canonical_slug_only_after_exact_ingame_name_match() {
        let profile = PublicProfile {
            ingame_name: "Some.Tenno".to_owned(),
            slug: "different-profile-name".to_owned(),
        };
        assert_eq!(
            verified_profile_url("some.tenno", &profile).expect("same ingame name"),
            "https://warframe.market/profile/different-profile-name"
        );
        assert!(verified_profile_url("SomeTenno", &profile).is_err());
        assert!(verified_profile_url("Some.Tenno#123", &profile).is_err());
    }

    #[test]
    fn rejects_other_persons_profile_and_external_redirects() {
        let other_person = PublicProfile {
            ingame_name: "AnotherPlayer".to_owned(),
            slug: "expected-player".to_owned(),
        };
        assert_eq!(
            verified_profile_url("ExpectedPlayer", &other_person),
            Err("profile_name_mismatch".to_owned())
        );
        for slug in ["//example.com", "../other", "abc?redirect=evil", "..", ""] {
            let profile = PublicProfile {
                ingame_name: "Tenno".to_owned(),
                slug: slug.to_owned(),
            };
            assert!(verified_profile_url("Tenno", &profile).is_err());
        }
    }
}
