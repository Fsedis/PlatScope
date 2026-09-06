import type { AccountOrder } from "./account";
import type { TradeShiftRow } from "./tradeShift";

export type SalesFilter = "all" | "attention" | "hidden";
export interface OrderChange { price: number | null; quantity: number | null; delete: boolean }
export interface ReviewedOrderChange { before: AccountOrder; name: string; change: OrderChange }

export function orderChange(row: TradeShiftRow): OrderChange | null {
  const quantity = row.suggestedQuantity !== null && row.suggestedQuantity !== row.order.quantity ? row.suggestedQuantity : null;
  const price = row.suggestedPrice !== null && row.suggestedPrice !== row.order.platinum ? row.suggestedPrice : null;
  return quantity === 0 ? { price: null, quantity, delete: true }
    : price === null && quantity === null ? null : { price, quantity, delete: false };
}

export function reviewedChanges(rows: readonly TradeShiftRow[]): ReviewedOrderChange[] {
  return rows.flatMap(row => {
    const change = orderChange(row);
    return change ? [{ before: { ...row.order }, name: row.item?.displayName ?? "Неизвестный предмет", change }] : [];
  });
}

export function orderUnchanged(before: AccountOrder, current: AccountOrder | undefined): boolean {
  return !!current && (Object.keys(before) as Array<keyof AccountOrder>).every(key => before[key] === current[key]);
}

export function reviewedQuantitiesMatch(reviewed: readonly ReviewedOrderChange[], current: readonly TradeShiftRow[]): boolean {
  return reviewed.every(proposal => proposal.change.quantity === null
    || current.find(row => row.order.id === proposal.before.id)?.suggestedQuantity === proposal.change.quantity);
}

export function salesFilterRows(rows: readonly TradeShiftRow[], filter: SalesFilter): TradeShiftRow[] {
  return rows.filter(row => filter === "all" || (filter === "hidden" ? !row.order.visible : row.needsAction));
}

export function orderMarketPrice(row: TradeShiftRow): number | null {
  const unit = row.recommendation?.listPrice;
  return unit == null || !Number.isFinite(unit) || unit <= 0 ? null : Math.round(unit * (row.order.perTrade ?? 1));
}

export function orderAdvice(row: TradeShiftRow): string {
  switch (row.health) {
    case "inventory_mismatch": return row.suggestedQuantity === 0 ? "Снять с продажи" : "Уменьшить количество";
    case "overpriced": return "Снизить цену";
    case "underpriced": return "Можно повысить цену";
    case "price_check_failed": return "Повторить проверку цены";
    case "unknown": return "Уточнить цену";
    case "hidden": return "Можно опубликовать";
    case "healthy": return "Изменения не нужны";
  }
}
