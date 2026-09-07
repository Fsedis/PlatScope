//! Проверка всей цепочки на копии БД. Аргумент: путь к отдельной диагностической БД.
use std::sync::Mutex;

use platscope_core::{AppSettings, BountyHunterService, SETTINGS_KEY, WorldActivityService};
use platscope_storage::Database;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Укажите путь к копии БД")?;
    let database = Database::open(path)?;
    let settings = database
        .get_setting::<AppSettings>(SETTINGS_KEY)?
        .unwrap_or_default();
    let database = Mutex::new(database);
    let world = WorldActivityService::production()?
        .view(&database, true)
        .await?;
    let bounties = BountyHunterService::production()?
        .view(&database, &settings, true)
        .await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({"world": world, "bounties": bounties}))?
    );
    Ok(())
}
