import { describe, expect, it } from "vitest";
import type { MarketVariantKey } from "./market";
import {
  buildMarketSparkline, marketAnalyticsKey, summarizeMarketAnalytics,
  type MarketAnalyticsItem,
} from "./marketAnalytics";

const key: MarketVariantKey = {
  slug: "test_mod", platform: "pc", rank: 0, charges: null, subtype: null,
  amberStars: null, cyanStars: null,
};
const asOf = "2026-09-05";

function item(): MarketAnalyticsItem {
  const dates = Array.from({ length: 14 }, (_, index) =>
    new Date(Date.parse(`${asOf}T00:00:00Z`) - (13 - index) * 86_400_000).toISOString().slice(0, 10));
  return {
    key, supported: true, coveredDates: dates,
    points: dates.map((sourceDate, index) => ({
      sourceDate, closedMedian: index < 7 ? 10 : 15,
      closedVolume: index < 7 ? 100 : 50, sellMedian: 40, buyMedian: 5,
    })),
  };
}

describe("аналитика объявлений рынка", () => {
  it("сравнивает две полные календарные недели по цене и суточному объёму", () => {
    const summary = summarizeMarketAnalytics(item(), asOf);
    expect(summary.days).toHaveLength(14);
    expect(summary.current).toEqual({
      coveredDays: 7, observedDays: 7, pricedDays: 7,
      medianPrice: 15, totalVolume: 350, dailyVolume: 50,
    });
    expect(summary.previous.dailyVolume).toBe(100);
    expect(summary.priceChangePct).toBe(50);
    expect(summary.volumeChangePct).toBe(-50);
  });

  it("считает медиану дневных медиан, а не придуманную медиану всех сделок", () => {
    const source = item();
    source.points[7].closedMedian = 200;
    source.points[7].closedVolume = 100_000;
    expect(summarizeMarketAnalytics(source, asOf).current.medianPrice).toBe(15);
  });

  it("не заменяет незагруженный день нулём даже при оставшейся точке в кеше", () => {
    const source = item();
    source.coveredDates = source.coveredDates.filter((date) => date !== asOf);
    const summary = summarizeMarketAnalytics(source, asOf);
    expect(summary.days.at(-1)).toEqual({ date: asOf, covered: false, price: null, volume: null });
    expect(summary.current.coveredDays).toBe(6);
    expect(summary.current.observedDays).toBe(6);
    expect(summary.current.dailyVolume).toBeNull();
    expect(summary.priceChangePct).toBeNull();
    expect(summary.volumeChangePct).toBeNull();
  });

  it("не выдумывает нулевые продажи отсутствующего варианта в полном архиве", () => {
    const source = item();
    source.points.pop();
    const summary = summarizeMarketAnalytics(source, asOf);
    expect(summary.current.coveredDays).toBe(7);
    expect(summary.current.observedDays).toBe(6);
    expect(summary.days.at(-1)?.volume).toBeNull();
    expect(summary.current.totalVolume).toBeNull();
  });

  it("отличает открытые заявки от реальной записи с нулевым объёмом", () => {
    const source = item();
    source.points[13].closedMedian = null;
    source.points[13].closedVolume = 0;
    expect(summarizeMarketAnalytics(source, asOf).days[13].volume).toBeNull();
    source.points[13].closedMedian = 0;
    const summary = summarizeMarketAnalytics(source, asOf);
    expect(summary.days[13].volume).toBe(0);
    expect(summary.days[13].price).toBeNull();
    expect(summary.current.totalVolume).toBe(300);
    expect(summary.priceChangePct).toBeNull();
  });

  it("не делит на ноль при появлении торгов после недели нулевого объёма", () => {
    const source = item();
    source.points.slice(0, 7).forEach((point) => { point.closedMedian = 0; point.closedVolume = 0; });
    const summary = summarizeMarketAnalytics(source, asOf);
    expect(summary.previous.totalVolume).toBe(0);
    expect(summary.volumeChangePct).toBeNull();
    expect(summary.priceChangePct).toBeNull();
  });

  it("не передвигает старую историю на текущую неделю и игнорирует незавершённый день", () => {
    const source = item();
    source.points.push({ ...source.points[13], sourceDate: "2026-09-06", closedMedian: 900 });
    source.coveredDates.push("2026-09-06");
    expect(summarizeMarketAnalytics(source, asOf).current.medianPrice).toBe(15);
    const stale = summarizeMarketAnalytics(source, "2026-09-25");
    expect(stale.current.observedDays).toBe(0);
    expect(stale.current.medianPrice).toBeNull();
    expect(stale.current.dailyVolume).toBeNull();
  });

  it("не переносит данные PC на неподдерживаемую платформу", () => {
    const source = { ...item(), supported: false, key: { ...key, platform: "switch" } };
    const summary = summarizeMarketAnalytics(source, asOf);
    expect(summary.current.coveredDays).toBe(0);
    expect(summary.current.medianPrice).toBeNull();
    expect(summary.current.dailyVolume).toBeNull();
    expect(summary.days.every((day) => day.price === null && day.volume === null)).toBe(true);
  });

  it("сохраняет точный ранг, заряды, подтип и звёзды; размер лота не меняет масштаб истории", () => {
    const variants = [
      key, { ...key, rank: null }, { ...key, charges: 0 }, { ...key, charges: 3 },
      { ...key, subtype: "regular" }, { ...key, amberStars: 0 },
      { ...key, cyanStars: 1 }, { ...key, platform: "xbox" },
    ];
    expect(new Set(variants.map(marketAnalyticsKey)).size).toBe(variants.length);
    const lot = { ...key, perTrade: 6, quantity: 24 };
    expect(marketAnalyticsKey(lot)).toBe(marketAnalyticsKey(key));
  });

  it("не пропускает некорректные даты и числа в расчёты", () => {
    const source = item();
    source.points[13].closedMedian = Number.NaN;
    source.points[13].closedVolume = Number.POSITIVE_INFINITY;
    const summary = summarizeMarketAnalytics(source, asOf);
    expect(summary.days[13].price).toBeNull();
    expect(summary.days[13].volume).toBeNull();
    expect(summarizeMarketAnalytics(source, "2026-02-31").days).toEqual([]);
    expect(summarizeMarketAnalytics(source, "not-a-date").current.dailyVolume).toBeNull();
  });

  it("разрывает миниграфик на пропущенном дне и сохраняет календарные позиции", () => {
    const source = item();
    source.points = source.points.filter((point) => point.sourceDate !== "2026-09-02");
    const days = summarizeMarketAnalytics(source, asOf).days.slice(7);
    const chart = buildMarketSparkline(days, "price");
    expect(chart?.points).toHaveLength(6);
    expect(chart?.path.match(/M/g)).toHaveLength(2);
    expect(chart?.path).not.toMatch(/NaN|Infinity/);
    expect(chart?.points[0].x).toBe(2);
    expect(chart?.points.at(-1)?.x).toBe(94);
    expect(chart?.points[3].x! - chart?.points[2].x!).toBeCloseTo(2 * (92 / 6));
  });

  it("не рисует историю при отсутствии данных", () => {
    const source = { ...item(), points: [] };
    expect(buildMarketSparkline(summarizeMarketAnalytics(source, asOf).days, "volume")).toBeNull();
  });
});
