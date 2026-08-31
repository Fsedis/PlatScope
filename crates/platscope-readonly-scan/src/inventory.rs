//! DE inventory fetch: memory-scan the running game for the session creds, then
//! call `inventory.php` with them.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use platscope_domain::{NightwaveVendorOffer, NightwaveVendorSnapshot};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::error::ScanError;
use crate::scan::{SessionInfo, find_wf_pid, scan_session};

const INVENTORY_URL: &str = "https://api.warframe.com/api/inventory.php";
const NIGHTWAVE_STATE_URL: &str = "https://api.warframestat.us/pc/nightwave?language=en";
const VENDOR_INFO_URL: &str = "https://api.warframe.com/api/getVendorInfo.php";
const MAX_VENDOR_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NightwaveVendorScanStatus {
    Captured,
    PublicStateUnavailable,
    SeasonTagInvalid,
    VendorUnavailable,
    ResponseInvalid,
}

impl NightwaveVendorScanStatus {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Captured => "captured",
            Self::PublicStateUnavailable => "public_state_unavailable",
            Self::SeasonTagInvalid => "season_tag_invalid",
            Self::VendorUnavailable => "vendor_unavailable",
            Self::ResponseInvalid => "response_invalid",
        }
    }
}

pub struct ReadOnlyScanResult {
    pub inventory_bytes: Vec<u8>,
    pub session: SessionInfo,
    pub nightwave_vendor: Option<NightwaveVendorSnapshot>,
    pub nightwave_status: NightwaveVendorScanStatus,
}

#[derive(Debug, Deserialize)]
struct NightwavePublicState {
    tag: String,
}

fn game_client(info: &SessionInfo) -> Result<Client> {
    Client::builder()
        .user_agent(format!(
            "Warframe/{}",
            info.build.as_deref().unwrap_or("unknown")
        ))
        .timeout(Duration::from_secs(60))
        .build()
        .context("building HTTP client")
}

fn fetch_inventory_for_session(
    client: &Client,
    info: &SessionInfo,
    platform_tag: Option<&str>,
) -> Result<Vec<u8>> {
    let ct = platform_tag.unwrap_or(&info.ct);

    let mut params: Vec<(&str, &str)> = vec![
        ("accountId", &info.account_id),
        ("nonce", &info.nonce),
        ("ct", ct),
    ];
    if let Some(b) = &info.build {
        params.push(("appVersion", b.as_str()));
    }
    let resp = client
        .get(INVENTORY_URL)
        .query(&params)
        .send()
        .context("inventory request failed")?;
    let status = resp.status();
    let bytes = resp.bytes().context("reading inventory response")?;
    if !status.is_success() || bytes.len() < 1024 {
        let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(400)]);
        bail!(
            "Inventory endpoint returned HTTP {status} ({} bytes).\nBody:\n{preview}\n\n\
             If the response was small or 4xx, DE may have rotated something.",
            bytes.len()
        );
    }
    Ok(bytes.to_vec())
}

fn fetch_game_data(pid: Option<u32>, platform_tag: Option<String>) -> Result<ReadOnlyScanResult> {
    let pid = match pid {
        Some(pid) => pid,
        None => find_wf_pid().ok_or_else(|| {
            anyhow!(
                "Warframe doesn't appear to be running.\n\
                 Start the game, log past the title screen, then retry."
            )
        })?,
    };
    let info = scan_session(pid).context("memory scan failed")?;
    let client = game_client(&info)?;

    // После единственного прохода памяти два независимых сетевых чтения идут
    // одновременно: получение магазина Норы не должно замедлять инвентарь.
    let (inventory_result, nightwave_result) = std::thread::scope(|scope| {
        let info_ref = &info;
        let platform_tag_ref = platform_tag.as_deref();
        let inventory_client = client.clone();
        let inventory = scope.spawn(move || {
            fetch_inventory_for_session(&inventory_client, info_ref, platform_tag_ref)
        });
        let vendor_client = client.clone();
        let vendor = scope.spawn(move || fetch_nightwave_vendor(&vendor_client, info_ref));
        let inventory_result = inventory
            .join()
            .map_err(|_| anyhow!("inventory request thread stopped unexpectedly"))
            .and_then(|result| result);
        let nightwave_result = vendor
            .join()
            .unwrap_or((None, NightwaveVendorScanStatus::ResponseInvalid));
        (inventory_result, nightwave_result)
    });
    let inventory_bytes = inventory_result?;
    Ok(ReadOnlyScanResult {
        inventory_bytes,
        session: info,
        nightwave_vendor: nightwave_result.0,
        nightwave_status: nightwave_result.1,
    })
}

