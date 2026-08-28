import type { TrendSummary } from "./history";
import type { UiLocale } from "./i18n";
import {
  inventoryCategory,
  type InventoryCategoryFilter,
  type InventorySnapshotMetadata,
  type InventoryViewItem,
} from "./inventory";
import type { MarketSnapshotSummary } from "./foundation";
import type {
  LiveQuoteState,
  MarketItemKind,
  PriceConfidence,
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

export type SellNowPreset = "all" | "sell_now" | "high_priority" | "unpriced";
export type SellNowSortKey = "priority" | "name" | "sellable" | "fair" | "volume" | "trend";
export type SellNowTimingFilter = "all" | "hold" | "neutral" | "sell" | "peak" | "unknown";
export type SellNowConfidenceFilter = "all" | PriceConfidence;
export type SellNowSortDirection = "asc" | "desc";

export interface SellNowFilters {
  query: string;
  category: InventoryCategoryFilter;
  preset: SellNowPreset;
  confidence: SellNowConfidenceFilter;
  timing: SellNowTimingFilter;
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
      const confidence = row.recommendation?.confidence ?? "unknown";
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
        (filters.preset === "sell_now" &&
          fair !== null &&
          (timing === "sell" || timing === "peak")) ||
        (filters.preset === "high_priority" && row.priority.band === "high") ||
        (filters.preset === "unpriced" && fair === null);
      const matchesConfidence =
        filters.confidence === "all" || confidence === filters.confidence;
      const matchesTiming =
        filters.timing === "all" ||
        (filters.timing === "unknown" ? timing === null : timing === filters.timing);
      return (
        matchesQuery &&
        matchesCategory &&
        matchesPreset &&
        matchesConfidence &&
        matchesTiming
      );
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
  if (locale === "ru") return row.priority.reasons;
  if (row.inventory.sellableQuantity === 0) return ["There is no confirmed sellable quantity, so priority is 0."];
  if (row.recommendation?.fairPrice == null) return ["There is no reliable fair price, so this item does not move up the sell queue."];
  const factors = row.priority.factors;
  return [
    `Sellable quantity is ${row.inventory.sellableQuantity}; the quantity factor is ${Math.round(factors.quantity * 100)}% and saturates after 5 copies.`,
    `Fair price and closed trades produce a ${Math.round(factors.price * 100)}% price factor and ${Math.round(factors.liquidity * 100)}% liquidity factor.`,
    `Confidence and timing apply ${Math.round(factors.confidenceMultiplier * 100)}% and ${Math.round(factors.timingMultiplier * 100)}% multipliers; the final ranking score is ${row.priority.score}/100.`,
    "Priority is a relative review order, not a platinum-per-day forecast.",
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
      return compareNullable(left.trend?.change7d, right.trend?.change7d);
  }
}

function compareNullable(left: number | null | undefined, right: number | null | undefined): number {
  if (left === null || left === undefined) return right === null || right === undefined ? 0 : -1;
  if (right === null || right === undefined) return 1;
  return left - right;
}
