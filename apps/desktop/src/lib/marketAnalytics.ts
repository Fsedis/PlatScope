import type { MarketHistoryPoint } from "./history";
import type { MarketVariantKey } from "./market";

export interface MarketAnalyticsItem {
  key: MarketVariantKey;
  supported: boolean;
  /** Дни загруженного архива; наличие дня не доказывает наличие статистики варианта. */
  coveredDates: string[];
  points: MarketHistoryPoint[];
}

export interface MarketAnalyticsBatch {
  /** Последний завершённый день UTC. Старая история не сдвигает это окно. */
  asOf: string;
  latestAvailableDate: string | null;
  requestedDays: 14;
  items: MarketAnalyticsItem[];
}

export interface MarketAnalyticsDay {
  date: string;
  covered: boolean;
  price: number | null;
  volume: number | null;
}

export interface MarketAnalyticsWindow {
  coveredDays: number;
  observedDays: number;
  pricedDays: number;
  /** Медиана дневных медиан; не медиана всех отдельных сделок за неделю. */
  medianPrice: number | null;
  /** Недельный итог и среднее за сутки доступны только при семи наблюдаемых днях. */
  totalVolume: number | null;
  dailyVolume: number | null;
}

export interface MarketAnalyticsSummary {
  supported: boolean;
  days: MarketAnalyticsDay[];
  current: MarketAnalyticsWindow;
  previous: MarketAnalyticsWindow;
  priceChangePct: number | null;
  volumeChangePct: number | null;
}

/** Сохраняет различия между rank0/null, зарядами, заполнением звёздами и платформой. */
export function marketAnalyticsKey(key: MarketVariantKey): string {
  return JSON.stringify([
    key.slug, key.platform, key.rank, key.charges ?? null, key.subtype,
    key.amberStars, key.cyanStars,
  ]);
}

function dateTimestamp(date: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return null;
  const timestamp = Date.parse(`${date}T00:00:00Z`);
  return Number.isFinite(timestamp) && new Date(timestamp).toISOString().slice(0, 10) === date
    ? timestamp : null;
}

function finiteNonnegative(value: number | null): value is number {
  return value !== null && Number.isFinite(value) && value >= 0;
}

function median(values: number[]): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

function summarizeWindow(days: MarketAnalyticsDay[]): MarketAnalyticsWindow {
  const volumes = days.flatMap((day) => day.volume === null ? [] : [day.volume]);
  const prices = days.flatMap((day) => day.price === null ? [] : [day.price]);
  const totalVolume = volumes.length === 7 ? volumes.reduce((total, volume) => total + volume, 0) : null;
  return {
    coveredDays: days.filter((day) => day.covered).length,
    observedDays: volumes.length,
    pricedDays: prices.length,
    medianPrice: median(prices),
    totalVolume,
    dailyVolume: totalVolume === null ? null : totalVolume / 7,
  };
}

function percentChange(current: number | null, previous: number | null): number | null {
  if (current === null || previous === null) return null;
  if (previous === 0) return current === 0 ? 0 : null;
  return ((current - previous) / previous) * 100;
}

/** Сохраняет единицы исходной статистики: размер лота объявления не меняет её масштаб. */
export function summarizeMarketAnalytics(
  item: MarketAnalyticsItem,
  asOf: string,
): MarketAnalyticsSummary {
  const end = dateTimestamp(asOf);
  const covered = new Set(item.coveredDates);
  const points = new Map(item.points.map((point) => [point.sourceDate, point]));
  const days: MarketAnalyticsDay[] = end === null ? [] : Array.from({ length: 14 }, (_, index) => {
    const date = new Date(end - (13 - index) * 86_400_000).toISOString().slice(0, 10);
    const dayCovered = item.supported && covered.has(date);
    const point = dayCovered ? points.get(date) : undefined;
    // В compact-кеше ноль также ставился, когда были только открытые заявки.
    // Без закрытой записи это не доказанный нулевой объём торгов.
    const closedObserved = point !== undefined
      && (finiteNonnegative(point.closedMedian) || point.closedVolume > 0);
    const volume = closedObserved && finiteNonnegative(point.closedVolume) ? point.closedVolume : null;
    const price = volume !== null && volume > 0 && point && finiteNonnegative(point.closedMedian)
      ? point.closedMedian : null;
    return { date, covered: dayCovered, price, volume };
  });
  const previous = summarizeWindow(days.slice(0, 7));
  const current = summarizeWindow(days.slice(7));
  const complete = current.observedDays === 7 && previous.observedDays === 7;
  return {
    supported: item.supported,
    days,
    current,
    previous,
    priceChangePct: complete && current.pricedDays === 7 && previous.pricedDays === 7
      ? percentChange(current.medianPrice, previous.medianPrice) : null,
    volumeChangePct: complete ? percentChange(current.totalVolume, previous.totalVolume) : null,
  };
}

export interface MarketSparklinePoint {
  x: number;
  y: number;
  day: MarketAnalyticsDay;
  value: number;
}

export interface MarketSparkline {
  path: string;
  points: MarketSparklinePoint[];
  min: number;
  max: number;
}

/** null разрывает линию; ось X сохраняет все семь календарных дней. */
export function buildMarketSparkline(
  days: MarketAnalyticsDay[],
  metric: "price" | "volume",
  width = 96,
  height = 26,
): MarketSparkline | null {
  const values = days.flatMap((day) => finiteNonnegative(day[metric]) ? [day[metric] as number] : []);
  if (values.length === 0 || width <= 4 || height <= 4) return null;
  const min = metric === "volume" ? 0 : Math.min(...values);
  const max = Math.max(...values);
  const span = max - min;
  let connected = false;
  const path: string[] = [];
  const points: MarketSparklinePoint[] = [];
  days.forEach((day, index) => {
    const value = day[metric];
    if (!finiteNonnegative(value)) { connected = false; return; }
    const x = 2 + (index / Math.max(1, days.length - 1)) * (width - 4);
    const y = span === 0 ? height / 2 : height - 2 - ((value - min) / span) * (height - 4);
    path.push(`${connected ? "L" : "M"}${x.toFixed(2)},${y.toFixed(2)}`);
    points.push({ x, y, day, value });
    connected = true;
  });
  return { path: path.join(" "), points, min, max };
}
