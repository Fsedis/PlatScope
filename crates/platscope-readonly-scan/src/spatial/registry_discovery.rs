//! Новые объекты из реестра текущего региона, без поиска по блокам памяти.
use super::{
    DiscoveryRules, Memory, MemoryModule, MemoryRange, Result, Scene, cancelled, context,
    source::{q, u32_at, u64_at},
    types::Decoder,
};
use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

const MAX_HANDLES: usize = 65_536;
const MAX_PENDING: usize = 4096;
const MAX_PROBES: usize = 2048;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_TIME: Duration = Duration::from_millis(80);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Region {
    root: u64,
    root_handle: u64,
    manager: u64,
    manager_handle: u64,
}
#[derive(Clone, Copy, Debug)]
struct Candidate {
    address: u64,
    handle: u64,
    vtable: u64,
    metadata: u64,
    family: &'static str,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct RegistryState {
    region: Option<Region>,
    previous: Arc<HashSet<u64>>,
    new_handles: VecDeque<u64>,
    cursor: usize,
    priority: VecDeque<Candidate>,
    ordinary: VecDeque<Candidate>,
}

// Бюджет охватывает также косвенные чтения свойств, а не только сам массив.
struct Bounded<'a> {
    inner: &'a mut dyn Memory,
    started: Instant,
    bytes: usize,
    exhausted: bool,
}
impl Memory for Bounded<'_> {
    fn read(&mut self, address: u64, length: usize) -> Result<Vec<u8>> {
        if length > MAX_BYTES.saturating_sub(self.bytes) || self.started.elapsed() >= MAX_TIME {
            self.exhausted = true;
            return Err("Исчерпан бюджет обновления реестра".into());
        }
        self.bytes += length;
        self.inner.read(address, length)
    }
    fn ranges(&self) -> Vec<MemoryRange> {
        vec![]
    }
    fn modules(&self) -> Vec<MemoryModule> {
        vec![]
    }
}

fn region(m: &mut dyn Memory, avatar: u64, base: u64) -> Result<Region> {
    let root = q(m, avatar + 0x1e0)?;
    if root == 0 || q(m, root)? != base + 0x21cecc0 || q(m, root + 8)? != base + 0x28f21b0 {
        return Err("Контекст сцены не подтверждён".into());
    }
    let root_handle = q(m, root + 16)?;
    if root_handle == 0 || q(m, root_handle)? != root {
        return Err("Контекст сцены сменился".into());
    }
    let manager_handle = q(m, root + 0x90)?;
    let manager = q(m, manager_handle)?;
    context::has_type(m, manager, base, 0x28f3470)?;
    if q(m, manager + 16)? != manager_handle
        || q(m, manager + 0x28)? != root_handle
        || q(m, manager)? != base + 0x21d29d8
    {
        return Err("Реестр принадлежит другому контексту".into());
    }
    // Два независимых метода подтверждают размер в байтах и границы массива.
    let count_method = q(m, base + 0x21d29d8 + 74 * 8)?;
    let range_method = q(m, base + 0x21d29d8 + 76 * 8)?;
    if count_method != base + 0xe03440
        || range_method != base + 0xc66420
        || m.read(count_method, 11)? != [0x8b, 0x81, 0x08, 0x02, 0, 0, 0x48, 0xc1, 0xe8, 3, 0xc3]
        || m.read(range_method, 31)?
            != [
                0x48, 0x8b, 0x81, 0, 2, 0, 0, 0x48, 0x89, 2, 0x8b, 0x81, 8, 2, 0, 0, 0x48, 3, 0x81,
                0, 2, 0, 0, 0x48, 0x89, 0x42, 8, 0x48, 0x8b, 0xc2, 0xc3,
            ]
    {
        return Err("Структура реестра отличается от исследованной".into());
    }
    Ok(Region {
        root,
        root_handle,
        manager,
        manager_handle,
    })
}

