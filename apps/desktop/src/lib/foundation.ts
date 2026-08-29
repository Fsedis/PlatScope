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

export interface HistoryBootstrapFailure {
  sourceDate: string;
  code: string;
  message: string;
}

export interface HistoryBootstrapOutcome {
  targetDays: number;
  importedDays: number;
  skippedDays: number;
  coverage: HistoryCoverage;
  failures: HistoryBootstrapFailure[];
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
    return locale === "en" ? "Saved data is not ready" : "Сохранённые данные не готовы";
  }

  return locale === "en"
    ? `Saved data is ready · format ${status.schemaVersion}`
    : `Сохранённые данные готовы · формат ${status.schemaVersion}`;
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
      return locale === "en" ? "saved data" : "сохранённые данные";
    case "import":
      return locale === "en" ? "imported file" : "импортированный файл";
  }
}
import type { UiLocale } from "./i18n";
