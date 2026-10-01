//! Локальное восстановление адресов только для прежней, независимо проверяемой схемы.
//! Здесь нет выполнения кода игры, изменения памяти или принятия неподписанного пакета.
use super::*;
use crate::spatial::{AnalysisProgress, cancelled, recovery_image::ModuleImage, types::Decoder};
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{Mutex, OnceLock, atomic::AtomicBool},
    time::Instant,
};

const COOLDOWN: Duration = Duration::from_secs(15 * 60);
const MAX_TIME: Duration = Duration::from_secs(20);
const MAX_INDIRECT_READS: usize = 4096;
const NATIVE_TYPES: [(u64, &str); 13] = [
    (0x28ed2a0, "PickUp *"),
    (0x297bf70, "TennoAvatar *"),
    (0x29c9600, "LotusHumanPlayer *"),
    (0x29e7ea0, "MiniMap *"),
    (0x28ecf80, "MultiAvatarTrigger *"),
    (0x28d6f40, "Waypoint *"),
    (0x2960650, "CipherAction *"),
    (0x28ce8c0, "ContextAction *"),
    (0x28dd4e0, "Zone *"),
    (0x28cc310, "Camera *"),
    (0x29cb220, "LotusOperatorAvatar *"),
    (0x28ac110, "AnimScene *"),
    (0x28f3470, "RegionMgrImpl *"),
];

// Это отпечатки расположения собственной таблицы Evolution Engine относительно строк,
// подтверждённые на двух архивах. Изменение этой схемы должно давать отказ.
const TABLE_ANCHORS: [(u64, &str, i64); 16] = [
    (0x21a96f8, "PickUp", 12),
    (0x23fb1e0, "csi.mItemCount", 48),
    (0x2326098, "LotusMainMenuAvatar", 24),
    (0x22e25b0, "LotusNpcAvatar", 400),
    (0x211bfe8, "SpawnPlayers", 400),
    (0x20fd898, "Decoration", 16),
    (0x2341320, "TINT_MASK_RGBA", 16),
    (0x2208c58, "SpawnAgent", 16),
    (0x21a6fb0, "OnUnfilled", 16),
    (0x2113b98, "Waypoint", -0x5d0),
    (0x22a1638, "CipherAction", 16),
    (0x213cf68, "OnFailed", 16),
    (0x21277b0, "NavMesh", 16),
    (0x21cecc0, "ContextImpl", 16),
    (0x21d29d8, "Parallel Layers", 16),
    (DECREE_FRAGMENT_RVA, "LotusFocusPickUp", 24),
];

static FAILED: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalProfile {
    format: u32,
    image_sha256: String,
    spec: ProfileSpec,
}

pub(crate) struct RecoveryAttempt {
    pub profile: Profile,
    pub cached: bool,
    key: String,
}

impl RecoveryAttempt {
    pub fn reject(&self) {
        remember_failure(&self.key);
    }

    /// Вызывается только после проверки реальной сцены, а не после совпадения строк.
    pub fn save(&self, directory: &Path) -> Result<()> {
        if self.cached {
            return Ok(());
        }
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let target = cache_path(directory, &self.key);
        let temporary = target.with_extension(format!("{}-tmp", std::process::id()));
        let bytes = serde_json::to_vec(&LocalProfile {
            format: 1,
            image_sha256: self.key.clone(),
            spec: (*self.profile.spec).clone(),
        })
        .map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, &target).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn cache_path(directory: &Path, key: &str) -> PathBuf {
    directory.join(format!("recovered-v1-{key}.json"))
}

fn remember_failure(key: &str) {
    if let Ok(mut failed) = FAILED.get_or_init(Default::default).lock() {
        failed.retain(|_, at| at.elapsed() < COOLDOWN);
        if failed.len() >= 8 {
            failed.clear();
        }
        failed.insert(key.into(), Instant::now());
    }
}

struct Budget<'a> {
    started: Instant,
    cancel: &'a AtomicBool,
    probes: usize,
}