fn handles(m: &mut dyn Memory, r: Region) -> Result<Vec<u64>> {
    let at = r.manager + 0x200;
    let head = m.read(at, 16)?;
    let pointer = u64_at(&head, 0)?;
    let size = u32_at(&head, 8)? as usize;
    let capacity = u32_at(&head, 12)? as usize;
    if size > capacity
        || capacity > MAX_HANDLES * 8 * 4
        || size > MAX_HANDLES * 8
        || !size.is_multiple_of(8)
    {
        return Err("Размер реестра не подтверждён".into());
    }
    let raw = if size == 0 {
        vec![]
    } else {
        if pointer == 0 || pointer > 0x0000_7fff_ffff_ffff - size as u64 {
            return Err("Некорректный массив реестра".into());
        }
        m.read(pointer, size)?
    };
    if m.read(at, 16)? != head {
        return Err("Массив реестра изменился при чтении".into());
    }
    Ok(raw
        .chunks_exact(8)
        .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
        .filter(|p| *p != 0 && *p <= 0x0000_7fff_ffef_ffff && p.is_multiple_of(8))
        .collect())
}

fn still_current(m: &mut dyn Memory, avatar: u64, r: Region) -> bool {
    [
        (avatar + 0x1e0, r.root),
        (r.root + 16, r.root_handle),
        (r.root_handle, r.root),
        (r.root + 0x90, r.manager_handle),
        (r.manager_handle, r.manager),
        (r.manager + 16, r.manager_handle),
        (r.manager + 0x28, r.root_handle),
    ]
    .into_iter()
    .all(|(at, expected)| q(m, at).ok() == Some(expected))
}

/// true: адресный реестр доступен, обход старых блоков в этом тике не нужен.
pub(super) fn discover(
    m: &mut dyn Memory,
    scene: &mut Scene,
    rules: &DiscoveryRules,
    cancel: &AtomicBool,
) -> Result<bool> {
    let Some(base) = scene.discovery_profile.as_ref().map(|p| p.base) else {
        return Ok(false);
    };
    let locals: Vec<_> = scene.players.iter().filter(|p| p.local).collect();
    let [player] = locals.as_slice() else {
        return Ok(false);
    };
    let Some(id) = scene
        .identities
        .iter()
        .find(|id| format!("0x{:x}", id.address) == player.avatar_key && id.missed_reads == 0)
    else {
        return Ok(false);
    };
    let avatar = id.address;
    let mut bounded = Bounded {
        inner: m,
        started: Instant::now(),
        bytes: 0,
        exhausted: false,
    };
    // Не выбираем регион по большинству объектов старого сканирования.
    let result = (|| {
        cancelled(cancel)?;
        let Ok(owner) = context::owner(&mut bounded, avatar, base) else {
            return Ok(false);
        };
        if context::local_map(&mut bounded, owner, avatar, base).is_err() {
            return Ok(false);
        }
        run(&mut bounded, scene, rules, avatar, cancel)
    })();
    scene.stats.scanned_bytes = scene
        .stats
        .scanned_bytes
        .saturating_add(bounded.bytes as u64);
    result
}

fn run(
    m: &mut Bounded<'_>,
    scene: &mut Scene,
    rules: &DiscoveryRules,
    avatar: u64,
    cancel: &AtomicBool,
) -> Result<bool> {
    let profile = scene.discovery_profile.clone().ok_or("Нет профиля")?;
    let Ok(current) = region(m, avatar, profile.base) else {
        return Ok(false);
    };
    let Ok(list) = handles(m, current) else {
        return Ok(false);
    };
    if !still_current(m, avatar, current) {
        return Ok(false);
    }
    let mut state = std::mem::take(&mut scene.registry_discovery);
    let result = advance(m, scene, rules, avatar, current, list, &mut state, cancel);
    scene.registry_discovery = state;
    result
}