fn fetch_nightwave_vendor(
    client: &Client,
    info: &SessionInfo,
) -> (Option<NightwaveVendorSnapshot>, NightwaveVendorScanStatus) {
    let public_state = match client.get(NIGHTWAVE_STATE_URL).send() {
        Ok(response) if response.status().is_success() => {
            match response.json::<NightwavePublicState>() {
                Ok(state) => state,
                Err(_) => return (None, NightwaveVendorScanStatus::PublicStateUnavailable),
            }
        }
        _ => return (None, NightwaveVendorScanStatus::PublicStateUnavailable),
    };
    let Some(vendor_type) = nightwave_vendor_type(&public_state.tag) else {
        return (None, NightwaveVendorScanStatus::SeasonTagInvalid);
    };
    let ct = info.ct.as_str();
    let mut params = vec![
        ("accountId", info.account_id.as_str()),
        ("nonce", info.nonce.as_str()),
        ("ct", ct),
        ("vendor", vendor_type.as_str()),
    ];
    if let Some(build) = info.build.as_deref() {
        params.push(("appVersion", build));
    }
    let response = match client.get(VENDOR_INFO_URL).query(&params).send() {
        Ok(response) if response.status().is_success() => response,
        _ => return (None, NightwaveVendorScanStatus::VendorUnavailable),
    };
    if response
        .content_length()
        .is_some_and(|length| length > MAX_VENDOR_RESPONSE_BYTES)
    {
        return (None, NightwaveVendorScanStatus::ResponseInvalid);
    }
    let bytes = match response.bytes() {
        Ok(bytes) if !bytes.is_empty() && bytes.len() as u64 <= MAX_VENDOR_RESPONSE_BYTES => bytes,
        _ => return (None, NightwaveVendorScanStatus::ResponseInvalid),
    };
    match parse_nightwave_vendor(&bytes, &public_state.tag, &vendor_type) {
        Ok(snapshot) => (Some(snapshot), NightwaveVendorScanStatus::Captured),
        Err(_) => (None, NightwaveVendorScanStatus::ResponseInvalid),
    }
}

fn nightwave_vendor_type(tag: &str) -> Option<String> {
    let compact = tag
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let stem = compact.strip_suffix("Syndicate")?;
    if !stem.starts_with("RadioLegion")
        || stem.len() > 96
        || !stem
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(format!(
        "/Lotus/Types/Game/VendorManifests/Events/{stem}VendorManifest"
    ))
}

fn parse_nightwave_vendor(
    bytes: &[u8],
    season_tag: &str,
    expected_vendor_type: &str,
) -> Result<NightwaveVendorSnapshot> {
    let root: Value = serde_json::from_slice(bytes).context("invalid vendor JSON")?;
    let vendor = root
        .get("VendorInfo")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("VendorInfo is missing"))?;
    let vendor_type = vendor
        .get("TypeName")
        .and_then(Value::as_str)
        .filter(|value| value.starts_with("/Lotus/Types/Game/VendorManifests/Events/RadioLegion"))
        .unwrap_or(expected_vendor_type)
        .to_owned();
    let items = vendor
        .get("ItemManifest")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("ItemManifest is missing"))?;
    if items.len() > 512 {
        bail!("ItemManifest exceeds the safe item limit");
    }
    let observed_at = Utc::now();
    let expires_at = vendor
        .get("Expiry")
        .and_then(mongo_date)
        .into_iter()
        .chain(
            items
                .iter()
                .filter_map(|item| item.get("Expiry").and_then(mongo_date)),
        )
        .filter(|expiry| *expiry > observed_at)
        .min()
        .ok_or_else(|| anyhow!("vendor expiry is missing"))?;

    let mut offers = BTreeMap::<String, u32>::new();
    for item in items {
        if item
            .get("Expiry")
            .and_then(mongo_date)
            .is_some_and(|expiry| expiry <= observed_at)
        {
            continue;
        }
        let Some(store_item) = item.get("StoreItem").and_then(Value::as_str) else {
            continue;
        };
        let Some(rest) = store_item.strip_prefix("/Lotus/StoreItems") else {
            continue;
        };
        let game_ref = format!("/Lotus{rest}");
        if game_ref.len() > 512 {
            continue;
        }
        let Some(cost) = item
            .get("ItemPrices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find_map(|price| {
                let item_type = price.get("ItemType")?.as_str()?;
                if !item_type.contains("/Nora") || !item_type.ends_with("Creds") {
                    return None;
                }
                u32::try_from(price.get("ItemCount")?.as_u64()?).ok()
            })
            .filter(|cost| *cost > 0)
        else {
            continue;
        };
        offers
            .entry(game_ref)
            .and_modify(|current| *current = (*current).min(cost))
            .or_insert(cost);
    }
    if offers.is_empty() {
        bail!("vendor response contains no Nightwave Cred offers");
    }
    Ok(NightwaveVendorSnapshot {
        observed_at,
        expires_at,
        season_tag: season_tag.to_owned(),
        vendor_type,
        offers: offers
            .into_iter()
            .map(|(game_ref, cred_cost)| NightwaveVendorOffer {
                game_ref,
                cred_cost,
            })
            .collect(),
    })
}

