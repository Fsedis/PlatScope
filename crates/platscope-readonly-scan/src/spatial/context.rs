//! Адресные связи игрока и родной мини-карты. Никаких глобальных обходов.
use super::source::{q, u32_at, u64_at};
use super::{Memory, Result, Scene, ScenePlayer, SceneZone, cancelled};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

const PLAYER: u64 = 0x29c9600;
const MINIMAP: u64 = 0x29e7ea0;
const ZONE: u64 = 0x28dd4e0;

fn checked(address: u64, offset: u64) -> Result<u64> {
    if address == 0 || address > 0x0000_7fff_ffef_ffff {
        return Err("Некорректный адрес связи".into());
    }
    address
        .checked_add(offset)
        .ok_or("Переполнение связи".into())
}
fn linked(m: &mut dyn Memory, address: u64, offset: u64) -> Result<u64> {
    let handle = q(m, checked(address, offset)?)?;
    let object = q(m, checked(handle, 0)?)?;
    if q(m, checked(object, 16)?)? != handle {
        return Err("Связь объекта изменилась".into());
    }
    Ok(object)
}
pub(super) fn has_type(m: &mut dyn Memory, address: u64, base: u64, expected: u64) -> Result<()> {
    let handle = q(m, checked(address, 16)?)?;
    if q(m, checked(handle, 0)?)? != address {
        return Err("Обратная ссылка объекта не совпадает".into());
    }
    let mut meta = q(m, checked(address, 8)?)?;
    for _ in 0..24 {
        let head = m.read(checked(meta, 0)?, 32)?;
        let vt = u64_at(&head, 0)?;
        if ![0x203fb50, 0x203fba8, 0x203fc60, 0x203fc00]
            .iter()
            .any(|r| base + r == vt)
        {
            return Err("Тип связи не подтверждён".into());
        }
        if meta == base + expected && vt == base + 0x203fc60 {
            return Ok(());
        }
        meta = u64_at(&head, 24)?;
    }
    Err("Не найден ожидаемый тип связи".into())
}
pub(super) fn owner(m: &mut dyn Memory, avatar: u64, base: u64) -> Result<u64> {
    let player = linked(m, avatar, 0x520)?;
    has_type(m, player, base, PLAYER)?;
    if linked(m, player, 0x118)? != avatar {
        return Err("Игрок не управляет этим персонажем".into());
    }
    Ok(player)
}
fn local_map(m: &mut dyn Memory, player: u64, avatar: u64, base: u64) -> Result<u64> {
    let camera = linked(m, player, 0x30)?;
    has_type(m, camera, base, 0x28cc310)?;
    let map = linked(m, player, 0xd098)?;
    has_type(m, map, base, MINIMAP)?;
    if linked(m, map, 0xe0)? != avatar || owner(m, avatar, base)? != player {
        return Err("Мини-карта связана с другим персонажем".into());
    }
    Ok(map)
}
fn zone_array(
    m: &mut dyn Memory,
    map: u64,
    offset: u64,
    stride: usize,
    base: u64,
) -> Result<BTreeMap<String, SceneZone>> {
    let header_at = checked(map, offset)?;
    let header = m.read(header_at, 16)?;
    let pointer = u64_at(&header, 0)?;
    let size = u32_at(&header, 8)? as usize;
    let capacity = u32_at(&header, 12)? as usize;
    if size == 0
        || size > capacity
        || capacity > 65_536
        || !size.is_multiple_of(stride)
        || size / stride > 512
    {
        return Err("Неверная длина массива зон".into());
    }
    let bytes = m.read(checked(pointer, 0)?, size)?;
    let mut zones = BTreeMap::new();
    for record in bytes.chunks_exact(stride) {
        let zone = u64_at(record, 0x58)?;
        has_type(m, zone, base, ZONE)?;
        let vector = |start| -> [f32; 3] {
            std::array::from_fn(|i| {
                f32::from_le_bytes(
                    record[start + i * 4..start + i * 4 + 4]
                        .try_into()
                        .expect("record bounds"),
                )
            })
        };
        let min = vector(0);
        let max = vector(16);
        if !min
            .iter()
            .chain(&max)
            .all(|v| v.is_finite() && v.abs() < 1_000_000.0)
            || (0..3).any(|i| min[i] > max[i])
        {
            return Err("Некорректные границы зоны".into());
        }
        if stride == 128 {
            let handle = u64_at(record, 0x60)?;
            if q(m, checked(handle, 0)?)? != zone || q(m, checked(zone, 16)?)? != handle {
                return Err("Изменилась ссылка зоны".into());
            }
        }
        let key = format!("0x{zone:x}");
        if zones
            .insert(key.clone(), SceneZone { key, min, max })
            .is_some()
        {
            return Err("Повтор зоны в массиве".into());
        }
    }
    if m.read(header_at, 16)? != header {
        return Err("Массив зон изменился во время чтения".into());
    }
    Ok(zones)
}
fn read_zones(m: &mut dyn Memory, map: u64, base: u64) -> Result<Vec<SceneZone>> {
    let first = zone_array(m, map, 0xe8, 128, base)?;
    let second = zone_array(m, map, 0x608, 96, base)?;
    if first != second {
        return Err("Два списка зон мини-карты не согласованы".into());
    }
    Ok(first.into_values().collect())
}
pub(super) fn update(
    m: &mut dyn Memory,
    scene: &mut Scene,
    base: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut players = Vec::new();
    let mut maps = Vec::new();
    for object in scene.objects.iter().filter(|o| o.kind == "avatar").take(16) {
        cancelled(cancel)?;
        if !object.position_fresh {
            continue;
        }
        let Ok(avatar) = u64::from_str_radix(object.key.trim_start_matches("0x"), 16) else {
            continue;
        };
        let Ok(player) = owner(m, avatar, base) else {
            continue;
        };
        let operator_key = linked(m, player, 0xd188)
            .ok()
            .filter(|p| has_type(m, *p, base, 0x29cb220).is_ok())
            .map(|p| format!("0x{p:x}"));
        let map = local_map(m, player, avatar, base).ok();
        if let Some(map) = map {
            maps.push((player, avatar, map));
        }
        players.push(ScenePlayer {
            key: format!("0x{player:x}"),
            avatar_key: object.key.clone(),
            operator_key,
            local: map.is_some(),
        });
    }
    // Несколько кандидатов не дают права выбрать одного произвольно.
    if maps.len() != 1 {
        for player in &mut players {
            player.local = false;
        }
    }
    scene.players = players;
    scene.zones_fresh = false;
    if let [(player, avatar, map)] = maps.as_slice()
        && let Ok(zones) = read_zones(m, *map, base)
        && local_map(m, *player, *avatar, base).ok() == Some(*map)
    {
        scene.zones = zones;
        scene.zones_fresh = true;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial::{MemoryModule, MemoryRange};
    const BASE: u64 = 0x100_0000;
    #[derive(Default)]
    struct Mock(BTreeMap<u64, u8>);
    impl Mock {
        fn put(&mut self, p: u64, bytes: &[u8]) {
            for (i, b) in bytes.iter().enumerate() {
                self.0.insert(p + i as u64, *b);
            }
        }
        fn q(&mut self, p: u64, v: u64) {
            self.put(p, &v.to_le_bytes());
        }
        fn native(&mut self, p: u64, kind: u64) {
            self.put(p, &[0; 24]);
            self.q(p + 8, BASE + kind);
            self.q(p + 16, p + 0x800);
            self.q(p + 0x800, p);
            self.put(BASE + kind, &[0; 32]);
            self.q(BASE + kind, BASE + 0x203fc60);
        }
        fn link(&mut self, p: u64, off: u64, to: u64) {
            self.q(p + off, to + 0x800);
        }
    }
    impl Memory for Mock {
        fn read(&mut self, p: u64, n: usize) -> Result<Vec<u8>> {
            (0..n)
                .map(|i| {
                    self.0
                        .get(&(p + i as u64))
                        .copied()
                        .ok_or("Нет данных".into())
                })
                .collect()
        }
        fn ranges(&self) -> Vec<MemoryRange> {
            vec![]
        }
        fn modules(&self) -> Vec<MemoryModule> {
            vec![]
        }
    }
    fn fixture() -> (Mock, Scene) {
        let mut m = Mock::default();
        for (p, kind) in [
            (0x1000, 0x297bf70),
            (0x3000, PLAYER),
            (0x6000, MINIMAP),
            (0x8000, 0x28cc310),
            (0xa000, ZONE),
        ] {
            m.native(p, kind);
        }
        for (p, off, to) in [
            (0x1000, 0x520, 0x3000),
            (0x3000, 0x118, 0x1000),
            (0x3000, 0x30, 0x8000),
            (0x3000, 0xd098, 0x6000),
            (0x6000, 0xe0, 0x1000),
        ] {
            m.link(p, off, to);
        }
        for (off, ptr, stride) in [(0xe8, 0x20000, 128usize), (0x608, 0x21000, 96)] {
            m.q(0x6000 + off, ptr);
            m.put(0x6000 + off + 8, &(stride as u32).to_le_bytes());
            m.put(0x6000 + off + 12, &(stride as u32).to_le_bytes());
            let mut bytes = vec![0; stride];
            for (at, v) in [
                (0, 1.0f32),
                (4, 2.0),
                (8, 3.0),
                (16, 11.0),
                (20, 12.0),
                (24, 13.0),
            ] {
                bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
            bytes[0x58..0x60].copy_from_slice(&0xa000u64.to_le_bytes());
            if stride == 128 {
                bytes[0x60..0x68].copy_from_slice(&0xa800u64.to_le_bytes());
            }
            m.put(ptr, &bytes);
        }
        let scene = serde_json::from_value(serde_json::json!({"format":1,"source":"live","startedAt":"test","capturedAt":"test","complete":true,"profile":"test","objects":[{"key":"0x1000","kind":"avatar","label":"Персонаж","nameEn":"Avatar","itemPath":null,"position":[1,2,3],"typeNames":[],"availability":"unknown","details":[]}],"meshes":[],"warnings":[],"stats":{"scannedBytes":0,"objectCount":1,"meshCount":0,"vertexCount":0,"faceCount":0}})).unwrap();
        (m, scene)
    }
    #[test]
    fn validates_local_player_and_both_bounded_zone_arrays() {
        let (mut m, mut scene) = fixture();
        update(&mut m, &mut scene, BASE, &AtomicBool::new(false)).unwrap();
        assert_eq!(scene.players.len(), 1);
        assert!(scene.players[0].local);
        assert!(scene.zones_fresh);
        assert_eq!(scene.zones[0].min, [1.0, 2.0, 3.0]);
        // Согласованность камеры, мини-карты и обратной ссылки обязательна.
        m.q(0x3000 + 0x118, 0);
        assert!(owner(&mut m, 0x1000, BASE).is_err());
        update(&mut m, &mut scene, BASE, &AtomicBool::new(false)).unwrap();
        assert!(scene.players.is_empty());
        assert!(!scene.zones_fresh);
        assert_eq!(scene.zones.len(), 1);
    }
    #[test]
    fn inconsistent_zone_frame_retains_last_good_map() {
        let (mut m, mut scene) = fixture();
        update(&mut m, &mut scene, BASE, &AtomicBool::new(false)).unwrap();
        let old = scene.zones.clone();
        m.put(0x21000, &9.0f32.to_le_bytes());
        update(&mut m, &mut scene, BASE, &AtomicBool::new(false)).unwrap();
        assert!(!scene.zones_fresh);
        assert_eq!(scene.zones, old);
        m.put(0x21000, &1.0f32.to_le_bytes());
        m.put(0x20000, &f32::NAN.to_le_bytes());
        assert!(read_zones(&mut m, 0x6000, BASE).is_err());
    }
    #[test]
    fn rejects_oversized_buffers_wrong_type_and_duplicate_local_candidates() {
        let (mut m, _) = fixture();
        m.put(0x6000 + 0xe8 + 8, &100_000u32.to_le_bytes());
        assert!(read_zones(&mut m, 0x6000, BASE).is_err());
        m.q(0x3000 + 8, BASE + ZONE);
        assert!(owner(&mut m, 0x1000, BASE).is_err());
        let (mut m, mut scene) = fixture();
        scene.objects.push(scene.objects[0].clone());
        update(&mut m, &mut scene, BASE, &AtomicBool::new(false)).unwrap();
        assert!(scene.players.iter().all(|p| !p.local));
        assert!(!scene.zones_fresh);
    }
}
