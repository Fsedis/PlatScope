export interface FoundationStatus {
  appName: string;
  appVersion: string;
  databasePath: string;
  schemaVersion: number;
  offlineReady: boolean;
  marketSnapshot: MarketSnapshotSummary | null;
  catalogItemCount: number | null;
  historyCoverage: HistoryCoverage;
  inventoryItemCount: number | null;
}

export interface HistoryCoverage {
  oldestDate: string | null;
  newestDate: string | null;
  dayCount: number;
}

export type ProviderId =
  | "relics_run"
  | "frame_forge_mirror"
  | "warframe_market"
  | "local_cache"
  | "import";

export interface MarketSnapshotSummary {
  provider: ProviderId;
  sourceDate: string;
  fetchedAt: string;
  promotedAt: string;
  itemCount: number;
  recordCount: number;
  checksumSha256: string;
}

export interface RefreshFailure {
  provider: ProviderId;
  code: string;
  message: string;
}

export interface MarketRefreshOutcome {
  snapshot: MarketSnapshotSummary;
  catalogItemCount: number;
  stale: boolean;
  usedFallback: boolean;
  catalogFromCache: boolean;
  failures: RefreshFailure[];
}

export function describeFoundationStatus(status: FoundationStatus, locale: UiLocale = "ru"): string {
  if (!status.offlineReady) {
    return locale === "en" ? "Local storage is not ready" : "Локальное хранилище не готово";
  }

  return locale === "en"
    ? `Local storage ready, schema ${status.schemaVersion}`
    : `Локальное хранилище готово, схема ${status.schemaVersion}`;
}

export function providerLabel(provider: ProviderId, locale: UiLocale = "ru"): string {
  switch (provider) {
    case "relics_run":
      return "relics.run";
    case "frame_forge_mirror":
      return "FrameForgePricing";
    case "warframe_market":
      return "Warframe.Market";
    case "local_cache":
      return locale === "en" ? "local cache" : "локальный кэш";
    case "import":
      return locale === "en" ? "import" : "импорт";
  }
}
import type { UiLocale } from "./i18n";
