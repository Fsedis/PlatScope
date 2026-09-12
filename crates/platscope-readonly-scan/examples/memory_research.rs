//! Исследование структуры JSON экипировки без дампа памяти и персональных строк.
use std::{collections::BTreeMap, path::PathBuf, time::Instant};
#[path = "../src/research_schema.rs"]
mod schema;
use schema::walk;

fn main() -> anyhow::Result<()> {
    let Some(pid) = platscope_readonly_scan::scan::find_wf_pid() else {
        anyhow::bail!("Warframe не запущен");
    };
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/memory-research-loadout-schema.json"));
    let start = Instant::now();
    let candidates = platscope_readonly_scan::squad::capture(pid)?;
    let mut fields = BTreeMap::new();
    for candidate in &candidates {
        walk("loadout", &candidate.value, &mut fields, 0);
    }
    let report = serde_json::json!({"capturedAt":chrono::Utc::now(), "elapsedMs":start.elapsed().as_millis(), "candidates":candidates.len(), "fields":fields});
    std::fs::write(&output, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "Наборов: {}, полей: {}, время: {:.2} с. Схема: {}",
        candidates.len(),
        fields.len(),
        start.elapsed().as_secs_f64(),
        output.display()
    );
    Ok(())
}
