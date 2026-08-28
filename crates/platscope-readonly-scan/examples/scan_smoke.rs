use platscope_readonly_scan::inventory::InventoryScanner;

fn main() {
    let scanner = InventoryScanner::new();
    match scanner.scan(None, None) {
        Ok((bytes, info)) => println!(
            "scan_ok bytes={} rows={} build={} credential_hits={} distinct_credentials={}",
            bytes.len(),
            platscope_inventory::parse_read_only_scan_json(
                std::str::from_utf8(&bytes).expect("DE inventory response must be UTF-8")
            )
            .expect("PlatScope must accept the real DE inventory response")
            .metadata
            .item_count,
            info.build.as_deref().unwrap_or("unknown"),
            info.cred_hits,
            info.distinct_creds
        ),
        Err(_) => {
            eprintln!("scan_failed; session credentials were discarded");
            std::process::exit(1);
        }
    }
}
