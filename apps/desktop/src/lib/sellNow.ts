import type { TrendSummary } from "./history";
import type { UiLocale } from "./i18n";
import {
  inventoryCategory,
  type InventoryCategoryFilter,
  type InventorySnapshotMetadata,
  type InventorySummary,
  type InventoryViewItem,
} from "./inventory";
import type { MarketSnapshotSummary } from "./foundation";
import type {
  LiveOrderView,
  LiveQuoteState,
  MarketItemKind,
  PriceRecommendation,
} from "./market";

export type SellPriorityBand = "none" | "low" | "medium" | "high";

export interface SellPriorityFactors {
  quantity: number;
  price: number;
  liquidity: number;
  timingMultiplier: number;
}

export interface SellPriorityScore {
  score: number;
  band: SellPriorityBand;
  factors: SellPriorityFactors;
  reasons: string[];
}

export interface SellNowRow {
  inventory: InventoryViewItem;
  itemKind: MarketItemKind;
  recommendation: PriceRecommendation | null;
  trend: TrendSummary | null;
  priority: SellPriorityScore;
  nominalValue: number | null;
}

export interface SellNowSummary {
  candidateRows: number;
  pricedRows: number;
  highPriorityRows: number;
  nominalValue: number;
}

export interface SellNowView {
  inventoryMetadata: InventorySnapshotMetadata;
  inventorySummary: InventorySummary;
  keepCopies: number;
  modUsageScanned: boolean;
  marketSnapshot: MarketSnapshotSummary | null;
  summary: SellNowSummary;
  rows: SellNowRow[];
}

export interface LiveSellNowResult {
  row: SellNowRow;
  fetchedAt: string;
  quoteState: LiveQuoteState;
  sellOrderCount: number;
  buyOrderCount: number;
  orders: LiveOrderView[];
  warning: string | null;
}

export type SellNowPreset =
  | "sellable"
  | "unavailable"
  | "equipped"
  | "all"
  | "duplicates"
  | "unpriced"
  | "attention";
export type SellNowSortKey = "priority" | "name" | "owned" | "sellable" | "fair" | "volume" | "trend";
export type SellNowSortDirection = "asc" | "desc";

export interface SellNowFilters {
  query: string;
  category: InventoryCategoryFilter;
  preset: SellNowPreset;
  sortKey: SellNowSortKey;
  sortDirection: SellNowSortDirection;
}

export function filterAndSortSellNowRows(
  rows: SellNowRow[],
  filters: SellNowFilters,
): SellNowRow[] {
  const words = normalizeInventorySearch(filters.query).split(" ").filter(Boolean);
  return rows
    .filter((row) => {
      const price = inventoryUnitPrice(row);
      const searchable = normalizeInventorySearch(`${row.inventory.displayName} ${row.inventory.key?.slug ?? ""}`);
      const matchesQuery = words.every(word => searchable.includes(word));
      const matchesCategory =
        filters.category === "all" ||
        inventoryCategory(row.inventory) === filters.category;
      const matchesPreset =
        filters.preset === "all" ||
        (filters.preset === "sellable" && hasSellableCopies(row)) ||
        (filters.preset === "unavailable" && !hasSellableCopies(row)) ||
        (filters.preset === "equipped" && row.inventory.equippedQuantity > 0) ||
        (filters.preset === "duplicates" && row.inventory.ownedQuantity > 1) ||
        (filters.preset === "unpriced" && price === null) ||
        (filters.preset === "attention" &&
          (row.inventory.resolution !== "resolved" || row.inventory.unknownQuantity > 0));
      return matchesQuery && matchesCategory && matchesPreset;
    })
    .sort((left, right) => {
      const missingOrder = nullableSortOrder(left, right, filters.sortKey);
      if (missingOrder !== 0) return missingOrder;
      const comparison = compareRows(left, right, filters.sortKey);
      return (filters.sortDirection === "asc" ? comparison : -comparison)
        || left.inventory.displayName.localeCompare(right.inventory.displayName, "ru", { numeric: true })
        || sellNowRowIdentity(left).localeCompare(sellNowRowIdentity(right), "en", { numeric: true });
    });
}

