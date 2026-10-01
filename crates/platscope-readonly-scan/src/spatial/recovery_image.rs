//! Ограниченный образ загруженного EXE для поиска опорных адресов карты.
use super::source::u32_at;
use super::{Memory, MemoryModule, Result, cancelled};
use sha2::{Digest, Sha256};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_READ_BYTES: usize = 256 * 1024;
const MAX_MATCHES: usize = 512;
const MAX_POINTER_REFERENCES: usize = 8192;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
const USER_ADDRESS_END: u64 = 0x0000_8000_0000_0000;
const SECTION_CODE: u32 = 0x0000_0020;
const SECTION_DATA: u32 = 0x0000_00c0;
const SECTION_EXECUTE: u32 = 0x2000_0000;
const SECTION_READ: u32 = 0x4000_0000;

struct Section {
    address: u64,
    bytes: Vec<u8>,
    executable: bool,
}

struct SectionSpec {
    offset: u64,
    size: u64,
    text: bool,
    pdata: bool,
    load: bool,
    executable: bool,
}

pub(super) struct ModuleImage {
    pub(super) module: MemoryModule,
    sections: Vec<Section>,
    functions: Vec<(u64, u64)>,
    fingerprint: String,
    started: Instant,
}

struct ImageReader<'a> {
    memory: &'a mut dyn Memory,
    cancel: &'a AtomicBool,
    module: &'a MemoryModule,
    read_bytes: u64,
    started: Instant,
}

