use crate::AppState;
use platscope_core::{MarketPresence, PresenceView};
use tauri::State;

#[tauri::command]
pub async fn account_presence(state: State<'_, AppState>) -> Result<PresenceView, String> {
    state
        .account_service
        .presence()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn account_set_presence(
    status: MarketPresence,
    state: State<'_, AppState>,
) -> Result<PresenceView, String> {
    state
        .account_service
        .set_presence(status)
        .await
        .map_err(|error| error.to_string())
}
