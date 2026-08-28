import type {
  InventoryCategoryFilter,
  InventoryDuplicateFilter,
  InventoryPriceFilter,
  InventoryVaultFilter,
} from "./inventory";
import type { MarketSortKey, PriceFilter, SortDirection } from "./market";
import type {
  SellNowConfidenceFilter,
  SellNowPreset,
  SellNowSortDirection,
  SellNowSortKey,
  SellNowTimingFilter,
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

export interface InventoryViewPreferences {
  category: InventoryCategoryFilter;
  duplicates: InventoryDuplicateFilter;
  vault: InventoryVaultFilter;
  price: InventoryPriceFilter;
}

export interface SellNowViewPreferences {
  category: InventoryCategoryFilter;
  preset: SellNowPreset;
  confidence: SellNowConfidenceFilter;
  timing: SellNowTimingFilter;
  sortKey: SellNowSortKey;
  sortDirection: SellNowSortDirection;
}

export const DEFAULT_MARKET_VIEW: MarketViewPreferences = {
  priceFilter: "all",
  sortKey: "volume",
  sortDirection: "desc",
};

export const DEFAULT_INVENTORY_VIEW: InventoryViewPreferences = {
  category: "all",
  duplicates: "all",
  vault: "all",
  price: "all",
};

export const DEFAULT_SELL_NOW_VIEW: SellNowViewPreferences = {
  category: "all",
  preset: "all",
  confidence: "all",
  timing: "all",
  sortKey: "priority",
  sortDirection: "desc",
};

const MARKET_KEY = "platscope.market-view.v1";
const INVENTORY_KEY = "platscope.inventory-view.v1";
const SELL_NOW_KEY = "platscope.sell-now-view.v1";

const priceFilters = ["all", "priced", "unpriced"] as const;
const marketSortKeys = ["name", "fair", "volume", "confidence"] as const;
const sortDirections = ["asc", "desc"] as const;
const inventoryDuplicates = ["all", "duplicates"] as const;
const inventoryVault = ["all", "available", "vaulted", "unknown"] as const;
const sellNowPresets = ["all", "sell_now", "high_priority", "unpriced"] as const;
const sellNowConfidence = ["all", "high", "medium", "low", "unknown"] as const;
const sellNowTiming = ["all", "hold", "neutral", "sell", "peak", "unknown"] as const;
const sellNowSortKeys = [
  "priority",
  "name",
  "sellable",
  "fair",
  "volume",
  "trend",
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

export function loadInventoryViewPreferences(
  storage: ViewPreferenceStorage | null = defaultStorage(),
): InventoryViewPreferences {
  const value = readRecord(INVENTORY_KEY, storage);
  return {
    category: validCategory(value?.category),
    duplicates: allowed(
      value?.duplicates,
      inventoryDuplicates,
      DEFAULT_INVENTORY_VIEW.duplicates,
    ),
    vault: allowed(value?.vault, inventoryVault, DEFAULT_INVENTORY_VIEW.vault),
    price: allowed(value?.price, priceFilters, DEFAULT_INVENTORY_VIEW.price),
  };
}

export function saveInventoryViewPreferences(
  preferences: InventoryViewPreferences,
  storage: ViewPreferenceStorage | null = defaultStorage(),
): boolean {
  return writeRecord(INVENTORY_KEY, preferences, storage);
}

export function loadSellNowViewPreferences(
  storage: ViewPreferenceStorage | null = defaultStorage(),
): SellNowViewPreferences {
  const value = readRecord(SELL_NOW_KEY, storage);
  return {
    category: validCategory(value?.category),
    preset: allowed(value?.preset, sellNowPresets, DEFAULT_SELL_NOW_VIEW.preset),
    confidence: allowed(
      value?.confidence,
      sellNowConfidence,
      DEFAULT_SELL_NOW_VIEW.confidence,
    ),
    timing: allowed(value?.timing, sellNowTiming, DEFAULT_SELL_NOW_VIEW.timing),
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
  return allowed(value, categories, DEFAULT_INVENTORY_VIEW.category);
}
