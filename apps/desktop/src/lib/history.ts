import type { HistoryCoverage } from "./foundation";
import { localeCode, type UiLocale } from "./i18n";
import type { MarketVariantKey } from "./market";

export type TimingSignal = "hold" | "neutral" | "sell" | "peak";

export interface MarketHistoryPoint {
  sourceDate: string;
  closedMedian: number | null;
  closedVolume: number;
  sellMedian: number | null;
  buyMedian: number | null;
}

export interface TrendSummary {
  median7d: number | null;
  median30d: number | null;
  median90d: number | null;
  change7d: number | null;
  change30d: number | null;
  change90d: number | null;
  volumeAvg7d: number | null;
  volumeAvg30d: number | null;
  volumeAvg90d: number | null;
  historicalLow: number | null;
  historicalHigh: number | null;
  timing: TimingSignal | null;
  trustedDays: number;
}

export interface MarketHistoryView {
  key: MarketVariantKey;
  requestedDays: 7 | 30 | 90;
  points: MarketHistoryPoint[];
  trend: TrendSummary;
  coverage: HistoryCoverage;
}

export interface HistoryChartDot {
  x: number;
  y: number;
  point: MarketHistoryPoint;
}

export interface HistoryChartModel {
  path: string;
  dots: HistoryChartDot[];
  minPrice: number;
  maxPrice: number;
  firstDate: string;
  lastDate: string;
}

export function buildHistoryChart(
  points: MarketHistoryPoint[],
  width = 320,
  height = 150,
): HistoryChartModel | null {
  const priced = points.filter(
    (point): point is MarketHistoryPoint & { closedMedian: number } =>
      point.closedMedian !== null && Number.isFinite(point.closedMedian),
  );
  if (priced.length < 2) return null;
  const prices = priced.map((point) => point.closedMedian);
  const rawMin = Math.min(...prices);
  const rawMax = Math.max(...prices);
  const padding = rawMax === rawMin ? Math.max(1, rawMax * 0.05) : (rawMax - rawMin) * 0.12;
  const minPrice = Math.max(0, rawMin - padding);
  const maxPrice = rawMax + padding;
  const plotLeft = 34;
  const plotRight = width - 8;
  const plotTop = 10;
  const plotBottom = height - 24;
  const dots = priced.map((point, index) => {
    const x =
      priced.length === 1
        ? (plotLeft + plotRight) / 2
        : plotLeft + (index / (priced.length - 1)) * (plotRight - plotLeft);
    const y =
      plotBottom -
      ((point.closedMedian - minPrice) / Math.max(0.001, maxPrice - minPrice)) *
        (plotBottom - plotTop);
    return { x, y, point };
  });
  return {
    path: dots.map((dot, index) => `${index === 0 ? "M" : "L"}${dot.x.toFixed(2)},${dot.y.toFixed(2)}`).join(" "),
    dots,
    minPrice,
    maxPrice,
    firstDate: priced[0]?.sourceDate ?? "",
    lastDate: priced.at(-1)?.sourceDate ?? "",
  };
}

export function timingLabel(value: TimingSignal, locale: UiLocale = "ru"): string {
  return (locale === "en" ? {
    hold: "Wait — the price is near its recent low",
    neutral: "Usual price — within the recent range",
    sell: "Good time to sell — the price is in the upper range",
    peak: "Best time to sell — current orders confirm the upper range",
  } : {
    hold: "Лучше подождать — цена у нижней границы последних значений",
    neutral: "Обычная цена — внутри привычного диапазона",
    sell: "Хорошее время для продажи — цена в верхней части диапазона",
    peak: "Лучший момент для продажи — текущие ордера подтверждают верх диапазона",
  })[value];
}

export function timingShortLabel(value: TimingSignal, locale: UiLocale = "ru"): string {
  return (locale === "en" ? {
    hold: "Wait",
    neutral: "Usual price",
    sell: "Good time",
    peak: "Best time",
  } : {
    hold: "Лучше подождать",
    neutral: "Обычная цена",
    sell: "Хорошее время",
    peak: "Лучший момент",
  })[value];
}

export function formatChange(value: number | null, locale: UiLocale = "ru"): string {
  if (value === null) return "—";
  const sign = value > 0 ? "+" : "";
  return `${sign}${new Intl.NumberFormat(localeCode(locale), { maximumFractionDigits: 1 }).format(value)}%`;
}
