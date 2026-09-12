use super::Result;
use crate::binary_snapshot::Process;
use std::sync::atomic::AtomicBool;
#[derive(Clone, Debug)]
pub struct MemoryModule {
    pub name: String,
    pub base: u64,
    pub size: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct MemoryRange {
    pub address: u64,
    pub length: usize,
}
/// Источник обязан отказать при пропуске, не подставляя байты прежнего снимка.
pub trait Memory {
    fn read(&mut self, address: u64, length: usize) -> Result<Vec<u8>>;
    fn ranges(&self) -> Vec<MemoryRange>;
    fn modules(&self) -> Vec<MemoryModule>;
    fn strict_errors(&self) -> bool {
        true
    }
}
pub(crate) struct LiveMemory {
    pub process: Process,
    pub ranges: Vec<MemoryRange>,
    pub modules: Vec<MemoryModule>,
}
impl LiveMemory {
    pub fn open(pid: u32, cancel: &AtomicBool) -> Result<Self> {
        let process = Process::open(pid).map_err(|e| e.to_string())?;
        let modules = process
            .modules()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|x| MemoryModule {
                name: x.name,
                base: x.base,
                size: x.size,
            })
            .collect();
        let regions = process
            .readable_regions(cancel)
            .map_err(|e| e.to_string())?;
        let mut ranges = Vec::new();
        for r in regions {
            let mut at = r.base;
            let end = r.base.checked_add(r.size).ok_or("Переполнение области")?;
            while at < end {
                let n = (end - at).min(256 * 1024) as usize;
                ranges.push(MemoryRange {
                    address: at,
                    length: n,
                });
                at += n as u64;
                if ranges.len() > 200_000 {
                    return Err("Слишком много блоков памяти".into());
                }
            }
        }
        Ok(Self {
            process,
            ranges,
            modules,
        })
    }
}
impl Memory for LiveMemory {
    fn read(&mut self, address: u64, length: usize) -> Result<Vec<u8>> {
        self.process
            .read_exact(address, length)
            .map_err(|e| e.to_string())
    }
    fn ranges(&self) -> Vec<MemoryRange> {
        self.ranges.clone()
    }
    fn modules(&self) -> Vec<MemoryModule> {
        self.modules.clone()
    }
    fn strict_errors(&self) -> bool {
        false
    }
}
pub(crate) fn q(m: &mut dyn Memory, a: u64) -> Result<u64> {
    {
        let value = u64::from_le_bytes(m.read(a, 8)?.try_into().map_err(|_| "Размер указателя")?);
        if value > 0x0000_7fff_ffff_ffff {
            return Err("Некорректный указатель".into());
        }
        Ok(value)
    }
}
pub(crate) fn u32_at(b: &[u8], i: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(i..i + 4)
            .ok_or("Нет поля u32")?
            .try_into()
            .map_err(|_| "Размер u32")?,
    ))
}
pub(crate) fn u64_at(b: &[u8], i: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        b.get(i..i + 8)
            .ok_or("Нет поля u64")?
            .try_into()
            .map_err(|_| "Размер u64")?,
    ))
}
pub(crate) fn string(m: &mut dyn Memory, a: u64) -> Result<String> {
    let bytes = m.read(a, 160)?;
    let end = bytes
        .iter()
        .position(|v| *v == 0)
        .ok_or("Слишком длинное имя типа")?;
    let s = std::str::from_utf8(&bytes[..end]).map_err(|_| "Имя типа не UTF-8")?;
    if !s
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"_ *:<>".contains(&b))
    {
        return Err("Некорректное имя типа".into());
    }
    Ok(s.into())
}
