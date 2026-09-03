#![forbid(unsafe_code)]

use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

/// Официальный общий лимит WFM равен трём запросам в секунду. Интервал в
/// 400 мс оставляет небольшой запас и, главное, применяется ко всем WFM-клиентам
/// процесса, а не отдельно к публичным ценам и операциям аккаунта.
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(400);

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::const_new(None);

/// Резервирует следующий общий слот запроса к API Warframe Market.
pub async fn wait_for_request_slot() {
    let mut last_request = LAST_REQUEST.lock().await;
    if let Some(previous) = *last_request {
        let elapsed = previous.elapsed();
        if let Some(wait) = MIN_REQUEST_INTERVAL.checked_sub(elapsed) {
            tokio::time::sleep(wait).await;
        }
    }
    *last_request = Some(Instant::now());
}
