import { describe, expect, it } from "vitest";

import {
  filterAndSortRows,
  formatPlatinum,
  liveQuoteLabel,
  liveUserStatusLabel,
  masteryRequirementLabel,
  priceReasonMessage,
  variantLabel,
  type MarketSearchRow,
} from "./market";

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
  it("отличает нулевое требование мастерства от отсутствующих метаданных", () => {
    expect(masteryRequirementLabel(0)).toBe("MR 0");
    expect(masteryRequirementLabel(12, "en")).toBe("MR 12");
    expect(masteryRequirementLabel(null)).toBe("Нет данных");
    expect(masteryRequirementLabel(null, "en")).toBe("No data");
  });

  it("локализует состояние игрока в live order book", () => {
    expect(liveUserStatusLabel("in_game")).toBe("В игре");
    expect(liveUserStatusLabel("online", "en")).toBe("Online");
  });

  it("localizes stable live pricing reason codes", () => {
    expect(priceReasonMessage({ code: "live_market_agreement", message: "fixture" }, "en"))
      .toBe("Live orders agree with the bulk estimate.");
    expect(priceReasonMessage({ code: "live_top_buy", message: "fixture" }, "en"))
      .toBe("Quick Sell uses the best active buy order for the exact variant.");
  });

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

  it("объясняет точный вариант", () => {
    expect(
      variantLabel({
        slug: "axi_test_relic",
        platform: "pc",
        rank: null,
        charges: null,
        subtype: "radiant",
        amberStars: null,
        cyanStars: null,
      }),
    ).toBe("radiant");
  });

  it("показывает число зарядов точного варианта", () => {
    expect(variantLabel({
      slug: "charged_item",
      platform: "pc",
      rank: null,
      charges: 3,
      subtype: null,
      amberStars: null,
      cyanStars: null,
    })).toContain("заряды 3");
  });

  it("явно помечает stale live-кэш", () => {
    expect(liveQuoteLabel("stale_cache")).toBe("Сохранённые ордера могли устареть");
  });

  it("localizes price explanations and live-cache state by stable codes", () => {
    expect(liveQuoteLabel("stale_cache", "en")).toBe("Saved orders may be outdated");
    expect(priceReasonMessage({ code: "source_fresh", message: "Свежие данные" }, "en"))
      .toBe("The bulk snapshot is fresh.");
    expect(priceReasonMessage({ code: "riven_pricing_unsupported", message: "Нет оценки" }, "en"))
      .toContain("separate model");
  });
});