impl Budget<'_> {
    fn step(&mut self) -> Result<()> {
        cancelled(self.cancel)?;
        self.probes += 1;
        if self.probes > MAX_INDIRECT_READS || self.started.elapsed() >= MAX_TIME {
            return Err("Достигнут предел автоматического поиска адресов".into());
        }
        Ok(())
    }
}

impl Profile {
    pub(crate) fn recover(
        m: &mut dyn Memory,
        pack: &ProfilePack,
        directory: &Path,
        cancel: &AtomicBool,
        progress: &impl Fn(AnalysisProgress),
    ) -> Result<RecoveryAttempt> {
        progress(AnalysisProgress {
            stage: "Поиск знакомых структур обновлённой игры".into(),
            ..Default::default()
        });
        let image = ModuleImage::load(m, cancel)?;
        let key = image.fingerprint();
        let mut budget = Budget {
            started: Instant::now(),
            cancel,
            probes: 0,
        };
        if let Ok(bytes) = bounded_file(&cache_path(directory, &key), MAX_PACK_BYTES)
            && let Ok(local) = serde_json::from_slice::<LocalProfile>(&bytes)
            && local.format == 1
            && local.image_sha256 == key
            && local.spec.id == format!("warframe-auto-{key}")
            && local.spec.module_size == image.module.size
            && pack
                .profiles
                .iter()
                .any(|p| p.offsets == local.spec.offsets)
            && pack.profiles.iter().any(|p| {
                p.registry_count_hex == local.spec.registry_count_hex
                    && p.registry_range_hex == local.spec.registry_range_hex
            })
        {
            let mut profile = Self {
                base: image.module.base,
                dictionary: Vec::new(),
                spec: Arc::new(local.spec),
            };
            if validate_shape(&profile.spec).is_ok()
                && validate_layout(m, &image, &profile, &mut budget).is_ok()
                && profile.validate_fingerprints(m).is_ok()
                && profile.validate_memory(m).is_ok()
                && validate_properties(m, &profile).is_ok()
            {
                return Ok(RecoveryAttempt {
                    profile,
                    cached: true,
                    key,
                });
            }
        }
        cancelled(cancel)?;
        budget.probes = 0;
        if FAILED
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| "Не удалось проверить время предыдущей попытки")?
            .get(&key)
            .is_some_and(|at| at.elapsed() < COOLDOWN)
        {
            return Err("Автоматический поиск уже не смог подтвердить эту сборку. Следующая попытка доступна через 15 минут.".into());
        }
        let result = discover(m, pack, &image, &key, &mut budget, progress);
        if result.is_err() && !cancel.load(std::sync::atomic::Ordering::Relaxed) {
            remember_failure(&key);
        }
        result.map(|profile| RecoveryAttempt {
            profile,
            cached: false,
            key,
        })
    }
}

fn validate_shape(spec: &ProfileSpec) -> Result<()> {
    let required: BTreeSet<_> = EXPECTED_RVAS
        .iter()
        .copied()
        .filter(|key| ![0x27c2a0, 0xabc2e0].contains(key))
        .chain([DECREE_FRAGMENT_RVA])
        .collect();
    let keys: BTreeSet<_> = spec.rvas.iter().map(|e| e.key).collect();
    let checks: BTreeSet<_> = spec.code_fingerprints.iter().map(|e| e.rva).collect();
    if keys != required
        || keys.len() != spec.rvas.len()
        || checks != BTreeSet::from([0xe03440, 0xc66420])
        || spec.code_fingerprints.len() != 2
        || !(1_048_576..=64 * 1024 * 1024).contains(&spec.module_size)
        || !valid_hex(&spec.dictionary_sha256, 64)
        || spec.vtable_slot3_rva >= spec.module_size
        || spec.rvas.iter().any(|e| e.current >= spec.module_size)
        || spec
            .code_fingerprints
            .iter()
            .any(|e| !valid_hex(&e.hex, 64))
        || !valid_hex_range(&spec.registry_count_hex, 8, 64)
        || !valid_hex_range(&spec.registry_range_hex, 16, 128)
        || [
            spec.offsets.dictionary_context,
            spec.offsets.minimap_link,
            spec.offsets.second_zone_array,
        ]
        .iter()
        .any(|offset| *offset == 0 || *offset > 0x20_000)
    {
        return Err("Локально восстановленная схема адресов повреждена".into());
    }
    Ok(())
}

