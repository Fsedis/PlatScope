//! Тайники миссии: назначение из типа, состояние из малой группы живых объектов.
use super::{
    Memory, Result,
    source::{q, u32_at, u64_at},
    types::{Decoder, TypeInfo},
};

pub(super) fn is_grineer_cache(info: &TypeInfo) -> bool {
    info.block("Mesh")
        .is_some_and(|v| v.rsplit('/').next() == Some("GrnStorageLocker_skel.fbx"))
        && info.block("OverrideMaterial").is_some_and(|v| {
            v.lines().any(|l| {
                l.trim() == "/Lotus/Objects/Grineer/Structural/Doors/GrineerDoorHatchWhiteCache"
            })
        })
        && info.block("ServerChildren").is_some_and(|v| {
            v.lines()
                .any(|l| l.trim() == "Type=CacheLockerReplicatedHitSwitch")
        })
}
fn opening_action(info: &TypeInfo) -> bool {
    info.names.iter().any(|n| n == "ContextAction *")
        && info.block("CompleteScript").is_some_and(|v| {
            v.lines()
                .any(|l| l.trim() == "Script=/Lotus/Scripts/SabotageCaches.lua")
                && v.lines().any(|l| l.trim() == "Function=CacheStorageLocker")
        })
        && info.block("ActionText").as_deref()
            == Some("/Lotus/Language/Game/OrokinSabotageOpenCache")
        && info.block("DestroyOnUse").as_deref() == Some("1")
}
pub(super) fn state(
    m: &mut dyn Memory,
    address: u64,
    decoder: &mut Decoder,
    base: u64,
) -> Result<&'static str> {
    let header = m.read(address + 0x3b0, 16)?;
    let pointer = u64_at(&header, 0)?;
    let size = u32_at(&header, 8)? as usize;
    let capacity = u32_at(&header, 12)? as usize;
    if size == 0 || !size.is_multiple_of(8) || size > capacity || capacity > 512 {
        return Ok("unknown");
    }
    let children = m.read(pointer, size)?;
    let mut available = false;
    for child in children.chunks_exact(8) {
        let handle = u64_at(child, 0)?;
        let native = q(m, handle)?;
        if native == 0 {
            continue;
        }
        let object = m.read(native, 24)?;
        if u64_at(&object, 0)? != base + 0x213cf68 {
            continue;
        }
        if u64_at(&object, 16)? != handle {
            return Ok("unknown");
        }
        let info = decoder.chain(m, u64_at(&object, 8)?)?;
        if opening_action(&info) {
            available = true;
        }
    }
    if m.read(address + 0x3b0, 16)? != header {
        return Ok("unknown");
    }
    if available {
        return Ok("available");
    }
    // Отсутствия действия недостаточно: подтверждаем включение анимации открытия.
    let active = q(m, address + 0x1e8)?;
    let animation = q(m, address + 0x4b8)?;
    if active != animation || active == 0 || m.read(address + 0x480, 1)?[0] & 0x10 == 0 {
        return Ok("unknown");
    }
    let native = q(m, animation)?;
    if native == 0 || q(m, native + 16)? != animation {
        return Ok("unknown");
    }
    // Проверяем конкретный AnimScene, не пытаясь декодировать не относящийся
    // к задаче базовый класс ресурса с другой структурой метаданных.
    super::context::has_type(m, native, base, 0x28ac110)?;
    if q(m, address + 0x1e8)? != active || m.read(address + 0x3b0, 16)? != header {
        return Ok("unknown");
    }
    Ok("opened")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Адресная проверка локальной записи трёх тайников"]
    fn recorded_cache_transition() {
        let path = std::env::var("PLATSCOPE_TEST_CACHE_ARCHIVE").expect("Путь локальной записи");
        for (sequence, expected) in [
            (1, ["opened", "available", "available"]),
            (5, ["opened", "available", "opened"]),
        ] {
            let mut memory = super::super::ArchiveMemory::open(
                std::path::Path::new(&path),
                sequence,
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
            let profile = super::super::profile::Profile::validate(&mut memory).unwrap();
            let mut decoder = Decoder::new(profile.clone()).unwrap();
            let mut variant = None;
            for (address, expected) in [0x2611a55b980, 0x2617ee10350, 0x26193f2a310]
                .into_iter()
                .zip(expected)
            {
                let metadata = q(&mut memory, address + 8).unwrap();
                let info = decoder.chain(&mut memory, metadata).unwrap();
                assert!(is_grineer_cache(&info));
                let key = info.variant_key();
                assert_eq!(variant.get_or_insert_with(|| key.clone()), &key);
                assert_eq!(
                    state(&mut memory, address, &mut decoder, profile.base).unwrap(),
                    expected
                );
            }
        }
    }
    fn info(properties: &[&str]) -> TypeInfo {
        TypeInfo {
            names: vec!["ContextAction *".into()],
            properties: properties.iter().map(|p| (*p).into()).collect(),
        }
    }
    const CACHE: &str = "Mesh=/Lotus/Objects/Grineer/Structural/Doors/GrnStorageLocker_skel.fbx\nOverrideMaterial={\n/Lotus/Objects/Grineer/Structural/Doors/GrineerDoorHatchWhiteCache\n}\nServerChildren={\n{\nType=CacheLockerReplicatedHitSwitch\n}\n}\n";
    #[test]
    fn cache_requires_semantics_and_respects_overrides() {
        assert!(is_grineer_cache(&info(&[CACHE])));
        assert!(!is_grineer_cache(&info(&[&CACHE.replace(
            "CacheLockerReplicatedHitSwitch",
            "LockerAttachments/LockerReplicatedHitSwitchRare"
        )])));
        assert!(!is_grineer_cache(&info(&["OverrideMaterial={}\n", CACHE])));
        assert!(!is_grineer_cache(&info(&[&format!(
            "Other={{\n{CACHE}}}\n"
        )])));
    }
    #[test]
    fn action_requires_same_script_block_and_exact_action() {
        let properties = "CompleteScript={\nScript=/Lotus/Scripts/SabotageCaches.lua\nFunction=CacheStorageLocker\n}\nActionText=/Lotus/Language/Game/OrokinSabotageOpenCache\nDestroyOnUse=1\n";
        assert!(opening_action(&info(&[properties])));
        assert!(!opening_action(&info(&["CompleteScript={}\n", properties])));
        assert!(!opening_action(&info(&[
            &properties.replace("DestroyOnUse=1", "DestroyOnUse=0")
        ])));
    }
}
