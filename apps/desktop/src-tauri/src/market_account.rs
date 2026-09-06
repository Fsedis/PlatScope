use platscope_core::{AccountOrder, AccountOrderType, AccountService};
use serde::Serialize;
use tauri::State;

use crate::{
    AppState, SellListingIntent, active_set_component_reservations, inventory_for_listing,
    prime_set_for_listing, validate_sell_listing_inventory,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrdersVisibilityResult {
    pub updated: u32,
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri извлекает State по значению.
pub(crate) async fn account_set_orders_visibility(
    order_type: AccountOrderType,
    visible: bool,
    expected_orders: Vec<AccountOrder>,
    confirmed: bool,
    state: State<'_, AppState>,
) -> Result<OrdersVisibilityResult, String> {
    if !confirmed {
        return Err("Подтвердите изменение видимости объявлений.".to_owned());
    }
    // Одновременная обработка EE.log не должна менять количество между проверкой
    // общих резервов и массовым показом.
    let _trade_guard = state.trade_reconciliation_lock.lock().await;
    let account = state
        .account_service
        .view()
        .await
        .map_err(|error| error.to_string())?;
    if !account.connected {
        return Err("Сначала подключите аккаунт Warframe Market.".to_owned());
    }
    AccountService::validate_visibility_snapshot(&expected_orders, &account.orders, order_type)
        .map_err(|error| error.to_string())?;

    if visible && order_type == AccountOrderType::Sell {
        let hidden_sales = account
            .orders
            .iter()
            .filter(|order| order.order_type == AccountOrderType::Sell && !order.visible)
            .collect::<Vec<_>>();
        if !hidden_sales.is_empty() {
            let inventory = inventory_for_listing(&state)?;
            let mut prospective = account.orders.clone();
            for order in &mut prospective {
                if order.order_type == AccountOrderType::Sell {
                    order.visible = true;
                }
            }
            for order in hidden_sales {
                let item_id = order.item_id.as_deref().ok_or_else(|| {
                    "Предмет объявления не определён. Обновите список.".to_owned()
                })?;
                let intent = SellListingIntent {
                    item_id: item_id.to_owned(),
                    quantity: order.quantity,
                    per_trade: order.per_trade.unwrap_or(1),
                    rank: order.rank,
                    charges: order.charges,
                    subtype: order.subtype.clone(),
                    amber_stars: order.amber_stars,
                    cyan_stars: order.cyan_stars,
                };
                let prime_set = prime_set_for_listing(&state, item_id)?;
                let set_reservations =
                    active_set_component_reservations(&state, &prospective, Some(&order.id))?;
                validate_sell_listing_inventory(
                    &intent,
                    &inventory,
                    &prospective,
                    Some(&order.id),
                    prime_set.as_ref(),
                    &set_reservations,
                )?;
            }
        }
    }

    let updated = state
        .account_service
        .set_orders_visibility(order_type, visible, &expected_orders, confirmed)
        .await
        .map_err(|error| error.to_string())?;
    tracing::info!(
        event = "wfm_orders_visibility_updated",
        ?order_type,
        visible,
        updated,
        "explicit WFM group visibility update completed"
    );
    Ok(OrdersVisibilityResult { updated })
}