impl ImageReader<'_> {
    fn read(&mut self, offset: u64, length: usize) -> Result<Vec<u8>> {
        if length == 0
            || offset
                .checked_add(length as u64)
                .is_none_or(|end| end > self.module.size)
        {
            return Err("Область EXE выходит за границы модуля".into());
        }
        let next_total = self
            .read_bytes
            .checked_add(length as u64)
            .filter(|total| *total <= MAX_IMAGE_BYTES)
            .ok_or("Превышен объём чтения для восстановления карты")?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| "Недостаточно памяти для исследования EXE")?;
        while bytes.len() < length {
            check_deadline(self.started)?;
            cancelled(self.cancel)?;
            let take = (length - bytes.len()).min(MAX_READ_BYTES);
            let address = self.module.base + offset + bytes.len() as u64;
            let chunk = self.memory.read(address, take)?;
            if chunk.len() != take {
                return Err("Неполное чтение EXE; восстановление карты остановлено".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        self.read_bytes = next_total;
        check_deadline(self.started)?;
        cancelled(self.cancel)?;
        Ok(bytes)
    }
}

impl ModuleImage {
    pub(super) fn load(memory: &mut dyn Memory, cancel: &AtomicBool) -> Result<Self> {
        let started = Instant::now();
        cancelled(cancel)?;
        let modules: Vec<_> = memory
            .modules()
            .into_iter()
            .filter(|module| module.name.eq_ignore_ascii_case("Warframe.x64.exe"))
            .collect();
        let [module] = modules.as_slice() else {
            return Err("Не найден единственный модуль Warframe.x64.exe".into());
        };
        if !(1024 * 1024..=MAX_IMAGE_BYTES).contains(&module.size)
            || module.base < 0x10000
            || module.base % 4096 != 0
            || module
                .base
                .checked_add(module.size)
                .is_none_or(|end| end > USER_ADDRESS_END)
        {
            return Err("Некорректная граница EXE для восстановления карты".into());
        }
        let mut reader = ImageReader {
            memory,
            cancel,
            module,
            read_bytes: 0,
            started,
        };
        let dos = reader.read(0, 64)?;
        if dos.get(..2) != Some(b"MZ") {
            return Err("Не подтверждён заголовок EXE игры".into());
        }
        let pe_offset = u32_at(&dos, 0x3c)? as u64;
        if !(64..=64 * 1024).contains(&pe_offset) {
            return Err("Некорректное положение PE-заголовка".into());
        }
        let pe = reader.read(pe_offset, 24)?;
        if pe.get(..4) != Some(b"PE\0\0") || u16_at(&pe, 4)? != 0x8664 {
            return Err("Не подтверждён 64-разрядный EXE игры".into());
        }
        let section_count = u16_at(&pe, 6)? as usize;
        let optional_length = u16_at(&pe, 20)? as usize;
        if !(1..=96).contains(&section_count) || !(112..=1024).contains(&optional_length) {
            return Err("Некорректное число секций или размер PE-заголовка".into());
        }
        let optional = reader.read(pe_offset + 24, optional_length)?;
        if u16_at(&optional, 0)? != 0x20b || u32_at(&optional, 56)? as u64 != module.size {
            return Err("Размер или формат PE не совпадает с модулем игры".into());
        }
        let table_offset = pe_offset + 24 + optional_length as u64;
        let table_length = section_count * 40;
        let header_length = u32_at(&optional, 60)? as u64;
        if header_length < table_offset + table_length as u64
            || header_length > 1024 * 1024
            || header_length >= module.size
        {
            return Err("Некорректная граница заголовков EXE".into());
        }
        let table = reader.read(table_offset, table_length)?;
        let mut specs = Vec::with_capacity(section_count);
        for entry in table.chunks_exact(40) {
            let name_length = entry[..8].iter().position(|byte| *byte == 0).unwrap_or(8);
            let name = &entry[..name_length];
            let size = u32_at(entry, 8)?.max(u32_at(entry, 16)?) as u64;
            let offset = u32_at(entry, 12)? as u64;
            let flags = u32_at(entry, 36)?;
            if size == 0
                || offset < header_length
                || offset.checked_add(size).is_none_or(|end| end > module.size)
            {
                return Err("Секция EXE выходит за границы модуля".into());
            }
            let executable = flags & SECTION_EXECUTE != 0;
            let known_data = matches!(name, b".rdata" | b".data");
            let pdata = name == b".pdata";
            let excluded = matches!(name, b".reloc" | b".pdata" | b".rsrc");
            let load = !excluded
                && flags & SECTION_READ != 0
                && (executable || known_data || flags & (SECTION_CODE | SECTION_DATA) != 0);
            if pdata && (flags & SECTION_READ == 0 || executable) {
                return Err("Секция границ функций EXE не подтверждена".into());
            }
            specs.push(SectionSpec {
                offset,
                size,
                text: name == b".text",
                pdata,
                load,
                executable,
            });
        }
        specs.sort_unstable_by_key(|spec| spec.offset);
        if specs
            .windows(2)
            .any(|pair| pair[0].offset + pair[0].size > pair[1].offset)
        {
            return Err("Секции EXE перекрываются; восстановление остановлено".into());
        }
        if specs
            .iter()
            .filter(|spec| spec.text && spec.load && spec.executable)
            .count()
            != 1
        {
            return Err("Не найдена единственная секция кода .text".into());
        }
        let text = specs
            .iter()
            .find(|spec| spec.text && spec.load && spec.executable)
            .ok_or("Не найдена секция .text")?;
        let text_start = module.base + text.offset;
        let text_end = text_start + text.size;
        if specs.iter().filter(|spec| spec.pdata).count() != 1 || u32_at(&optional, 108)? < 4 {
            return Err("Не найдена единственная таблица границ функций EXE".into());
        }
        let exception_offset = u32_at(&optional, 136)? as u64;
        let exception_size = u32_at(&optional, 140)? as usize;
        if exception_size == 0 || exception_size % 12 != 0 {
            return Err("Некорректный размер таблицы границ функций EXE".into());
        }
        let headers = reader.read(0, header_length as usize)?;
        let mut fingerprint = Sha256::new();
        fingerprint.update(headers);
        let mut sections = Vec::new();
        let mut pdata_bytes = None;
        for spec in specs {
            if !spec.load && !spec.pdata {
                continue;
            }
            let bytes = reader.read(spec.offset, spec.size as usize)?;
            if spec.pdata {
                pdata_bytes = Some((spec.offset, bytes));
                continue;
            }
            if spec.text {
                fingerprint.update(&bytes);
            }
            sections.push(Section {
                address: module.base + spec.offset,
                bytes,
                executable: spec.executable,
            });
        }
        let (pdata_offset, pdata_bytes) =
            pdata_bytes.ok_or("Не прочитана таблица границ функций EXE")?;
        let table_start = exception_offset
            .checked_sub(pdata_offset)
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or("Таблица границ функций выходит за секцию .pdata")?;
        let table_end = table_start
            .checked_add(exception_size)
            .ok_or("Переполнение таблицы границ функций")?;
        let entries = pdata_bytes
            .get(table_start..table_end)
            .ok_or("Таблица границ функций выходит за секцию .pdata")?;
        let mut functions = Vec::with_capacity(exception_size / 12);
        for (index, entry) in entries.chunks_exact(12).enumerate() {
            if index % 4096 == 0 {
                check_deadline(started)?;
                cancelled(cancel)?;
            }
            let begin = module.base + u32_at(entry, 0)? as u64;
            let end = module.base + u32_at(entry, 4)? as u64;
            let unwind = module.base + u32_at(entry, 8)? as u64;
            if begin >= end
                || begin < text_start
                || end > text_end
                || unwind % 4 != 0
                || !sections.iter().any(|section| {
                    !section.executable
                        && unwind >= section.address
                        && unwind + 4 <= section.address + section.bytes.len() as u64
                })
                || functions
                    .last()
                    .is_some_and(|previous: &(u64, u64)| begin < previous.1)
            {
                return Err("Некорректные или перекрывающиеся границы функций EXE".into());
            }
            functions.push((begin, end));
        }
        check_deadline(started)?;
        cancelled(cancel)?;
        Ok(Self {
            module: module.clone(),
            sections,
            functions,
            fingerprint: hex::encode(fingerprint.finalize()),
            started,
        })
    }

    pub(super) fn bytes(&self, address: u64, length: usize) -> Result<&[u8]> {
        let end = address
            .checked_add(length as u64)
            .ok_or("Переполнение адреса в EXE")?;
        let section = self
            .sections
            .iter()
            .find(|section| {
                address >= section.address && end <= section.address + section.bytes.len() as u64
            })
            .ok_or("Адрес отсутствует в проверенных секциях EXE")?;
        let offset = (address - section.address) as usize;
        Ok(&section.bytes[offset..offset + length])
    }

    /// Не захватывает соседнюю функцию даже при коротком конструкторе.
    pub(super) fn function_bytes(&self, address: u64, max_len: usize) -> Result<&[u8]> {
        if max_len == 0 {
            return Err("Пустой диапазон чтения функции EXE".into());
        }
        let index = self.functions.partition_point(|entry| entry.0 <= address);
        let (_, end) = self
            .functions
            .get(
                index
                    .checked_sub(1)
                    .ok_or("Не подтверждена граница функции EXE")?,
            )
            .filter(|entry| address < entry.1)
            .ok_or("Не подтверждена граница функции EXE")?;
        let length = max_len.min((end - address) as usize);
        self.bytes(address, length)
    }

    /// Любая неоднозначность сверх лимита прерывает поиск, а не скрывает совпадения.
    pub(super) fn find(&self, needle: &[u8]) -> Result<Vec<u64>> {
        if needle.is_empty() {
            return Err("Пустой образец поиска в EXE".into());
        }
        let finder = memchr::memmem::Finder::new(needle);
        let mut found = Vec::new();
        for section in &self.sections {
            let mut offset = 0;
            loop {
                check_deadline(self.started)?;
                let Some(relative) = finder.find(&section.bytes[offset..]) else {
                    break;
                };
                offset += relative;
                push_match(&mut found, section.address + offset as u64, MAX_MATCHES)?;
                // Сдвиг на один байт сохраняет перекрывающиеся точные совпадения.
                offset += 1;
            }
        }
        check_deadline(self.started)?;
        Ok(found)
    }

    pub(super) fn pointer_references(&self, target: u64) -> Result<Vec<u64>> {
        let needle = target.to_le_bytes();
        let finder = memchr::memmem::Finder::new(&needle);
        let mut found = Vec::new();
        for section in self.sections.iter().filter(|section| !section.executable) {
            let mut offset = 0;
            loop {
                check_deadline(self.started)?;
                let Some(relative) = finder.find(&section.bytes[offset..]) else {
                    break;
                };
                offset += relative;
                let address = section.address + offset as u64;
                if address % 8 == 0 {
                    push_match(&mut found, address, MAX_POINTER_REFERENCES)?;
                }
                offset += 1;
            }
        }
        check_deadline(self.started)?;
        Ok(found)
    }

    pub(super) fn is_code(&self, address: u64) -> bool {
        self.sections.iter().any(|section| {
            section.executable
                && address >= section.address
                && address < section.address + section.bytes.len() as u64
        })
    }

    pub(super) fn is_data(&self, address: u64) -> bool {
        self.sections.iter().any(|section| {
            !section.executable
                && address >= section.address
                && address < section.address + section.bytes.len() as u64
        })
    }

    pub(super) fn fingerprint(&self) -> String {
        self.fingerprint.clone()
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16> {
    let word: [u8; 2] = bytes
        .get(offset..offset + 2)
        .ok_or("Нет поля u16 в PE")?
        .try_into()
        .map_err(|_| "Размер u16 в PE")?;
    Ok(u16::from_le_bytes(word))
}

fn push_match(found: &mut Vec<u64>, address: u64, limit: usize) -> Result<()> {
    if found.len() == limit {
        return Err("Слишком много совпадений в EXE; адрес нельзя подтвердить".into());
    }
    found.push(address);
    Ok(())
}

fn check_deadline(started: Instant) -> Result<()> {
    if started.elapsed() > SEARCH_TIMEOUT {
        Err("Превышено время восстановления карты".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial::MemoryRange;

    struct PeMemory {
        headers: Vec<u8>,
        read_end: u64,
    }

    impl Memory for PeMemory {
        fn read(&mut self, address: u64, length: usize) -> Result<Vec<u8>> {
            let offset = address.checked_sub(0x140000000).ok_or("Адрес вне EXE")? as usize;
            self.read_end = self.read_end.max(offset as u64 + length as u64);
            self.headers
                .get(offset..offset + length)
                .map(|bytes| bytes.to_vec())
                .ok_or_else(|| "Тест запрещает читать секции до проверки границ".into())
        }

        fn ranges(&self) -> Vec<MemoryRange> {
            Vec::new()
        }

        fn modules(&self) -> Vec<MemoryModule> {
            vec![MemoryModule {
                name: "Warframe.x64.exe".into(),
                base: 0x140000000,
                size: 1024 * 1024,
            }]
        }
    }

    #[test]
    fn rejects_overlapping_or_out_of_module_sections_before_payload_read() {
        let mut headers = vec![0; 0x400];
        headers[..2].copy_from_slice(b"MZ");
        headers[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
        headers[0x80..0x84].copy_from_slice(b"PE\0\0");
        headers[0x84..0x86].copy_from_slice(&0x8664_u16.to_le_bytes());
        headers[0x86..0x88].copy_from_slice(&2_u16.to_le_bytes());
        headers[0x94..0x96].copy_from_slice(&0xf0_u16.to_le_bytes());
        headers[0x98..0x9a].copy_from_slice(&0x20b_u16.to_le_bytes());
        headers[0xd0..0xd4].copy_from_slice(&(1024 * 1024_u32).to_le_bytes());
        headers[0xd4..0xd8].copy_from_slice(&0x400_u32.to_le_bytes());
        let first = 0x188;
        let second = first + 40;
        headers[first..first + 5].copy_from_slice(b".text");
        headers[first + 8..first + 12].copy_from_slice(&0x1000_u32.to_le_bytes());
        headers[first + 12..first + 16].copy_from_slice(&0x1000_u32.to_le_bytes());
        headers[first + 36..first + 40]
            .copy_from_slice(&(SECTION_CODE | SECTION_READ | SECTION_EXECUTE).to_le_bytes());
        headers[second..second + 6].copy_from_slice(b".rdata");
        headers[second + 8..second + 12].copy_from_slice(&0x2000_u32.to_le_bytes());
        headers[second + 36..second + 40].copy_from_slice(&SECTION_READ.to_le_bytes());
        for offset in [0x1800_u32, 0xff000_u32] {
            headers[second + 12..second + 16].copy_from_slice(&offset.to_le_bytes());
            let mut memory = PeMemory {
                headers: headers.clone(),
                read_end: 0,
            };
            let result = ModuleImage::load(&mut memory, &AtomicBool::new(false));
            let Err(error) = result else {
                panic!("Повреждённые PE-секции не должны допускаться");
            };
            assert!(error.contains("перекрываются") || error.contains("границы"));
            assert!(memory.read_end <= 0x400);
        }
    }
}