/** Одна оценка для таблицы, сортировки и отбора «без цены». */
export function inventoryUnitPrice(row: SellNowRow): number | null {
  const price = row.recommendation?.listPrice ?? row.recommendation?.fairPrice;
  return price != null && Number.isFinite(price) && price > 0 ? price : null;
}

export function hasSellableCopies(row: SellNowRow): boolean {
  return row.inventory.sellableQuantity > 0 && row.inventory.resolution === "resolved"
    && row.inventory.key !== null && row.inventory.itemId !== null;
}

function normalizeInventorySearch(value: string): string {
  return value.toLocaleLowerCase("ru").replaceAll("ё", "е").replace(/[^\p{L}\p{N}]+/gu, " ").trim();
}

/** Снимок количества всегда новый; проверка цены не может воскресить проданные копии. */
export function withCheckedPrice(row: SellNowRow, quote: LiveSellNowResult | undefined, now: number, ttlSeconds = 90): SellNowRow {
  if (!quote || !isCheckedPriceCurrent(quote, now, ttlSeconds) || sellNowRowIdentity(row) !== sellNowRowIdentity(quote.row)) return row;
  const fair = quote.row.recommendation?.fairPrice ?? null;
  return {
    ...row,
    recommendation: quote.row.recommendation,
    nominalValue: fair === null ? null : fair * row.inventory.sellableQuantity,
  };
}

export function isCheckedPriceCurrent(quote: LiveSellNowResult, now: number, ttlSeconds = 90): boolean {
  const age = now - Date.parse(quote.fetchedAt);
  return Number.isFinite(age) && age >= 0 && age <= Math.max(15, Math.min(600, ttlSeconds)) * 1000 && quote.quoteState !== "stale_cache";
}

export function inventoryPage(rows: SellNowRow[], requested: number, size = 50) {
  size = Math.max(1, Number.isFinite(size) ? Math.trunc(size) : 50);
  const count = Math.max(1, Math.ceil(rows.length / size));
  const page = Math.max(1, Math.min(Number.isFinite(requested) ? Math.trunc(requested) : 1, count));
  return { page, count, rows: rows.slice((page - 1) * size, page * size), from: rows.length ? (page - 1) * size + 1 : 0, to: Math.min(page * size, rows.length) };
}

export function summarizeInventoryRows(rows: SellNowRow[]) {
  const candidates = rows.filter(row => row.inventory.sellableQuantity > 0 && row.inventory.resolution === "resolved");
  return {
    candidateRows: candidates.length,
    pricedRows: candidates.filter(row => row.recommendation?.fairPrice != null).length,
    highPriorityRows: candidates.filter(row => row.priority.band === "high").length,
    nominalValue: candidates.reduce((sum, row) => sum + (row.nominalValue ?? 0), 0),
  };
}

export function sellNowRowIdentity(row: SellNowRow): string {
  const key = row.inventory.key;
  return key
    ? [key.slug, key.platform, key.rank ?? "", key.charges ?? "", key.subtype ?? "", key.amberStars ?? "", key.cyanStars ?? ""].join(":")
    : row.inventory.canonicalGameId;
}

/**
 * Возвращает уникальный ключ строки для DOM даже тогда, когда снимок содержит
 * несколько неразрешённых предметов с одинаковым игровым идентификатором.
 */
export function sellNowRowDomKey(row: SellNowRow, position: number): string {
  return `${sellNowRowIdentity(row)}::${position}`;
}

export function resolveSellNowSelection(
  visibleRows: SellNowRow[],
  selectedIdentity: string,
  pageRows: SellNowRow[] = visibleRows,
): SellNowRow | null {
  return visibleRows.find((row) => sellNowRowIdentity(row) === selectedIdentity)
    ?? pageRows[0]
    ?? null;
}

/** Возвращает устойчивое место предмета в общей очереди продажи. */
export function sellPriorityRanks(rows: SellNowRow[]): Map<string, number> {
  const ranked = rows
    .filter((row) => row.inventory.sellableQuantity > 0 && row.priority.score > 0)
    .sort((left, right) =>
      right.priority.score - left.priority.score
      || left.inventory.displayName.localeCompare(right.inventory.displayName, "ru")
    );
  const result = new Map<string, number>();
  let lastScore: number | null = null;
  let currentRank = 0;
  for (const [index, row] of ranked.entries()) {
    if (row.priority.score !== lastScore) {
      currentRank = index + 1;
      lastScore = row.priority.score;
    }
    result.set(sellNowRowIdentity(row), currentRank);
  }
  return result;
}

