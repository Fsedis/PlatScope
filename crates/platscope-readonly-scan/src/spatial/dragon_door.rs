//! Двери хранилищ: назначение по требуемому ключу размещённого действия.
use super::{
    Memory, Result,
    source::q,
    types::{Decoder, TypeInfo},
};

pub(super) const REQUIRED_ITEM: u64 = 0x7a8;

pub(super) struct DragonKey {
    pub code: &'static str,
    pub label: &'static str,
    pub english: &'static str,
}

fn key_type(info: &TypeInfo) -> Option<DragonKey> {
    if !info.names.iter().any(|n| n == "Restorative *") {
        return None;
    }
    let (code, label, english, function) = match info.block("LocalizeTag")?.as_str() {
        "/Lotus/Language/Items/HealthDebuffKeyName" => (
            "health",
            "ключ здоровья",
            "Bleeding Dragon Key",
            "HealthKeyOnUse",
        ),
        "/Lotus/Language/Items/ShieldDebuffKeyName" => (
            "shield",
            "ключ щитов",
            "Decaying Dragon Key",
            "ShieldKeyOnUse",
        ),
        "/Lotus/Language/Items/DamageDebuffKeyName" => (
            "damage",
            "ключ урона",
            "Extinguished Dragon Key",
            "DamageKeyOnUse",
        ),
        "/Lotus/Language/Items/SpeedDebuffKeyName" => (
            "speed",
            "ключ скорости",
            "Hobbled Dragon Key",
            "SpeedKeyOnUse",
        ),
        _ => return None,
    };
    let script = info.block("ScriptInstance")?;
    if !script
        .lines()
        .any(|l| l == "Script=/Lotus/Scripts/Restoratives/CorruptedKey.lua")
        || !script.lines().any(|l| l == format!("Function={function}"))
    {
        return None;
    }
    Some(DragonKey {
        code,
        label,
        english,
    })
}

pub(super) fn required_key(
    m: &mut dyn Memory,
    action: u64,
    decoder: &mut Decoder,
) -> Result<(DragonKey, u64)> {
    let reference = m.read(action + REQUIRED_ITEM, 16)?;
    let handle = super::source::u64_at(&reference, 0)?;
    let metadata = super::source::u64_at(&reference, 8)?;
    let item = q(m, handle)?;
    if item == 0 || q(m, item + 16)? != handle || q(m, item + 8)? != metadata {
        return Err("Не подтверждён требуемый предмет двери".into());
    }
    let key = key_type(&decoder.chain(m, metadata)?).ok_or("Действие не требует ключа Дракона")?;
    if m.read(action + REQUIRED_ITEM, 16)? != reference || q(m, handle)? != item {
        return Err("Требование ключа изменилось при чтении".into());
    }
    Ok((key, item))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::Path, sync::atomic::AtomicBool};

    #[test]
    #[ignore = "Адресная проверка полной записи двери Дракона"]
    fn recorded_dragon_door_and_four_keys() {
        let path = std::env::var("PLATSCOPE_TEST_DRAGON_ARCHIVE").expect("Путь записи");
        for seq in 1..=3 {
            let mut m =
                super::super::ArchiveMemory::open(Path::new(&path), seq, &AtomicBool::new(false))
                    .unwrap();
            let profile = super::super::profile::Profile::validate(&mut m).unwrap();
            let mut decoder = Decoder::new(profile.clone()).unwrap();
            for (meta, code) in [
                (0x2611cdb7170, "damage"),
                (0x2611cdb7220, "health"),
                (0x2611cdb7278, "shield"),
                (0x2611cdb72d0, "speed"),
            ] {
                let mut info = decoder.chain(&mut m, meta).unwrap();
                assert_eq!(key_type(&info).unwrap().code, code);
                info.properties.insert(0, "ScriptInstance={}\n".into());
                assert!(key_type(&info).is_none());
            }
            let (object, id) = super::super::analyze::object(
                &mut m,
                0x2620c5ae9f0,
                "dragon_door",
                profile.base + 0x213cf68,
                &mut decoder,
                profile.base,
            )
            .unwrap();
            assert_eq!(object.kind, "dragon_door");
            assert_eq!(object.variant_key.as_deref(), Some("dragon-door-v1:health"));
            assert_eq!(id.item_offset, REQUIRED_ITEM);
            assert!((object.position[0] - 208.74991).abs() < 0.01);
            assert!(
                super::super::analyze::object(
                    &mut m,
                    0x2619a3eada0,
                    "dragon_door",
                    profile.base + 0x213cf68,
                    &mut decoder,
                    profile.base
                )
                .is_err()
            );
        }
    }
}
