import type { InventoryCategoryFilter } from "./inventory";
import type { MarketSortKey, PriceFilter, SortDirection } from "./market";
import type {
  EquippedFilter,
  SellNowPreset,
  SellNowSortDirection,
  SellNowSortKey,
} from "./sellNow";

export interface ViewPreferenceStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export interface MarketViewPreferences {
  priceFilter: PriceFilter;
  sortKey: MarketSortKey;
  sortDirection: SortDirection;
}

export interface SellNowViewPreferences {
  category: InventoryCategoryFilter;
  preset: SellNowPreset;
  equipped: EquippedFilter;
  sortKey: SellNowSortKey;
  sortDirection: SellNowSortDirection;
}

export type InsightsViewMode =
  | "overview"
  | "resources"
  | "relics"
  | "complete_sets"
  | "sell_sets"
  | "ducats";

export interface InsightsViewPreferences {
  mode: InsightsViewMode;
}

export const DEFAULT_MARKET_VIEW: MarketViewPreferences = {
  priceFilter: "all",
  sortKey: "volume",
  sortDirection: "desc",
};

export const DEFAULT_SELL_NOW_VIEW: SellNowViewPreferences = {
  category: "all",
  preset: "sell_now",
  equipped: "all",
  sortKey: "priority",
  sortDirection: "desc",
};

export const DEFAULT_INSIGHTS_VIEW: InsightsViewPreferences = {
  mode: "overview",
};

const MARKET_KEY = "platscope.market-view.v1";
const SELL_NOW_KEY = "platscope.sell-now-view.v1";
const INSIGHTS_KEY = "platscope.insights-view.v1";

const priceFilters = ["all", "priced", "unpriced"] as const;
const marketSortKeys = ["name", "fair", "volume"] as const;
const sortDirections = ["asc", "desc"] as const;
const sellNowPresets = [
  "sellable",
  "sell_now",
  "hold",
  "all",
  "duplicates",
  "unpriced",
  "attention",
] as const;
const sellNowSortKeys = [
  "priority",
  "name",
  "sellable",
  "fair",
  "volume",
  "trend",
] as const;
const equippedFilters = ["all", "free", "equipped"] as const;
const insightsViewModes = [
  "overview",
  "resources",
  "relics",
  "complete_sets",
  "sell_sets",
  "ducats",
] as const;

export function loadMarketViewPreferences(
  storage: ViewPreferenceStorage | null = defaultStorage(),
): MarketViewPreferences {
  const value = readRecord(MARKET_KEY, storage);
  return {
    priceFilter: allowed(value?.priceFilter, priceFilters, DEFAULT_MARKET_VIEW.priceFilter),
    sortKey: allowed(value?.sortKey, marketSortKeys, DEFAULT_MARKET_VIEW.sortKey),
    sortDirection: allowed(
      value?.sortDirection,
      sortDirections,
      DEFAULT_MARKET_VIEW.sortDirection,
    ),
  };
}

export function saveMarketViewPreferences(
  preferences: MarketViewPreferences,
  storage: ViewPreferenceStorage | null = defaultStorage(),
): boolean {
  return writeRecord(MARKET_KEY, preferences, storage);
}

export function loadSellNowViewPreferences(
  storage: ViewPreferenceStorage | null = defaultStorage(),
): SellNowViewPreferences {
  const value = readRecord(SELL_NOW_KEY, storage);
  return {
    category: validCategory(value?.category),
    preset: allowed(value?.preset, sellNowPresets, DEFAULT_SELL_NOW_VIEW.preset),
    equipped: allowed(value?.equipped, equippedFilters, DEFAULT_SELL_NOW_VIEW.equipped),
    sortKey: allowed(value?.sortKey, sellNowSortKeys, DEFAULT_SELL_NOW_VIEW.sortKey),
    sortDirection: allowed(
      value?.sortDirection,
      sortDirections,
      DEFAULT_SELL_NOW_VIEW.sortDirection,
    ),
  };
}

export function saveSellNowViewPreferences(
  preferences: SellNowViewPreferences,
  storage: ViewPreferenceStorage | null = defaultStorage(),
): boolean {
  return writeRecord(SELL_NOW_KEY, preferences, storage);
}

export function loadInsightsViewPreferences(
  storage: ViewPreferenceStorage | null = defaultStorage(),
): InsightsViewPreferences {
  const value = readRecord(INSIGHTS_KEY, storage);
  return {
    mode: allowed(value?.mode, insightsViewModes, DEFAULT_INSIGHTS_VIEW.mode),
  };
}

export function saveInsightsViewPreferences(
  preferences: InsightsViewPreferences,
  storage: ViewPreferenceStorage | null = defaultStorage(),
): boolean {
  return writeRecord(INSIGHTS_KEY, preferences, storage);
}

function defaultStorage(): ViewPreferenceStorage | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function readRecord(
  key: string,
  storage: ViewPreferenceStorage | null,
): Record<string, unknown> | null {
  if (!storage) return null;
  try {
    const raw = storage.getItem(key);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!isRecord(parsed) || parsed.version !== 1) return null;
    return parsed;
  } catch {
    return null;
  }
}

function writeRecord(
  key: string,
  value: object,
  storage: ViewPreferenceStorage | null,
): boolean {
  if (!storage) return false;
  try {
    storage.setItem(key, JSON.stringify({ version: 1, ...value }));
    return true;
  } catch {
    return false;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function allowed<T extends string>(
  value: unknown,
  values: readonly T[],
  fallback: T,
): T {
  return typeof value === "string" && values.includes(value as T)
    ? (value as T)
    : fallback;
}

function validCategory(value: unknown): InventoryCategoryFilter {
  const categories = [
    "all",
    "mod",
    "arcane_enhancement",
    "relic",
    "component",
    "weapon",
    "warframe",
    "misc",
  ] as const;
  return allowed(value, categories, DEFAULT_SELL_NOW_VIEW.category);
}
