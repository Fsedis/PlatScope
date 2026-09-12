//! Проверка на собственном дочернем процессе, без чтения Warframe.
use anyhow::Result;
use platscope_readonly_scan::binary_snapshot::{Archive, Process};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
};

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn fixture() -> Result<()> {
    let mut data = vec![0x37u8; 1024 * 1024];
    data[..8].copy_from_slice(&123.5f64.to_le_bytes());
    let mut pointer = vec![0x91u8; 4096];
    pointer[..8].copy_from_slice(&(data.as_ptr() as u64).to_le_bytes());
    println!(
        "{}",
        json!({"pid":std::process::id(),"data":data.as_ptr() as u64,"pointer":pointer.as_ptr() as u64})
    );
    std::io::stdout().flush()?;
    for line in std::io::stdin().lock().lines() {
        match line?.as_str() {
            "change" => {
                data[..8].copy_from_slice(&456.25f64.to_le_bytes());
                println!("ready");
                std::io::stdout().flush()?;
            }
            "quit" => break,
            _ => {}
        }
    }
    std::hint::black_box((&data, &pointer));
    Ok(())
}

fn main() -> Result<()> {
    if std::env::args().any(|arg| arg == "--fixture") {
        return fixture();
    }
    let mut child = ChildGuard(
        Command::new(std::env::current_exe()?)
            .arg("--fixture")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let mut input = child.0.stdin.take().unwrap();
    let mut output = BufReader::new(child.0.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line)?;
    let meta: Value = serde_json::from_str(&line)?;
    let process = Process::open(meta["pid"].as_u64().unwrap() as u32)?;
    let directory = std::env::current_dir()?.join("target").join(format!(
        "binary-probe-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%9f")
    ));
    let mut archive = Archive::create(&directory, &process, 2 * 1024 * 1024 * 1024)?;
    let cancel = AtomicBool::new(false);
    for step in 1..=2 {
        if step == 2 {
            writeln!(input, "change")?;
            input.flush()?;
            line.clear();
            output.read_line(&mut line)?;
            anyhow::ensure!(line.trim() == "ready");
        }
        let snapshot = archive.capture(&process, &cancel, |_| {})?;
        anyhow::ensure!(snapshot.reason.is_none(), "{:?}", snapshot.reason);
        println!(
            "Снимок {step}: complete={}, прочитано={}, изменено блоков={}, пропусков={}",
            snapshot.complete,
            snapshot.progress.read_bytes,
            snapshot.changed.len(),
            snapshot.holes.len()
        );
    }
    // Проверяем публикацию неполного снимка при остановке в ходе чтения.
    let partial = archive.capture(&process, &cancel, |_| {
        cancel.store(true, Ordering::Relaxed);
    })?;
    anyhow::ensure!(!partial.complete && partial.reason.is_some());
    archive.finish("Проверка завершена")?;
    std::fs::write(directory.join("fixture.json"), serde_json::to_vec(&meta)?)?;
    writeln!(input, "quit")?;
    input.flush()?;
    child.0.wait()?;
    anyhow::ensure!(!process.alive());
    println!("Папка проверки: {}", directory.display());
    Ok(())
}
