import type {
  AccountOrder,
  AccountOrderItem,
  AccountView,
  UpdateListingInput,
} from "./account";
import type { InventoryView, InventoryViewItem } from "./inventory";
import type {
  MarketItemKind,
  MarketVariantKey,
  PriceRecommendation,
} from "./market";

export type OrderHealth =
  | "inventory_mismatch"
  | "overpriced"
  | "underpriced"
  | "stale"
  | "hidden"
  | "healthy"
  | "unknown";

export interface TradeItem {
  name: string;
  quantity: number;
}

export type TradeEventStatus = "pending" | "reconciled" | "ignored";

export interface TradeEvent {
  id: number;
  occurredAt: string;
  partner: string | null;
  platinumGiven: number;
  platinumReceived: number;
  givenItems: TradeItem[];
  receivedItems: TradeItem[];
  status: TradeEventStatus;
  matchedOrderId: string | null;
  reconciliationJson: string | null;
}

export interface TradeShiftRow {
  order: AccountOrder;
  item: AccountOrderItem | null;
  key: MarketVariantKey | null;
  itemKind: MarketItemKind;
  inventory: InventoryViewItem | null;
  recommendation: PriceRecommendation | null;
  health: OrderHealth;
  suggestedPrice: number | null;
  suggestedQuantity: number | null;
  needsAction: boolean;
}

export interface TradeReconciliationAction {
  kind: "update" | "delete";
  before: AccountOrder;
  itemName: string;
  soldQuantity: number;
}

export interface TradeMatchPlan {
  actions: TradeReconciliationAction[];
  unmatched: TradeItem[];
  unsafe: TradeItem[];
}

export function orderVariantKey(
  order: AccountOrder,
  item: AccountOrderItem | undefined,
  platform: string,
): MarketVariantKey | null {
  if (!item) return null;
  return {
    slug: item.slug,
    platform,
    rank: order.rank,
    subtype: order.subtype,
    amberStars: order.amberStars,
    cyanStars: order.cyanStars,
  };
}

export function recommendationIdentity(key: MarketVariantKey): string {
  return [
    key.slug,
    key.platform,
    key.rank ?? "",
    key.subtype ?? "",
    key.amberStars ?? "",
    key.cyanStars ?? "",
  ].join("|");
}

export function buildTradeShiftRows(
  account: AccountView,
  inventory: InventoryView | null,
  recommendations: ReadonlyMap<string, PriceRecommendation | null>,
  now = new Date(),
): TradeShiftRow[] {
  const platform = account.profile?.platform || "pc";
  return account.orders
    .filter((order) => order.type === "sell")
    .map((order) => {
      const item = order.itemId ? account.orderItems?.[order.itemId] : undefined;
      const key = orderVariantKey(order, item, platform);
      const owned = inventory?.items.find((candidate) =>
        sameInventoryVariant(candidate, order, order.itemId)
      ) ?? null;
      const recommendation = key
        ? recommendations.get(recommendationIdentity(key)) ?? null
        : null;
      const result = evaluateOrder(order, owned, recommendation, Boolean(inventory), now);
      return {
        order,
        item: item ?? null,
        key,
        itemKind: item?.itemKind ?? "standard",
        inventory: owned,
        recommendation,
        ...result,
      };
    })
    .sort((left, right) =>
      healthPriority(left.health) - healthPriority(right.health)
        || (left.item?.displayName ?? "").localeCompare(right.item?.displayName ?? "", "ru")
    );
}

export function updateInput(changes: {
  platinum?: number;
  quantity?: number;
  visible?: boolean;
}): UpdateListingInput {
  return {
    platinum: changes.platinum ?? null,
    quantity: changes.quantity ?? null,
    visible: changes.visible ?? null,
    perTrade: null,
    rank: null,
    charges: null,
    subtype: null,
    amberStars: null,
    cyanStars: null,
  };
}

