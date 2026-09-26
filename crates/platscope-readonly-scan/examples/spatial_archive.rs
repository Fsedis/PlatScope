use platscope_readonly_scan::spatial::{ProfilePack, analyze_archive_with_profiles};
use std::{path::Path, sync::atomic::AtomicBool};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 && args.len() != 6 {
        eprintln!(
            "spatial_archive <archive> <snapshot> <output.json> [profile.json profile.json.sig]"
        );
        std::process::exit(2)
    }
    let pack = if args.len() == 6 {
        ProfilePack::from_signed(
            &std::fs::read(&args[4]).expect("profile.json"),
            &std::fs::read(&args[5]).expect("profile.json.sig"),
        )
    } else {
        ProfilePack::bundled()
    }
    .expect("подписанный профиль");
    let seq = args[2].parse().expect("snapshot");
    let start = std::time::Instant::now();
    match analyze_archive_with_profiles(
        Path::new(&args[1]),
        seq,
        &pack,
        &AtomicBool::new(false),
        |p| {
            if p.stage == "Готово" {
                eprintln!("{} objects, {} bytes", p.object_count, p.scanned_bytes)
            }
        },
    ) {
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
