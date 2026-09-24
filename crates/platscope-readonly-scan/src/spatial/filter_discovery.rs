//! Приоритетный поиск новых экземпляров сохранённых пользовательских типов.
use super::{
    Memory, MemoryRange, Result, Scene, SceneObject, cancelled, source::u64_at, types::Decoder,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashSet, VecDeque},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_PENDING: usize = 8192;
const MAX_TIME: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoveryRules {
    keys: BTreeSet<String>,
    kinds: BTreeSet<String>,
}
fn resource_name(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_lowercase()
}
impl DiscoveryRules {
    pub fn parse(keys: Vec<String>) -> Result<Self> {
        if keys.len() > 8000 {
            return Err("Слишком много типов в фильтрах карты".into());
        }
        let mut rules = Self::default();
        for key in keys {
            if key.len() > 16384 {
                return Err("Слишком длинное правило фильтра".into());
            }
            let Ok(mut value) = serde_json::from_str::<Value>(&key) else {
                continue;
            };
            let Some(parts) = value.as_array_mut() else {
                continue;
            };
            if parts.len() < 3 {
                continue;
            }
            let Some(kind) = parts[0].as_str().map(str::to_owned) else {
                continue;
            };
            match parts[1].as_str() {
                Some("variant" | "item") if parts.len() == 3 && parts[2].is_string() => {}
                Some("resource") if parts.len() == 3 && parts[2].is_string() => {
                    parts[2] = json!(resource_name(parts[2].as_str().unwrap()))
                }
                Some("type")
                    if parts.len() == 4
                        && parts[2]
                            .as_array()
                            .is_some_and(|a| a.iter().all(Value::is_string))
                        && parts[3].is_string() => {}
                _ => continue, // Неизвестные старые правила остаются в хранилище интерфейса.
            }
            rules.kinds.insert(kind);
            rules.keys.insert(value.to_string());
        }
        Ok(rules)
    }
    fn matches(&self, object: &SceneObject) -> bool {
        if !self.kinds.contains(&object.kind) {
            return false;
        }
        if let Some(variant) = &object.variant_key
            && self
                .keys
                .contains(&json!([object.kind, "variant", variant]).to_string())
        {
            return true;
        }
        let key = if let Some(item) = &object.item_path {
            json!([
                object.kind,
                "item",
                item.replacen("/Lotus/StoreItems/", "/Lotus/", 1)
            ])
        } else if let Some(resource) = object.details.iter().find(|d| d.label == "Ресурс") {
            json!([object.kind, "resource", resource_name(&resource.value)])
        } else {
            let mut names = object.type_names.clone();
            names.sort();
            json!([object.kind, "type", names, object.name_en])
        };
        self.keys.contains(&key.to_string())
    }
    pub(super) fn wants_family(&self, family: &str) -> bool {
        let kinds: &[&str] = match family {
            "pickup" => &["pickup", "feather"],
            "npc" => &["npc", "hostage"],
            "decoration" | "effect" => &["decoration", "cache", "locker", "panel"],
            "waypoint" => &["lootspot"],
            "terminal" => &["terminal"],
            "dragon_door" => &["dragon_door"],
            "avatar" => &["avatar"],
            "spawnpoint" => &["spawnpoint"],
            "extraction" => &["extraction"],
            _ => &[],
        };
        kinds.iter().any(|kind| self.kinds.contains(*kind))
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DiscoveryState {
    rules: DiscoveryRules,
    focus: BTreeSet<usize>,
    focus_cursor: usize,
    cursor: usize,
    pending: VecDeque<(u64, u64, &'static str)>,
    // Метаданные описывают тип, а не экземпляр. Не декодируем одинаковый неподходящий тип каждый тик.
    excluded: HashSet<(u64, u64)>,
}
fn range_index(ranges: &[MemoryRange], address: u64) -> Option<usize> {
    let index = ranges
        .partition_point(|r| r.address <= address)
        .checked_sub(1)?;
    (address < ranges[index].address + ranges[index].length as u64).then_some(index)
}

pub(super) fn discover(
    m: &mut dyn Memory,
    scene: &mut Scene,
    rules: &DiscoveryRules,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut state = std::mem::take(&mut scene.filter_discovery);
    if state.rules != *rules {
        state = DiscoveryState {
            rules: rules.clone(),
            ..Default::default()
        };
    }
    let result = discover_inner(m, scene, &mut state, cancel);
    scene.filter_discovery = state;
    result
}
fn discover_inner(
    m: &mut dyn Memory,
    scene: &mut Scene,
    state: &mut DiscoveryState,
    cancel: &AtomicBool,
) -> Result<()> {
    if state.rules.keys.is_empty()
        || scene.discovery_ranges.is_empty()
        || scene.objects.len() >= super::analyze::MAX_OBJECTS
    {
        return Ok(());
    }
    let Some(profile) = scene.discovery_profile.clone() else {
        return Ok(());
    };
    let targets: Vec<_> = profile
        .targets()?
        .into_iter()
        .filter(|(_, family)| state.rules.wants_family(family))
        .collect();
    if targets.is_empty() {
        return Ok(());
    }
    let started = Instant::now();
    for object in &scene.objects {
        if state.rules.matches(object)
            && let Ok(address) = u64::from_str_radix(object.key.trim_start_matches("0x"), 16)
            && let Some(index) = range_index(&scene.discovery_ranges, address)
        {
            state.focus.insert(index);
        }
    }
    let known: HashSet<_> = scene.identities.iter().map(|id| id.address).collect();
    let mut queued: HashSet<_> = state.pending.iter().map(|c| c.0).collect();
    let mut visited = HashSet::new();
    let mut bytes = 0;
    // Половина бюджета — блоки, где уже встречались свои типы. Остальное — остальные
    // подтверждённые блоки, даже если нужного предмета ещё не было в исходной сцене.
    let focus: Vec<_> = state.focus.iter().copied().collect();
    for step in 0..focus.len() + scene.discovery_ranges.len() {
        cancelled(cancel)?;
        if bytes >= MAX_BYTES
            || started.elapsed() >= Duration::from_millis(30)
            || state.pending.len() >= MAX_PENDING
        {
            break;
        }
        let index = if step < focus.len() {
            if bytes >= MAX_BYTES / 2 || started.elapsed() >= Duration::from_millis(15) {
                continue;
            }
            let index = focus[state.focus_cursor % focus.len()];
            state.focus_cursor = (state.focus_cursor + 1) % focus.len();
            index
        } else {
            let index = state.cursor % scene.discovery_ranges.len();
            state.cursor = (state.cursor + 1) % scene.discovery_ranges.len();
            index
        };
        if !visited.insert(index) {
            continue;
        }
        let range = scene.discovery_ranges[index];
        if range.length > MAX_BYTES - bytes {
            break;
        }
        bytes += range.length;
        let Ok(raw) = m.read(range.address, range.length) else {
            continue;
        };
        scene.stats.scanned_bytes = scene.stats.scanned_bytes.saturating_add(raw.len() as u64);
        for (vt, family) in &targets {
            for offset in memchr::memmem::find_iter(&raw, &vt.to_le_bytes()) {
                let address = range.address + offset as u64;
                if !address.is_multiple_of(8)
                    || known.contains(&address)
                    || queued.contains(&address)
                {
                    continue;
                }
                if let Ok(meta) = u64_at(&raw, offset + 8)
                    && state.excluded.contains(&(*vt, meta))
                {
                    continue;
                }
                if state.pending.len() >= MAX_PENDING {
                    break;
                }
                state.pending.push_back((address, *vt, *family));
                queued.insert(address);
            }
        }
    }
    let mut decoder = Decoder::new(profile.clone())?;
    while started.elapsed() < MAX_TIME && scene.objects.len() < super::analyze::MAX_OBJECTS {
        cancelled(cancel)?;
        let Some((address, vt, family)) = state.pending.pop_front() else {
            break;
        };
        if known.contains(&address) {
            continue;
        }
        let Ok((object, id)) =
            super::analyze::object(m, address, family, vt, &mut decoder, &profile)
        else {
            continue;
        };
        if state.rules.matches(&object) {
            if let Some(index) = range_index(&scene.discovery_ranges, address) {
                state.focus.insert(index);
            }
            scene.objects.push(object);
            scene.identities.push(id);
        } else if family != "dragon_door" && state.excluded.len() < 20_000 {
            state.excluded.insert((vt, id.metadata));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Mock {
        bytes: Vec<u8>,
        reads: usize,
    }
    impl Memory for Mock {
        fn read(&mut self, address: u64, length: usize) -> Result<Vec<u8>> {
            self.reads += 1;
            self.bytes
                .get(address as usize..address as usize + length)
                .map(<[u8]>::to_vec)
                .ok_or("Нет области".into())
        }
        fn ranges(&self) -> Vec<MemoryRange> {
            vec![]
        }
        fn modules(&self) -> Vec<super::super::MemoryModule> {
            vec![]
        }
    }
    fn spawn(m: &mut Mock, address: usize) {
        for (offset, value) in [
            (0, 0x2208c58u64),
            (16, (address + 0x600) as u64),
            (0x600, address as u64),
        ] {
            m.bytes[address + offset..address + offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        for i in 0..4 {
            m.bytes[address + 0xa0 + i * 20..address + 0xa4 + i * 20]
                .copy_from_slice(&1f32.to_le_bytes());
        }
        for (i, value) in [1f32, 2., 3.].into_iter().enumerate() {
            for offset in [0x70, 0xd0] {
                m.bytes[address + offset + i * 4..address + offset + i * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
    }
    fn empty_scene() -> Scene {
        let mut scene: Scene = serde_json::from_value(json!({"format":1,"source":"live","startedAt":"test","capturedAt":"test","complete":true,"profile":"test","objects":[],"meshes":[],"warnings":[],"stats":{"scannedBytes":0,"objectCount":0,"meshCount":0,"vertexCount":0,"faceCount":0}})).unwrap();
        scene.discovery_profile = Some(super::super::profile::Profile::legacy(0));
        scene.discovery_ranges = vec![
            MemoryRange {
                address: 0,
                length: 0x1000,
            },
            MemoryRange {
                address: 0x1000,
                length: 0x1000,
            },
        ];
        scene
    }
    #[test]
    fn new_filtered_instances_appear_without_rescanning_scene() {
        let mut scene = empty_scene();
        let mut memory = Mock {
            bytes: vec![0; 0x3000],
            reads: 0,
        };
        let rules = DiscoveryRules::parse(vec![
            json!(["spawnpoint", "type", [], "NpcSpawnPoint"]).to_string(),
        ])
        .unwrap();
        let cancel = AtomicBool::new(false);
        discover(&mut memory, &mut scene, &rules, &cancel).unwrap();
        assert!(scene.objects.is_empty());
        // Типа ещё не было на карте; новый экземпляр появился в другом известном блоке.
        spawn(&mut memory, 0x1100);
        discover(&mut memory, &mut scene, &rules, &cancel).unwrap();
        assert_eq!(
            scene
                .objects
                .iter()
                .map(|o| o.key.as_str())
                .collect::<Vec<_>>(),
            ["0x1100"]
        );
        spawn(&mut memory, 0x100);
        discover(&mut memory, &mut scene, &rules, &cancel).unwrap();
        assert_eq!(scene.objects.len(), 2);
        discover(&mut memory, &mut scene, &rules, &cancel).unwrap();
        assert_eq!(
            scene.objects.len(),
            2,
            "Повторный тик не дублирует экземпляры"
        );
        let reads = memory.reads;
        discover(&mut memory, &mut scene, &DiscoveryRules::default(), &cancel).unwrap();
        assert_eq!(
            reads, memory.reads,
            "Без своих правил нет дополнительного чтения"
        );
    }
    #[test]
    fn saved_variant_and_model_rules_keep_their_meaning() {
        let mut scene = empty_scene();
        let mut memory = Mock {
            bytes: vec![0; 0x3000],
            reads: 0,
        };
        spawn(&mut memory, 0x100);
        let mut decoder = Decoder::new(scene.discovery_profile.take().unwrap()).unwrap();
        let (mut object, _) = super::super::analyze::object(
            &mut memory,
            0x100,
            "spawnpoint",
            0x2208c58,
            &mut decoder,
            &super::super::profile::Profile::legacy(0),
        )
        .unwrap();
        object.kind = "decoration".into();
        object.variant_key = Some("type-v1:rare".into());
        object.details.push(super::super::Detail {
            label: "Ресурс".into(),
            value: "GrnStorageLocker_skel.fbx".into(),
        });
        let variant = DiscoveryRules::parse(vec![
            json!(["decoration", "variant", "type-v1:rare"]).to_string(),
        ])
        .unwrap();
        let model = DiscoveryRules::parse(vec![
            json!([
                "decoration",
                "resource",
                "/Lotus/Models/GrnStorageLocker_skel.fbx"
            ])
            .to_string(),
        ])
        .unwrap();
        assert!(variant.matches(&object) && model.matches(&object));
        object.variant_key = Some("type-v1:ordinary".into());
        assert!(!variant.matches(&object));
        assert!(model.matches(&object));
        assert!(model.wants_family("decoration"));
        assert!(!model.wants_family("pickup"));
    }
}
