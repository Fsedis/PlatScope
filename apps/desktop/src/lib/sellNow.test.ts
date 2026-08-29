import { describe, expect, it } from "vitest";

import type { InventoryViewItem } from "./inventory";
import type { PriceRecommendation } from "./market";
import {
  filterAndSortSellNowRows,
  priorityReasonMessages,
  sellNowRowIdentity,
  type SellNowFilters,
  type SellNowRow,
} from "./sellNow";

const filters: SellNowFilters = {
  query: "",
  category: "all",
  preset: "all",
  confidence: "all",
  timing: "all",
  sortKey: "priority",
  sortDirection: "desc",
};

function row(
  slug: string,
  score: number,
  fairPrice: number | null,
  timing: "hold" | "neutral" | "sell" | "peak" | null,
): SellNowRow {
  const key = {
    slug,
    platform: "pc" as const,
    rank: null,
    subtype: null,
    amberStars: null,
    cyanStars: null,
  };
  const inventory: InventoryViewItem = {
    canonicalGameId: slug,
    itemId: `wfm-${slug}`,
    bulkTradable: false,
    displayName: slug,
    tags: ["component"],
    key,
    rank: null,
    subtype: null,
    ownedQuantity: 2,
    tradeableQuantity: 2,
    untradeableQuantity: 0,
    unknownQuantity: 0,
    leveledQuantity: 0,
    sellableQuantity: 1,
    resolution: "resolved",
    vaultStatus: "unknown",
    closedMedian48h: fairPrice,
    hasReliablePrice: fairPrice !== null,
  };
  const recommendation = {
    key,
    provider: "relics_run",
    sourceDate: "2026-08-26",
    fairPrice,
    listPrice: fairPrice,
    quickSell: null,
    lowestAsk: null,
    depthThree: null,
    depthPrice: null,
    closedVolume: 12,
    liveSellOrderCount: 0,
    liveBuyOrderCount: 0,
    confidence: fairPrice === null ? "unknown" : "high",
    freshness: "fresh",
    reasons: [],
  } satisfies PriceRecommendation;
  return {
    inventory,
    itemKind: "standard",
    recommendation,
    trend: {
      median7d: fairPrice,
      median30d: fairPrice,
      median90d: fairPrice,
      change7d: 5,
      change30d: null,
      change90d: 5,
      volumeAvg7d: 12,
      volumeAvg30d: null,
      volumeAvg90d: 12,
      historicalLow: fairPrice,
      historicalHigh: fairPrice,
      timing,
      trustedDays: 7,
    },
    priority: {
      score,
      band: score >= 50 ? "high" : score > 0 ? "low" : "none",
      factors: {
        quantity: 0.2,
        price: 0.4,
        liquidity: 0.5,
        confidenceMultiplier: 1,
        timingMultiplier: 1,
      },
      reasons: [],
    },
    nominalValue: fairPrice,
  };
}

describe("sell now presentation", () => {
  it("sorts by priority descending", () => {
    const result = filterAndSortSellNowRows(
      [row("low", 10, 5, "neutral"), row("high", 70, 40, "sell")],
      filters,
    );
    expect(result.map((item) => item.inventory.canonicalGameId)).toEqual(["high", "low"]);
  });

  it("sorts trend by the robust 90-day price change", () => {
    const stronger90d = row("stronger_90d", 10, 5, "neutral");
    stronger90d.trend!.change7d = -20;
    stronger90d.trend!.change90d = 30;
    const stronger7d = row("stronger_7d", 10, 5, "neutral");
    stronger7d.trend!.change7d = 40;
    stronger7d.trend!.change90d = 10;

    const result = filterAndSortSellNowRows([stronger7d, stronger90d], {
      ...filters,
      sortKey: "trend",
      sortDirection: "desc",
    });

    expect(result.map((item) => item.inventory.canonicalGameId)).toEqual([
      "stronger_90d",
      "stronger_7d",
    ]);
  });

  it("sorts sales volume independently from price growth", () => {
    const liquid = row("liquid", 10, 5, "neutral");
    liquid.recommendation!.closedVolume = 80;
    liquid.trend!.change90d = -10;
    const growing = row("growing", 10, 5, "hold");
    growing.recommendation!.closedVolume = 12;
    growing.trend!.change90d = 30;

    const bySales = filterAndSortSellNowRows([growing, liquid], {
      ...filters,
      sortKey: "volume",
      sortDirection: "desc",
    });
    const byGrowth = filterAndSortSellNowRows([liquid, growing], {
      ...filters,
      sortKey: "trend",
      sortDirection: "desc",
    });

    expect(bySales.map((item) => item.inventory.canonicalGameId)).toEqual([
      "liquid",
      "growing",
    ]);
    expect(byGrowth.map((item) => item.inventory.canonicalGameId)).toEqual([
      "growing",
      "liquid",
    ]);
  });

  it("keeps unpriced rows explicit", () => {
    const unpriced = row("missing", 0, null, null);
    expect(
      filterAndSortSellNowRows([unpriced], { ...filters, preset: "unpriced" }),
    ).toEqual([unpriced]);
  });

  it("sell now preset requires priced SELL or PEAK timing", () => {
    const result = filterAndSortSellNowRows(
      [row("hold", 40, 20, "hold"), row("sell", 60, 20, "sell")],
      { ...filters, preset: "sell_now" },
    );
    expect(result.map((item) => item.inventory.canonicalGameId)).toEqual(["sell"]);
  });

  it("filters mutually exclusive item types", () => {
    const warframeMod = row("warframe_mod", 40, 20, "neutral");
    warframeMod.inventory.tags = ["mod", "warframe", "rare"];
    const warframeSet = row("warframe_set", 40, 20, "neutral");
    warframeSet.inventory.tags = ["set", "prime", "warframe"];

    expect(
      filterAndSortSellNowRows([warframeMod, warframeSet], {
        ...filters,
        category: "mod",
      }).map((item) => item.inventory.canonicalGameId),
    ).toEqual(["warframe_mod"]);
    expect(
      filterAndSortSellNowRows([warframeMod, warframeSet], {
        ...filters,
        category: "warframe",
      }).map((item) => item.inventory.canonicalGameId),
    ).toEqual(["warframe_set"]);
  });

  it("identity preserves exact rank", () => {
    const ranked = row("primed_flow", 40, 70, "neutral");
    ranked.inventory.key!.rank = 10;
    expect(sellNowRowIdentity(ranked)).toContain(":10:");
  });

  it("builds English priority explanations from typed factors", () => {
    const result = priorityReasonMessages(row("flow", 70, 40, "sell"), "en");
    expect(result).toHaveLength(4);
    expect(result.join(" ")).toContain("helps choose what to review first");
    expect(result.join(" ")).not.toContain("70/100");
  });
});