fn mongo_date(value: &Value) -> Option<DateTime<Utc>> {
    let date = value.get("$date").unwrap_or(value);
    let millis = date
        .get("$numberLong")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<i64>().ok())
        .or_else(|| date.as_i64())
        .or_else(|| date.as_str().and_then(|value| value.parse::<i64>().ok()));
    if let Some(millis) = millis {
        return DateTime::from_timestamp_millis(millis);
    }
    date.as_str()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

/// Serializes memory scans so two concurrent callers never run two scans at
/// once. Without this, two concurrent `scan_inventory` invokes firing together
/// would each walk the game's whole address space.
/// The second caller gets `ScanError::Busy` (a transient, retryable state)
/// rather than a redundant parallel scan.
#[derive(Default)]
pub struct InventoryScanner {
    scan_lock: Mutex<()>,
}

impl InventoryScanner {
    pub fn new() -> Self {
        InventoryScanner {
            scan_lock: Mutex::new(()),
        }
    }

    /// Single-flight read-only scan. Holds the scan lock across the
    /// whole scan + HTTP fetch; a concurrent call returns `ScanError::Busy`.
    pub fn scan(
        &self,
        pid: Option<u32>,
        platform_tag: Option<String>,
    ) -> std::result::Result<ReadOnlyScanResult, ScanError> {
        // A scan thread that panicked would poison the lock; recover the guard
        // rather than wedge the route into permanent "busy".
        let _guard = match self.scan_lock.try_lock() {
            Ok(g) => g,
            Err(std::sync::TryLockError::WouldBlock) => return Err(ScanError::Busy),
            Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
        };
        fetch_game_data(pid, platform_tag).map_err(ScanError::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaced_season_tag_maps_to_vendor_manifest() {
        assert_eq!(
            nightwave_vendor_type("Radio Legion Intermission16 Syndicate").as_deref(),
            Some(
                "/Lotus/Types/Game/VendorManifests/Events/RadioLegionIntermission16VendorManifest"
            )
        );
        assert!(nightwave_vendor_type("Other Syndicate").is_none());
    }

    #[test]
    fn exact_vendor_response_keeps_only_cred_offers() {
        let raw = br#"{
          "VendorInfo": {
            "TypeName": "/Lotus/Types/Game/VendorManifests/Events/RadioLegionIntermission16VendorManifest",
            "Expiry": {"$date":{"$numberLong":"4102444800000"}},
            "ItemManifest": [
              {
                "StoreItem": "/Lotus/StoreItems/Upgrades/Mods/Aura/EnemyArmorReductionAuraMod",
                "ItemPrices": [{"ItemType":"/Lotus/Types/Items/MiscItems/NoraIntermissionSixteenCreds","ItemCount":20}]
              },
              {
                "StoreItem": "/Lotus/StoreItems/Types/Items/MiscItems/OrokinCatalyst",
                "RegularPrice": [1]
              }
            ]
          }
        }"#;
        let snapshot = parse_nightwave_vendor(
            raw,
            "Radio Legion Intermission16 Syndicate",
            "/Lotus/Types/Game/VendorManifests/Events/RadioLegionIntermission16VendorManifest",
        )
        .expect("vendor response parses");
        assert_eq!(snapshot.offers.len(), 1);
        assert_eq!(
            snapshot.offers[0].game_ref,
            "/Lotus/Upgrades/Mods/Aura/EnemyArmorReductionAuraMod"
        );
        assert_eq!(snapshot.offers[0].cred_cost, 20);
    }
}
