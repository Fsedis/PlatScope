use super::{Memory, Result, SceneMesh};
use super::{
    profile::Profile,
    source::{q, u32_at, u64_at},
};
use std::collections::{BTreeMap, BTreeSet};
fn float(bytes: &[u8], i: usize) -> Result<f32> {
    Ok(f32::from_bits(u32_at(bytes, i)?))
}
fn finite(v: f32) -> bool {
    v.is_finite() && v.abs() < 100_000.
}
pub(super) fn matrix(m: &mut dyn Memory, address: u64) -> Result<[[f32; 4]; 4]> {
    matrix_impl(m, address, false)
}
fn matrix_impl(m: &mut dyn Memory, address: u64, scaled: bool) -> Result<[[f32; 4]; 4]> {
    let b = m.read(address + 0xa0, 64)?;
    let mut out = [[0.; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = float(&b, i * 16 + j * 4)?;
            if !finite(*v) {
                return Err("Недопустимая матрица".into());
            }
        }
    }
    for i in 0..3 {
        for j in 0..3 {
            let mut dot = (0..3).map(|k| out[i][k] * out[j][k]).sum::<f32>();
            if scaled {
                let norm_i = (0..3).map(|k| out[i][k] * out[i][k]).sum::<f32>().sqrt();
                let norm_j = (0..3).map(|k| out[j][k] * out[j][k]).sum::<f32>().sqrt();
                if !(0.001..100.).contains(&norm_i) || !(0.001..100.).contains(&norm_j) {
                    return Err("Недопустимый масштаб объекта".into());
                }
                dot /= norm_i * norm_j;
            }
            if (dot - if i == j { 1. } else { 0. }).abs() > 0.003 {
                return Err("Матрица поворота не согласована".into());
            }
        }
    }
    if out[..3].iter().any(|row| row[3].abs() > 0.001) || (out[3][3] - 1.).abs() > 0.001 {
        return Err("Матрица не инициализирована".into());
    }
    Ok(out)
}
pub(super) fn position(m: &mut dyn Memory, a: u64, moving: bool) -> Result<[f32; 3]> {
    let b = m.read(a + 0x70, 12)?;
    let p = [float(&b, 0)?, float(&b, 4)?, float(&b, 8)?];
    if !p.iter().copied().all(finite) {
        return Err("Некорректные координаты".into());
    }
    let mat = matrix_impl(m, a, true)?;
    if (0..3).any(|i| (p[i] - mat[3][i]).abs() > if moving { 2.0 } else { 0.02 }) {
        return Err("Положение и матрица прочитаны в разных состояниях".into());
    }
    Ok(p)
}
pub(super) fn mesh(m: &mut dyn Memory, p: u64, profile: &Profile) -> Result<SceneMesh> {
    let handle = q(m, p + 0x4d8)?;
    let geo = q(m, handle)?;
    if q(m, geo)? != profile.base + 0x21277b0 || q(m, geo + 0x10)? != handle {
        return Err("Не подтверждён ресурс геометрии".into());
    }
    let mut arrays = Vec::new();
    for off in [0x38, 0x48, 0x58, 0x68] {
        let b = m.read(geo + off, 16)?;
        let pointer = u64_at(&b, 0)?;
        let size = u32_at(&b, 8)? as usize;
        let capacity = u32_at(&b, 12)? as usize;
        if size == 0 || size > 5_000_000 || size > capacity {
            return Err("Недопустимый массив геометрии".into());
        }
        arrays.push(m.read(pointer, size)?);
    }
    if arrays[0].len() % 12 != 0
        || arrays[1].len() % 10 != 0
        || arrays[2].len() % 22 != 0
        || arrays[3].len() % 2 != 0
    {
        return Err("Не совпадает схема геометрии".into());
    }
    let vertex_count = arrays[0].len() / 12;
    let face_count = arrays[2].len() / 22;
    let edge_count = arrays[1].len() / 10;
    if vertex_count > 65_535 || face_count > 65_535 || edge_count > 65_535 {
        return Err("Слишком сложная геометрия".into());
    }
    let mat = matrix(m, p)?;
    let mut vertices = Vec::with_capacity(vertex_count);
    for b in arrays[0].as_chunks::<12>().0 {
        let v = [float(b, 0)?, float(b, 4)?, float(b, 8)?];
        if !v.iter().copied().all(finite) {
            return Err("Некорректная вершина".into());
        }
        let world =
            std::array::from_fn(|i| (0..3).map(|j| v[j] * mat[j][i]).sum::<f32>() + mat[3][i]);
        if !world.iter().copied().all(finite) {
            return Err("Некорректная мировая вершина".into());
        }
        vertices.push(world);
    }
    let mut face_edges = vec![Vec::new(); face_count];
    let mut adjacency = BTreeSet::new();
    for b in arrays[1].as_chunks::<10>().0 {
        let e: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect();
        if e[0] as usize >= vertex_count || e[1] as usize >= vertex_count || e[0] == e[1] {
            return Err("Неверное ребро".into());
        }
        for f in &e[2..4] {
            if *f != u16::MAX {
                if *f as usize >= face_count {
                    return Err("Индекс грани вне диапазона".into());
                }
                face_edges[*f as usize].push((e[0], e[1]));
            }
        }
        if e[2] != u16::MAX && e[3] != u16::MAX && e[2] != e[3] {
            adjacency.insert([u32::from(e[2].min(e[3])), u32::from(e[2].max(e[3]))]);
        }
    }
    for b in arrays[3].as_chunks::<2>().0 {
        if u16::from_le_bytes([b[0], b[1]]) as usize >= edge_count {
            return Err("Индекс ребра вне диапазона".into());
        }
    }
    let mut faces = Vec::with_capacity(face_count);
    for edges in face_edges {
        let mut neighbours: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        for (a, b) in edges {
            neighbours.entry(a).or_default().push(b);
            neighbours.entry(b).or_default().push(a);
        }
        if neighbours.len() < 3
            || neighbours.len() > 1024
            || neighbours.values().any(|v| v.len() != 2)
        {
            return Err("Грань не образует замкнутый контур".into());
        }
        let first = *neighbours.keys().next().ok_or("Пустая грань")?;
        let mut path = vec![first];
        let mut previous = u16::MAX;
        let mut current = first;
        loop {
            let next = *neighbours
                .get(&current)
                .and_then(|v| v.iter().find(|v| **v != previous))
                .ok_or("Разорванная грань")?;
            if next == first {
                break;
            }
            if path.contains(&next) || path.len() >= neighbours.len() {
                return Err("Повтор вершины в контуре".into());
            }
            path.push(next);
            previous = current;
            current = next;
        }
        if path.len() != neighbours.len() {
            return Err("Несвязная грань".into());
        }
        faces.push(path.into_iter().map(u32::from).collect());
    }
    Ok(SceneMesh {
        key: format!("0x{p:x}"),
        vertices,
        faces,
        adjacency: adjacency.into_iter().collect(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_nan() {
        assert!(!finite(f32::NAN));
        assert!(!finite(f32::INFINITY));
        assert!(finite(-123.4));
    }
    struct Mock(Vec<u8>);
    impl Memory for Mock {
        fn read(&mut self, a: u64, n: usize) -> Result<Vec<u8>> {
            self.0
                .get(a as usize..a as usize + n)
                .map(|b| b.to_vec())
                .ok_or("gap".into())
        }
        fn ranges(&self) -> Vec<super::super::MemoryRange> {
            vec![]
        }
        fn modules(&self) -> Vec<super::super::MemoryModule> {
            vec![]
        }
    }
    fn put(memory: &mut Mock, a: usize, bytes: &[u8]) {
        memory.0[a..a + bytes.len()].copy_from_slice(bytes)
    }
    fn fixture() -> Mock {
        let mut m = Mock(vec![0; 8192]);
        let p = 0x100usize;
        let geo = 0x800usize;
        let h = 0x700usize;
        put(&mut m, p + 0x4d8, &(h as u64).to_le_bytes());
        put(&mut m, h, &(geo as u64).to_le_bytes());
        put(&mut m, geo, &0x21277b0u64.to_le_bytes());
        put(&mut m, geo + 0x10, &(h as u64).to_le_bytes());
        for i in 0..4 {
            put(&mut m, p + 0xa0 + i * 20, &1f32.to_le_bytes());
        }
        for (off, ptr, size) in [
            (0x38, 0x1000, 36),
            (0x48, 0x1100, 30),
            (0x58, 0x1200, 22),
            (0x68, 0x1300, 6),
        ] {
            put(&mut m, geo + off, &(ptr as u64).to_le_bytes());
            put(&mut m, geo + off + 8, &(size as u32).to_le_bytes());
            put(&mut m, geo + off + 12, &(size as u32).to_le_bytes());
        }
        for (i, v) in [0f32, 0., 0., 1., 0., 0., 0., 0., 1.]
            .into_iter()
            .enumerate()
        {
            put(&mut m, 0x1000 + i * 4, &v.to_le_bytes());
        }
        for (i, e) in [
            [0u16, 1, 0, 65535, 0],
            [1, 2, 0, 65535, 0],
            [2, 0, 0, 65535, 0],
        ]
        .iter()
        .enumerate()
        {
            for (j, v) in e.iter().enumerate() {
                put(&mut m, 0x1100 + i * 10 + j * 2, &v.to_le_bytes());
            }
            put(&mut m, 0x1300 + i * 2, &(i as u16).to_le_bytes());
        }
        m
    }
    #[test]
    fn reconstructs_closed_polygon_and_rejects_invalid_index() {
        let profile = Profile {
            base: 0,
            dictionary: vec![],
        };
        let mut m = fixture();
        let scene = mesh(&mut m, 0x100, &profile).unwrap();
        assert_eq!(scene.vertices.len(), 3);
        assert_eq!(scene.faces, vec![vec![0, 1, 2]]);
        put(&mut m, 0x1100, &9u16.to_le_bytes());
        assert!(mesh(&mut m, 0x100, &profile).is_err());
    }
    #[test]
    fn rejects_changed_matrix() {
        let profile = Profile {
            base: 0,
            dictionary: vec![],
        };
        let mut m = fixture();
        put(&mut m, 0x100 + 0xa0, &2f32.to_le_bytes());
        assert!(mesh(&mut m, 0x100, &profile).is_err());
    }
    #[test]
    fn scaled_object_and_animated_position_are_not_mistaken_for_bad_mesh() {
        let mut m = fixture();
        for i in 0..3 {
            put(&mut m, 0x100 + 0xa0 + i * 20, &1.2f32.to_le_bytes());
        }
        for (i, v) in [3f32, 4., 5.].iter().enumerate() {
            put(&mut m, 0x100 + 0x70 + i * 4, &v.to_le_bytes());
            put(&mut m, 0x100 + 0xd0 + i * 4, &v.to_le_bytes());
        }
        assert_eq!(position(&mut m, 0x100, false).unwrap(), [3., 4., 5.]);
        put(&mut m, 0x100 + 0x70, &3.15f32.to_le_bytes());
        assert!(position(&mut m, 0x100, false).is_err());
        assert!(position(&mut m, 0x100, true).is_ok());
        put(&mut m, 0x100 + 0x70, &300f32.to_le_bytes());
        assert!(position(&mut m, 0x100, true).is_err());
    }
}
