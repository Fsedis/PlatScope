//! Локальные двоичные снимки. Никакой фильтрации содержимого и отправки в сеть.
use anyhow::{Context, Result, bail};
use chrono::Utc;
use flate2::{Compression, write::ZlibEncoder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{BOOL, CloseHandle, ERROR_NO_MORE_FILES, FILETIME, HANDLE, STILL_ACTIVE},
        Storage::FileSystem::GetDiskFreeSpaceExW,
        System::{
            Diagnostics::{
                Debug::ReadProcessMemory,
                ToolHelp::{
                    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW,
                    TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32,
                },
            },
            Memory::{
                MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
                PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOCACHE, PAGE_READONLY, PAGE_READWRITE,
                PAGE_WRITECOMBINE, PAGE_WRITECOPY, VirtualQueryEx,
            },
            Threading::{
                GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_INFORMATION,
                PROCESS_VM_READ,
            },
        },
    },
    core::PCWSTR,
};

pub const BLOCK_SIZE: usize = 256 * 1024;
pub const RESERVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_READ_BYTES: u64 = 24 * 1024 * 1024 * 1024;

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn free_bytes(directory: &Path) -> Result<u64> {
    let path: Vec<u16> = directory
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut available = 0;
    unsafe {
        GetDiskFreeSpaceExW(PCWSTR(path.as_ptr()), Some(&mut available), None, None)?;
    }
    Ok(available)
}

pub struct Process {
    handle: Handle,
    pub pid: u32,
    pub created: String,
}
impl Process {
    /// Строгое чтение: частичный результат не дополняется старыми байтами.
    pub(crate) fn read_exact(&self, address: u64, length: usize) -> Result<Vec<u8>> {
        if length == 0 || length > 16 * 1024 * 1024 || address.checked_add(length as u64).is_none()
        {
            bail!("Недопустимая длина чтения");
        }
        let mut data = vec![0; length];
        let mut count = 0;
        let result = unsafe {
            ReadProcessMemory(
                self.handle.0,
                address as *const _,
                data.as_mut_ptr().cast(),
                length,
                Some(&mut count),
            )
        };
        if result.is_err() || count != length {
            bail!("Область 0x{address:x} изменилась или недоступна");
        }
        Ok(data)
    }

