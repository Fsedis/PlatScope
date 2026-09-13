import { describe, expect, it } from "vitest";

import { filterAndSortRows, formatPlatinum, type MarketSearchRow } from "./market";

function row(name: string, fair: number | null, volume: number): MarketSearchRow {
  return {
    itemId: name,
    displayName: name,
    itemKind: "standard",
    masteryRequirement: null,
    recommendation: {
      key: {
        slug: name.toLowerCase(),
        platform: "pc",
        rank: null,
        charges: null,
        subtype: null,
        amberStars: null,
        cyanStars: null,
      },
      provider: "relics_run",
      sourceDate: "2026-08-26",
      fairPrice: fair,
      listPrice: fair,
      quickSell: null,
      lowestAsk: null,
      depthThree: null,
      depthPrice: null,
      closedVolume: volume,
      liveSellOrderCount: 0,
      liveBuyOrderCount: 0,
      confidence: fair === null ? "unknown" : "medium",
      freshness: "fresh",
      reasons: [],
    },
  };
}

describe("market presentation helpers", () => {

  it("не превращает отсутствие цены в 0p", () => {
    expect(formatPlatinum(null)).toBe("—");
    expect(formatPlatinum(12.5)).toBe("12,5p");
  });

  it("сохраняет неизвестные цены в фильтре без цены", () => {
    const rows = [row("Known", 10, 5), row("Unknown", null, 0)];
    expect(filterAndSortRows(rows, "unpriced", "name", "asc")).toHaveLength(1);
    expect(filterAndSortRows(rows, "unpriced", "name", "asc")[0]?.displayName).toBe(
      "Unknown",
    );
  });

  it("при любой сортировке оставляет отсутствующие числа внизу", () => {
    const rows = [row("Unknown", null, 0), row("Known", 10, 5)];
    expect(filterAndSortRows(rows, "all", "fair", "asc").at(-1)?.displayName).toBe("Unknown");
    expect(filterAndSortRows(rows, "all", "fair", "desc").at(-1)?.displayName).toBe("Unknown");
  });

  it("сортирует числовые столбцы, не смешивая строковые значения", () => {
    const rows = [row("Low", 5, 2), row("High", 20, 10)];
    expect(filterAndSortRows(rows, "all", "fair", "desc")[0]?.displayName).toBe(
      "High",
    );
  });
});
