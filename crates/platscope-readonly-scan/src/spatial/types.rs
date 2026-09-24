use super::{Memory, Result};
use super::{
    profile::Profile,
    source::{q, string, u32_at, u64_at},
};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Default)]
pub(super) struct TypeInfo {
    pub names: Vec<String>,
    pub properties: Vec<String>,
}
impl TypeInfo {
    /// Стабильное описание игрового типа: без адреса экземпляра и текущего состояния.
    pub fn variant_key(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        for value in self.names.iter().chain(self.properties.iter()) {
            // Один Mesh может быть записан полным или относительным путём.
            let normalized = value
                .lines()
                .map(|line| {
                    if let Some(mesh) = line.strip_prefix("Mesh=") {
                        format!("Mesh={}", mesh.rsplit('/').next().unwrap_or(mesh))
                    } else {
                        line.trim_end().to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            hash.update((normalized.len() as u64).to_le_bytes());
            hash.update(normalized.as_bytes());
        }
        format!("type-v1:{:x}", hash.finalize())
    }
    /// Первое верхнеуровневое поле с учётом переопределений производного типа.
    pub fn block(&self, name: &str) -> Option<String> {
        let prefix = format!("{name}=");
        for properties in &self.properties {
            let mut depth = 0i32;
            let mut selected = None;
            for line in properties.lines() {
                if depth == 0
                    && let Some(value) = line.strip_prefix(&prefix)
                {
                    selected = Some(value.to_owned());
                } else if let Some(value) = &mut selected {
                    value.push('\n');
                    value.push_str(line);
                }
                depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                if depth == 0 && selected.is_some() {
                    return selected;
                }
            }
        }
        None
    }
    pub fn field(&self, name: &str) -> Option<String> {
        let prefix = format!("{name}=");
        self.properties
            .iter()
            .flat_map(|s| s.lines())
            .find_map(|l| l.strip_prefix(&prefix))
            .filter(|s| !s.is_empty() && *s != "\"\"")
            .map(str::to_owned)
    }
}
pub(super) struct Decoder {
    profile: Profile,
    context: zstd_safe::DCtx<'static>,
    cache: HashMap<u64, TypeInfo>,
    cache_bytes: usize,
}
impl Decoder {
    pub fn new(profile: Profile) -> Result<Self> {
        let mut context = zstd_safe::DCtx::create();
        context
            .set_parameter(zstd_safe::DParameter::Format(
                zstd_safe::FrameFormat::Magicless,
            ))
            .map_err(|e| format!("Zstd: {}", zstd_safe::get_error_name(e)))?;
        context
            .set_parameter(zstd_safe::DParameter::WindowLogMax(20))
            .map_err(|e| format!("Zstd: {}", zstd_safe::get_error_name(e)))?;
        Ok(Self {
            profile,
            context,
            cache: HashMap::new(),
            cache_bytes: 0,
        })
    }
    pub fn chain(&mut self, m: &mut dyn Memory, start: u64) -> Result<TypeInfo> {
        if let Some(v) = self.cache.get(&start) {
            return Ok(v.clone());
        }
        let mut p = start;
        let mut seen = HashSet::new();
        let mut info = TypeInfo::default();
        for _ in 0..32 {
            if p == 0 {
                let cost = info.properties.iter().map(String::len).sum::<usize>()
                    + info.names.iter().map(String::len).sum::<usize>();
                if self.cache_bytes + cost > 32 * 1024 * 1024 || self.cache.len() >= 20_000 {
                    self.cache.clear();
                    self.cache_bytes = 0;
                }
                self.cache_bytes += cost;
                self.cache.insert(start, info.clone());
                return Ok(info);
            }
            if p > 0x0000_7fff_ffef_ffff {
                return Err("Некорректный адрес метаданных".into());
            }
            if !seen.insert(p) {
                return Err("Цикл родителей типа".into());
            }
            let head = m.read(p, 32)?;
            let vt = u64_at(&head, 0)?;
            if ![
                self.profile.address(0x203fb50)?,
                self.profile.address(0x203fba8)?,
                self.profile.address(0x203fc60)?,
            ]
            .contains(&vt)
            {
                return Err("Неизвестная структура метаданных".into());
            }
            if vt == self.profile.address(0x203fc60)? {
                let binding = q(m, p + 0x60)?;
                if binding != 0 {
                    let nameptr = q(m, binding + 8)?;
                    let name = string(m, nameptr)?;
                    if !info.names.contains(&name) {
                        info.names.push(name);
                    }
                }
            }
            if vt != self.profile.address(0x203fb50)? {
                let data = m.read(p + 0x48, 16)?;
                let at = u64_at(&data, 0)?;
                let n = u32_at(&data, 8)? as usize;
                if n > 1_048_576 {
                    return Err("Свойства типа слишком велики".into());
                }
                if n != 0 {
                    let bytes = m.read(at, n)?;
                    let decoded = if data[13] != 0 {
                        let (size, skip) = varint(&bytes)?;
                        if size == 0 || size > 1_048_576 {
                            return Err("Недопустимый размер свойств".into());
                        }
                        let mut output = vec![0; size];
                        let count = self
                            .context
                            .decompress_using_dict(
                                output.as_mut_slice(),
                                &bytes[skip..],
                                &self.profile.dictionary,
                            )
                            .map_err(|e| {
                                format!("Не распакованы свойства: {}", zstd_safe::get_error_name(e))
                            })?;
                        if count != size {
                            return Err("Не совпадает длина свойств".into());
                        }
                        output
                    } else {
                        bytes
                    };
                    let text = String::from_utf8(decoded).map_err(|_| "Свойства не UTF-8")?;
                    if info.properties.iter().map(String::len).sum::<usize>() + text.len()
                        > 2_097_152
                    {
                        return Err("Слишком большая цепь свойств".into());
                    }
                    info.properties.push(text);
                }
            }
            p = u64_at(&head, 24)?;
        }
        Err("Слишком длинная цепь типов".into())
    }
}
fn varint(bytes: &[u8]) -> Result<(usize, usize)> {
    let mut value = 0u32;
    for (i, b) in bytes.iter().copied().take(5).enumerate() {
        if i == 4 && b > 15 {
            return Err("Переполнение varint".into());
        }
        value |= u32::from(b & 127) << (i * 7);
        if b & 128 == 0 {
            return Ok((value as usize, i + 1));
        }
    }
    Err("Незавершённый varint".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn varint_rejects_overflow() {
        assert!(varint(&[255; 5]).is_err());
        assert!(varint(&[128]).is_err());
        assert_eq!(varint(&[0x92, 1]).unwrap(), (146, 2));
    }
}
