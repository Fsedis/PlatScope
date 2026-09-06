export interface MarketPeriodPoint {
  sourceDate: string;
  closedMedian: number | null;
  closedVolume: number | null;
}

export function summarizeMarketPeriod(points: MarketPeriodPoint[]) {
  const finite = (value: number | null): value is number => value !== null && Number.isFinite(value) && value >= 0;
  const prices = points.flatMap(point => finite(point.closedMedian) ? [point.closedMedian] : []).sort((a, b) => a - b);
  const volumes = points.flatMap(point => finite(point.closedVolume) ? [point.closedVolume] : []);
  const completeVolume = points.length > 0 && volumes.length === points.length;
  const volume = completeVolume ? volumes.reduce((sum, value) => sum + value, 0) : null;
  return {
    median: prices.length ? (prices[Math.floor((prices.length - 1) / 2)] + prices[Math.ceil((prices.length - 1) / 2)]) / 2 : null,
    low: prices[0] ?? null, high: prices.at(-1) ?? null,
    pricedDays: prices.length, observedDays: volumes.length,
    volume, dailyVolume: volume === null ? null : volume / points.length,
  };
}

export function marketChangeLabel(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "Нет сравнения";
  if (value === 0) return "Без изменений";
  const amount = Math.abs(value) < .1 ? "<0,1" : Math.abs(value).toLocaleString("ru-RU", { maximumFractionDigits: 1 });
  return `${value > 0 ? "↑" : "↓"} ${amount}%`;
}
