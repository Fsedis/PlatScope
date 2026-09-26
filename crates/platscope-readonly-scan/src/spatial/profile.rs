use super::source::{q, string};
use super::{Memory, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::Arc,
    time::Duration,
};

const MAX_PACK_BYTES: usize = 256 * 1024;
const CURRENT_URL: &str = "https://raw.githubusercontent.com/Fsedis/PlatScope/main/crates/platscope-readonly-scan/src/spatial/profiles/current-v2.signed.json";
const EXPECTED_RVAS: [u64; 38] = [
    0x27c2a0, 0xabc2e0, 0x203fb50, 0x203fba8, 0x203fc00, 0x203fc60, 0x28ed2a0, 0x297bf70,
    0x29c9600, 0x29e7ea0, 0x28ecf80, 0x28d6f40, 0x2960650, 0x28ce8c0, 0x28a4888, 0x21a96f8,
    0x23fb1e0, 0x2326098, 0x22e25b0, 0x211bfe8, 0x20fd898, 0x2341320, 0x2208c58, 0x21a6fb0,
    0x2113b98, 0x21277b0, 0x22a1638, 0x213cf68, 0x28dd4e0, 0x28cc310, 0x29cb220, 0x28ac110,
    0x28f3470, 0x21cecc0, 0x28f21b0, 0x21d29d8, 0xe03440, 0xc66420,
];
const DECREE_FRAGMENT_RVA: u64 = 0x2317cc8;
const DECREE_FRAGMENT_PROFILE_ID: &str = "warframe-2026-09-26-validated";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfilePack {
    schema_version: u32,
    pub revision: u64,
    profiles: Vec<ProfileSpec>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileSpec {
    id: String,
    module_size: u64,
    code_fingerprints: Vec<CodeFingerprint>,
    dictionary_sha256: String,
    vtable_slot3_rva: u64,
    registry_count_hex: String,
    registry_range_hex: String,
    rvas: Vec<RvaEntry>,
    offsets: ProfileOffsets,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CodeFingerprint {
    rva: u64,
    hex: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RvaEntry {
    key: u64,
    current: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileOffsets {
    dictionary_context: u64,
    minimap_link: u64,
    second_zone_array: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheEnvelope {
    payload: String,
    signature: String,
}

impl ProfilePack {
    pub fn bundled() -> Result<Self> {
        Self::from_signed(
            include_bytes!("profiles/bundled.json"),
            include_bytes!("profiles/bundled.json.sig"),
        )
    }

    pub fn from_signed(bytes: &[u8], encoded_signature: &[u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_PACK_BYTES || encoded_signature.len() > 2048 {
            return Err("Размер пакета профилей недопустим".into());
        }
        let encoded = std::str::from_utf8(encoded_signature)
            .map_err(|_| "Подпись профилей повреждена")?
            .trim();
        let signature_text = STANDARD
            .decode(encoded)
            .map_err(|_| "Подпись профилей повреждена")?;
        let signature_text =
            std::str::from_utf8(&signature_text).map_err(|_| "Подпись профилей повреждена")?;
        let signature =
            Signature::decode(signature_text).map_err(|_| "Подпись профилей повреждена")?;
        let public_key = PublicKey::decode(include_str!("profiles/updater.pub"))
            .map_err(|_| "Открытый ключ профилей повреждён")?;
        public_key
            .verify(bytes, &signature, false)
            .map_err(|_| "Подпись профилей не совпадает")?;
        let pack: Self =
            serde_json::from_slice(bytes).map_err(|_| "Формат пакета профилей повреждён")?;
        pack.validate_shape()?;
        Ok(pack)
    }

    /// Загружает только подписанные пакеты из каталога приложения; повреждённые файлы игнорируются.
    pub fn latest_cached(cache_dir: &Path) -> Result<Self> {
        let mut latest = Self::bundled()?;
        let Ok(entries) = fs::read_dir(cache_dir) else {
            return Ok(latest);
        };
        let mut candidates = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if valid_cache_name(&name) && entry.file_type().is_ok_and(|kind| kind.is_file()) {
                candidates.push(entry.path());
            }
        }
        candidates.sort();
        for path in candidates {
            let Ok(bytes) = bounded_file(&path, MAX_PACK_BYTES * 2) else {
                continue;
            };
            let Ok(cache) = serde_json::from_slice::<CacheEnvelope>(&bytes) else {
                continue;
            };
            let Ok(payload) = STANDARD.decode(cache.payload) else {
                continue;
            };
            let Ok(pack) = Self::from_signed(&payload, cache.signature.as_bytes()) else {
                continue;
            };
            if pack.revision > latest.revision && pack.validate_extension_of(&latest).is_ok() {
                latest = pack;
            }
        }
        Ok(latest)
    }

    /// Проверяет подпись до записи и принимает только более новую ревизию.
    pub fn install_signed(
        cache_dir: &Path,
        bytes: &[u8],
        signature: &[u8],
    ) -> Result<Option<Self>> {
        let pack = Self::from_signed(bytes, signature)?;
        let previous = Self::latest_cached(cache_dir)?;
        if pack.revision <= previous.revision {
            return Ok(None);
        }
        pack.validate_extension_of(&previous)?;
        fs::create_dir_all(cache_dir)
            .map_err(|e| format!("Не удалось создать каталог профилей: {e}"))?;
        let digest = hex::encode(Sha256::digest(bytes));
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let filename = format!(
            "pack-{:020}-{digest}-{}-{nonce}.json",
            pack.revision,
            std::process::id()
        );
        let target = cache_dir.join(filename);
        let temporary = target.with_extension("tmp");
        let envelope = CacheEnvelope {
            payload: STANDARD.encode(bytes),
            signature: std::str::from_utf8(signature)
                .map_err(|_| "Подпись профилей повреждена")?
                .trim()
                .to_owned(),
        };
        let encoded = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| format!("Не удалось сохранить профиль: {e}"))?;
        let result = file
            .write_all(&encoded)
            .and_then(|()| file.sync_all())
            .and_then(|()| fs::rename(&temporary, &target));
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(format!("Не удалось сохранить профиль: {error}"));
        }
        Ok(Some(pack))
    }

    pub fn download_newer(cache_dir: &Path) -> Result<Option<Self>> {
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(8))
            .build()
            .map_err(|e| e.to_string())?;
        let encoded = download_bounded(&client, CURRENT_URL, MAX_PACK_BYTES * 2)?;
        let envelope: CacheEnvelope = serde_json::from_slice(&encoded)
            .map_err(|_| "Формат опубликованного пакета профилей повреждён")?;
        let json = STANDARD
            .decode(envelope.payload)
            .map_err(|_| "Данные опубликованного пакета профилей повреждены")?;
        Self::install_signed(cache_dir, &json, envelope.signature.as_bytes())
    }

    fn validate_shape(&self) -> Result<()> {
        if self.schema_version != 1
            || self.revision == 0
            || self.profiles.is_empty()
            || self.profiles.len() > 32
        {
            return Err("Версия или размер пакета профилей не поддерживается".into());
        }
        let mut names = std::collections::BTreeSet::new();
        for spec in &self.profiles {
            if !names.insert(spec.id.as_str())
                || spec.id.len() > 80
                || spec.id.is_empty()
                || !spec
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || !(1_048_576..=1_073_741_824).contains(&spec.module_size)
                || !valid_hex(&spec.dictionary_sha256, 64)
                || spec.vtable_slot3_rva >= spec.module_size
                || !valid_hex_range(&spec.registry_count_hex, 8, 64)
                || !valid_hex_range(&spec.registry_range_hex, 16, 128)
                || !(2..=8).contains(&spec.code_fingerprints.len())
            {
                return Err("Некорректные сведения о версии игры".into());
            }
            let mut map = BTreeMap::new();
            for entry in &spec.rvas {
                if entry.current >= spec.module_size
                    || map.insert(entry.key, entry.current).is_some()
                {
                    return Err("Некорректная таблица адресов игры".into());
                }
            }
            let has_decree_fragment = map.contains_key(&DECREE_FRAGMENT_RVA);
            if map.len() != EXPECTED_RVAS.len() + usize::from(has_decree_fragment)
                || EXPECTED_RVAS.iter().any(|rva| !map.contains_key(rva))
            {
                return Err("В профиле указаны не все адреса игры".into());
            }
            let mut checked = std::collections::BTreeSet::new();
            for check in &spec.code_fingerprints {
                if !checked.insert(check.rva)
                    || !map.contains_key(&check.rva)
                    || !valid_hex(&check.hex, 64)
                    || map[&check.rva] > spec.module_size.saturating_sub(32)
                {
                    return Err("Некорректные отпечатки кода игры".into());
                }
            }
            for offset in [
                spec.offsets.dictionary_context,
                spec.offsets.minimap_link,
                spec.offsets.second_zone_array,
            ] {
                if offset == 0 || offset > 0x20_000 {
                    return Err("Некорректные смещения структур игры".into());
                }
            }
        }
        Ok(())
    }

    fn validate_extension_of(&self, previous: &Self) -> Result<()> {
        for old in &previous.profiles {
            let unchanged = self.profiles.iter().any(|new| {
                if new == old {
                    return true;
                }
                if old.id != DECREE_FRAGMENT_PROFILE_ID
                    || old
                        .rvas
                        .iter()
                        .any(|entry| entry.key == DECREE_FRAGMENT_RVA)
                {
                    return false;
                }
                let mut without_decree_fragment = new.clone();
                without_decree_fragment
                    .rvas
                    .retain(|entry| entry.key != DECREE_FRAGMENT_RVA);
                without_decree_fragment == *old && new.rvas.len() == old.rvas.len() + 1
            });
            if !unchanged {
                return Err(format!(
                    "Новый пакет изменяет или удаляет проверенный профиль {}",
                    old.id
                ));
            }
        }
        Ok(())
    }
}

fn valid_cache_name(name: &str) -> bool {
    let Some(stem) = name
        .strip_prefix("pack-")
        .and_then(|value| value.strip_suffix(".json"))
    else {
        return false;
    };
    let mut parts = stem.split('-');
    let (Some(revision), Some(digest), Some(pid), Some(nonce), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return false;
    };
    revision.len() == 20
        && revision.bytes().all(|b| b.is_ascii_digit())
        && valid_hex(digest, 64)
        && !pid.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && !nonce.is_empty()
        && nonce.bytes().all(|b| b.is_ascii_digit())
}

fn bounded_file(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("Пакет профилей слишком велик".into());
    }
    Ok(bytes)
}

fn download_bounded(
    client: &reqwest::blocking::Client,
    url: &str,
    limit: usize,
) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|e| format!("Не удалось получить профиль карты: {e}"))?;
    let mut bytes = Vec::new();
    response
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("Пакет профилей слишком велик".into());
    }
    Ok(bytes)
}

