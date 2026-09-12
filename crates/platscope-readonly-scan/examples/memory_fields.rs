//! Разовое исследование без записи сырых значений процесса.
fn main() -> anyhow::Result<()> {
    let pid = platscope_readonly_scan::scan::find_wf_pid()
        .ok_or_else(|| anyhow::anyhow!("Warframe не запущен"))?;
    let report = platscope_readonly_scan::research::capture(pid)?;
    let output = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "target/memory-research-fields.json".into());
    std::fs::write(&output, serde_json::to_vec_pretty(&report)?)?;
    println!("Схема сохранена: {}", output.display());
    Ok(())
}