export function planTradeReconciliation(
  event: TradeEvent,
  account: AccountView,
): TradeMatchPlan {
  const result: TradeMatchPlan = { actions: [], unmatched: [], unsafe: [] };
  if (event.platinumReceived <= 0 || event.platinumGiven > 0) {
    result.unsafe.push(...event.givenItems);
    return result;
  }
  for (const sold of event.givenItems) {
    const candidates = account.orders.filter((order) => {
      if (order.type !== "sell" || !order.itemId) return false;
      const item = account.orderItems?.[order.itemId];
      return item
        && normalizeTradeName(item.displayNameEn) === normalizeTradeName(sold.name);
    });
    if (candidates.length !== 1) {
      result.unmatched.push(sold);
      continue;
    }
    const order = candidates[0];
    const orderItem = order.itemId ? account.orderItems?.[order.itemId] : undefined;
    if (
      order.rank !== null
      || order.charges !== null
      || order.subtype !== null
      || order.amberStars !== null
      || order.cyanStars !== null
      || new Date(order.updatedAt).getTime() > new Date(event.occurredAt).getTime()
      || (order.perTrade !== null && order.quantity > sold.quantity
        && (order.quantity - sold.quantity) % order.perTrade !== 0)
    ) {
      result.unsafe.push(sold);
      continue;
    }
    result.actions.push({
      kind: order.quantity > sold.quantity ? "update" : "delete",
      before: order,
      itemName: orderItem?.displayName ?? sold.name,
      soldQuantity: sold.quantity,
    });
  }
  return result;
}

export function normalizeTradeName(value: string): string {
  return value
    .normalize("NFKC")
    .toLocaleLowerCase("en")
    .replace(/[’']/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

function sameInventoryVariant(
  item: InventoryViewItem,
  order: AccountOrder,
  itemId: string | null,
): boolean {
  return item.itemId === itemId
    && item.key?.rank === order.rank
    && item.key?.subtype === order.subtype
    && item.key?.amberStars === order.amberStars
    && item.key?.cyanStars === order.cyanStars;
}

function evaluateOrder(
  order: AccountOrder,
  inventory: InventoryViewItem | null,
  recommendation: PriceRecommendation | null,
  inventoryAvailable: boolean,
  now: Date,
): Pick<TradeShiftRow, "health" | "suggestedPrice" | "suggestedQuantity" | "needsAction"> {
  if (inventoryAvailable && (!inventory || order.quantity > inventory.sellableQuantity)) {
    return {
      health: "inventory_mismatch",
      suggestedPrice: recommendation?.listPrice === null ? null : roundedPrice(recommendation?.listPrice),
      suggestedQuantity: inventory?.sellableQuantity ?? 0,
      needsAction: true,
    };
  }
  const listPrice = recommendation?.listPrice ?? null;
  const fairPrice = recommendation?.fairPrice ?? null;
  if (listPrice !== null) {
    const tolerance = Math.max(1, Math.round(listPrice * 0.05));
    if (order.platinum > listPrice + tolerance) {
      return {
        health: "overpriced",
        suggestedPrice: roundedPrice(listPrice),
        suggestedQuantity: null,
        needsAction: true,
      };
    }
    if (fairPrice !== null && order.platinum < fairPrice - Math.max(1, Math.round(fairPrice * 0.1))) {
      return {
        health: "underpriced",
        suggestedPrice: roundedPrice(listPrice),
        suggestedQuantity: null,
        needsAction: true,
      };
    }
  }
  if (!order.visible) {
    return { health: "hidden", suggestedPrice: null, suggestedQuantity: null, needsAction: false };
  }
  const age = now.getTime() - new Date(order.updatedAt).getTime();
  if (Number.isFinite(age) && age > 72 * 60 * 60 * 1_000) {
    return { health: "stale", suggestedPrice: listPrice === null ? null : roundedPrice(listPrice), suggestedQuantity: null, needsAction: true };
  }
  if (!recommendation || recommendation.listPrice === null) {
    return { health: "unknown", suggestedPrice: null, suggestedQuantity: null, needsAction: false };
  }
  return { health: "healthy", suggestedPrice: null, suggestedQuantity: null, needsAction: false };
}

function roundedPrice(value: number | null | undefined): number | null {
  return value === null || value === undefined ? null : Math.max(1, Math.round(value));
}

function healthPriority(health: OrderHealth): number {
  return ({
    inventory_mismatch: 0,
    underpriced: 1,
    overpriced: 2,
    stale: 3,
    unknown: 4,
    hidden: 5,
    healthy: 6,
  } satisfies Record<OrderHealth, number>)[health];
}