fn valid_hex(value: &str, size: usize) -> bool {
    value.len() == size && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_hex_range(value: &str, min_bytes: usize, max_bytes: usize) -> bool {
    value.len().is_multiple_of(2)
        && (min_bytes * 2..=max_bytes * 2).contains(&value.len())
        && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Clone, Debug)]
pub(crate) struct Profile {
    pub base: u64,
    pub dictionary: Vec<u8>,
    spec: Arc<ProfileSpec>,
}

impl Profile {
    #[cfg(test)]
    pub fn legacy(base: u64) -> Self {
        let spec = ProfilePack::bundled()
            .unwrap()
            .profiles
            .into_iter()
            .find(|spec| spec.id == "warframe-2026-09-12-validated")
            .unwrap();
        Self {
            base,
            dictionary: Vec::new(),
            spec: Arc::new(spec),
        }
    }

    pub fn rva(&self, key: u64) -> Result<u64> {
        self.spec
            .rvas
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.current)
            .ok_or_else(|| format!("RVA 0x{key:x} не подтверждён для этой версии игры"))
    }

    pub fn address(&self, key: u64) -> Result<u64> {
        self.base
            .checked_add(self.rva(key)?)
            .ok_or("Переполнение адреса модуля".into())
    }

    pub fn minimap_link_offset(&self) -> u64 {
        self.spec.offsets.minimap_link
    }

    pub fn second_zone_array_offset(&self) -> u64 {
        self.spec.offsets.second_zone_array
    }

    pub fn id(&self) -> &str {
        &self.spec.id
    }

    #[cfg(test)]
    pub fn validate(m: &mut dyn Memory) -> Result<Self> {
        let pack = ProfilePack::bundled()?;
        Self::validate_with_pack(m, &pack)
    }

    pub fn validate_with_pack(m: &mut dyn Memory, pack: &ProfilePack) -> Result<Self> {
        let module = m
            .modules()
            .into_iter()
            .find(|x| x.name.eq_ignore_ascii_case("Warframe.x64.exe"))
            .ok_or("В источнике нет Warframe.x64.exe")?;
        if module
            .size
            .checked_add(module.base)
            .is_none_or(|end| end > 0x0000_7fff_ffff_ffff)
        {
            return Err("Некорректная граница модуля".into());
        }
        let mut candidates = pack
            .profiles
            .iter()
            .filter(|spec| spec.module_size == module.size);
        let Some(first) = candidates.next() else {
            return Err("Эта версия игры пока не поддерживается исследованием карты".into());
        };
        let mut last_error = None;
        let mut matching = Vec::new();
        for spec in std::iter::once(first).chain(candidates) {
            let profile = Self {
                base: module.base,
                dictionary: Vec::new(),
                spec: Arc::new(spec.clone()),
            };
            match profile.validate_fingerprints(m) {
                Ok(()) => matching.push(profile),
                Err(error) => {
                    last_error = Some(error);
                }
            }
        }
        if matching.is_empty() {
            return Err(last_error.unwrap_or_else(|| "Версия игры не подтверждена".into()));
        }
        let mut selected = None;
        for mut profile in matching {
            match profile.validate_memory(m) {
                Ok(()) if selected.is_some() => {
                    return Err("Версия игры совпала с несколькими профилями".into());
                }
                Ok(()) => selected = Some(profile),
                Err(error) => last_error = Some(error),
            }
        }
        selected.ok_or_else(|| last_error.unwrap_or_else(|| "Версия игры не подтверждена".into()))
    }

    fn validate_fingerprints(&self, m: &mut dyn Memory) -> Result<()> {
        for check in &self.spec.code_fingerprints {
            let bytes = m.read(self.address(check.rva)?, 32)?;
            if hex::encode(bytes) != check.hex {
                return Err("Код игры отличается от проверенной версии; карта не построена".into());
            }
        }
        Ok(())
    }

    fn validate_memory(&mut self, m: &mut dyn Memory) -> Result<()> {
        for (meta, name) in [
            (0x28ed2a0, "PickUp *"),
            (0x297bf70, "TennoAvatar *"),
            (0x29c9600, "LotusHumanPlayer *"),
            (0x29e7ea0, "MiniMap *"),
            (0x28ecf80, "MultiAvatarTrigger *"),
            (0x28d6f40, "Waypoint *"),
            (0x2960650, "CipherAction *"),
            (0x28ce8c0, "ContextAction *"),
        ] {
            if q(m, self.address(meta)?)? != self.address(0x203fc60)? {
                return Err("Не подтверждена структура типов игры".into());
            }
            let binding = q(m, self.address(meta)? + 0x60)?;
            let nameptr = q(m, binding + 8)?;
            if string(m, nameptr)? != name {
                return Err("Не подтверждены имена типов игры".into());
            }
        }
        self.validate_object_tables(m)?;
        self.validate_registry_methods(m)?;
        let context = q(m, self.address(0x28a4888)?)?;
        let ddict = q(
            m,
            context
                .checked_add(self.spec.offsets.dictionary_context)
                .ok_or("Переполнение контекста")?,
        )?;
        let pointer = q(m, ddict + 8)?;
        let size = q(m, ddict + 16)?;
        if size != 1_048_576 {
            return Err("Не подтверждён словарь свойств игры".into());
        }
        let dictionary = m.read(pointer, size as usize)?;
        if hex::encode(Sha256::digest(&dictionary)) != self.spec.dictionary_sha256 {
            return Err("Словарь свойств изменился; исследование остановлено".into());
        }
        self.dictionary = dictionary;
        Ok(())
    }

    fn validate_object_tables(&self, m: &mut dyn Memory) -> Result<()> {
        let expected = self
            .base
            .checked_add(self.spec.vtable_slot3_rva)
            .ok_or("Переполнение адреса модуля")?;
        let end = self
            .base
            .checked_add(self.spec.module_size)
            .ok_or("Переполнение адреса модуля")?;
        for (table, _) in self.targets()? {
            for offset in [0, 8, 16] {
                let function = q(m, table + offset)?;
                if function < self.base || function >= end {
                    return Err("Таблицы объектов игры изменились; карта не построена".into());
                }
            }
            if q(m, table + 24)? != expected {
                return Err("Таблицы объектов игры изменились; карта не построена".into());
            }
        }
        for key in [0x21cecc0, 0x21d29d8] {
            let function = q(m, self.address(key)?)?;
            if function < self.base || function >= end {
                return Err("Таблицы объектов игры изменились; карта не построена".into());
            }
        }
        for key in [0x28f21b0, 0x28f3470] {
            if q(m, self.address(key)?)? != self.address(0x203fc60)? {
                return Err("Таблицы объектов игры изменились; карта не построена".into());
            }
        }
        Ok(())
    }

    pub(crate) fn validate_registry_methods(&self, m: &mut dyn Memory) -> Result<()> {
        let manager_vtable = self.address(0x21d29d8)?;
        let count = self.address(0xe03440)?;
        let range = self.address(0xc66420)?;
        let count_bytes = hex::decode(&self.spec.registry_count_hex)
            .map_err(|_| "Некорректный отпечаток метода реестра")?;
        let range_bytes = hex::decode(&self.spec.registry_range_hex)
            .map_err(|_| "Некорректный отпечаток метода реестра")?;
        if q(m, manager_vtable + 74 * 8)? != count
            || q(m, manager_vtable + 76 * 8)? != range
            || m.read(count, count_bytes.len())? != count_bytes
            || m.read(range, range_bytes.len())? != range_bytes
        {
            return Err("Структура реестра отличается от исследованной".into());
        }
        Ok(())
    }

    pub fn targets(&self) -> Result<Vec<(u64, &'static str)>> {
        let mut targets: Vec<_> = [
            (0x21a96f8, "pickup"),
            (0x23fb1e0, "pickup"),
            (0x2326098, "avatar"),
            (0x22e25b0, "npc"),
            (0x211bfe8, "level"),
            (0x20fd898, "decoration"),
            (0x2341320, "effect"),
            (0x2208c58, "spawnpoint"),
            (0x21a6fb0, "extraction"),
            (0x2113b98, "waypoint"),
            (0x22a1638, "terminal"),
            (0x213cf68, "dragon_door"),
        ]
        .into_iter()
        .map(|(r, n)| Ok((self.address(r)?, n)))
        .collect::<Result<_>>()?;
        if self
            .spec
            .rvas
            .iter()
            .any(|entry| entry.key == DECREE_FRAGMENT_RVA)
        {
            targets.push((self.address(DECREE_FRAGMENT_RVA)?, "decree_fragment"));
        }
        Ok(targets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_pack_is_signed_complete_and_keeps_old_profiles() {
        let pack = ProfilePack::bundled().unwrap();
        assert_eq!(pack.revision, 3);
        assert_eq!(pack.profiles.len(), 3);
        let old = Profile::legacy(0x1000);
        assert_eq!(old.address(0x28a4888).unwrap(), 0x1000 + 0x28a4888);
        assert!(old.address(0x28a4889).is_err());
    }

    #[test]
    fn modified_pack_or_signature_is_rejected() {
        let mut bytes = include_bytes!("profiles/bundled.json").to_vec();
        bytes[10] ^= 1;
        assert!(
            ProfilePack::from_signed(&bytes, include_bytes!("profiles/bundled.json.sig")).is_err()
        );
        assert!(
            ProfilePack::from_signed(include_bytes!("profiles/bundled.json"), b"invalid").is_err()
        );
    }

    #[test]
    fn only_signed_newer_complete_pack_is_cached() {
        let directory = tempfile::tempdir().unwrap();
        let bundled = ProfilePack::latest_cached(directory.path()).unwrap();
        assert_eq!(bundled.revision, 3);
        let current = include_bytes!("profiles/current-v2.json");
        let signature = include_bytes!("profiles/current-v2.json.sig");
        let installed = ProfilePack::install_signed(directory.path(), current, signature)
            .unwrap()
            .unwrap();
        assert_eq!(installed.revision, 4);
        assert_eq!(
            ProfilePack::latest_cached(directory.path())
                .unwrap()
                .revision,
            4
        );
        assert!(
            ProfilePack::install_signed(directory.path(), current, signature)
                .unwrap()
                .is_none()
        );
        let mut modified = current.to_vec();
        modified[10] ^= 1;
        assert!(ProfilePack::install_signed(directory.path(), &modified, signature).is_err());
        assert_eq!(
            ProfilePack::latest_cached(directory.path())
                .unwrap()
                .revision,
            4
        );
    }

    #[test]
    fn incomplete_rva_table_is_rejected() {
        let mut pack: ProfilePack =
            serde_json::from_slice(include_bytes!("profiles/bundled.json")).unwrap();
        pack.profiles[0].rvas.pop();
        assert!(pack.validate_shape().is_err());
    }

    #[test]
    fn decree_fragment_address_is_supported_for_future_profiles_and_additive() {
        let new: ProfilePack =
            serde_json::from_slice(include_bytes!("profiles/bundled.json")).unwrap();
        new.validate_shape().unwrap();
        let latest = new
            .profiles
            .iter()
            .find(|spec| spec.id == DECREE_FRAGMENT_PROFILE_ID)
            .unwrap();
        let profile = Profile {
            base: 0x1000,
            dictionary: Vec::new(),
            spec: Arc::new(latest.clone()),
        };
        assert!(
            profile
                .targets()
                .unwrap()
                .contains(&(0x1000 + DECREE_FRAGMENT_RVA, "decree_fragment"))
        );
        assert!(
            new.profiles[..2]
                .iter()
                .all(|spec| spec.rvas.len() == EXPECTED_RVAS.len())
        );

        let mut previous = new.clone();
        previous.revision = 2;
        previous.profiles[2]
            .rvas
            .retain(|entry| entry.key != DECREE_FRAGMENT_RVA);
        previous.validate_shape().unwrap();
        new.validate_extension_of(&previous).unwrap();

        let mut changed = new.clone();
        changed.profiles[2].rvas[0].current += 1;
        assert!(changed.validate_extension_of(&previous).is_err());
        let mut removed = new.clone();
        removed.profiles[2].rvas.remove(0);
        assert!(removed.validate_extension_of(&previous).is_err());
        let mut future = new.clone();
        future.revision += 1;
        let mut future_profile = latest.clone();
        future_profile.id = "warframe-2026-09-27-validated".into();
        future_profile.module_size += 4096;
        future.profiles.push(future_profile);
        future.validate_shape().unwrap();
        future.validate_extension_of(&new).unwrap();

        let mut changed_old = new.clone();
        changed_old.profiles[0].rvas.push(RvaEntry {
            key: DECREE_FRAGMENT_RVA,
            current: DECREE_FRAGMENT_RVA,
        });
        changed_old.validate_shape().unwrap();
        assert!(changed_old.validate_extension_of(&new).is_err());
    }

    #[test]
    fn published_envelope_contains_the_verified_hotfix() {
        let envelope: CacheEnvelope =
            serde_json::from_slice(include_bytes!("profiles/current-v2.signed.json")).unwrap();
        let payload = STANDARD.decode(envelope.payload).unwrap();
        assert_eq!(payload, include_bytes!("profiles/current-v2.json"));
        assert_eq!(
            envelope.signature,
            std::str::from_utf8(include_bytes!("profiles/current-v2.json.sig"))
                .unwrap()
                .trim()
        );
        let pack = ProfilePack::from_signed(&payload, envelope.signature.as_bytes()).unwrap();
        assert_eq!(pack.revision, 4);
        assert_eq!(pack.profiles.len(), 3);
    }

    #[test]
    fn previous_update_channel_remains_signed_and_compatible() {
        let envelope: CacheEnvelope =
            serde_json::from_slice(include_bytes!("profiles/current.signed.json")).unwrap();
        let payload = STANDARD.decode(envelope.payload).unwrap();
        assert_eq!(payload, include_bytes!("profiles/current.json"));
        assert_eq!(
            envelope.signature,
            std::str::from_utf8(include_bytes!("profiles/current.json.sig"))
                .unwrap()
                .trim()
        );
        let pack = ProfilePack::from_signed(&payload, envelope.signature.as_bytes()).unwrap();
        assert_eq!(pack.revision, 2);
        assert!(
            pack.profiles
                .iter()
                .all(|profile| profile.rvas.len() == EXPECTED_RVAS.len())
        );
    }

    #[test]
    fn newer_pack_cannot_replace_an_existing_profile() {
        let old = ProfilePack::bundled().unwrap();
        let mut new = ProfilePack::from_signed(
            include_bytes!("profiles/current-v2.json"),
            include_bytes!("profiles/current-v2.json.sig"),
        )
        .unwrap();
        assert!(new.validate_extension_of(&old).is_ok());
        new.profiles[0].rvas[0].current += 1;
        assert!(new.validate_extension_of(&old).is_err());
        new.profiles.remove(0);
        assert!(new.validate_extension_of(&old).is_err());
    }
}