fn unique(values: BTreeSet<u64>, description: &str) -> Result<u64> {
    if values.len() != 1 {
        return Err(format!(
            "Не удалось однозначно подтвердить {description}: совпадений {}",
            values.len()
        ));
    }
    Ok(*values.first().expect("one value"))
}

fn image_q(image: &ModuleImage, address: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(
        image
            .bytes(address, 8)?
            .try_into()
            .map_err(|_| "Размер адреса")?,
    ))
}

fn named_metadata(
    m: &mut dyn Memory,
    image: &ModuleImage,
    name: &str,
    budget: &mut Budget<'_>,
) -> Result<u64> {
    let mut matches = BTreeSet::new();
    let needle = format!("{name}\0");
    for text in image.find(needle.as_bytes())? {
        for reference in image.pointer_references(text)? {
            let Some(binding) = reference.checked_sub(8) else {
                continue;
            };
            for reference in image.pointer_references(binding)? {
                budget.step()?;
                let Some(meta) = reference.checked_sub(0x60) else {
                    continue;
                };
                if !meta.is_multiple_of(8) || !image.is_data(meta) {
                    continue;
                }
                if image_q(image, meta + 0x60).ok() == Some(binding)
                    && image_q(image, binding + 8).ok() == Some(text)
                    && string(m, text).ok().as_deref() == Some(name)
                    && image_q(image, meta).is_ok_and(|vt| image.is_data(vt))
                    && image_q(image, meta + 0x78).is_ok_and(|ctor| image.is_code(ctor))
                {
                    matches.insert(meta);
                }
            }
        }
    }
    unique(matches, &format!("тип {name}"))
}

fn valid_table(image: &ModuleImage, table: u64) -> bool {
    table.is_multiple_of(8)
        && image.is_data(table)
        && (0..4).all(|slot| image_q(image, table + slot * 8).is_ok_and(|p| image.is_code(p)))
}

fn anchored_table(image: &ModuleImage, name: &str, distance: i64) -> Result<u64> {
    let needle = format!("{name}\0");
    let matches = image
        .find(needle.as_bytes())?
        .into_iter()
        .filter(|address| {
            // Перед именем иногда стоит двоичный hash, последним байтом которого
            // может быть цифра. Четыре буквы префикса исключают CrewshipDecoration.
            image
                .bytes(address.saturating_sub(4), 4)
                .is_ok_and(|b| b.iter().any(|v| !v.is_ascii_alphanumeric() && *v != b'_'))
        })
        .filter_map(|address| address.checked_add_signed(distance))
        .filter(|address| valid_table(image, *address))
        .collect();
    unique(matches, &format!("таблицу {name}"))
}

// Принимаем только короткие E9-переходы и LEA RAX,[RIP+disp32], за которым
// непосредственно следует запись в self. Код лишь читается, никогда не вызывается.
fn constructor_tables(image: &ModuleImage, mut ctor: u64) -> Result<BTreeSet<u64>> {
    let mut seen = BTreeSet::new();
    for _ in 0..4 {
        if !image.is_code(ctor) || !seen.insert(ctor) {
            return Err("Не подтверждён конструктор типа".into());
        }
        let head = image.bytes(ctor, 5)?;
        if head[0] != 0xe9 {
            break;
        }
        let displacement =
            i32::from_le_bytes(head[1..5].try_into().map_err(|_| "Размер перехода")?);
        ctor = (ctor + 5)
            .checked_add_signed(i64::from(displacement))
            .ok_or("Переполнение перехода")?;
    }
    let raw = image.function_bytes(ctor, 160)?;
    let mut tables = BTreeSet::new();
    for offset in memchr::memmem::find_iter(raw, &[0x48, 0x8d, 0x05]) {
        let Some(bytes) = raw.get(offset + 3..offset + 7) else {
            continue;
        };
        let tail = &raw[offset + 7..raw.len().min(offset + 23)];
        if ![
            &[0x48, 0x89, 0x01][..],
            &[0x48, 0x89, 0x03][..],
            &[0x48, 0x89, 0x06][..],
        ]
        .iter()
        .any(|store| memchr::memmem::find(tail, store).is_some())
        {
            continue;
        }
        let displacement =
            i32::from_le_bytes(bytes.try_into().map_err(|_| "Размер адреса таблицы")?);
        if let Some(table) = (ctor + offset as u64 + 7).checked_add_signed(i64::from(displacement))
            && valid_table(image, table)
        {
            tables.insert(table);
        }
    }
    Ok(tables)
}

