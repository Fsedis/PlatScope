//! Ограниченное чтение JSON экипировки. Идея сигнатуры из synqark/warframe-peer-overlay
//! (58d9497, Unlicense). Не ищет и не сохраняет учётные данные, не пишет в процесс.
use anyhow::{Context, Result, bail};
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

const HEAD: &[u8] = b"{\"PlayerLevel\":";
const MAX_JSON: usize = 1024 * 1024;
const MAX_INVENTORY_JSON: usize = 8 * 1024 * 1024;
const CHUNK: usize = 4 * 1024 * 1024;
const MAX_CANDIDATES: usize = 256;
pub(crate) static SCAN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub struct Candidate {
    pub bytes: usize,
    pub copies: usize,
    pub value: Value,
}

struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Возвращает все различные подходящие JSON. Неполный обход — ошибка, иначе
/// единственный найденный кандидат можно ошибочно принять за однозначный.
pub fn capture(pid: u32) -> Result<Vec<Candidate>> {
    scan_json(pid, HEAD, collect, MAX_CANDIDATES, MAX_JSON)
}

/// Отдельный ручной поиск конфигураций. Владелец и актуальность не установлены.
pub fn capture_suits(pid: u32) -> Result<Vec<Candidate>> {
    let inventory = scan_json(
        pid,
        b"{\"SubscribedToEmails\":",
        collect_inventory,
        32,
        MAX_INVENTORY_JSON,
    )?;
    if !inventory.is_empty() {
        return Ok(inventory);
    }
    scan_json(pid, b"\"Suits\":", collect_suits, 32, MAX_JSON)
}

/// Проекция одного целого JSON. Ссылки модов остаются внутри процесса PlatScope;
/// сведения аккаунта, пользовательские имена конфигураций и прочие разделы удаляются.
fn collect_inventory(bytes: &[u8], found: &mut BTreeMap<Vec<u8>, usize>) {
    let Some(Ok(value)) =
        serde_json::Deserializer::from_slice(&bytes[..bytes.len().min(MAX_INVENTORY_JSON)])
            .into_iter::<Value>()
            .next()
    else {
        return;
    };
    let Some(object) = value.as_object() else {
        return;
    };
    if !value["Suits"].is_array() || !value["Upgrades"].is_array() {
        return;
    }
    let mut projection = serde_json::Map::new();
    for key in ["Upgrades", "RawUpgrades"] {
        if let Some(entries) = value[key].as_array() {
            if entries.len() > 100_000 {
                return;
            }
            projection.insert(
                key.into(),
                Value::Array(
                    entries
                        .iter()
                        .map(|item| {
                            let mut clean = serde_json::Map::new();
                            for field in ["ItemId", "ItemType", "UpgradeFingerprint", "ItemCount"] {
                                if let Some(v) = item.get(field) {
                                    clean.insert(field.into(), v.clone());
                                }
                            }
                            Value::Object(clean)
                        })
                        .collect(),
                ),
            );
        }
    }
    for (category, value) in object {
        let Some(items) = value.as_array() else {
            continue;
        };
        if items.len() > 4096 || !items.iter().any(|i| i["Configs"].is_array()) {
            continue;
        }
        let items = items
            .iter()
            .filter(|item| {
                item["ItemType"]
                    .as_str()
                    .is_some_and(|p| p.starts_with("/Lotus/") && p.len() <= 1024)
            })
            .map(|item| {
                let mut clean = serde_json::Map::new();
                for field in [
                    "ItemType",
                    "Level",
                    "Polarized",
                    "ArchonCrystalUpgrades",
                    "ModularPartTypes",
                ] {
                    if let Some(v) = item.get(field) {
                        clean.insert(field.into(), v.clone());
                    }
                }
                clean.insert(
                    "Configs".into(),
                    Value::Array(
                        item["Configs"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .take(12)
                            .map(|config| {
                                let mut c = serde_json::Map::new();
                                for field in ["Upgrades", "AbilityOverride"] {
                                    if let Some(v) = config.get(field) {
                                        c.insert(field.into(), v.clone());
                                    }
                                }
                                Value::Object(c)
                            })
                            .collect(),
                    ),
                );
                Value::Object(clean)
            })
            .collect();
        projection.insert(category.clone(), Value::Array(items));
    }
    if let Ok(json) = serde_json::to_vec(&projection) {
        *found.entry(json).or_default() += 1;
    }
}

#[cfg(test)]
mod inventory_tests {
    use super::*;

    #[test]
    fn projects_one_complete_inventory_and_removes_unrelated_private_fields() {
        let input = serde_json::json!({"SubscribedToEmails":true,"AccountId":"private-account",
            "Upgrades":[{"ItemId":{"$oid":"mod-reference"},"ItemType":"/Lotus/Upgrades/Mods/A","UpgradeFingerprint":"{\"lvl\":3}","Owner":"private-owner"}],
            "RawUpgrades":[{"ItemType":"/Lotus/Upgrades/Mods/B","ItemCount":5}],
            "Suits":[{"ItemType":"/Lotus/Powersuits/Test","ItemId":"private-gear","Configs":[{"Name":"private-name","Upgrades":["mod-reference"]}]}],
            "Pistols":[{"ItemType":"/Lotus/Weapons/Test","Configs":[{"Upgrades":[]}]}]});
        let mut found = BTreeMap::new();
        collect_inventory(&serde_json::to_vec(&input).unwrap(), &mut found);
        assert_eq!(found.len(), 1);
        let text = String::from_utf8(found.keys().next().unwrap().clone()).unwrap();
        assert!(!text.contains("private"));
        assert!(text.contains("mod-reference"));
        assert!(text.contains("RawUpgrades"));
        assert!(text.contains("Pistols"));
    }

    #[test]
    fn incomplete_or_non_inventory_json_does_not_create_a_resolvable_candidate() {
        let mut found = BTreeMap::new();
        for bytes in [
            br#"{"Suits":[],"Upgrades":["#.as_slice(),
            br#"{"Suits":[]}"#,
            br#"[]"#,
            br#"{"Suits":{},"Upgrades":[]}"#,
        ] {
            collect_inventory(bytes, &mut found);
        }
        assert!(found.is_empty());
    }
}

fn collect_suits(bytes: &[u8], found: &mut BTreeMap<Vec<u8>, usize>) {
    let Some(bytes) = bytes.strip_prefix(b"\"Suits\":") else {
        return;
    };
    let Some(Ok(value)) = serde_json::Deserializer::from_slice(&bytes[..bytes.len().min(MAX_JSON)])
        .into_iter::<Value>()
        .next()
    else {
        return;
    };
    let Some(items) = value.as_array() else {
        return;
    };
    if items.is_empty()
        || items.len() > 512
        || !items.iter().all(|item| {
            item["ItemType"]
                .as_str()
                .is_some_and(|p| p.starts_with("/Lotus/Powersuits/") && p.len() < 512)
        })
    {
        return;
    }
    if let Ok(json) = serde_json::to_vec(&value) {
        *found.entry(json).or_default() += 1;
    }
}

type Collector = fn(&[u8], &mut BTreeMap<Vec<u8>, usize>);

fn scan_json(
    pid: u32,
    head: &[u8],
    collector: Collector,
    limit: usize,
    json_limit: usize,
) -> Result<Vec<Candidate>> {
    let _guard = SCAN_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("Предыдущее чтение памяти завершилось с ошибкой."))?;
    let process = Process(
        unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, BOOL(0), pid) }
            .context("Не удалось прочитать процесс Warframe")?,
    );
    let start = Instant::now();
    let mut found = BTreeMap::<Vec<u8>, usize>::new();
    let mut address = 0usize;
    let mut buffer = vec![0u8; CHUNK + json_limit];
    loop {
        if start.elapsed() > Duration::from_secs(30) {
            bail!("Чтение экипировки заняло слишком много времени. Повторите попытку.");
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
                bail!("Чтение экипировки заняло слишком много времени. Повторите попытку.");
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
                for hit in memchr::memmem::find_iter(&buffer[..read], head) {
                    if hit >= CHUNK {
                        break;
                    }
                    collector(&buffer[hit..read], &mut found);
                    if found.len() > limit {
                        bail!(
                            "Слишком много вариантов экипировки. Повторите после входа в миссию."
                        );
                    }
                }
            }
            offset = offset.saturating_add(CHUNK);
        }
    }
    Ok(found
        .into_iter()
        .filter_map(|(json, copies)| {
            serde_json::from_slice(&json).ok().map(|value| Candidate {
                bytes: json.len(),
                copies,
                value,
            })
        })
        .collect())
}

