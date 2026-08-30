use platscope_readonly_scan::inventory::InventoryScanner;

fn main() {
    let scanner = InventoryScanner::new();
    match scanner.scan(None, None) {
        Ok(result) => println!(
            "scan_ok bytes={} rows={} build={} credential_hits={} distinct_credentials={} nightwave={} offers={}",
            result.inventory_bytes.len(),
            platscope_inventory::parse_read_only_scan_json(
                std::str::from_utf8(&result.inventory_bytes)
                    .expect("DE inventory response must be UTF-8")
            )
            .expect("PlatScope must accept the real DE inventory response")
            .metadata
            .item_count,
            result.session.build.as_deref().unwrap_or("unknown"),
            result.session.cred_hits,
            result.session.distinct_creds,
            result.nightwave_status.code(),
            result
                .nightwave_vendor
                .as_ref()
                .map_or(0, |snapshot| snapshot.offers.len())
        ),
        Err(_) => {
            eprintln!("scan_failed; session credentials were discarded");
            std::process::exit(1);
        }
    }
}