fn root_metadata(
    image: &ModuleImage,
    native: u64,
    root_table: u64,
    budget: &mut Budget<'_>,
) -> Result<u64> {
    let mut matches = BTreeSet::new();
    for meta in image.pointer_references(native)? {
        budget.step()?;
        if image_q(image, meta + 0x60).ok() != Some(0) {
            continue;
        }
        if let Ok(ctor) = image_q(image, meta + 0x78)
            && constructor_tables(image, ctor).is_ok_and(|tables| tables.contains(&root_table))
        {
            matches.insert(meta);
        }
    }
    unique(matches, "тип текущего региона")
}

fn validate_layout(
    m: &mut dyn Memory,
    image: &ModuleImage,
    profile: &Profile,
    budget: &mut Budget<'_>,
) -> Result<()> {
    for (key, name) in NATIVE_TYPES {
        budget.step()?;
        if named_metadata(m, image, name, budget)? != profile.address(key)? {
            return Err("Изменились адреса типов игры".into());
        }
    }
    for (key, name, distance) in TABLE_ANCHORS {
        budget.step()?;
        if anchored_table(image, name, distance)? != profile.address(key)? {
            return Err("Изменились адреса таблиц игры".into());
        }
    }
    let native = profile.address(0x203fc60)?;
    let object = native
        .checked_sub(284)
        .ok_or("Граница структуры метаданных")?;
    if image.bytes(object, 7)? != b"Object\0" {
        return Err("Не подтверждена схема метаданных".into());
    }
    for (key, delta) in [
        (0x203fb50, 12),
        (0x203fba8, 100),
        (0x203fc00, 188),
        (0x203fc60, 284),
    ] {
        if profile.address(key)? != object + delta || !valid_table(image, object + delta) {
            return Err("Не подтверждена схема метаданных".into());
        }
    }
    if root_metadata(image, native, profile.address(0x21cecc0)?, budget)?
        != profile.address(0x28f21b0)?
    {
        return Err("Не подтверждён тип текущего региона".into());
    }
    let object_meta = named_metadata(m, image, "Object *", budget)?;
    if image_q(image, object_meta)? != native
        || image.bytes(object_meta + 0x80, 8)? != [0x26, 0, 0, 0, 1, 0, 0, 0]
        || profile.address(0x28a4888)? != object_meta + 0x88
    {
        return Err("Не подтверждена связь словаря с типом Object".into());
    }
    Ok(())
}

fn validate_properties(m: &mut dyn Memory, profile: &Profile) -> Result<()> {
    let mut decoder = Decoder::new(profile.clone())?;
    let mut nonempty = 0;
    for (key, name) in NATIVE_TYPES.into_iter().filter(|(key, _)| {
        // Эти типы разбираются тем же Decoder, что и реальные объекты карты.
        // У игрока/камеры есть дополнительная нативная схема без текстовых свойств;
        // их проверяют отдельные двусторонние связи в context, а не Decoder.
        [
            0x28ed2a0, 0x297bf70, 0x28ecf80, 0x28d6f40, 0x2960650, 0x28ce8c0, 0x28dd4e0,
        ]
        .contains(key)
    }) {
        let info = decoder
            .chain(m, profile.address(key)?)
            .map_err(|error| format!("{name}: {error}"))?;
        if !info.names.iter().any(|n| n == name) {
            return Err("Словарь не согласован с типами игры".into());
        }
        if info.properties.iter().any(|p| !p.is_empty()) {
            nonempty += 1
        }
    }
    if nonempty < 4 {
        return Err("Недостаточно подтверждённых свойств типов игры".into());
    }
    Ok(())
}

