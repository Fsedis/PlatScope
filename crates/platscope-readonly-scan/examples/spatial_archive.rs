use platscope_readonly_scan::spatial::analyze_archive;
use std::{path::Path, sync::atomic::AtomicBool};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("spatial_archive <archive> <snapshot> <output.json>");
        std::process::exit(2)
    }
    let seq = args[2].parse().expect("snapshot");
    let start = std::time::Instant::now();
    match analyze_archive(Path::new(&args[1]), seq, &AtomicBool::new(false), |p| {
        if p.stage == "Готово" {
            eprintln!("{} objects, {} bytes", p.object_count, p.scanned_bytes)
        }
    }) {
        Ok(scene) => {
            let file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&args[3])
                .expect("new output");
            serde_json::to_writer(file, &scene).unwrap();
            println!(
                "{:.2}s objects={} meshes={} vertices={} faces={}",
                start.elapsed().as_secs_f32(),
                scene.objects.len(),
                scene.meshes.len(),
                scene.stats.vertex_count,
                scene.stats.face_count
            )
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1)
        }
    }
}