    pub(crate) fn readable_regions(&self, cancel: &AtomicBool) -> Result<Vec<Region>> {
        let mut address = 0usize;
        let mut regions = Vec::new();
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("Исследование отменено");
            }
            let mut info = MEMORY_BASIC_INFORMATION::default();
            if unsafe {
                VirtualQueryEx(
                    self.handle.0,
                    Some(address as *const _),
                    &mut info,
                    std::mem::size_of_val(&info),
                )
            } == 0
            {
                let error = windows::core::Error::from_win32();
                if error.code() != windows::core::HRESULT::from_win32(87) {
                    return Err(error.into());
                }
                break;
            }
            let base = info.BaseAddress as usize;
            let next = base
                .checked_add(info.RegionSize)
                .filter(|v| *v > address)
                .context("Некорректная область памяти")?;
            address = next;
            let readable = info.State == MEM_COMMIT
                && info.Protect.0 & (PAGE_GUARD.0 | PAGE_NOCACHE.0 | PAGE_WRITECOMBINE.0) == 0
                && info.Protect.0
                    & (PAGE_READONLY.0
                        | PAGE_READWRITE.0
                        | PAGE_WRITECOPY.0
                        | PAGE_EXECUTE_READ.0
                        | PAGE_EXECUTE_READWRITE.0
                        | PAGE_EXECUTE_WRITECOPY.0)
                    != 0;
            if readable {
                regions.push(Region {
                    base: base as u64,
                    size: info.RegionSize as u64,
                    allocation_base: info.AllocationBase as u64,
                    protection: info.Protect.0,
                    state: info.State.0,
                    kind: info.Type.0,
                    readable,
                });
            }
            if regions.len() > 200_000 {
                bail!("Слишком много областей памяти");
            }
        }
        Ok(regions)
    }
}
impl Process {
    pub fn open(pid: u32) -> Result<Self> {
        let handle = Handle(unsafe {
            OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, BOOL(0), pid)?
        });
        let (mut creation, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe {
            GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user)?;
        }
        Ok(Self {
            handle,
            pid,
            created: format!(
                "{:08x}{:08x}",
                creation.dwHighDateTime, creation.dwLowDateTime
            ),
        })
    }

    pub fn alive(&self) -> bool {
        let mut code = 0;
        unsafe {
            GetExitCodeProcess(self.handle.0, &mut code).is_ok() && code == STILL_ACTIVE.0 as u32
        }
    }

    pub(crate) fn modules(&self) -> Result<Vec<Module>> {
        let snapshot = Handle(unsafe {
            CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, self.pid)?
        });
        let mut entry = MODULEENTRY32W {
            dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
            ..Default::default()
        };
        unsafe {
            Module32FirstW(snapshot.0, &mut entry)?;
        }
        let mut result = Vec::new();
        loop {
            let end = entry
                .szModule
                .iter()
                .position(|x| *x == 0)
                .unwrap_or(entry.szModule.len());
            result.push(Module {
                name: String::from_utf16_lossy(&entry.szModule[..end]),
                base: entry.modBaseAddr as u64,
                size: u64::from(entry.modBaseSize),
            });
            match unsafe { Module32NextW(snapshot.0, &mut entry) } {
                Ok(()) => {}
                Err(error)
                    if error.code()
                        == windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0) =>
                {
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(result)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Module {
    pub(crate) name: String,
    pub(crate) base: u64,
    pub(crate) size: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Region {
    pub(crate) base: u64,
    pub(crate) size: u64,
    pub(crate) allocation_base: u64,
    pub(crate) protection: u32,
    pub(crate) state: u32,
    pub(crate) kind: u32,
    pub(crate) readable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub sha256: String,
    pub offset: u64,
    pub compressed_size: u64,
    pub size: usize,
}

#[derive(Serialize)]
pub struct Hole {
    address: u64,
    size: usize,
}

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub sequence: u64,
    pub read_bytes: u64,
    pub stored_bytes: u64,
    pub blocks: u64,
    pub holes: u64,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub format: u32,
    pub sequence: u64,
    pub parent: Option<u64>,
    pub started_at: String,
    pub ended_at: String,
    pub complete: bool,
    pub reason: Option<String>,
    pub module_error: Option<String>,
    pub modules: Vec<Module>,
    pub regions: Vec<Region>,
    pub holes: Vec<Hole>,
    pub changed: BTreeMap<u64, Block>,
    pub removed: Vec<u64>,
    pub progress: Progress,
}

/// Адреса в манифестах — десятичные ключи; указатели в блоках — исходные x64 LE.
pub struct Archive {
    directory: PathBuf,
    pack: File,
    bytes: u64,
    limit: u64,
    sequence: u64,
    previous: BTreeMap<u64, Block>,
    content: HashMap<String, Block>,
}

fn create_json(path: &Path, value: &impl Serialize) -> Result<u64> {
    let bytes = serde_json::to_vec(value)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(bytes.len() as u64)
}

impl Archive {
    pub fn create(directory: &Path, process: &Process, max_bytes: u64) -> Result<Self> {
        std::fs::create_dir(directory).context("Не удалось создать новую папку записи")?;
        if free_bytes(directory)? < RESERVE_BYTES + 1024 * 1024 * 1024 {
            bail!("Для начала записи нужно не менее 3 ГиБ свободного места.");
        }
        let pack = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(directory.join("blocks.bin"))?;
        let bytes = create_json(
            &directory.join("session.json"),
            &json!({
                "format":1, "createdAt":Utc::now(), "pid":process.pid, "processCreatedFiletime":process.created,
                "pointerSize":8, "byteOrder":"little", "blockSize":BLOCK_SIZE, "compression":"zlib",
                "maxBytes":max_bytes, "reserveBytes":RESERVE_BYTES, "snapshotTimeoutSeconds":90,
                "maxReadBytesPerSnapshot":MAX_READ_BYTES,
                "description":"Сырые локальные данные памяти, включая возможные учётные данные. Не атомарный снимок. Только успешно прочитанные блоки доступны для анализа."
            }),
        )?;
        Ok(Self {
            directory: directory.into(),
            pack,
            bytes,
            limit: max_bytes,
            sequence: 0,
            previous: BTreeMap::new(),
            content: HashMap::new(),
        })
    }

    fn store(&mut self, bytes: &[u8]) -> Result<Block> {
        let hash = format!("{:x}", Sha256::digest(bytes));
        if let Some(block) = self.content.get(&hash) {
            return Ok(block.clone());
        }
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(bytes)?;
        let compressed = encoder.finish()?;
        // Оставляем запас под карту областей и завершение сеанса.
        if self.bytes + compressed.len() as u64 + 64 * 1024 * 1024 > self.limit {
            bail!("Достигнут предел размера записи.");
        }
        use std::io::Seek;
        let block = Block {
            sha256: hash.clone(),
            offset: self.pack.stream_position()?,
            compressed_size: compressed.len() as u64,
            size: bytes.len(),
        };
        self.pack.write_all(&compressed)?;
        self.bytes += compressed.len() as u64;
        self.content.insert(hash, block.clone());
        Ok(block)
    }

    fn publish(&mut self, snapshot: &Snapshot, current: BTreeMap<u64, Block>) -> Result<()> {
        self.pack.sync_all()?;
        let path = self
            .directory
            .join(format!("snapshot-{:06}.json", snapshot.sequence));
        let temporary = path.with_extension("partial");
        let size = create_json(&temporary, snapshot)?;
        if self.bytes + size > self.limit {
            bail!("Достигнут предел размера записи при сохранении индекса.");
        }
        std::fs::rename(temporary, path)?;
        self.bytes += size;
        self.previous = current;
        self.sequence = snapshot.sequence;
        Ok(())
    }

    pub fn finish(&mut self, reason: &str) -> Result<()> {
        self.pack.sync_all()?;
        self.bytes += create_json(
            &self.directory.join("end.json"),
            &json!({"endedAt":Utc::now(),"snapshots":self.sequence,"reason":reason,"storedBytes":self.bytes}),
        )?;
        Ok(())
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn capture(
        &mut self,
        process: &Process,
        cancel: &AtomicBool,
        mut report: impl FnMut(Progress),
    ) -> Result<Snapshot> {
        // Остановка остаётся доступной даже во время ожидания другого сканера.
        let _guard = loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("Остановлено до начала снимка.");
            }
            match crate::squad::SCAN_LOCK.try_lock() {
                Ok(guard) => break guard,
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(_) => bail!("Предыдущее чтение памяти завершилось с ошибкой."),
            }
        };
        if !process.alive() {
            bail!("Процесс игры завершился.");
        }
        let start = Instant::now();
        let (modules, module_error) = match process.modules() {
            Ok(v) => (v, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        let mut snapshot = Snapshot {
            format: 1,
            sequence: self.sequence + 1,
            parent: (self.sequence > 0).then_some(self.sequence),
            started_at: Utc::now().to_rfc3339(),
            ended_at: String::new(),
            complete: true,
            reason: None,
            modules,
            module_error,
            regions: Vec::new(),
            holes: Vec::new(),
            changed: BTreeMap::new(),
            removed: Vec::new(),
            progress: Progress {
                sequence: self.sequence + 1,
                stored_bytes: self.bytes,
                ..Progress::default()
            },
        };
        let mut current = BTreeMap::new();
        let mut address = 0usize;
        let mut buffer = vec![0u8; BLOCK_SIZE];
        let mut last_report = Instant::now() - Duration::from_millis(400);
        let mut last_disk = Instant::now();
        'regions: loop {
            if let Some(reason) = stop_reason(cancel, start, snapshot.progress.read_bytes, process)
            {
                snapshot.reason = Some(reason);
                break;
            }
            let mut info = MEMORY_BASIC_INFORMATION::default();
            if unsafe {
                VirtualQueryEx(
                    process.handle.0,
                    Some(address as *const _),
                    &mut info,
                    std::mem::size_of_val(&info),
                )
            } == 0
            {
                // VirtualQueryEx заканчивает обход с ERROR_INVALID_PARAMETER выше пользовательского диапазона.
                let error = windows::core::Error::from_win32();
                if error.code() != windows::core::HRESULT::from_win32(87) {
                    snapshot.reason = Some(format!("Обход областей прерван: {error}"));
                }
                break;
            }
            let base = info.BaseAddress as usize;
            let Some(next) = base.checked_add(info.RegionSize).filter(|v| *v > address) else {
                snapshot.reason = Some("Некорректная граница области.".into());
                break;
            };
            address = next;
            let readable = info.State == MEM_COMMIT
                && info.Protect.0 & (PAGE_GUARD.0 | PAGE_NOCACHE.0 | PAGE_WRITECOMBINE.0) == 0
                && info.Protect.0
                    & (PAGE_READONLY.0
                        | PAGE_READWRITE.0
                        | PAGE_WRITECOPY.0
                        | PAGE_EXECUTE_READ.0
                        | PAGE_EXECUTE_READWRITE.0
                        | PAGE_EXECUTE_WRITECOPY.0)
                    != 0;
            snapshot.regions.push(Region {
                base: base as u64,
                size: info.RegionSize as u64,
                allocation_base: info.AllocationBase as u64,
                protection: info.Protect.0,
                state: info.State.0,
                kind: info.Type.0,
                readable,
            });
            if !readable {
                continue;
            }
            let mut offset = 0usize;
            while offset < info.RegionSize {
                if let Some(reason) =
                    stop_reason(cancel, start, snapshot.progress.read_bytes, process)
                {
                    snapshot.reason = Some(reason);
                    break 'regions;
                }
                if last_disk.elapsed() >= Duration::from_secs(1) {
                    match free_bytes(&self.directory) {
                        Ok(free) if free >= RESERVE_BYTES + BLOCK_SIZE as u64 => {}
                        Ok(_) => {
                            snapshot.reason =
                                Some("Остановка: на диске осталось менее 2 ГиБ.".into());
                            break 'regions;
                        }
                        Err(e) => {
                            snapshot.reason =
                                Some(format!("Не удалось проверить свободное место: {e}"));
                            break 'regions;
                        }
                    }
                    last_disk = Instant::now();
                }
                let want = BLOCK_SIZE.min(info.RegionSize - offset);
                let va = base + offset;
                let mut count = 0usize;
                // При частичном чтении сохраняем только реально возвращённый префикс.
                let _ = unsafe {
                    ReadProcessMemory(
                        process.handle.0,
                        va as *const _,
                        buffer.as_mut_ptr().cast(),
                        want,
                        Some(&mut count),
                    )
                };
                count = count.min(want);
                if count > 0 {
                    let block = match self.store(&buffer[..count]) {
                        Ok(v) => v,
                        Err(e) => {
                            snapshot.reason = Some(e.to_string());
                            break 'regions;
                        }
                    };
                    if self.previous.get(&(va as u64)) != Some(&block) {
                        snapshot.changed.insert(va as u64, block.clone());
                    }
                    current.insert(va as u64, block);
                    snapshot.progress.read_bytes += count as u64;
                    snapshot.progress.blocks += 1;
                }
                if count < want {
                    snapshot.holes.push(Hole {
                        address: (va + count) as u64,
                        size: want - count,
                    });
                    snapshot.progress.holes += 1;
                }
                offset += want;
                snapshot.progress.stored_bytes = self.bytes;
                if last_report.elapsed() >= Duration::from_millis(400) {
                    report(snapshot.progress.clone());
                    last_report = Instant::now();
                }
            }
        }
        if !process.alive() {
            snapshot.reason = Some("Процесс игры завершился.".into());
        }
        snapshot.complete = snapshot.reason.is_none()
            && snapshot.holes.is_empty()
            && snapshot.module_error.is_none();
        snapshot.removed = self
            .previous
            .keys()
            .filter(|key| !current.contains_key(key))
            .copied()
            .collect();
        snapshot.ended_at = Utc::now().to_rfc3339();
        self.publish(&snapshot, current)?;
        snapshot.progress.stored_bytes = self.bytes;
        report(snapshot.progress.clone());
        Ok(snapshot)
    }
}

fn stop_reason(
    cancel: &AtomicBool,
    start: Instant,
    bytes: u64,
    process: &Process,
) -> Option<String> {
    if cancel.load(Ordering::Relaxed) {
        Some("Остановлено пользователем.".into())
    } else if start.elapsed() >= SNAPSHOT_TIMEOUT {
        Some("Достигнут предел снимка 90 секунд.".into())
    } else if bytes >= MAX_READ_BYTES {
        Some("Достигнут предел чтения снимка 24 ГиБ.".into())
    } else if !process.alive() {
        Some("Процесс игры завершился.".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    #[test]
    fn archive_reuses_content_and_preserves_binary_bytes() {
        let dir = std::env::temp_dir().join(format!(
            "platscope-binary-test-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let process = Process::open(std::process::id()).unwrap();
        let mut archive = Archive::create(&dir, &process, 128 * 1024 * 1024).unwrap();
        let bytes: Vec<u8> = (0..65536).map(|n| (n % 256) as u8).collect();
        let first = archive.store(&bytes).unwrap();
        let size = archive.bytes();
        assert_eq!(archive.store(&bytes).unwrap(), first);
        assert_eq!(archive.bytes(), size);
        archive.limit = archive.bytes + 64 * 1024 * 1024;
        assert!(archive.store(b"different bytes beyond the limit").is_err());
        assert_eq!(archive.bytes(), size);
        archive.finish("Тест").unwrap();
        let compressed = std::fs::read(dir.join("blocks.bin")).unwrap();
        let mut decoded = Vec::new();
        flate2::read::ZlibDecoder::new(&compressed[..])
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, bytes);
        assert!(!process.modules().unwrap().is_empty());
        drop(archive);
        // Удаляем только созданную этим тестом папку в системной временной директории.
        assert!(dir.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
