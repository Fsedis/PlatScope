//! Структурная диагностика истории освоения без записи ответа или сессионных данных.
use platscope_readonly_scan::inventory::InventoryScanner;

fn main() {
    let detailed = std::env::args().any(|arg| arg == "--details");
    let result = InventoryScanner::new()
        .scan(None, None)
        .unwrap_or_else(|_| {
            eprintln!("scan_failed; credentials discarded");
            std::process::exit(1);
        });
    let value: serde_json::Value =
        serde_json::from_slice(&result.inventory_bytes).expect("inventory must contain JSON");
    let root = value.get("Inventory").unwrap_or(&value);
    // Только игровые типы и опыт; полный ответ и сессия не выводятся.
    let profile = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .ok()
        .and_then(|client| {
            client
                .get("https://api.warframe.com/cdn/getProfileViewingData.php")
                .query(&[("playerId", result.session.account_id.as_str())])
                .send()
                .ok()
        })
        .filter(|response| response.status().is_success())
        .and_then(|response| response.json::<serde_json::Value>().ok());
    let profile_xp = profile
        .as_ref()
        .and_then(|value| value.pointer("/Results/0/LoadOutInventory/XPInfo"))
        .and_then(serde_json::Value::as_array);
    println!("profile_xp_entries={}", profile_xp.map_or(0, Vec::len));
    if let Some(entries) = root.get("XPInfo").and_then(serde_json::Value::as_array) {
        let mut matched = 0;
        for entry in entries {
            let item_type = entry["ItemType"].as_str().unwrap_or("");
            let profile_entry = profile_xp
                .and_then(|entries| entries.iter().find(|item| item["ItemType"] == item_type));
            matched += usize::from(profile_entry.is_some_and(|other| other["XP"] == entry["XP"]));
            if detailed || focus_item(item_type) {
                println!(
                    "history item={item_type} xp={} profile_xp={}",
                    entry["XP"],
                    profile_entry.map_or(serde_json::Value::Null, |entry| entry["XP"].clone())
                );
            }
        }
        println!("profile_matches={matched}/{}", entries.len());
    }
    for category in [
        "Suits",
        "LongGuns",
        "Pistols",
        "Melee",
        "MechSuits",
        "OperatorAmps",
        "MoaPets",
        "KubrowPets",
        "Hoverboards",
    ] {
        if let Some(entries) = root.get(category).and_then(serde_json::Value::as_array) {
            for entry in entries {
                if !detailed && !focus_item(entry["ItemType"].as_str().unwrap_or("")) {
                    continue;
                }
                println!(
                    "owned category={category} item={} xp={} polarized={} features={} parts={}",
                    entry["ItemType"],
                    entry["XP"],
                    entry["Polarized"],
                    entry["Features"],
                    entry["ModularParts"]
                );
            }
        }
    }
    for key in [
        "XPInfo",
        "LoadOutInventory",
        "Suits",
        "LongGuns",
        "Pistols",
        "Melee",
    ] {
        let branch = root.get(key);
        println!(
            "branch={key} present={} array_count={}",
            branch.is_some(),
            branch
                .and_then(serde_json::Value::as_array)
                .map_or(0, Vec::len)
        );
        if key == "XPInfo"
            && let Some(entries) = branch.and_then(serde_json::Value::as_array)
        {
            let with_xp = entries
                .iter()
                .filter(|entry| {
                    entry
                        .get("XP")
                        .and_then(serde_json::Value::as_u64)
                        .is_some()
                })
                .count();
            let with_type = entries
                .iter()
                .filter(|entry| {
                    entry
                        .get("ItemType")
                        .and_then(serde_json::Value::as_str)
                        .is_some()
                })
                .count();
            println!("xp_entries={with_xp} typed_entries={with_type}");
            if let Some(keys) = entries.first().and_then(serde_json::Value::as_object) {
                println!("entry_fields={:?}", keys.keys().collect::<Vec<_>>());
            }
        }
    }
}

fn focus_item(game_ref: &str) -> bool {
    [
        "OperatorAmplifiers",
        "CodaHema",
        "ZanukaPets",
        "PrimeVectis",
        "CreaturePets",
    ]
    .iter()
    .any(|part| game_ref.contains(part))
}
