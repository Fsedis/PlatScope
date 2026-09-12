//! Ограниченное исследование игровых JSON-полей. Возвращает только фильтрованную
//! схему и игровые примеры; сырые значения остаются внутри этого вызова.
use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{BOOL, CloseHandle, HANDLE},
    System::{
        Diagnostics::Debug::ReadProcessMemory,
        Memory::{
            MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_GUARD, PAGE_NOACCESS, PAGE_NOCACHE,
            PAGE_READWRITE, PAGE_WRITECOMBINE, PAGE_WRITECOPY, VirtualQueryEx,
        },
        Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
    },
};
#[path = "research_schema.rs"]
mod schema;

const KEYS: &[&str] = &[
    "ArchonCrystalUpgrades",
    "AbilityOverride",
    "InfestedFoundry",
    "UpgradeFingerprint",
    "EvolutionProgress",
    "EvolutionChoices",
    "MissionInfo",
    "missionType",
    "MissionRewards",
    "missionRewards",
    "RewardInfo",
    "CurrentWave",
    "PlayerPosition",
    "Pickups",
    "InventoryChanges",
    "LastInventorySync",
    "Suits",
    "Upgrades",
    "LongGuns",
    "PendingRecipes",
    "Boosters",
    "FocusAbility",
    "VoidProjection",
    "NORMAL",
    "OPERATOR",
    "OPERATOR_ADULT",
    "CrewShipLoadOut",
    "Consumables",
    "ExtraConsumables",
    "MissionStatus",
    "MissionProgress",
    "ActiveChallenges",
    "CollectedItems",
];
const CHUNK: usize = 4 * 1024 * 1024;
const WINDOW: usize = 1024 * 1024;
const MAX_RESOURCES: usize = 8000;

fn collect_resources(bytes: &[u8], resources: &mut BTreeMap<String, usize>) -> bool {
    let mut capped = false;
    for hit in memchr::memmem::find_iter(bytes, b"/Lotus/") {
        if hit >= CHUNK {
            break;
        }
        let tail = &bytes[hit..];
        let length = tail
            .iter()
            .take(513)
            .position(|b| !b.is_ascii_alphanumeric() && !b"/_-.".contains(b));
        let Some(length) = length.filter(|n| *n < 512) else {
            continue;
        };
        let Ok(path) = std::str::from_utf8(&tail[..length]) else {
            continue;
        };
        if ![
            "/Lotus/Types/Gameplay/",
            "/Lotus/Types/Enemies/",
            "/Lotus/Characters/",
            "/Lotus/Types/Game/",
            "/Lotus/Types/Items/",
            "/Lotus/Types/PickUps/",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix))
        {
            continue;
        }
        if let Some(count) = resources.get_mut(path) {
            *count += 1;
        } else if resources.len() < MAX_RESOURCES {
            resources.insert(path.into(), 1);
        } else {
            capped = true;
        }
    }
    capped
}

struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn capture(pid: u32) -> Result<Value> {
    let _guard = crate::squad::SCAN_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("Предыдущее чтение памяти завершилось с ошибкой."))?;
    let process = Process(
        unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, BOOL(0), pid) }
            .context("Не удалось открыть Warframe для чтения")?,
    );
    let needles: Vec<_> = KEYS.iter().map(|key| format!("\"{key}\":")).collect();
    let finders: Vec<_> = needles.iter().map(memchr::memmem::Finder::new).collect();
    let mut values = vec![BTreeMap::<String, usize>::new(); KEYS.len()];
    let mut hits = vec![0usize; KEYS.len()];
    let start = Instant::now();
    let mut address = 0usize;
    let mut read_bytes = 0usize;
    let mut buffer = vec![0u8; CHUNK + WINDOW];
    let mut complete = true;
    let mut resources = BTreeMap::new();
    let mut resources_capped = false;
    'regions: loop {
        if start.elapsed() > Duration::from_secs(30) {
            complete = false;
            break;
        }
        let mut info = MEMORY_BASIC_INFORMATION::default();
        if unsafe {
            VirtualQueryEx(
                process.0,
                Some(address as *const _),
                &mut info,
                std::mem::size_of_val(&info),
            )
        } == 0
        {
            break;
        }
        let base = info.BaseAddress as usize;
        let next = base.saturating_add(info.RegionSize);
        if next <= address {
            break;
        }
        address = next;
        if info.RegionSize > 512 * 1024 * 1024
            || info.State != MEM_COMMIT
            || info.Protect.0
                & (PAGE_NOACCESS.0 | PAGE_GUARD.0 | PAGE_WRITECOMBINE.0 | PAGE_NOCACHE.0)
                != 0
            || info.Protect.0 & (PAGE_READWRITE.0 | PAGE_WRITECOPY.0) == 0
        {
            continue;
        }
        let mut offset = 0;
        while offset < info.RegionSize {
            if start.elapsed() > Duration::from_secs(30) {
                complete = false;
                break 'regions;
            }
            let want = buffer.len().min(info.RegionSize - offset);
            let mut read = 0;
            let result = unsafe {
                ReadProcessMemory(
                    process.0,
                    (base + offset) as *const _,
                    buffer.as_mut_ptr().cast(),
                    want,
                    Some(&mut read),
                )
            };
            if result.is_ok() && read > 0 {
                read_bytes += read;
                resources_capped |= collect_resources(&buffer[..read], &mut resources);
                for (index, finder) in finders.iter().enumerate() {
                    if hits[index] >= 16384 || values[index].len() >= 32 {
                        continue;
                    }
                    for hit in finder.find_iter(&buffer[..read]) {
                        if hit >= CHUNK || hits[index] >= 16384 || values[index].len() >= 32 {
                            break;
                        }
                        hits[index] += 1;
                        let begin = hit + needles[index].len();
                        let end = read.min(begin + WINDOW);
                        if let Some(Ok(value)) =
                            serde_json::Deserializer::from_slice(&buffer[begin..end])
                                .into_iter::<Value>()
                                .next()
                        {
                            let json = serde_json::to_string(&value)?;
                            *values[index].entry(json).or_default() += 1;
                        }
                    }
                }
            }
            offset = offset.saturating_add(CHUNK);
        }
    }
    let mut results = BTreeMap::new();
    for (index, key) in KEYS.iter().enumerate() {
        let mut fields = BTreeMap::new();
        let mut suit_details = std::collections::BTreeSet::new();
        for json in values[index].keys() {
            let value: Value = serde_json::from_str(json)?;
            schema::walk(&format!("field.{key}"), &value, &mut fields, 0);
            if *key == "Suits" {
                for item in value.as_array().into_iter().flatten().take(256) {
                    suit_details.insert(serde_json::to_string(&schema::suit_details(item))?);
                }
            }
        }
        let details: Vec<Value> = suit_details
            .iter()
            .map(|s| serde_json::from_str(s))
            .collect::<std::result::Result<_, _>>()?;
        results.insert(key, serde_json::json!({"hits":hits[index], "distinctValues":values[index].len(), "capped":hits[index]>=16384 || values[index].len()>=32, "schema":fields, "suitDetails":details}));
    }
    Ok(
        serde_json::json!({"capturedAt":chrono::Utc::now(),"complete":complete,"elapsedMs":start.elapsed().as_millis(),"readMiB":read_bytes/1024/1024,"results":results,"resourcePaths":resources,"resourcePathsCapped":resources_capped}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_discovery_only_retains_complete_game_paths() {
        let mut resources = BTreeMap::new();
        collect_resources(b"private-name /Lotus/Types/Items/TestPickup\0 token=secret /Lotus/Private/secret\0 /Lotus/Types/Gameplay/Truncated", &mut resources);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources["/Lotus/Types/Items/TestPickup"], 1);
        assert!(
            !serde_json::to_string(&resources)
                .unwrap()
                .contains("secret")
        );
    }
}