export function priorityLabel(band: SellPriorityBand, locale: UiLocale = "ru"): string {
  switch (band) {
    case "high":
      return locale === "en" ? "High" : "Высокий";
    case "medium":
      return locale === "en" ? "Medium" : "Средний";
    case "low":
      return locale === "en" ? "Low" : "Низкий";
    case "none":
      return locale === "en" ? "No signal" : "Нет сигнала";
  }
}

export function priorityReasonMessages(
  row: SellNowRow,
  locale: UiLocale = "ru",
  rank: number | null = null,
): string[] {
  if (row.inventory.sellableQuantity === 0) return [locale === "ru"
    ? "Нет подтверждённых копий для продажи, поэтому предмет не поднимается в очереди."
    : "There are no confirmed copies to sell, so the item does not move up the queue."];
  if (row.recommendation?.fairPrice == null) return [locale === "ru"
    ? "Нет надёжной цены, поэтому предмет не поднимается в очереди."
    : "There is no reliable price, so the item does not move up the queue."];
  const position = rank === null ? [] : [locale === "ru"
    ? `№${rank} — место предмета среди всех оценённых позиций. Чем меньше номер, тем раньше его стоит выставить.`
    : `No. ${rank} is this item's place among all evaluated listings. A lower number should be listed earlier.`];
  return locale === "ru" ? [...position,
    `Можно выставить ${row.inventory.sellableQuantity} шт. Количество повышает позицию в очереди, но после пяти копий влияние не растёт.`,
    "Цена и число завершённых сделок повышают позицию предмета.",
    "Тренд цены за 90 дней и положение текущей цены формируют рекомендацию: продавать или ждать.",
    "Приоритет показывает порядок выставления, но не гарантирует скорость продажи.",
  ] : [...position,
    `${row.inventory.sellableQuantity} can be listed. Quantity raises the item in the queue, with no extra weight after five copies.`,
    "Price and completed trades raise the item's position.",
    "The 90-day price trend and current price position determine whether to sell or wait.",
    "Priority shows listing order; it does not promise a quick sale.",
  ];
}

function compareRows(left: SellNowRow, right: SellNowRow, key: SellNowSortKey): number {
  switch (key) {
    case "priority":
      return left.priority.score - right.priority.score;
    case "name":
      return left.inventory.displayName.localeCompare(right.inventory.displayName, "ru", { numeric: true });
    case "owned":
      return left.inventory.ownedQuantity - right.inventory.ownedQuantity;
    case "sellable":
      return left.inventory.sellableQuantity - right.inventory.sellableQuantity;
    case "fair":
      return compareNullable(inventoryUnitPrice(left), inventoryUnitPrice(right));
    case "volume":
      return compareNullable(left.recommendation?.closedVolume, right.recommendation?.closedVolume);
    case "trend":
      return compareNullable(left.trend?.change90d, right.trend?.change90d);
  }
}

function compareNullable(left: number | null | undefined, right: number | null | undefined): number {
  if (left === null || left === undefined) return right === null || right === undefined ? 0 : -1;
  if (right === null || right === undefined) return 1;
  return left - right;
}

function nullableSortOrder(
  left: SellNowRow,
  right: SellNowRow,
  key: SellNowSortKey,
): number {
  const value = (row: SellNowRow): number | null | undefined => {
    switch (key) {
      case "fair": return inventoryUnitPrice(row);
      case "volume": return row.recommendation?.closedVolume;
      case "trend": return row.trend?.change90d;
      default: return 0;
    }
  };
  const leftValue = value(left);
  const rightValue = value(right);
  const leftMissing = leftValue === null || leftValue === undefined;
  const rightMissing = rightValue === null || rightValue === undefined;
  if (leftMissing === rightMissing) return 0;
  return leftMissing ? 1 : -1;
}
