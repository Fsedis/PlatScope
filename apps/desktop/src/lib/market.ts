import type { MarketSnapshotSummary, ProviderId } from "./foundation";
import { localeCode, type UiLocale } from "./i18n";

export type PriceConfidence = "high" | "medium" | "low" | "unknown";
export type PriceFreshness = "fresh" | "aging" | "stale" | "unknown";
export type MarketItemKind = "standard" | "relic" | "riven";

export interface MarketVariantKey {
  slug: string;
  platform: string;
  rank: number | null;
  subtype: string | null;
  amberStars: number | null;
  cyanStars: number | null;
}

export interface PriceReason {
  code: string;
  message: string;
}

export function priceReasonMessage(reason: PriceReason, locale: UiLocale = "ru"): string {
  if (locale === "ru") return reason.message;
  const messages: Record<string, string> = {
    trusted_closed_trades: "Fair price is supported by exact-variant closed trades.",
    closed_volume_too_low: "Closed-trade volume is too low for a strong signal.",
    conservative_sell_adjustment: "The sell estimate was adjusted conservatively.",
    sell_only_fallback: "Only sell-side data is available, so confidence is limited.",
    relic_sell_ignored: "Sell-only relic data was not used as fair value.",
    no_exact_variant: "No data exists for the exact market variant.",
    live_book_variant_mismatch: "Live orders for other variants were ignored.",
    isolated_ask_ignored: "An isolated ask was ignored as an unreliable outlier.",
    live_cluster_shift: "A coherent live cluster indicates a market shift.",
    thin_market_protection: "Thin-market protection limits the recommendation.",
    live_market_agreement: "Live orders agree with the bulk estimate.",
    live_market_disagreement: "Live orders disagree with the bulk estimate, reducing confidence.",
    live_top_buy: "Quick Sell uses the best active buy order for the exact variant.",
    no_live_top_buy: "No active exact-variant buy order is available for Quick Sell.",
    source_fresh: "The bulk snapshot is fresh.",
    source_aging: "The bulk snapshot is aging.",
    source_stale: "The bulk snapshot is stale.",
    source_date_invalid: "The source date is invalid.",
    fallback_provider: "The current snapshot came from a fallback provider.",
    riven_pricing_unsupported: "Unique Riven rolls require a separate model; standard item medians were not used.",
    insufficient_signal: "There is not enough reliable market data for a price.",
  };
  return messages[reason.code] ?? "A bounded pricing rule contributed to this recommendation.";
}

export interface PriceRecommendation {
  key: MarketVariantKey;
  provider: ProviderId;
  sourceDate: string;
  fairPrice: number | null;
  listPrice: number | null;
  quickSell: number | null;
  lowestAsk: number | null;
  depthThree: number | null;
  depthPrice: number | null;
  closedVolume: number | null;
  liveSellOrderCount: number;
  liveBuyOrderCount: number;
  confidence: PriceConfidence;
  freshness: PriceFreshness;
  reasons: PriceReason[];
}

export interface MarketSearchRow {
  itemId: string;
  displayName: string;
  imageUrl?: string | null;
  itemKind: MarketItemKind;
  masteryRequirement: number | null;
  recommendation: PriceRecommendation;
}

export interface MarketSearchResult {
  query: string;
  rows: MarketSearchRow[];
  truncated: boolean;
  snapshot: MarketSnapshotSummary | null;
}

export function masteryRequirementLabel(
  value: number | null,
  locale: UiLocale = "ru",
): string {
  if (value === null) return locale === "en" ? "No data" : "Нет данных";
  return `MR ${value}`;
}

export type LiveQuoteState = "network" | "cache" | "stale_cache";
export type LiveOrderSide = "buy" | "sell";
export type LiveUserStatus = "in_game" | "online" | "offline";

export interface LiveOrderView {
  side: LiveOrderSide;
  platinum: number;
  quantity: number;
  perTrade: number;
  userStatus: LiveUserStatus;
}

export interface LivePricingResult {
  recommendation: PriceRecommendation;
  fetchedAt: string;
  quoteState: LiveQuoteState;
  sellOrderCount: number;
  buyOrderCount: number;
  orders: LiveOrderView[];
  warning: string | null;
}

export function liveQuoteLabel(value: LiveQuoteState, locale: UiLocale = "ru"): string {
  return (locale === "en" ? {
    network: "Just fetched from WFM",
    cache: "From the local live cache",
    stale_cache: "Stale live cache",
  } : {
    network: "Только что с WFM",
    cache: "Из локального live-кэша",
    stale_cache: "Устаревший live-кэш",
  })[value];
}

