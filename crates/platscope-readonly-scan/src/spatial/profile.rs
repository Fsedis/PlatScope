use super::source::{q, string};
use super::{Memory, Result};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug)]
pub(crate) struct Profile {
    pub base: u64,
    pub dictionary: Vec<u8>,
}
impl Profile {
    pub fn validate(m: &mut dyn Memory) -> Result<Self> {
        let module = m
            .modules()
            .into_iter()
            .find(|x| x.name.eq_ignore_ascii_case("Warframe.x64.exe"))
            .ok_or("В источнике нет Warframe.x64.exe")?;
        if module.size != 47_116_288 {
            return Err("Эта версия игры пока не поддерживается исследованием карты".into());
        }
        let base = module.base;
        if base > 0x0000_7fff_ffff_ffff - module.size {
            return Err("Некорректная граница модуля".into());
        }
        for (rva, expected) in [
            (
                0x27c2a0,
                "4883ec38488b094885c97502cd2c488b4424604889442420e8939a640133c948",
            ),
            (
                0xabc2e0,
                "488bca33d2e9f6b6bd00cccccccccccc48895c2418574883ec40488b053f39dd",
            ),
        ] {
            let bytes = m.read(base.checked_add(rva).ok_or("Переполнение модуля")?, 32)?;
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
        ] {
            if q(m, base + meta)? != base + 0x203fc60 {
                return Err("Не подтверждена структура типов игры".into());
            }
            let binding = q(m, base + meta + 0x60)?;
            let nameptr = q(m, binding + 8)?;
            if string(m, nameptr)? != name {
                return Err("Не подтверждены имена типов игры".into());
            }
        }
        // Только относительный адрес проверенного глобального контекста; heap-адреса берутся из текущего источника.
        let context = q(m, base + 0x28a4888)?;
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
        if format!("{:x}", Sha256::digest(&dictionary))
            != "1ac925c4d06ec74c9c58d21034102f53722da08ead9e9cc48ee60cf7aabb2f49"
        {
            return Err("Словарь свойств изменился; исследование остановлено".into());
        }
        Ok(Self { base, dictionary })
    }
    pub fn targets(&self) -> Vec<(u64, &'static str)> {
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
        ]
        .into_iter()
        .map(|(r, n)| (self.base + r, n))
        .collect()
    }
}
