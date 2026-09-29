//! Журнал сделок, сопоставление продаж и синхронизация с Warframe Market.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::Utc;
use platscope_core::{
    AccountOrder, AccountOrderItemView, AccountOrderType, AccountSetComponentView, AccountView,
    AppSettings, SETTINGS_KEY, enrich_account_view,
};
use platscope_storage::{
    NewTradeEvent, TradeEvent, TradeEventStatus, TradeItem, TradeSalesSummary,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{AppState, trade_log};

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
pub(crate) fn trade_events(state: State<'_, AppState>) -> Result<Vec<TradeEvent>, String> {
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .recent_trade_events(30)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
pub(crate) fn trade_sales_summary(state: State<'_, AppState>) -> Result<TradeSalesSummary, String> {
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .trade_sales_summary()
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
pub(crate) fn trade_event_reconciled(
    id: i64,
    order_id: Option<String>,
    reconciliation_json: Option<String>,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    if reconciliation_json
        .as_ref()
        .is_some_and(|value| value.len() > 16_384)
    {
        return Err("trade reconciliation payload is too long".to_owned());
    }
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .set_trade_event_status(
            id,
            TradeEventStatus::Reconciled,
            order_id.as_deref(),
            reconciliation_json.as_deref(),
        )
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
pub(crate) fn trade_event_ignore(id: i64, state: State<'_, AppState>) -> Result<bool, String> {
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .set_trade_event_status(id, TradeEventStatus::Ignored, None, None)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
pub(crate) fn trade_event_restore(id: i64, state: State<'_, AppState>) -> Result<bool, String> {
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .set_trade_event_status(id, TradeEventStatus::Pending, None, None)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AutomaticTradeActionKind {
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutomaticTradeAction {
    kind: AutomaticTradeActionKind,
    before: AccountOrder,
    item_name: String,
    sold_quantity: u32,
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
pub(crate) async fn trade_event_retry(id: i64, app_handle: AppHandle) -> Result<bool, String> {
    let event = app_handle
        .state::<AppState>()
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .recent_trade_events(100)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|event| event.id == id)
        .ok_or_else(|| "Сделка не найдена в локальном журнале.".to_owned())?;
    reconcile_trade_event(&app_handle, event).await
}

fn is_confirmed_sale(event: &TradeEvent) -> bool {
    event.platinum_received > 0
        && event.platinum_given == 0
        && !event.given_items.is_empty()
        && event.received_items.is_empty()
}

fn plan_automatic_trade_reconciliation(
    event: &TradeEvent,
    account: &AccountView,
) -> Option<Vec<AutomaticTradeAction>> {
    if !is_confirmed_sale(event) {
        return None;
    }
    let sold_items = aggregate_trade_items(&event.given_items);
    if sold_items.is_empty() {
        return None;
    }
    let complete_set_candidates = account
        .orders
        .iter()
        .filter_map(|order| {
            if order.order_type != AccountOrderType::Sell {
                return None;
            }
            let item = order
                .item_id
                .as_ref()
                .and_then(|item_id| account.order_items.get(item_id))?;
            if item.set_components.is_empty() {
                return None;
            }
            let sold_quantity = complete_set_quantity(&sold_items, &item.set_components)?;
            Some((order, item, sold_quantity))
        })
        .collect::<Vec<_>>();
    if !complete_set_candidates.is_empty() {
        if complete_set_candidates.len() != 1 {
            return None;
        }
        let (order, item, sold_quantity) = complete_set_candidates[0];
        if sold_quantity > order.quantity || !is_safe_trade_order(order, sold_quantity, event, None)
        {
            return None;
        }
        return Some(vec![AutomaticTradeAction {
            kind: AutomaticTradeActionKind::Close,
            before: order.clone(),
            item_name: item.display_name.clone(),
            sold_quantity,
        }]);
    }

    let mut used_order_ids = HashSet::new();
    let mut actions = Vec::with_capacity(sold_items.len());
    for sold in sold_items {
        let candidates = account
            .orders
            .iter()
            .filter_map(|order| {
                if order.order_type != AccountOrderType::Sell {
                    return None;
                }
                let item = order
                    .item_id
                    .as_ref()
                    .and_then(|item_id| account.order_items.get(item_id))?;
                (item_matches_trade_name(item, &sold.name)
                    && order_matches_known_trade_rank(order, &sold.name))
                .then_some((order, item))
            })
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return None;
        }
        let (order, item) = candidates[0];
        if !used_order_ids.insert(order.id.clone())
            || sold.quantity > order.quantity
            || !is_safe_trade_order(order, sold.quantity, event, Some(&sold))
        {
            return None;
        }
        actions.push(AutomaticTradeAction {
            kind: AutomaticTradeActionKind::Close,
            before: order.clone(),
            item_name: item.display_name.clone(),
            sold_quantity: sold.quantity,
        });
    }
    (!actions.is_empty()).then_some(actions)
}

fn aggregate_trade_items(items: &[TradeItem]) -> Vec<TradeItem> {
    let mut positions = HashMap::<String, usize>::new();
    let mut aggregated = Vec::<TradeItem>::new();
    for item in items.iter().filter(|item| item.quantity > 0) {
        let normalized_name = normalize_trade_name(&item.name);
        if normalized_name.is_empty() {
            continue;
        }
        let identity = format!(
            "{}|{}",
            normalized_name,
            trade_rank(&item.name).map_or_else(String::new, |rank| rank.to_string())
        );
        if let Some(position) = positions.get(&identity).copied() {
            aggregated[position].quantity =
                aggregated[position].quantity.saturating_add(item.quantity);
        } else {
            positions.insert(identity, aggregated.len());
            aggregated.push(item.clone());
        }
    }
    aggregated
}

fn complete_set_quantity(
    sold_items: &[TradeItem],
    components: &[AccountSetComponentView],
) -> Option<u32> {
    if components.is_empty() || sold_items.len() != components.len() {
        return None;
    }
    let mut used_items = HashSet::new();
    let mut complete_sets = None;
    for component in components {
        if component.required_quantity == 0 {
            return None;
        }
        let aliases = [
            normalize_trade_name(&component.display_name),
            normalize_trade_name(&component.display_name_en),
        ];
        let matching = sold_items
            .iter()
            .enumerate()
            .filter(|(index, sold)| {
                !used_items.contains(index) && aliases.contains(&normalize_trade_name(&sold.name))
            })
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return None;
        }
        let (index, sold) = matching[0];
        if !sold.quantity.is_multiple_of(component.required_quantity) {
            return None;
        }
        let quantity = sold.quantity / component.required_quantity;
        if quantity == 0 || complete_sets.is_some_and(|current| current != quantity) {
            return None;
        }
        complete_sets = Some(quantity);
        used_items.insert(index);
    }
    (used_items.len() == sold_items.len())
        .then_some(complete_sets)
        .flatten()
}

fn item_matches_trade_name(item: &AccountOrderItemView, trade_name: &str) -> bool {
    let normalized = normalize_trade_name(trade_name);
    [&item.display_name, &item.display_name_en]
        .into_iter()
        .any(|candidate| normalize_trade_name(candidate) == normalized)
}

fn order_matches_known_trade_rank(order: &AccountOrder, trade_name: &str) -> bool {
    trade_rank(trade_name).is_none_or(|rank| {
        order_rank_matches_trade(order.rank, Some(rank))
            && order
                .subtype
                .as_deref()
                .is_none_or(|subtype| subtype == "regular")
    })
}

const fn order_rank_matches_trade(order_rank: Option<u16>, sold_rank: Option<u16>) -> bool {
    match (order_rank, sold_rank) {
        (Some(order_rank), Some(sold_rank)) => order_rank == sold_rank,
        // WFM may omit the default rank while EE.log always prints `РАНГ 0`.
        // This fallback is used only after an exact item-name match and the
        // caller still requires exactly one candidate order.
        (None, Some(0) | None) => true,
        _ => false,
    }
}

fn is_safe_trade_order(
    order: &AccountOrder,
    sold_quantity: u32,
    event: &TradeEvent,
    sold: Option<&TradeItem>,
) -> bool {
    let sold_rank = sold.and_then(|item| trade_rank(&item.name));
    let subtype_matches = if sold_rank.is_some() {
        order
            .subtype
            .as_deref()
            .is_none_or(|subtype| subtype == "regular")
    } else {
        order.subtype.is_none()
    };
    order_rank_matches_trade(order.rank, sold_rank)
        && order.charges.is_none()
        && subtype_matches
        && order.amber_stars.is_none()
        && order.cyan_stars.is_none()
        && order.updated_at <= event.occurred_at
        && order.per_trade.is_none_or(|per_trade| {
            per_trade > 0
                && sold_quantity.is_multiple_of(per_trade)
                && (order.quantity <= sold_quantity
                    || (order.quantity - sold_quantity).is_multiple_of(per_trade))
        })
}

fn normalize_trade_name(value: &str) -> String {
    let (value, _) = strip_trade_rank_suffix(value);
    let mut value = value
        .trim()
        .to_lowercase()
        .replace('ё', "е")
        .replace(['’', '\'', 'ʼ'], "");
    for prefix in ["чертеж:", "blueprint:"] {
        if let Some(stripped) = value.strip_prefix(prefix) {
            value = stripped.trim().to_owned();
            break;
        }
    }
    for suffix in ["(чертеж)", "(blueprint)"] {
        if let Some(stripped) = value.strip_suffix(suffix) {
            value = stripped.trim().to_owned();
            break;
        }
    }
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" :", ":")
        .replace(": ", ":")
}

fn trade_rank(value: &str) -> Option<u16> {
    strip_trade_rank_suffix(value).1
}

fn strip_trade_rank_suffix(value: &str) -> (&str, Option<u16>) {
    let value = value.trim();
    let Some(open_index) = value.rfind('(') else {
        return (value, None);
    };
    let Some(descriptor) = value[open_index + 1..].strip_suffix(')') else {
        return (value, None);
    };
    let words = descriptor.split_whitespace().collect::<Vec<_>>();
    let Some(rank_index) = words
        .iter()
        .position(|word| word.eq_ignore_ascii_case("rank") || word.to_lowercase() == "ранг")
    else {
        return (value, None);
    };
    let Some(rank) = words
        .get(rank_index + 1)
        .and_then(|rank| rank.parse::<u16>().ok())
    else {
        return (value, None);
    };
    if rank_index + 2 != words.len() {
        return (value, None);
    }
    (value[..open_index].trim_end(), Some(rank))
}

async fn reconcile_trade_event(app_handle: &AppHandle, event: TradeEvent) -> Result<bool, String> {
    let state = app_handle.state::<AppState>();
    let _reconciliation_guard = state.trade_reconciliation_lock.lock().await;
    let still_pending = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .recent_trade_events(100)
        .map_err(|error| error.to_string())?
        .into_iter()
        .any(|stored| stored.id == event.id && stored.status == TradeEventStatus::Pending);
    if !still_pending {
        return Ok(false);
    }
    let account = state
        .account_service
        .view()
        .await
        .map_err(|error| error.to_string())?;
    if !account.connected {
        return Ok(false);
    }
    let language = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default()
        .language;
    let account = enrich_account_view(&state.database, language, account)
        .map_err(|error| error.to_string())?;
    let Some(actions) = plan_automatic_trade_reconciliation(&event, &account) else {
        return Ok(false);
    };
    let planned_count = actions.len();
    let mut completed = Vec::with_capacity(planned_count);
    let mut failure = None;
    for action in actions {
        match state
            .account_service
            .close_listing(&action.before.id, action.sold_quantity)
            .await
        {
            Ok(()) => completed.push(action),
            Err(error) => {
                failure = Some(error.to_string());
                break;
            }
        }
    }
    if completed.is_empty() {
        return failure.map_or(Ok(false), Err);
    }
    let reconciliation_json =
        serde_json::to_string(&completed).map_err(|error| error.to_string())?;
    let matched_order_id = (completed.len() == 1).then(|| completed[0].before.id.as_str());
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .set_trade_event_status(
            event.id,
            TradeEventStatus::Reconciled,
            matched_order_id,
            Some(&reconciliation_json),
        )
        .map_err(|error| error.to_string())?;
    tracing::info!(
        event = "wfm_trade_auto_closed",
        trade_event_id = event.id,
        completed = completed.len(),
        planned = planned_count,
        partial = failure.is_some(),
        "confirmed game sale was automatically recorded through WFM order close"
    );
    if let Some(error) = failure {
        tracing::warn!(
            event = "wfm_trade_auto_close_partial",
            trade_event_id = event.id,
            error = %error,
            "only a safe completed subset was recorded; automatic retry is disabled"
        );
    }
    let _ = app_handle.emit("trade-reconciled", event.id);
    Ok(completed.len() == planned_count)
}

fn spawn_trade_reconciliation(app_handle: AppHandle, event: TradeEvent) {
    tauri::async_runtime::spawn(async move {
        const ATTEMPTS: usize = 3;
        let trade_event_id = event.id;
        let mut last_error = None;
        for attempt in 0..ATTEMPTS {
            match reconcile_trade_event(&app_handle, event.clone()).await {
                Ok(true) => return,
                Ok(false) => {}
                Err(error) => last_error = Some(error),
            }
            let still_pending = app_handle
                .state::<AppState>()
                .database
                .lock()
                .ok()
                .and_then(|database| database.recent_trade_events(100).ok())
                .is_some_and(|events| {
                    events.into_iter().any(|stored| {
                        stored.id == trade_event_id && stored.status == TradeEventStatus::Pending
                    })
                });
            if !still_pending || attempt + 1 == ATTEMPTS {
                break;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        tracing::warn!(
            event = "wfm_trade_auto_close_failed",
            trade_event_id,
            attempts = ATTEMPTS,
            error = last_error
                .as_deref()
                .unwrap_or("no unique active order match"),
            "confirmed game sale remains pending after bounded automatic retries"
        );
        let _ = app_handle.emit("trade-reconciliation-failed", ());
    });
}

pub(crate) fn spawn_pending_trade_reconciliation(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let events = app_handle
            .state::<AppState>()
            .database
            .lock()
            .ok()
            .and_then(|database| database.recent_trade_events(100).ok())
            .unwrap_or_default();
        for event in events
            .into_iter()
            .filter(|event| event.status == TradeEventStatus::Pending && is_confirmed_sale(event))
        {
            if let Err(error) = reconcile_trade_event(&app_handle, event).await {
                tracing::warn!(
                    event = "wfm_pending_trade_retry_failed",
                    error = %error,
                    "pending confirmed sale could not be synchronized"
                );
            }
        }
    });
}

pub(crate) fn handle_trade_log_chunk(
    app_handle: &AppHandle,
    machine: &mut trade_log::TradeMachine,
    line_tail: &mut String,
    chunk: &str,
    now_ms: u64,
) {
    line_tail.push_str(chunk);
    let has_partial_line = !line_tail.ends_with('\n');
    let mut lines: Vec<String> = line_tail
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_owned())
        .collect();
    *line_tail = if has_partial_line {
        lines.pop().unwrap_or_default()
    } else {
        String::new()
    };
    for line in lines {
        let Some(trade) = machine.feed(&line, now_ms) else {
            continue;
        };
        let fingerprint = format!(
            "ee:{}:{}:{}:{}:{}:{}",
            trade.log_stamp.as_deref().unwrap_or("no-stamp"),
            trade.partner.as_deref().unwrap_or("unknown"),
            trade.platinum_given,
            trade.platinum_received,
            serde_json::to_string(&trade.given_items).unwrap_or_default(),
            serde_json::to_string(&trade.received_items).unwrap_or_default(),
        );
        let event = NewTradeEvent {
            fingerprint,
            occurred_at: Utc::now(),
            partner: trade.partner,
            platinum_given: trade.platinum_given,
            platinum_received: trade.platinum_received,
            given_items: trade.given_items,
            received_items: trade.received_items,
        };
        let inserted_id = app_handle
            .state::<AppState>()
            .database
            .lock()
            .ok()
            .and_then(|database| database.record_trade_event(&event).ok())
            .flatten();
        let Some(event_id) = inserted_id else {
            continue;
        };
        let trade_event = TradeEvent {
            id: event_id,
            occurred_at: event.occurred_at,
            partner: event.partner,
            platinum_given: event.platinum_given,
            platinum_received: event.platinum_received,
            given_items: event.given_items,
            received_items: event.received_items,
            status: TradeEventStatus::Pending,
            matched_order_id: None,
            reconciliation_json: None,
        };
        tracing::info!(
            event = "confirmed_trade_detected",
            "confirmed trade was recorded from EE.log"
        );
        if let Err(error) = app_handle.emit("trade-detected", ()) {
            tracing::warn!(
                event = "trade_detected_event_failed",
                error = %error,
                "trade was recorded but UI event failed"
            );
        }
        spawn_trade_reconciliation(app_handle.clone(), trade_event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use platscope_domain::MarketItemKind;

    fn automatic_trade_order(
        item_id: &str,
        quantity: u32,
        occurred_at: chrono::DateTime<Utc>,
    ) -> AccountOrder {
        AccountOrder {
            id: format!("order-{item_id}"),
            item_id: Some(item_id.to_owned()),
            order_type: AccountOrderType::Sell,
            platinum: 25,
            quantity,
            per_trade: None,
            rank: None,
            charges: None,
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
            visible: true,
            created_at: occurred_at - chrono::Duration::hours(1),
            updated_at: occurred_at - chrono::Duration::seconds(1),
        }
    }

    fn automatic_trade_event(
        items: Vec<TradeItem>,
        occurred_at: chrono::DateTime<Utc>,
    ) -> TradeEvent {
        TradeEvent {
            id: 7,
            occurred_at,
            partner: Some("MarketTenno".into()),
            platinum_given: 0,
            platinum_received: 25,
            given_items: items,
            received_items: Vec::new(),
            status: TradeEventStatus::Pending,
            matched_order_id: None,
            reconciliation_json: None,
        }
    }

    #[test]
    fn automatic_trade_plan_closes_instead_of_deleting_order() {
        let occurred_at = Utc::now();
        let order = automatic_trade_order("item-123", 3, occurred_at);
        let account = AccountView {
            connected: true,
            profile: None,
            orders: vec![order.clone()],
            order_items: HashMap::from([(
                "item-123".into(),
                AccountOrderItemView {
                    bulk_tradable: false,
                    slug: "strun_prime_stock".into(),
                    display_name: "Стран Прайм: Приклад".into(),
                    display_name_en: "Strun Prime Stock".into(),
                    image_url: None,
                    item_kind: MarketItemKind::Standard,
                    set_components: Vec::new(),
                },
            )]),
        };
        let event = automatic_trade_event(
            vec![TradeItem {
                name: "Стран Прайм: Приклад".into(),
                quantity: 1,
            }],
            occurred_at,
        );

        let actions = plan_automatic_trade_reconciliation(&event, &account)
            .expect("sale matches exactly one order");

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind, AutomaticTradeActionKind::Close);
        assert_eq!(actions[0].before.id, order.id);
        assert_eq!(actions[0].sold_quantity, 1);
    }

    #[test]
    fn automatic_trade_plan_matches_ranked_mod_label_from_russian_game_log() {
        let occurred_at = Utc::now();
        let mut order = automatic_trade_order("transient-fortitude", 1, occurred_at);
        order.platinum = 10;
        order.rank = Some(0);
        order.subtype = Some("regular".into());
        let mut wrong_rank = order.clone();
        wrong_rank.id = "order-transient-fortitude-rank-5".into();
        wrong_rank.rank = Some(5);
        let account = AccountView {
            connected: true,
            profile: None,
            orders: vec![order.clone(), wrong_rank],
            order_items: HashMap::from([(
                "transient-fortitude".into(),
                AccountOrderItemView {
                    bulk_tradable: false,
                    slug: "transient_fortitude".into(),
                    display_name: "Кратковременное усиление".into(),
                    display_name_en: "Transient Fortitude".into(),
                    image_url: None,
                    item_kind: MarketItemKind::Standard,
                    set_components: Vec::new(),
                },
            )]),
        };
        let event = automatic_trade_event(
            vec![TradeItem {
                name: "Кратковременное усиление (РЕДКИЙ РАНГ 0)".into(),
                quantity: 1,
            }],
            occurred_at,
        );

        let actions = plan_automatic_trade_reconciliation(&event, &account)
            .expect("точный ранг мода сопоставляется с ордером");

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind, AutomaticTradeActionKind::Close);
        assert_eq!(actions[0].before.id, order.id);
        assert_eq!(actions[0].sold_quantity, 1);
    }

    #[test]
    fn automatic_trade_plan_treats_missing_wfm_rank_as_explicit_rank_zero() {
        let occurred_at = Utc::now();
        let mut default_rank = automatic_trade_order("toxic-flight", 1, occurred_at);
        default_rank.platinum = 3;
        let mut wrong_rank = default_rank.clone();
        wrong_rank.id = "order-toxic-flight-rank-5".into();
        wrong_rank.rank = Some(5);
        let account = AccountView {
            connected: true,
            profile: None,
            orders: vec![default_rank.clone(), wrong_rank],
            order_items: HashMap::from([(
                "toxic-flight".into(),
                AccountOrderItemView {
                    bulk_tradable: false,
                    slug: "toxic_flight".into(),
                    display_name: "Токсичный Полёт".into(),
                    display_name_en: "Toxic Flight".into(),
                    image_url: None,
                    item_kind: MarketItemKind::Standard,
                    set_components: Vec::new(),
                },
            )]),
        };
        let event = automatic_trade_event(
            vec![TradeItem {
                name: "Токсичный полёт (РЕДКИЙ РАНГ 0)".into(),
                quantity: 1,
            }],
            occurred_at,
        );

        let actions = plan_automatic_trade_reconciliation(&event, &account)
            .expect("пустой ранг WFM означает нулевой ранг мода");

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].before.id, default_rank.id);
        assert_eq!(actions[0].sold_quantity, 1);
    }

    #[test]
    fn automatic_trade_plan_recognizes_complete_set_from_components() {
        let occurred_at = Utc::now();
        let order = automatic_trade_order("hildryn-set", 1, occurred_at);
        let component = |slug: &str, ru: &str, en: &str| AccountSetComponentView {
            slug: slug.into(),
            required_quantity: 1,
            display_name: ru.into(),
            display_name_en: en.into(),
        };
        let components = vec![
            component(
                "hildryn_prime_blueprint",
                "Хильдрин Прайм",
                "Hildryn Prime Blueprint",
            ),
            component(
                "hildryn_prime_chassis",
                "Хильдрин Прайм: Каркас",
                "Hildryn Prime Chassis Blueprint",
            ),
            component(
                "hildryn_prime_neuroptics",
                "Хильдрин Прайм: Нейрооптика",
                "Hildryn Prime Neuroptics Blueprint",
            ),
            component(
                "hildryn_prime_systems",
                "Хильдрин Прайм: Система",
                "Hildryn Prime Systems Blueprint",
            ),
        ];
        let account = AccountView {
            connected: true,
            profile: None,
            orders: vec![order],
            order_items: HashMap::from([(
                "hildryn-set".into(),
                AccountOrderItemView {
                    bulk_tradable: false,
                    slug: "hildryn_prime_set".into(),
                    display_name: "Хильдрин Прайм: Комплект".into(),
                    display_name_en: "Hildryn Prime Set".into(),
                    image_url: None,
                    item_kind: MarketItemKind::Standard,
                    set_components: components,
                },
            )]),
        };
        let event = automatic_trade_event(
            vec![
                TradeItem {
                    name: "Хильдрин Прайм".into(),
                    quantity: 1,
                },
                TradeItem {
                    name: "Хильдрин Прайм: Каркас".into(),
                    quantity: 1,
                },
                TradeItem {
                    name: "Хильдрин Прайм: Нейрооптика".into(),
                    quantity: 1,
                },
                TradeItem {
                    name: "Хильдрин Прайм: Система".into(),
                    quantity: 1,
                },
            ],
            occurred_at,
        );

        let actions = plan_automatic_trade_reconciliation(&event, &account)
            .expect("all components form one complete set");

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind, AutomaticTradeActionKind::Close);
        assert_eq!(actions[0].item_name, "Хильдрин Прайм: Комплект");
        assert_eq!(actions[0].sold_quantity, 1);
    }
}
