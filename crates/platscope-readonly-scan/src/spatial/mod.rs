//! Ограниченное исследование сцены. Состояния доступности и принадлежность игроку не угадываются.
mod analyze;
mod archive;
mod cache;
mod context;
mod geometry;
mod profile;
mod source;
mod types;

pub use analyze::{analyze_archive, analyze_live, refresh_live};
pub use archive::ArchiveMemory;
use serde::{Deserialize, Serialize};
pub use source::{Memory, MemoryModule, MemoryRange};
use std::{path::Path, sync::atomic::AtomicBool};

pub type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisProgress {
    pub stage: String,
    pub scanned_bytes: u64,
    pub total_bytes: u64,
    pub object_count: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub label: String,
    pub value: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneObject {
    pub key: String,
    pub kind: String,
    pub label: String,
    pub name_en: String,
    pub item_path: Option<String>,
    #[serde(default)]
    pub variant_key: Option<String>,
    pub position: [f32; 3],
    #[serde(default = "fresh_position")]
    pub position_fresh: bool,
    pub type_names: Vec<String>,
    pub availability: String,
    pub details: Vec<Detail>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneMesh {
    pub key: String,
    pub vertices: Vec<[f32; 3]>,
    pub faces: Vec<Vec<u32>>,
    pub adjacency: Vec<[u32; 2]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenePlayer {
    pub key: String,
    pub avatar_key: String,
    pub operator_key: Option<String>,
    pub local: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneZone {
    pub key: String,
    pub min: [f32; 3],
    pub max: [f32; 3],
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneStats {
    pub scanned_bytes: u64,
    pub object_count: u64,
    pub mesh_count: u64,
    pub vertex_count: u64,
    pub face_count: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub format: u32,
    pub source: String,
    pub started_at: String,
    pub captured_at: String,
    pub complete: bool,
    pub profile: String,
    pub objects: Vec<SceneObject>,
    pub meshes: Vec<SceneMesh>,
    #[serde(default)]
    pub players: Vec<ScenePlayer>,
    #[serde(default)]
    pub zones: Vec<SceneZone>,
    #[serde(default)]
    pub zones_fresh: bool,
    pub warnings: Vec<String>,
    pub stats: SceneStats,
    #[serde(skip)]
    pub(crate) identities: Vec<Identity>,
    #[serde(skip)]
    pub(crate) process: Option<(u32, String)>,
    #[serde(skip)]
    pub(crate) discovery_profile: Option<profile::Profile>,
    #[serde(skip)]
    pub(crate) discovery_ranges: Vec<MemoryRange>,
    #[serde(skip)]
    pub(crate) discovery_cursor: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct Identity {
    pub address: u64,
    pub vtable: u64,
    pub metadata: u64,
    pub handle: u64,
    pub zone: Option<u64>,
    pub item: Option<u64>,
    pub moving: bool,
    pub missed_reads: u8,
}
fn fresh_position() -> bool {
    true
}
/// Краткий перечень завершённых снимков, без содержимого памяти.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSnapshot {
    pub sequence: u64,
    pub started_at: String,
    pub ended_at: String,
    pub complete: bool,
    pub bytes: u64,
    pub holes: u64,
}
pub fn list_archive(path: &Path) -> Result<Vec<ArchiveSnapshot>> {
    archive::list(path)
}
pub(crate) fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        Err("Исследование отменено".into())
    } else {
        Ok(())
    }
}
