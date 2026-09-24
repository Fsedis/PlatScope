use super::source::{q, string};
use super::{Memory, Result};
use sha2::{Digest, Sha256};
#[derive(Clone, Copy, Debug)]
pub(crate) enum RvaMap {
    /// Проверенная версия от 12 сентября: старые RVA уже являются адресами этой версии.
    Legacy,
    /// Для следующей версии каждый используемый RVA должен быть указан явно.
    Mapped(&'static [(u64, u64)]),
}

// Каждый адрес новой сборки подтверждается отдельно; неизвестный RVA отклоняется.
const SEPTEMBER_24_RVAS: &[(u64, u64)] = &[
    (0x27c2a0, 0x68b1c0),
    (0xabc2e0, 0x8863a0),
    (0x203fb50, 0x200acc0),
    (0x203fba8, 0x200ad18),
    (0x203fc00, 0x200ad70),
    (0x203fc60, 0x200add0),
    (0x28ed2a0, 0x28d6bf0),
    (0x297bf70, 0x2965cc0),
    (0x29c9600, 0x29b37d0),
    (0x29e7ea0, 0x29d20b0),
    (0x28ecf80, 0x28d68d0),
    (0x28d6f40, 0x28c0890),
    (0x2960650, 0x294a0a0),
    (0x28ce8c0, 0x28b8160),
    (0x28a4888, 0x288e4f8),
    (0x21a96f8, 0x2176ad0),
    (0x23fb1e0, 0x23c9a20),
    (0x2326098, 0x22f40b0),
    (0x22e25b0, 0x22af0a0),
    (0x211bfe8, 0x20e8858),
    (0x20fd898, 0x20c9858),
    (0x2341320, 0x230f388),
    (0x2208c58, 0x21d5d18),
    (0x21a6fb0, 0x2174358),
    (0x2113b98, 0x20dff50),
    (0x21277b0, 0x20f40e0),
    (0x22a1638, 0x226df20),
    (0x213cf68, 0x2109978),
    (0x28dd4e0, 0x28c6e90),
    (0x28cc310, 0x28b5bc0),
    (0x29cb220, 0x29b53e0),
    (0x28ac110, 0x2895d70),
    (0x28f3470, 0x28dcd80),
    (0x21cecc0, 0x219bbc0),
    (0x28f21b0, 0x28dbae0),
    (0x21d29d8, 0x219f8e8),
    (0xe03440, 0x183e210),
    (0xc66420, 0x128e8c0),
];
#[derive(Clone, Debug)]
pub(crate) struct Profile {
    pub base: u64,
    pub dictionary: Vec<u8>,
    pub rva_map: RvaMap,
}
impl Profile {
    #[cfg(test)]
    pub fn legacy(base: u64) -> Self {
        Self {
            base,
            dictionary: Vec::new(),
            rva_map: RvaMap::Legacy,
        }
    }
    pub fn rva(&self, legacy: u64) -> Result<u64> {
        match self.rva_map {
            RvaMap::Legacy => Ok(legacy),
            RvaMap::Mapped(entries) => entries
                .iter()
                .find(|(old, _)| *old == legacy)
                .map(|(_, current)| *current)
                .ok_or_else(|| format!("RVA 0x{legacy:x} не подтверждён для этой версии игры")),
        }
    }
    pub fn address(&self, legacy: u64) -> Result<u64> {
        self.base
            .checked_add(self.rva(legacy)?)
            .ok_or("Переполнение адреса модуля".into())
    }
    pub fn minimap_link_offset(&self) -> u64 {
        match self.rva_map {
            RvaMap::Legacy => 0xd098,
            RvaMap::Mapped(_) => 0xd0d0,
        }
    }
    pub fn second_zone_array_offset(&self) -> u64 {
        match self.rva_map {
            RvaMap::Legacy => 0x608,
            RvaMap::Mapped(_) => 0x5f8,
        }
    }
    pub fn id(&self) -> &'static str {
        match self.rva_map {
            RvaMap::Legacy => "warframe-2026-09-12-validated",
            RvaMap::Mapped(_) => "warframe-2026-09-24-validated",
        }
    }
    pub fn validate(m: &mut dyn Memory) -> Result<Self> {
        let module = m
            .modules()
            .into_iter()
            .find(|x| x.name.eq_ignore_ascii_case("Warframe.x64.exe"))
            .ok_or("В источнике нет Warframe.x64.exe")?;
        let rva_map = match module.size {
            47_116_288 => RvaMap::Legacy,
            47_022_080 => RvaMap::Mapped(SEPTEMBER_24_RVAS),
            _ => return Err("Эта версия игры пока не поддерживается исследованием карты".into()),
        };
        let base = module.base;
        if base > 0x0000_7fff_ffff_ffff - module.size {
            return Err("Некорректная граница модуля".into());
        }
        let mut profile = Self {
            base,
            dictionary: Vec::new(),
            rva_map,
        };
        for (rva, legacy, current) in [
            (
                0x27c2a0,
                "4883ec38488b094885c97502cd2c488b4424604889442420e8939a640133c948",
                "4883ec38488b094885c97502cd2c488b4424604889442420e8b3c4900033c948",
            ),
            (
                0xabc2e0,
                "488bca33d2e9f6b6bd00cccccccccccc48895c2418574883ec40488b053f39dd",
                "488bca33d2e916477900cccccccccccc48895c2410488974241848897c242041",
            ),
        ] {
            let bytes = m.read(profile.address(rva)?, 32)?;
            let expected = if matches!(rva_map, RvaMap::Legacy) {
                legacy
            } else {
                current
            };
            if bytes.iter().map(|b| format!("{b:02x}")).collect::<String>() != expected {
                return Err("Код игры отличается от проверенной версии; карта не построена".into());
            }
        }
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
            if q(m, profile.address(meta)?)? != profile.address(0x203fc60)? {
                return Err("Не подтверждена структура типов игры".into());
            }
            let binding = q(m, profile.address(meta)? + 0x60)?;
            let nameptr = q(m, binding + 8)?;
            if string(m, nameptr)? != name {
                return Err("Не подтверждены имена типов игры".into());
            }
        }
        // Только относительный адрес проверенного глобального контекста; heap-адреса берутся из текущего источника.
        let context = q(m, profile.address(0x28a4888)?)?;
        let ddict = q(
            m,
            context
                .checked_add(0x75f0)
                .ok_or("Переполнение контекста")?,
        )?;
        let pointer = q(m, ddict + 8)?;
        let size = q(m, ddict + 16)?;
        if size != 1_048_576 {
            return Err("Не подтверждён словарь свойств игры".into());
        }
        let dictionary = m.read(pointer, size as usize)?;
        let expected_dictionary = if matches!(rva_map, RvaMap::Legacy) {
            "1ac925c4d06ec74c9c58d21034102f53722da08ead9e9cc48ee60cf7aabb2f49"
        } else {
            "ca53aed745c568a42e4ebd325f46315c0c10cafd62d81f5669efd78447b043d5"
        };
        if format!("{:x}", Sha256::digest(&dictionary)) != expected_dictionary {
            return Err("Словарь свойств изменился; исследование остановлено".into());
        }
        profile.dictionary = dictionary;
        Ok(profile)
    }
    pub fn targets(&self) -> Result<Vec<(u64, &'static str)>> {
        [
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
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_profile_rejects_unconfirmed_rva() {
        let profile = Profile {
            base: 0x1000,
            dictionary: Vec::new(),
            rva_map: RvaMap::Mapped(&[(0x200, 0x300)]),
        };
        assert_eq!(profile.address(0x200).unwrap(), 0x1300);
        assert!(profile.address(0x201).is_err());
    }
}
