import { createListingInput, type AccountOrder, type CreateListingInput } from "./account";
import type { MarketSearchRow } from "./market";

/** Покупается именно выбранный вариант: нулевой ранг и пустые звёзды значимы. */
export function buyListingInput(row: MarketSearchRow, platinum: number, quantity: number, visible: boolean): CreateListingInput {
  return { ...createListingInput(row, platinum, quantity, visible, null), type: "buy" };
}

export function existingBuyOrder(row: MarketSearchRow, orders: readonly AccountOrder[]): AccountOrder | null {
  const key = row.recommendation.key;
  return orders.find((order) => order.type === "buy" && order.itemId === row.itemId
    && order.rank === key.rank && order.charges === key.charges && order.subtype === key.subtype
    && order.amberStars === key.amberStars && order.cyanStars === key.cyanStars) ?? null;
}
