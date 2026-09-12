use platscope_readonly_scan::spatial::{analyze_live, refresh_live};
use std::sync::atomic::AtomicBool;
fn main() {
    let pid = std::env::args()
        .nth(1)
        .expect("pid")
        .parse()
        .expect("pid number");
    let cancel = AtomicBool::new(false);
    let start = std::time::Instant::now();
    let scene = analyze_live(pid, &cancel, |p| {
        if p.stage == "Готово" {
            eprintln!("Готово: {} объектов", p.object_count)
        }
    })
    .expect("live analysis");
    println!(
        "full_seconds={:.3} objects={} meshes={} complete={}",
        start.elapsed().as_secs_f64(),
        scene.objects.len(),
        scene.meshes.len(),
        scene.complete
    );
    let mut s = scene;
    println!(
        "players={} npc={}",
        s.objects.iter().filter(|o| o.kind == "avatar").count(),
        s.objects.iter().filter(|o| o.kind == "npc").count()
    );
    for _ in 0..3 {
        let start = std::time::Instant::now();
        let scanned_before = s.stats.scanned_bytes;
        s = refresh_live(pid, &s, &cancel).expect("refresh");
        println!(
            "refresh_ms={:.2} objects={} discovery_bytes={} stale_positions={}",
            start.elapsed().as_secs_f64() * 1000.,
            s.objects.len(),
            s.stats.scanned_bytes.saturating_sub(scanned_before),
            s.objects.iter().filter(|o| !o.position_fresh).count()
        );
    }
}
