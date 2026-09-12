use super::{ArchiveSnapshot, Memory, MemoryModule, MemoryRange, Result};
use crate::binary_snapshot::{Block, Module};
use flate2::read::ZlibDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
#[derive(Deserialize)]
struct Session {
    format: u32,
    compression: String,
    #[serde(rename = "pointerSize")]
    pointer_size: Option<u8>,
    #[serde(rename = "pointerWidth")]
    pointer_width: Option<u8>,
}
#[derive(Deserialize)]
struct Hole {
    address: u64,
    size: u64,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexProgress {
    #[serde(default)]
    read_bytes: u64,
}
#[derive(Deserialize)]
struct Index {
    #[serde(default)]
    progress: IndexProgress,
    format: u32,
    sequence: u64,
    parent: Option<u64>,
    started_at: String,
    ended_at: String,
    complete: bool,
    modules: Vec<Module>,
    holes: Vec<Hole>,
    changed: BTreeMap<u64, Block>,
    removed: Vec<u64>,
}
fn json<T: serde::de::DeserializeOwned>(p: &Path) -> Result<T> {
    let f = File::open(p).map_err(|e| format!("Не открыть {}: {e}", p.display()))?;
    if f.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("Индекс превышает 64 МиБ".into());
    }
    serde_json::from_reader(std::io::BufReader::new(f))
        .map_err(|e| format!("Повреждён индекс {}: {e}", p.display()))
}
pub struct ArchiveMemory {
    directory: PathBuf,
    pack: File,
    pack_len: u64,
    blocks: BTreeMap<u64, Block>,
    cache: VecDeque<(u64, Vec<u8>)>,
    holes: Vec<(u64, u64)>,
    modules: Vec<MemoryModule>,
    pub started_at: String,
    pub ended_at: String,
    pub complete: bool,
}
impl ArchiveMemory {
    pub fn open(directory: &Path, sequence: u64, cancel: &AtomicBool) -> Result<Self> {
        if sequence == 0 {
            return Err("Номер снимка должен быть положительным".into());
        }
        let session: Session = json(&directory.join("session.json"))?;
        if session.format != 1
            || session.compression != "zlib"
            || session
                .pointer_size
                .or(session.pointer_width)
                .is_some_and(|v| v != 8)
        {
            return Err("Неподдерживаемый формат двоичной записи".into());
        }
        let mut current = Some(sequence);
        let mut seen = HashSet::new();
        let mut chain = Vec::new();
        let mut index_bytes = 0u64;
        while let Some(seq) = current {
            super::cancelled(cancel)?;
            if chain.len() >= 1024 || !seen.insert(seq) {
                return Err("Повреждена цепочка снимков".into());
            }
            let index_path = directory.join(format!("snapshot-{seq:06}.json"));
            index_bytes = index_bytes.saturating_add(
                std::fs::metadata(&index_path)
                    .map_err(|e| e.to_string())?
                    .len(),
            );
            if index_bytes > 256 * 1024 * 1024 {
                return Err("Цепочка индексов превышает 256 МиБ".into());
            }
            let index: Index = json(&index_path)?;
            if index.format != 1
                || index.sequence != seq
                || index.parent.is_some_and(|p| p >= seq || p == 0)
            {
                return Err("Некорректный родитель снимка".into());
            }
            if index.changed.len() > 200_000
                || index.removed.len() > 200_000
                || index.holes.len() > 200_000
            {
                return Err("Превышен предел индекса".into());
            }
            current = index.parent;
            chain.push(index);
        }
        let selected = &chain[0];
        let started_at = selected.started_at.clone();
        let ended_at = selected.ended_at.clone();
        let complete = selected.complete;
        let modules = selected
            .modules
            .iter()
            .map(|m| MemoryModule {
                name: m.name.clone(),
                base: m.base,
                size: m.size,
            })
            .collect();
        let holes = selected
            .holes
            .iter()
            .map(|h| {
                h.address
                    .checked_add(h.size)
                    .map(|e| (h.address, e))
                    .ok_or("Переполнение пропуска".to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        let pack = File::open(directory.join("blocks.bin")).map_err(|e| e.to_string())?;
        let pack_len = pack.metadata().map_err(|e| e.to_string())?.len();
        let mut blocks = BTreeMap::new();
        for index in chain.into_iter().rev() {
            super::cancelled(cancel)?;
            for a in index.removed {
                blocks.remove(&a);
            }
            for (a, b) in index.changed {
                if b.size == 0
                    || b.size > 256 * 1024
                    || b.compressed_size == 0
                    || b.compressed_size > 512 * 1024
                    || b.offset
                        .checked_add(b.compressed_size)
                        .is_none_or(|end| end > pack_len)
                    || b.sha256.len() != 64
                    || !b.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err("Некорректная ссылка на двоичный блок".into());
                }
                blocks.insert(a, b);
            }
            if blocks.len() > 200_000 {
                return Err("Слишком много блоков снимка".into());
            }
        }
        let mut end = 0;
        for (a, b) in &blocks {
            if *a < end {
                return Err("Перекрывающиеся блоки снимка".into());
            }
            end = a.checked_add(b.size as u64).ok_or("Переполнение адреса")?;
        }
        Ok(Self {
            directory: directory.to_owned(),
            pack,
            pack_len,
            blocks,
            cache: VecDeque::new(),
            holes,
            modules,
            started_at,
            ended_at,
            complete,
        })
    }
    fn block(&mut self, a: u64) -> Result<Vec<u8>> {
        if let Some(i) = self.cache.iter().position(|(p, _)| *p == a) {
            let item = self.cache.remove(i).ok_or("Ошибка кэша")?;
            let bytes = item.1.clone();
            self.cache.push_back(item);
            return Ok(bytes);
        }
        let b = self.blocks.get(&a).ok_or("Блок отсутствует")?;
        if b.offset + b.compressed_size > self.pack_len {
            return Err("Блок выходит за файл".into());
        }
        self.pack
            .seek(SeekFrom::Start(b.offset))
            .map_err(|e| e.to_string())?;
        let mut encoded = vec![0; b.compressed_size as usize];
        self.pack
            .read_exact(&mut encoded)
            .map_err(|e| e.to_string())?;
        let mut decoder = ZlibDecoder::new(encoded.as_slice());
        let mut raw = Vec::with_capacity(b.size);
        decoder
            .by_ref()
            .take(b.size as u64 + 1)
            .read_to_end(&mut raw)
            .map_err(|e| format!("Повреждено сжатие: {e}"))?;
        if raw.len() != b.size
            || decoder.total_in() != b.compressed_size
            || format!("{:x}", Sha256::digest(&raw)) != b.sha256.to_ascii_lowercase()
        {
            return Err(format!("Проверка блока 0x{a:x} не пройдена"));
        }
        self.cache.push_back((a, raw.clone()));
        if self.cache.len() > 16 {
            self.cache.pop_front();
        }
        Ok(raw)
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
}
impl Memory for ArchiveMemory {
    fn read(&mut self, address: u64, length: usize) -> Result<Vec<u8>> {
        if length == 0 || length > 16 * 1024 * 1024 {
            return Err("Чтение ограничено 16 МиБ".into());
        }
        let end = address
            .checked_add(length as u64)
            .ok_or("Переполнение чтения")?;
        if self.holes.iter().any(|(a, b)| address < *b && end > *a) {
            return Err(format!("Адрес 0x{address:x} пересекает пропуск снимка"));
        }
        let mut data = Vec::with_capacity(length);
        while data.len() < length {
            let at = address + data.len() as u64;
            let (base, size) = self
                .blocks
                .range(..=at)
                .next_back()
                .map(|(a, b)| (*a, b.size))
                .ok_or("Адрес отсутствует в снимке")?;
            let offset = (at - base) as usize;
            if offset >= size {
                return Err(format!("Адрес 0x{at:x} не сохранён"));
            }
            let raw = self.block(base)?;
            let n = (length - data.len()).min(raw.len() - offset);
            data.extend_from_slice(&raw[offset..offset + n]);
        }
        Ok(data)
    }
    fn ranges(&self) -> Vec<MemoryRange> {
        self.blocks
            .iter()
            .map(|(a, b)| MemoryRange {
                address: *a,
                length: b.size,
            })
            .collect()
    }
    fn modules(&self) -> Vec<MemoryModule> {
        self.modules.clone()
    }
}
pub(super) fn list(path: &Path) -> Result<Vec<ArchiveSnapshot>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
        let p = entry.map_err(|e| e.to_string())?.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with("snapshot-") && name.ends_with(".json") {
            if out.len() >= 1024 {
                return Err("Слишком много снимков".into());
            }
            let i: Index = json(&p)?;
            out.push(ArchiveSnapshot {
                sequence: i.sequence,
                started_at: i.started_at,
                ended_at: i.ended_at,
                complete: i.complete,
                bytes: i.progress.read_bytes,
                holes: i.holes.len() as u64,
            });
        }
    }
    out.sort_by_key(|s| s.sequence);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "platscope-spatial-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn index(
            &self,
            n: u64,
            parent: Option<u64>,
            changed: serde_json::Value,
            removed: Vec<u64>,
            holes: serde_json::Value,
        ) {
            std::fs::write(self.0.join(format!("snapshot-{n:06}.json")),serde_json::to_vec(&serde_json::json!({"format":1,"sequence":n,"parent":parent,"started_at":"s","ended_at":"e","complete":true,"modules":[],"changed":changed,"removed":removed,"holes":holes})).unwrap()).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> Fixture {
        let f = Fixture::new();
        std::fs::write(
            f.0.join("session.json"),
            br#"{"format":1,"compression":"zlib","pointerSize":8}"#,
        )
        .unwrap();
        let mut encoded = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoded.write_all(b"abcd").unwrap();
        let data = encoded.finish().unwrap();
        std::fs::write(f.0.join("blocks.bin"), &data).unwrap();
        let b = serde_json::json!({"sha256":format!("{:x}",Sha256::digest(b"abcd")),"offset":0,"compressed_size":data.len(),"size":4});
        f.index(
            1,
            None,
            serde_json::json!({"4096":b,"4100":b}),
            vec![],
            serde_json::json!([]),
        );
        f
    }
    #[test]
    fn reads_across_blocks_and_applies_removed() {
        let f = fixture();
        let c = AtomicBool::new(false);
        let mut a = ArchiveMemory::open(&f.0, 1, &c).unwrap();
        assert_eq!(a.read(4098, 4).unwrap(), b"cdab");
        f.index(
            2,
            Some(1),
            serde_json::json!({}),
            vec![4100],
            serde_json::json!([]),
        );
        let mut a = ArchiveMemory::open(&f.0, 2, &c).unwrap();
        assert!(a.read(4098, 4).is_err());
        assert_eq!(a.read(4096, 4).unwrap(), b"abcd");
    }
    #[test]
    fn holes_override_inherited_data() {
        let f = fixture();
        f.index(
            2,
            Some(1),
            serde_json::json!({}),
            vec![],
            serde_json::json!([{"address":4098,"size":2}]),
        );
        let mut a = ArchiveMemory::open(&f.0, 2, &AtomicBool::new(false)).unwrap();
        assert!(a.read(4096, 4).is_err());
        assert_eq!(a.read(4100, 4).unwrap(), b"abcd");
    }
    #[test]
    fn rejects_bad_parent_cancelled_and_corruption() {
        let f = fixture();
        assert!(ArchiveMemory::open(&f.0, 1, &AtomicBool::new(true)).is_err());
        f.index(
            2,
            Some(2),
            serde_json::json!({}),
            vec![],
            serde_json::json!([]),
        );
        assert!(ArchiveMemory::open(&f.0, 2, &AtomicBool::new(false)).is_err());
        let mut data = std::fs::read(f.0.join("blocks.bin")).unwrap();
        data[3] ^= 0xff;
        std::fs::write(f.0.join("blocks.bin"), data).unwrap();
        let mut a = ArchiveMemory::open(&f.0, 1, &AtomicBool::new(false)).unwrap();
        assert!(a.read(4096, 4).is_err());
    }
    #[test]
    fn ignores_partial_indexes() {
        let f = fixture();
        std::fs::write(f.0.join("snapshot-000002.partial"), b"broken").unwrap();
        assert_eq!(list(&f.0).unwrap().len(), 1);
    }
}