fn discover(
    m: &mut dyn Memory,
    pack: &ProfilePack,
    image: &ModuleImage,
    key: &str,
    budget: &mut Budget<'_>,
    progress: &impl Fn(AnalysisProgress),
) -> Result<Profile> {
    let base = image.module.base;
    let mut addresses = BTreeMap::new();
    for (key, name) in NATIVE_TYPES {
        addresses.insert(key, named_metadata(m, image, name, budget)?);
    }
    let native = image_q(image, addresses[&0x28ed2a0])?;
    if NATIVE_TYPES
        .iter()
        .any(|(key, _)| image_q(image, addresses[key]).ok() != Some(native))
    {
        return Err("Типы игры используют разные схемы метаданных".into());
    }
    let object = native
        .checked_sub(284)
        .ok_or("Граница структуры метаданных")?;
    if image.bytes(object, 7)? != b"Object\0" {
        return Err("Не подтверждена схема метаданных".into());
    }
    for (key, delta) in [
        (0x203fb50, 12),
        (0x203fba8, 100),
        (0x203fc00, 188),
        (0x203fc60, 284),
    ] {
        if !valid_table(image, object + delta) {
            return Err("Не подтверждена таблица метаданных".into());
        }
        addresses.insert(key, object + delta);
    }
    for (key, name, distance) in TABLE_ANCHORS {
        budget.step()?;
        addresses.insert(key, anchored_table(image, name, distance)?);
    }
    let slot3 = image_q(image, addresses[&0x21a96f8] + 24)?;
    if image.bytes(slot3, 3)? != [0xc2, 0, 0]
        || TABLE_ANCHORS
            .iter()
            .any(|(key, _, _)| image_q(image, addresses[key] + 24).ok() != Some(slot3))
    {
        return Err("Изменилась схема таблиц объектов".into());
    }
    addresses.insert(
        0x28f21b0,
        root_metadata(image, native, addresses[&0x21cecc0], budget)?,
    );
    let manager = addresses[&0x21d29d8];
    let count = image_q(image, manager + 74 * 8)?;
    let range = image_q(image, manager + 76 * 8)?;
    let registry = pack
        .profiles
        .iter()
        .rev()
        .find(|spec| {
            hex::decode(&spec.registry_count_hex)
                .ok()
                .is_some_and(|b| image.bytes(count, b.len()).ok() == Some(b.as_slice()))
                && hex::decode(&spec.registry_range_hex)
                    .ok()
                    .is_some_and(|b| image.bytes(range, b.len()).ok() == Some(b.as_slice()))
        })
        .ok_or("Методы реестра игры изменились")?;
    addresses.insert(0xe03440, count);
    addresses.insert(0xc66420, range);
    // Старые два адреса были лишь произвольными отпечатками кода и картой не используются.
    // Локальная схема вместо них фиксирует два независимо найденных и проверенных метода.
    let fingerprints = [(0xe03440, count), (0xc66420, range)]
        .into_iter()
        .map(|(rva, address)| {
            Ok(CodeFingerprint {
                rva,
                hex: hex::encode(image.bytes(address, 32)?),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    progress(AnalysisProgress {
        stage: "Проверка найденных типов и словаря игры".into(),
        ..Default::default()
    });
    let offsets: BTreeSet<_> = pack
        .profiles
        .iter()
        .map(|s| s.offsets.dictionary_context)
        .collect();
    // Изменённые поля внутри игрока/мини-карты не угадываем. Пробуем только известную
    // новую схему; реальные двусторонние связи и два списка зон проверяет построение сцены.
    let template = pack.profiles.last().ok_or("Нет исходной схемы игры")?;
    let mut spec = ProfileSpec {
        id: format!("warframe-auto-{key}"),
        module_size: image.module.size,
        code_fingerprints: fingerprints,
        dictionary_sha256: String::new(),
        vtable_slot3_rva: slot3 - base,
        registry_count_hex: registry.registry_count_hex.clone(),
        registry_range_hex: registry.registry_range_hex.clone(),
        rvas: addresses
            .iter()
            .map(|(key, value)| RvaEntry {
                key: *key,
                current: value - base,
            })
            .collect(),
        offsets: template.offsets.clone(),
    };
    let object_meta = named_metadata(m, image, "Object *", budget)?;
    if image_q(image, object_meta)? != native
        || image.bytes(object_meta + 0x80, 8)? != [0x26, 0, 0, 0, 1, 0, 0, 0]
    {
        return Err("Изменилась схема контекста словаря игры".into());
    }
    let global = object_meta + 0x88;
    let context = q(m, global)?;
    let mut matching = Vec::new();
    let mut validation_error = None;
    {
        for offset in &offsets {
            budget.step()?;
            let Some(at) = context.checked_add(*offset) else {
                continue;
            };
            let Ok(ddict) = q(m, at) else { continue };
            let Ok(header) = m.read(ddict, 24) else {
                continue;
            };
            if header.len() != 24 {
                continue;
            }
            let pointer = u64::from_le_bytes(header[8..16].try_into().map_err(|_| "Словарь")?);
            let size = u64::from_le_bytes(header[16..24].try_into().map_err(|_| "Словарь")?);
            if size != 1_048_576 || pointer == 0 || pointer > 0x0000_7fff_ffef_ffff {
                continue;
            }
            let Ok(dictionary) = m.read(pointer, size as usize) else {
                continue;
            };
            if dictionary.get(..4) != Some(&[0x37, 0xa4, 0x30, 0xec]) {
                continue;
            }
            spec.dictionary_sha256 = hex::encode(Sha256::digest(&dictionary));
            spec.offsets.dictionary_context = *offset;
            spec.rvas.retain(|e| e.key != 0x28a4888);
            spec.rvas.push(RvaEntry {
                key: 0x28a4888,
                current: global - base,
            });
            let mut profile = Profile {
                base,
                dictionary,
                spec: Arc::new(spec.clone()),
            };
            let validated = validate_shape(&profile.spec)
                .and_then(|_| profile.validate_fingerprints(m))
                .and_then(|_| profile.validate_memory(m))
                .and_then(|_| validate_properties(m, &profile));
            match validated {
                Ok(()) => matching.push(profile),
                Err(error) => validation_error = Some(error),
            }
        }
    }
    if matching.len() != 1 {
        if let Some(error) = validation_error {
            return Err(format!(
                "Не подтверждена восстановленная схема игры: {error}"
            ));
        }
        return Err(format!(
            "Не удалось однозначно подтвердить словарь игры: совпадений {}",
            matching.len()
        ));
    }
    Ok(matching.remove(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial::{ArchiveMemory, MemoryModule, MemoryRange, analyze};

    struct ChangedCode<'a> {
        inner: &'a mut dyn Memory,
        address: u64,
    }
    impl Memory for ChangedCode<'_> {
        fn read(&mut self, address: u64, length: usize) -> Result<Vec<u8>> {
            let mut bytes = self.inner.read(address, length)?;
            if let Some(offset) = self.address.checked_sub(address)
                && offset < bytes.len() as u64
            {
                bytes[offset as usize] ^= 1;
            }
            Ok(bytes)
        }
        fn modules(&self) -> Vec<MemoryModule> {
            self.inner.modules()
        }
        fn ranges(&self) -> Vec<MemoryRange> {
            self.inner.ranges()
        }
    }

    /// Существенный сценарий: новый EXE, новый словарь и адреса, но прежняя схема.
    /// Собственный подписанный профиль сборки специально исключён из исходных данных.
    #[test]
    #[ignore = "Требует локальную двоичную запись миссии в PLATSCOPE_RECOVERY_ARCHIVE"]
    fn unknown_archive_recovers_scene_and_rejects_changed_schema() {
        let path = PathBuf::from(std::env::var_os("PLATSCOPE_RECOVERY_ARCHIVE").expect("archive"));
        let sequence = std::env::var("PLATSCOPE_RECOVERY_SNAPSHOT")
            .unwrap_or_else(|_| "2".into())
            .parse()
            .unwrap();
        let cancel = AtomicBool::new(false);
        let mut memory = ArchiveMemory::open(&path, sequence, &cancel).unwrap();
        let mut pack = ProfilePack::bundled().unwrap();
        let module = memory
            .modules()
            .into_iter()
            .find(|m| m.name == "Warframe.x64.exe")
            .unwrap();
        let expected = (*Profile::validate_with_pack(&mut memory, &pack)
            .unwrap()
            .spec)
            .clone();
        pack.profiles.retain(|p| p.module_size != module.size);
        assert!(Profile::validate_with_pack(&mut memory, &pack).is_err());
        let directory = tempfile::tempdir().unwrap();
        let attempt =
            Profile::recover(&mut memory, &pack, directory.path(), &cancel, &|_| {}).unwrap();
        assert!(!attempt.cached);
        for entry in &attempt.profile.spec.rvas {
            let signed = expected.rvas.iter().find(|e| e.key == entry.key).unwrap();
            assert_eq!(entry.current, signed.current, "key 0x{:x}", entry.key);
        }
        let scene = analyze::run_with_profile(
            &mut memory,
            attempt.profile.clone(),
            "archive",
            memory_started(&path),
            true,
            &cancel,
            &|_| {},
        )
        .unwrap();
        analyze::validate_recovered_scene(&mut memory, &scene, &attempt.profile).unwrap();
        attempt.save(directory.path()).unwrap();
        let cached =
            Profile::recover(&mut memory, &pack, directory.path(), &cancel, &|_| {}).unwrap();
        assert!(cached.cached);
        // Изменённый локальный адрес словаря не принимается даже с прежним отпечатком EXE.
        let mut wrong = cached.profile.clone();
        let mut spec = (*wrong.spec).clone();
        spec.rvas
            .iter_mut()
            .find(|e| e.key == 0x28a4888)
            .unwrap()
            .current += 8;
        wrong.spec = Arc::new(spec);
        let image = ModuleImage::load(&mut memory, &cancel).unwrap();
        assert!(
            validate_layout(
                &mut memory,
                &image,
                &wrong,
                &mut Budget {
                    started: Instant::now(),
                    cancel: &cancel,
                    probes: 0
                }
            )
            .is_err()
        );
        // Частичная порча кэша после проверки всех адресов должна позволять новый поиск.
        let cache = cache_path(directory.path(), &cached.key);
        let mut local: LocalProfile = serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
        local.spec.dictionary_sha256 = "0".repeat(64);
        fs::write(&cache, serde_json::to_vec(&local).unwrap()).unwrap();
        let repaired =
            Profile::recover(&mut memory, &pack, directory.path(), &cancel, &|_| {}).unwrap();
        assert!(!repaired.cached);
        assert_eq!(
            repaired.profile.spec.dictionary_sha256,
            expected.dictionary_sha256
        );
        // Изменение метода реестра — изменение схемы, а не ещё один перенос адреса.
        let count = cached.profile.address(0xe03440).unwrap();
        let mut changed = ChangedCode {
            inner: &mut memory,
            address: count,
        };
        let other_cache = tempfile::tempdir().unwrap();
        assert!(
            Profile::recover(&mut changed, &pack, other_cache.path(), &cancel, &|_| {}).is_err()
        );
        println!(
            "Восстановлено: {} объектов, {} частей геометрии, {} зон; кэш проверен и восстановлен после порчи, изменение схемы отклонено",
            scene.objects.len(),
            scene.meshes.len(),
            scene.zones.len()
        );
    }

    fn memory_started(path: &Path) -> String {
        path.display().to_string()
    }
}