fn collect(bytes: &[u8], found: &mut BTreeMap<Vec<u8>, usize>) {
    if !bytes.starts_with(HEAD) {
        return;
    }
    let Some(end) = memchr::memchr(0, &bytes[..bytes.len().min(MAX_JSON)]) else {
        return;
    };
    let json = &bytes[..end];
    if json.last() != Some(&b'}') {
        return;
    }
    if let Some(count) = found.get_mut(json) {
        *count += 1;
        return;
    }
    let Ok(value) = serde_json::from_slice::<Value>(json) else {
        return;
    };
    if value.get("PlayerLevel").and_then(Value::as_u64).is_none()
        || !value.get("NORMAL").is_some_and(Value::is_array)
    {
        return;
    }
    found.insert(json.to_vec(), 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn suit_arrays_must_be_complete_and_contain_only_suits() {
        let mut found = BTreeMap::new();
        collect_suits(
            br#""Suits":[{"ItemType":"/Lotus/Powersuits/Test"}],"next":1"#,
            &mut found,
        );
        assert_eq!(found.len(), 1);
        collect_suits(
            br#""Suits":[{"ItemType":"/Lotus/Powersuits/Test"}"#,
            &mut found,
        );
        collect_suits(br#""Suits":[{"ItemType":"/Lotus/NotASuit"}]"#, &mut found);
        assert_eq!(found.len(), 1);
    }
    #[test]
    fn rejects_partial_and_unrelated_json_and_counts_duplicates() {
        let mut found = BTreeMap::new();
        collect(br#"{"PlayerLevel":3,"NORMAL":[]}"#, &mut found);
        collect(b"{\"PlayerLevel\":3}\0", &mut found);
        let valid = b"{\"PlayerLevel\":3,\"NORMAL\":[]}\0";
        collect(valid, &mut found);
        collect(valid, &mut found);
        assert_eq!(found.len(), 1);
        assert_eq!(*found.values().next().unwrap(), 2);
    }
}