export function liveUserStatusLabel(
  value: LiveUserStatus,
  locale: UiLocale = "ru",
): string {
  return (locale === "en" ? {
    in_game: "In game",
    online: "Online",
    offline: "Offline",
  } : {
    in_game: "В игре",
    online: "В сети",
    offline: "Не в сети",
  })[value];
}

export type MarketSortKey = "name" | "fair" | "volume" | "confidence";
export type SortDirection = "asc" | "desc";
export type PriceFilter = "all" | "priced" | "unpriced";

const confidenceOrder: Record<PriceConfidence, number> = {
  high: 3,
  medium: 2,
  low: 1,
  unknown: 0,
};

export function formatPlatinum(value: number | null, locale: UiLocale = "ru"): string {
  if (value === null) return "—";
  return `${new Intl.NumberFormat(localeCode(locale), { maximumFractionDigits: 1 }).format(value)}p`;
}

export function formatVolume(value: number | null, locale: UiLocale = "ru"): string {
  if (value === null) return "—";
  return new Intl.NumberFormat(localeCode(locale), { maximumFractionDigits: 0 }).format(value);
}

export function confidenceLabel(value: PriceConfidence, locale: UiLocale = "ru"): string {
  return (locale === "en" ? {
    high: "High",
    medium: "Medium",
    low: "Low",
    unknown: "Not rated",
  } : {
    high: "Высокая",
    medium: "Средняя",
    low: "Низкая",
    unknown: "Нет оценки",
  })[value];
}

export function freshnessLabel(value: PriceFreshness, locale: UiLocale = "ru"): string {
  return (locale === "en" ? {
    fresh: "Fresh data",
    aging: "Aging data",
    stale: "Stale data",
    unknown: "Unknown date",
  } : {
    fresh: "Свежие данные",
    aging: "Данные устаревают",
    stale: "Устаревшие данные",
    unknown: "Неизвестная дата",
  })[value];
}

export function variantLabel(key: MarketVariantKey, locale: UiLocale = "ru"): string {
  const parts: string[] = [];
  if (key.rank !== null) parts.push(locale === "en" ? `rank ${key.rank}` : `ранг ${key.rank}`);
  if (key.subtype) parts.push(key.subtype);
  if (key.amberStars !== null || key.cyanStars !== null) {
    parts.push(`${locale === "en" ? "stars" : "звёзды"} ${key.amberStars ?? 0}/${key.cyanStars ?? 0}`);
  }
  return parts.length ? parts.join(" · ") : locale === "en" ? "base variant" : "базовый вариант";
}

export function rowIdentity(row: MarketSearchRow): string {
  const key = row.recommendation.key;
  return [
    key.slug,
    key.platform,
    key.rank ?? "",
    key.subtype ?? "",
    key.amberStars ?? "",
    key.cyanStars ?? "",
  ].join("|");
}

export function filterAndSortRows(
  rows: MarketSearchRow[],
  filter: PriceFilter,
  sortKey: MarketSortKey,
  direction: SortDirection,
): MarketSearchRow[] {
  const filtered = rows.filter((row) => {
    const priced = row.recommendation.fairPrice !== null;
    return filter === "all" || (filter === "priced" ? priced : !priced);
  });
  const multiplier = direction === "asc" ? 1 : -1;
  return [...filtered].sort((left, right) => {
    let comparison = 0;
    switch (sortKey) {
      case "name":
        comparison = left.displayName.localeCompare(right.displayName, "ru");
        break;
      case "fair":
        comparison = nullableNumber(
          left.recommendation.fairPrice,
          right.recommendation.fairPrice,
        );
        break;
      case "volume":
        comparison = nullableNumber(
          left.recommendation.closedVolume,
          right.recommendation.closedVolume,
        );
        break;
      case "confidence":
        comparison =
          confidenceOrder[left.recommendation.confidence] -
          confidenceOrder[right.recommendation.confidence];
        break;
    }
    return comparison === 0
      ? left.displayName.localeCompare(right.displayName, "ru")
      : comparison * multiplier;
  });
}

function nullableNumber(left: number | null, right: number | null): number {
  if (left === null && right === null) return 0;
  if (left === null) return -1;
  if (right === null) return 1;
  return left - right;
}