fn advance(
    m: &mut Bounded<'_>,
    scene: &mut Scene,
    rules: &DiscoveryRules,
    avatar: u64,
    current: Region,
    list: Vec<u64>,
    state: &mut RegistryState,
    cancel: &AtomicBool,
) -> Result<bool> {
    if state.region != Some(current) {
        *state = RegistryState {
            region: Some(current),
            ..Default::default()
        };
    }
    let present: HashSet<_> = list.iter().copied().collect();
    state.new_handles.retain(|h| present.contains(h));
    let mut queued: HashSet<_> = state.new_handles.iter().copied().collect();
    // Новый адрес имеет приоритет даже после перестановки элементов или перевыделения массива.
    let additions: Vec<_> = list
        .iter()
        .copied()
        .filter(|h| !state.previous.contains(h) && queued.insert(*h))
        .collect();
    for handle in additions.into_iter().rev() {
        state.new_handles.push_front(handle);
    }
    state.previous = Arc::new(present);
    state
        .priority
        .retain(|c| state.previous.contains(&c.handle));
    state
        .ordinary
        .retain(|c| state.previous.contains(&c.handle));
    // Изменённый пользователем фильтр сразу влияет и на уже накопленную очередь.
    let candidates = std::mem::take(&mut state.priority)
        .into_iter()
        .chain(std::mem::take(&mut state.ordinary));
    for c in candidates {
        if rules.wants_family(c.family) {
            state.priority.push_back(c);
        } else {
            state.ordinary.push_back(c);
        }
    }
    let known_handles: HashSet<_> = scene.identities.iter().map(|id| id.handle).collect();
    let mut known: HashSet<_> = scene.identities.iter().map(|id| id.address).collect();
    let mut pending: HashSet<_> = state
        .priority
        .iter()
        .chain(&state.ordinary)
        .map(|c| c.handle)
        .collect();
    let targets: Vec<_> = scene
        .discovery_profile
        .as_ref()
        .unwrap()
        .targets()
        .into_iter()
        .filter(|(_, family)| *family != "level")
        .collect();
    let mut visited = HashSet::new();
    let probe_started = Instant::now();
    for step in 0..MAX_PROBES {
        cancelled(cancel)?;
        if list.is_empty()
            || probe_started.elapsed() >= Duration::from_millis(20)
            || m.started.elapsed() >= Duration::from_millis(50)
            || pending.len() >= MAX_PENDING
        {
            break;
        }
        let handle = if step % 2 == 0 && !state.new_handles.is_empty() {
            state.new_handles.pop_front().unwrap()
        } else {
            let h = list[state.cursor % list.len()];
            state.cursor = (state.cursor + 1) % list.len();
            h
        };
        if !visited.insert(handle) || known_handles.contains(&handle) || pending.contains(&handle) {
            continue;
        }
        let candidate = (|| -> Result<Candidate> {
            let address = q(m, handle)?;
            if address == 0 || address > 0x0000_7fff_ffef_ffff {
                return Err("Нет экземпляра".into());
            }
            let head = m.read(address, 24)?;
            let vtable = u64_at(&head, 0)?;
            let family = targets
                .iter()
                .find(|(vt, _)| *vt == vtable)
                .ok_or("Другой тип")?
                .1;
            if u64_at(&head, 16)? != handle || q(m, address + 0x1e0)? != current.root {
                return Err("Изменилась связь экземпляра".into());
            }
            Ok(Candidate {
                address,
                handle,
                vtable,
                metadata: u64_at(&head, 8)?,
                family,
            })
        })();
        if m.exhausted {
            state.new_handles.push_front(handle);
            break;
        }
        if let Ok(candidate) = candidate {
            pending.insert(handle);
            if rules.wants_family(candidate.family) {
                state.priority.push_back(candidate);
            } else {
                state.ordinary.push_back(candidate);
            }
        }
    }
    let mut decoder = Decoder::new(scene.discovery_profile.clone().unwrap())?;
    let mut additions = Vec::new();
    let mut step = 0;
    while m.started.elapsed() < Duration::from_millis(65)
        && m.bytes < MAX_BYTES - 4096
        && scene.objects.len() + additions.len() < super::analyze::MAX_OBJECTS
    {
        cancelled(cancel)?;
        let candidate = if step % 4 != 3 {
            state
                .priority
                .pop_front()
                .or_else(|| state.ordinary.pop_front())
        } else {
            state
                .ordinary
                .pop_front()
                .or_else(|| state.priority.pop_front())
        };
        step += 1;
        let Some(c) = candidate else {
            break;
        };
        if known.contains(&c.address) {
            continue;
        }
        let decoded = super::analyze::object(
            m,
            c.address,
            c.family,
            c.vtable,
            &mut decoder,
            scene.discovery_profile.as_ref().unwrap().base,
        );
        if m.exhausted {
            if rules.wants_family(c.family) {
                state.priority.push_front(c);
            } else {
                state.ordinary.push_front(c);
            }
            break;
        }
        let Ok((object, id)) = decoded else {
            continue;
        };
        if id.handle != c.handle
            || id.metadata != c.metadata
            || q(m, c.address + 0x1e0).ok() != Some(current.root)
            || q(m, c.handle).ok() != Some(c.address)
        {
            continue;
        }
        if let Some(zone) = id.zone
            && q(m, zone + 0x1e0).ok() != Some(current.root)
        {
            continue;
        }
        known.insert(c.address);
        additions.push((object, id, c));
    }
    // Сбой/смена сцены не удаляет карту и не публикует частично проверенную пачку.
    if still_current(m, avatar, current) {
        for (object, id, _) in additions {
            scene.objects.push(object);
            scene.identities.push(id);
        }
    } else {
        for (_, _, c) in additions.into_iter().rev() {
            if rules.wants_family(c.family) {
                state.priority.push_front(c);
            } else {
                state.ordinary.push_front(c);
            }
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Mock {
        bytes: BTreeMap<u64, u8>,
        reads: usize,
    }
    impl Mock {
        fn put(&mut self, at: u64, raw: &[u8]) {
            for (i, b) in raw.iter().enumerate() {
                self.bytes.insert(at + i as u64, *b);
            }
        }
        fn q(&mut self, at: u64, value: u64) {
            self.put(at, &value.to_le_bytes());
        }
        fn spawn(&mut self, at: u64, handle: u64, root: u64) {
            self.put(at, &vec![0; 0x600]);
            self.q(at, 0x2208c58);
            self.q(at + 16, handle);
            self.q(handle, at);
            self.q(at + 0x1e0, root);
            for i in 0..4 {
                self.put(at + 0xa0 + i * 20, &1f32.to_le_bytes());
            }
            for (i, v) in [1f32, 2., 3.].into_iter().enumerate() {
                for off in [0x70, 0xd0] {
                    self.put(at + off + i as u64 * 4, &v.to_le_bytes());
                }
            }
        }
        fn array(&mut self, pointer: u64, handles: &[u64]) {
            self.q(0x3200, pointer);
            self.put(0x3208, &((handles.len() * 8) as u32).to_le_bytes());
            self.put(0x320c, &((handles.len() * 8) as u32).to_le_bytes());
            for (i, h) in handles.iter().enumerate() {
                self.q(pointer + i as u64 * 8, *h);
            }
        }
    }
    impl Memory for Mock {
        fn read(&mut self, at: u64, length: usize) -> Result<Vec<u8>> {
            self.reads += length;
            (0..length)
                .map(|i| {
                    self.bytes
                        .get(&(at + i as u64))
                        .copied()
                        .ok_or("Нет байтов".into())
                })
                .collect()
        }
        fn ranges(&self) -> Vec<MemoryRange> {
            panic!("Обход памяти запрещён в адресном поиске")
        }
        fn modules(&self) -> Vec<MemoryModule> {
            vec![]
        }
    }
    fn fixture() -> (Mock, Scene) {
        let mut m = Mock::default();
        for (at, value) in [
            (0x11e0, 0x2000),
            (0x2000, 0x21cecc0),
            (0x2008, 0x28f21b0),
            (0x2010, 0x2800),
            (0x2800, 0x2000),
            (0x2090, 0x3800),
            (0x3800, 0x3000),
            (0x3000, 0x21d29d8),
            (0x3008, 0x28f3470),
            (0x3010, 0x3800),
            (0x3028, 0x2800),
            (0x21d29d8 + 74 * 8, 0xe03440),
            (0x21d29d8 + 76 * 8, 0xc66420),
        ] {
            m.q(at, value);
        }
        m.put(0x28f3470, &[0; 32]);
        m.q(0x28f3470, 0x203fc60);
        m.put(
            0xe03440,
            &[0x8b, 0x81, 8, 2, 0, 0, 0x48, 0xc1, 0xe8, 3, 0xc3],
        );
        m.put(
            0xc66420,
            &[
                0x48, 0x8b, 0x81, 0, 2, 0, 0, 0x48, 0x89, 2, 0x8b, 0x81, 8, 2, 0, 0, 0x48, 3, 0x81,
                0, 2, 0, 0, 0x48, 0x89, 0x42, 8, 0x48, 0x8b, 0xc2, 0xc3,
            ],
        );
        let mut scene: Scene = serde_json::from_value(serde_json::json!({"format":1,"source":"live","startedAt":"test","capturedAt":"test","complete":true,"profile":"test","objects":[],"meshes":[],"warnings":[],"stats":{"scannedBytes":0,"objectCount":0,"meshCount":0,"vertexCount":0,"faceCount":0}})).unwrap();
        scene.discovery_profile = Some(super::super::profile::Profile {
            base: 0,
            dictionary: vec![],
        });
        (m, scene)
    }
    fn tick(m: &mut Mock, scene: &mut Scene) -> bool {
        run(
            &mut Bounded {
                inner: m,
                started: Instant::now(),
                bytes: 0,
                exhausted: false,
            },
            scene,
            &DiscoveryRules::default(),
            0x1000,
            &AtomicBool::new(false),
        )
        .unwrap()
    }
    #[test]
    fn new_allocations_and_reused_handles_are_found_without_reset_or_global_reads() {
        let (mut m, mut scene) = fixture();
        m.spawn(0x10_000, 0x8000, 0x2000);
        m.q(0x8010, 0);
        m.array(0x4000, &[0x8000, 0x8010]);
        assert!(tick(&mut m, &mut scene));
        assert_eq!(scene.objects.len(), 1);
        let original = scene.objects[0].clone();
        // Объект в новом выделении, массив тоже перевыделен. Старых discovery_ranges нет вообще.
        m.spawn(0x70_000, 0x8020, 0x2000);
        m.array(0x5000, &[0x8000, 0x8010, 0x8020]);
        assert!(tick(&mut m, &mut scene));
        assert_eq!(scene.objects.len(), 2);
        assert_eq!(scene.objects[0], original);
        // Пустая ссылка стала экземпляром без изменения самого массива.
        m.spawn(0x90_000, 0x8010, 0x2000);
        assert!(tick(&mut m, &mut scene));
        assert_eq!(scene.objects.len(), 3);
        m.spawn(0xa0_000, 0x8030, 0xb000); // Чужой регион.
        m.spawn(0xb0_000, 0x8040, 0x2000);
        m.q(0xb0_010, 0x9990); // Неверная обратная ссылка.
        m.array(0x5000, &[0x8000, 0x8010, 0x8020, 0x8030, 0x8040]);
        assert!(tick(&mut m, &mut scene));
        assert_eq!(scene.objects.len(), 3);
        // Испорченный заголовок не превращается в большое чтение и не очищает карту.
        m.put(0x3208, &u32::MAX.to_le_bytes());
        assert!(!tick(&mut m, &mut scene));
        assert_eq!(scene.objects.len(), 3);
        assert!(m.reads < 100_000);
    }

    #[test]
    #[ignore = "Ручная проверка на локальном архиве: PLATSCOPE_REGISTRY_ARCHIVE, _SEQUENCE, _SCENE"]
    fn archived_mission_recovers_objects_from_only_the_local_player() {
        use super::super::{ArchiveMemory, profile::Profile};
        // Архив неизменяем. Кэш адресных чтений отделяет проверку алгоритма от
        // повторной распаковки дисковых блоков, которой в живой игре нет.
        struct Cached {
            source: ArchiveMemory,
            cache: BTreeMap<(u64, usize), Result<Vec<u8>>>,
        }
        impl Memory for Cached {
            fn read(&mut self, at: u64, length: usize) -> Result<Vec<u8>> {
                self.cache
                    .entry((at, length))
                    .or_insert_with(|| self.source.read(at, length))
                    .clone()
            }
            fn ranges(&self) -> Vec<MemoryRange> {
                panic!("Полное сканирование не требуется")
            }
            fn modules(&self) -> Vec<MemoryModule> {
                self.source.modules()
            }
        }
        let archive = std::env::var("PLATSCOPE_REGISTRY_ARCHIVE").unwrap();
        let sequence = std::env::var("PLATSCOPE_REGISTRY_SEQUENCE")
            .unwrap()
            .parse()
            .unwrap();
        let source = std::env::var("PLATSCOPE_REGISTRY_SCENE").unwrap();
        let cancel = AtomicBool::new(false);
        let mut m = Cached {
            source: ArchiveMemory::open(std::path::Path::new(&archive), sequence, &cancel).unwrap(),
            cache: BTreeMap::new(),
        };
        let mut scene: Scene =
            serde_json::from_reader(std::fs::File::open(source).unwrap()).unwrap();
        let expected: HashSet<_> = scene
            .objects
            .iter()
            .filter(|o| o.kind == "feather")
            .map(|o| o.key.clone())
            .collect();
        let profile = Profile::validate(&mut m).unwrap();
        let targets = profile.targets();
        let avatar_vt = targets.iter().find(|(_, f)| *f == "avatar").unwrap().0;
        let mut decoder = Decoder::new(profile.clone()).unwrap();
        let mut avatars = Vec::new();
        for object in &scene.objects {
            let address = u64::from_str_radix(object.key.trim_start_matches("0x"), 16).unwrap();
            if let Ok((object, id)) = super::super::analyze::object(
                &mut m,
                address,
                "avatar",
                avatar_vt,
                &mut decoder,
                profile.base,
            ) {
                avatars.push((object, id));
            }
        }
        scene.objects.clear();
        scene.identities.clear();
        scene.discovery_profile = Some(profile.clone());
        for (object, id) in avatars {
            scene.objects.push(object);
            scene.identities.push(id);
        }
        context::update(&mut m, &mut scene, profile.base, &cancel).unwrap();
        let local = scene
            .players
            .iter()
            .find(|p| p.local)
            .expect("В архиве нужен подтверждённый локальный игрок")
            .avatar_key
            .clone();
        scene.objects.retain(|o| o.key == local);
        scene
            .identities
            .retain(|id| format!("0x{:x}", id.address) == local);
        let initial = scene.objects.len();
        let before = scene.stats.scanned_bytes;
        let rules = DiscoveryRules::parse(vec![
            serde_json::json!([
                "feather",
                "item",
                "/Lotus/Types/Items/MiscItems/ZarimanDogTagCommon"
            ])
            .to_string(),
        ])
        .unwrap();
        let started = Instant::now();
        let mut ticks = 0;
        for _ in 0..3000 {
            ticks += 1;
            assert!(
                discover(&mut m, &mut scene, &rules, &cancel).unwrap(),
                "Реестр не подтверждён"
            );
            let found: HashSet<_> = scene.objects.iter().map(|o| o.key.clone()).collect();
            if !expected.is_empty() && expected.is_subset(&found) {
                break;
            }
            if expected.is_empty() && scene.objects.len() > initial + 200 {
                break;
            }
            if started.elapsed() > Duration::from_secs(150) {
                break;
            }
        }
        let found: HashSet<_> = scene.objects.iter().map(|o| o.key.clone()).collect();
        eprintln!(
            "Диагностика: local={local}, объектов={}, реестр={:?}, курсор={}, ссылки={}, новых={}, очереди={}/{}, байт={}",
            scene.objects.len(),
            scene.registry_discovery.region,
            scene.registry_discovery.cursor,
            scene.registry_discovery.previous.len(),
            scene.registry_discovery.new_handles.len(),
            scene.registry_discovery.priority.len(),
            scene.registry_discovery.ordinary.len(),
            scene.stats.scanned_bytes - before
        );
        assert!(
            expected.is_subset(&found),
            "Не найдены перья: {:?}",
            expected.difference(&found).collect::<Vec<_>>()
        );
        assert!(scene.objects.len() > initial + 10);
        assert!(scene.discovery_ranges.is_empty());
        eprintln!(
            "Из одного игрока найдено {} объектов, перьев {}/{}, прочитано адресно {} байт; циклов {ticks}, архив {:.2} с",
            scene.objects.len() - initial,
            scene.objects.iter().filter(|o| o.kind == "feather").count(),
            expected.len(),
            scene.stats.scanned_bytes - before,
            started.elapsed().as_secs_f64()
        );
    }
}
