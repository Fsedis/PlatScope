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
  LiveQuoteState,
  MarketItemKind,
  PriceRecommendation,
} from "./market";

export type SellPriorityBand = "none" | "low" | "medium" | "high";

export interface SellPriorityFactors {
  quantity: number;
  price: number;
  liquidity: number;
  confidenceMultiplier: number;
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
  inventoryNominalValue: number;
  nominalValue: number;
}

export interface SellNowView {
  inventoryMetadata: InventorySnapshotMetadata;
  inventorySummary: InventorySummary;
  keepCopies: number;
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
  warning: string | null;
}

export type SellNowPreset =
  | "sellable"
  | "sell_now"
  | "hold"
  | "all"
  | "duplicates"
  | "unpriced"
  | "attention";
export type SellNowSortKey = "priority" | "name" | "sellable" | "fair" | "volume" | "trend";
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
  const query = filters.query.trim().toLocaleLowerCase("ru");
  return rows
    .filter((row) => {
      const fair = row.recommendation?.fairPrice ?? null;
      const timing = row.trend?.timing ?? null;
      const matchesQuery =
        !query ||
        row.inventory.displayName.toLocaleLowerCase("ru").includes(query) ||
        row.inventory.canonicalGameId.toLocaleLowerCase("ru").includes(query) ||
        row.inventory.key?.slug.includes(query);
      const matchesCategory =
        filters.category === "all" ||
        inventoryCategory(row.inventory) === filters.category;
      const matchesPreset =
        filters.preset === "all" ||
        (filters.preset === "sellable" && row.inventory.sellableQuantity > 0) ||
        (filters.preset === "sell_now" &&
          row.inventory.sellableQuantity > 0 &&
          fair !== null &&
          (timing === "sell" || timing === "peak")) ||
        (filters.preset === "hold" &&
          row.inventory.sellableQuantity > 0 &&
          timing === "hold") ||
        (filters.preset === "duplicates" && row.inventory.ownedQuantity > 1) ||
        (filters.preset === "unpriced" && fair === null) ||
        (filters.preset === "attention" &&
          (row.inventory.resolution !== "resolved" || row.inventory.unknownQuantity > 0));
      return matchesQuery && matchesCategory && matchesPreset;
    })
    .sort((left, right) => {
      const comparison = compareRows(left, right, filters.sortKey);
      return filters.sortDirection === "asc" ? comparison : -comparison;
    });
}

export function sellNowRowIdentity(row: SellNowRow): string {
  const key = row.inventory.key;
  return key
    ? [key.slug, key.platform, key.rank ?? "", key.subtype ?? "", key.amberStars ?? "", key.cyanStars ?? ""].join(":")
    : row.inventory.canonicalGameId;
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

export function priorityReasonMessages(row: SellNowRow, locale: UiLocale = "ru"): string[] {
  if (row.inventory.sellableQuantity === 0) return [locale === "ru"
    ? "Нет подтверждённых копий для продажи, поэтому предмет не поднимается в очереди."
    : "There are no confirmed copies to sell, so the item does not move up the queue."];
  if (row.recommendation?.fairPrice == null) return [locale === "ru"
    ? "Нет надёжной цены, поэтому предмет не поднимается в очереди."
    : "There is no reliable price, so the item does not move up the queue."];
  return locale === "ru" ? [
    `Можно выставить ${row.inventory.sellableQuantity} шт. Количество повышает позицию в очереди, но после пяти копий влияние не растёт.`,
    "Цена и число завершённых сделок повышают позицию предмета.",
    "Тренд цены за 90 дней и положение текущей цены формируют рекомендацию: продавать или ждать.",
    "Очередность помогает выбрать, что проверить первым; она не обещает быструю продажу.",
  ] : [
    `${row.inventory.sellableQuantity} can be listed. Quantity raises the item in the queue, with no extra weight after five copies.`,
    "Price and completed trades raise the item's position.",
    "The 90-day price trend and current price position determine whether to sell or wait.",
    "The queue helps choose what to review first; it does not promise a quick sale.",
  ];
}

function compareRows(left: SellNowRow, right: SellNowRow, key: SellNowSortKey): number {
  switch (key) {
    case "priority":
      return left.priority.score - right.priority.score;
    case "name":
      return left.inventory.displayName.localeCompare(right.inventory.displayName, "ru");
    case "sellable":
      return left.inventory.sellableQuantity - right.inventory.sellableQuantity;
    case "fair":
      return compareNullable(left.recommendation?.fairPrice, right.recommendation?.fairPrice);
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
