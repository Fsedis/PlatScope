//! Ручная проверка реальных публичных источников, без аккаунта и действий в игре.
use platscope_providers::{WarframeWorldstateProvider, WorldActivityProvider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let activity = WorldActivityProvider::production()?;
    let world = WarframeWorldstateProvider::production()?;
    let (activity, bounties, daily) =
        tokio::join!(activity.fetch(), world.fetch_bounties(), world.fetch());
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "activity": activity?, "bounties": bounties?, "daily": daily?,
        }))?
    );
    Ok(())
}
