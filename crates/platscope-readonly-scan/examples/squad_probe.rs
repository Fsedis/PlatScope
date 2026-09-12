//! Проверка только структуры экипировки: без имён, сетевых адресов и сырого JSON.
fn main() -> anyhow::Result<()> {
    let Some(pid) = platscope_readonly_scan::scan::find_wf_pid() else {
        println!("Warframe не запущен");
        return Ok(());
    };
    let candidates = platscope_readonly_scan::squad::capture(pid)?;
    println!("Различных наборов: {}", candidates.len());
    for (index, candidate) in candidates.iter().enumerate().take(8) {
        println!(
            "Набор {index}: {} байт; {} копий",
            candidate.bytes, candidate.copies
        );
        for item in candidate.value["NORMAL"]
            .as_array()
            .into_iter()
            .flatten()
            .take(4)
        {
            println!(
                "Поля предмета: {:?}",
                item.as_object().map(|o| o.keys().collect::<Vec<_>>())
            );
            if let Some(upgrades) = item["WeaponUpgrades"].as_array() {
                println!(
                    "WeaponUpgrades: {} элементов; примеры: {}",
                    upgrades.len(),
                    serde_json::to_string(
                        &upgrades
                            .iter()
                            .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                            .take(4)
                            .collect::<Vec<_>>()
                    )?
                );
            }
        }
    }
    Ok(())
}
