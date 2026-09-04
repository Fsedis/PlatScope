fn main() {
    let manifest_directory = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
    );
    let source = manifest_directory.join("../../../THIRD_PARTY_NOTICES.md");
    let destination = manifest_directory.join("resources/THIRD_PARTY_NOTICES.md");
    println!("cargo:rerun-if-changed={}", source.display());
    std::fs::create_dir_all(
        destination
            .parent()
            .expect("bundle notice destination has a parent"),
    )
    .expect("bundle resources directory is created");
    std::fs::copy(&source, &destination).expect("third-party notices are copied into the bundle");
    tauri_build::build();
    // Tauri встраивает манифест в основной бинарник, но не в отдельные examples.
    // Предпросмотру тоже нужен Common Controls v6 для TaskDialogIndirect.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-examples=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-examples=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
}
