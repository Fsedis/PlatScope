import type {
  AccountOrder,
  AccountOrderItem,
  AccountSetComponent,
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

export interface TradeSalesSummary {
  saleCount: number;
  platinumReceived: number;
}

export function isSaleTrade(event: TradeEvent): boolean {
  return event.platinumReceived > 0
    && event.platinumGiven === 0
    && event.givenItems.length > 0
    && event.receivedItems.length === 0;
}

export function pendingSaleEvents(events: readonly TradeEvent[]): TradeEvent[] {
  return events.filter((event) => isSaleTrade(event) && event.status === "pending");
}

export function visibleTradeHistory(
  events: readonly TradeEvent[],
  limit = 8,
): TradeEvent[] {
  return events
    .filter((event) => !isSaleTrade(event) || event.status !== "pending")
    .slice(0, Math.max(0, limit));
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
  kind: "update" | "delete" | "close";
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
    platform: normalizeMarketPlatform(platform),
    rank: order.rank,
    charges: order.charges,
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
    key.charges ?? "",
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
      const owned = availableInventoryForOrder(account, inventory, order, item, key);
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
  perTrade?: number;
}): UpdateListingInput {
  return {
    platinum: changes.platinum ?? null,
    quantity: changes.quantity ?? null,
    visible: changes.visible ?? null,
    perTrade: changes.perTrade ?? null,
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
  if (!isSaleTrade(event)) {
    result.unsafe.push(...event.givenItems);
    return result;
  }
  const soldItems = aggregateTradeItems(event.givenItems);
  const completeSetCandidates = account.orders.flatMap((order) => {
    if (order.type !== "sell" || !order.itemId) return [];
    const item = account.orderItems?.[order.itemId];
    if (!item?.setComponents?.length) return [];
    const soldQuantity = completeSetQuantity(soldItems, item.setComponents);
    return soldQuantity === null ? [] : [{ order, item, soldQuantity }];
  });
  if (completeSetCandidates.length > 0) {
    if (completeSetCandidates.length !== 1) {
      result.unsafe.push(...soldItems);
      return result;
    }
    const [{ order, item, soldQuantity }] = completeSetCandidates;
    if (soldQuantity > order.quantity || !isSafeOrderMatch(order, soldQuantity, event)) {
      result.unsafe.push(...soldItems);
      return result;
    }
    result.actions.push({
      kind: order.quantity > soldQuantity ? "update" : "delete",
      before: order,
      itemName: item.displayName,
      soldQuantity,
    });
    return result;
  }
  const usedOrderIds = new Set<string>();
  for (const sold of soldItems) {
    const candidates = account.orders.filter((order) => {
      if (order.type !== "sell" || !order.itemId) return false;
      const item = account.orderItems?.[order.itemId];
      return item && itemMatchesTradeName(item, sold.name);
    });
    if (candidates.length !== 1) {
      result.unmatched.push(sold);
      continue;
    }
    const order = candidates[0];
    if (usedOrderIds.has(order.id) || sold.quantity > order.quantity) {
      result.unsafe.push(sold);
      continue;
    }
    const orderItem = order.itemId ? account.orderItems?.[order.itemId] : undefined;
    if (!isSafeOrderMatch(order, sold.quantity, event)) {
      result.unsafe.push(sold);
      continue;
    }
    usedOrderIds.add(order.id);
    result.actions.push({
      kind: order.quantity > sold.quantity ? "update" : "delete",
      before: order,
      itemName: orderItem?.displayName ?? sold.name,
      soldQuantity: sold.quantity,
    });
  }
  return result;
}

function completeSetQuantity(
  soldItems: readonly TradeItem[],
  components: readonly AccountSetComponent[],
): number | null {
  if (components.length === 0 || soldItems.length !== components.length) return null;
  const usedItems = new Set<number>();
  let completeSets: number | null = null;
  for (const component of components) {
    if (!Number.isInteger(component.requiredQuantity) || component.requiredQuantity <= 0) {
      return null;
    }
    const aliases = new Set([
      normalizeTradeName(component.displayName),
      normalizeTradeName(component.displayNameEn),
    ]);
    const matchingIndexes = soldItems
      .map((sold, index) => ({ sold, index }))
      .filter(({ sold, index }) =>
        !usedItems.has(index) && aliases.has(normalizeTradeName(sold.name))
      );
    if (matchingIndexes.length !== 1) return null;
    const [{ sold, index }] = matchingIndexes;
    if (sold.quantity % component.requiredQuantity !== 0) return null;
    const quantity = sold.quantity / component.requiredQuantity;
    if (quantity <= 0 || (completeSets !== null && completeSets !== quantity)) return null;
    completeSets = quantity;
    usedItems.add(index);
  }
  return usedItems.size === soldItems.length ? completeSets : null;
}

function itemMatchesTradeName(item: AccountOrderItem, tradeName: string): boolean {
  const normalized = normalizeTradeName(tradeName);
  return [item.displayName, item.displayNameEn]
    .some((name) => normalizeTradeName(name) === normalized);
}

function isSafeOrderMatch(
  order: AccountOrder,
  soldQuantity: number,
  event: TradeEvent,
): boolean {
  return order.rank === null
    && order.charges === null
    && order.subtype === null
    && order.amberStars === null
    && order.cyanStars === null
    && new Date(order.updatedAt).getTime() <= new Date(event.occurredAt).getTime()
    && (order.perTrade === null || (
      soldQuantity % order.perTrade === 0
      && (order.quantity <= soldQuantity
        || (order.quantity - soldQuantity) % order.perTrade === 0)
    ));
}

function aggregateTradeItems(items: readonly TradeItem[]): TradeItem[] {
  const aggregated = new Map<string, TradeItem>();
  for (const item of items) {
    if (!Number.isInteger(item.quantity) || item.quantity <= 0) continue;
    const identity = normalizeTradeName(item.name);
    const existing = aggregated.get(identity);
    if (existing) existing.quantity += item.quantity;
    else aggregated.set(identity, { name: item.name, quantity: item.quantity });
  }
  return [...aggregated.values()];
}

function normalizeMarketPlatform(platform: string): string {
  switch (platform.toLowerCase()) {
    case "ps4":
    case "ps5":
      return "playstation";
    case "xb1":
    case "xboxone":
      return "xbox";
    default:
      return platform.toLowerCase();
  }
}

export function normalizeTradeName(value: string): string {
  return value
    .normalize("NFKC")
    .toLocaleLowerCase("ru-RU")
    .replace(/ё/g, "е")
    .replace(/^\s*(?:чертеж|blueprint)\s*:\s*/u, "")
    .replace(/\s*\((?:чертеж|blueprint)\)\s*$/u, "")
    .replace(/[’'ʼ]/g, "")
    .replace(/\s*:\s*/g, ":")
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
    && item.key?.charges === order.charges
    && item.key?.subtype === order.subtype
    && item.key?.amberStars === order.amberStars
    && item.key?.cyanStars === order.cyanStars;
}

function aggregateInventoryVariant(
  inventory: InventoryView | null,
  order: AccountOrder,
  itemId: string | null,
): InventoryViewItem | null {
  if (!inventory) return null;
  const matches = inventory.items.filter((candidate) =>
    sameInventoryVariant(candidate, order, itemId)
  );
  if (matches.length === 0) return null;
  return matches.slice(1).reduce<InventoryViewItem>((total, item) => ({
    ...total,
    ownedQuantity: total.ownedQuantity + item.ownedQuantity,
    tradeableQuantity: total.tradeableQuantity + item.tradeableQuantity,
    untradeableQuantity: total.untradeableQuantity + item.untradeableQuantity,
    unknownQuantity: total.unknownQuantity + item.unknownQuantity,
    leveledQuantity: total.leveledQuantity + item.leveledQuantity,
    equippedQuantity: total.equippedQuantity + item.equippedQuantity,
    equippedPlacements: [...total.equippedPlacements, ...item.equippedPlacements],
    sellableQuantity: total.sellableQuantity + item.sellableQuantity,
  }), { ...matches[0], equippedPlacements: [...matches[0].equippedPlacements] });
}

function availableInventoryForOrder(
  account: AccountView,
  inventory: InventoryView | null,
  order: AccountOrder,
  item: AccountOrderItem | undefined,
  key: MarketVariantKey | null,
): InventoryViewItem | null {
  if (!inventory) return null;
  if (item?.setComponents?.length) {
    return aggregateSetInventory(account, inventory, order, item, key);
  }
  const aggregated = aggregateInventoryVariant(inventory, order, order.itemId);
  if (!aggregated) return null;
  const directReservations = account.orders
    .filter((candidate) =>
      candidate.id !== order.id
      && candidate.visible
      && candidate.type === "sell"
      && sameOrderVariant(candidate, order)
    )
    .reduce((total, candidate) => total + candidate.quantity, 0);
  const setReservations = item
    ? activeSetComponentReservations(account, order.id).get(item.slug) ?? 0
    : 0;
  return {
    ...aggregated,
    sellableQuantity: Math.max(
      0,
      aggregated.sellableQuantity - directReservations - setReservations,
    ),
  };
}

function aggregateSetInventory(
  account: AccountView,
  inventory: InventoryView,
  order: AccountOrder,
  item: AccountOrderItem,
  key: MarketVariantKey | null,
): InventoryViewItem {
  const setReservations = activeSetComponentReservations(account, order.id);
  const availableSets = item.setComponents
    ?.filter((component) => component.requiredQuantity > 0)
    .map((component) => {
      const componentItems = inventory.items.filter((candidate) =>
        candidate.resolution === "resolved" && candidate.key?.slug === component.slug
      );
      const available = componentItems.reduce(
        (total, candidate) => total + candidate.sellableQuantity,
        0,
      );
      const directReservations = account.orders
        .filter((candidate) =>
          candidate.id !== order.id
          && candidate.visible
          && candidate.type === "sell"
          && componentItems.some((inventoryItem) =>
            sameInventoryVariant(inventoryItem, candidate, candidate.itemId)
          )
        )
        .reduce((total, candidate) => total + candidate.quantity, 0);
      const reservedBySets = setReservations.get(component.slug) ?? 0;
      return Math.floor(
        Math.max(0, available - directReservations - reservedBySets)
          / component.requiredQuantity,
      );
    }) ?? [];
  const sellableQuantity = availableSets.length > 0
    ? Math.min(...availableSets)
    : 0;
  return {
    canonicalGameId: `set:${item.slug}`,
    itemId: order.itemId,
    bulkTradable: false,
    displayName: item.displayName,
    imageUrl: item.imageUrl,
    tags: ["set"],
    key,
    rank: null,
    subtype: null,
    ownedQuantity: sellableQuantity,
    tradeableQuantity: sellableQuantity,
    untradeableQuantity: 0,
    unknownQuantity: 0,
    leveledQuantity: 0,
    equippedQuantity: 0,
    equippedPlacements: [],
    sellableQuantity,
    resolution: "resolved",
    vaultStatus: "unknown",
  };
}

function activeSetComponentReservations(
  account: AccountView,
  excludedOrderId: string,
): Map<string, number> {
  const reservations = new Map<string, number>();
  for (const order of account.orders) {
    if (
      order.id === excludedOrderId
      || !order.visible
      || order.type !== "sell"
      || order.rank !== null
      || order.charges !== null
      || order.subtype !== null
      || order.amberStars !== null
      || order.cyanStars !== null
      || !order.itemId
    ) continue;
    const components = account.orderItems?.[order.itemId]?.setComponents;
    if (!components?.length) continue;
    for (const component of components) {
      if (component.requiredQuantity <= 0) continue;
      reservations.set(
        component.slug,
        (reservations.get(component.slug) ?? 0)
          + order.quantity * component.requiredQuantity,
      );
    }
  }
  return reservations;
}

function sameOrderVariant(left: AccountOrder, right: AccountOrder): boolean {
  return left.itemId === right.itemId
    && left.rank === right.rank
    && left.charges === right.charges
    && left.subtype === right.subtype
    && left.amberStars === right.amberStars
    && left.cyanStars === right.cyanStars;
}

function evaluateOrder(
  order: AccountOrder,
  inventory: InventoryViewItem | null,
  recommendation: PriceRecommendation | null,
  inventoryAvailable: boolean,
  now: Date,
): Pick<TradeShiftRow, "health" | "suggestedPrice" | "suggestedQuantity" | "needsAction"> {
  const lotSize = order.perTrade ?? 1;
  const listPrice = recommendation?.listPrice === null || recommendation?.listPrice === undefined
    ? null
    : recommendation.listPrice * lotSize;
  const fairPrice = recommendation?.fairPrice === null || recommendation?.fairPrice === undefined
    ? null
    : recommendation.fairPrice * lotSize;
  if (inventoryAvailable && (!inventory || order.quantity > inventory.sellableQuantity)) {
    return {
      health: "inventory_mismatch",
      suggestedPrice: roundedPrice(listPrice),
      suggestedQuantity: inventory
        ? executableQuantity(inventory.sellableQuantity, order.perTrade)
        : 0,
      needsAction: true,
    };
  }
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

function executableQuantity(quantity: number, perTrade: number | null): number {
  if (perTrade === null) return quantity;
  return quantity - (quantity % perTrade);
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
