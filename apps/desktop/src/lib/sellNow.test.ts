import { describe, expect, it } from "vitest";

import type { InventoryViewItem } from "./inventory";
import type { PriceRecommendation } from "./market";
import {
  filterAndSortSellNowRows,
  priorityReasonMessages,
  resolveSellNowSelection,
  sellNowRowDomKey,
  sellPriorityRanks,
  sellNowRowIdentity,
  type SellNowFilters,
  type SellNowRow,
} from "./sellNow";

const filters: SellNowFilters = {
  query: "",
  category: "all",
  preset: "all",
  equipped: "all",
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
    charges: null,
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
    equippedQuantity: 0,
    equippedPlacements: [],
    sellableQuantity: 1,
    resolution: "resolved",
    vaultStatus: "unknown",
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

  it("shows a stable queue position instead of collapsing scores into one band", () => {
    const first = row("first", 48, 40, "sell");
    const tied = row("tied", 48, 35, "sell");
    const third = row("third", 41, 30, "sell");
    const unavailable = row("unavailable", 0, null, null);

    const ranks = sellPriorityRanks([third, unavailable, tied, first]);

    expect(ranks.get(sellNowRowIdentity(first))).toBe(1);
    expect(ranks.get(sellNowRowIdentity(tied))).toBe(1);
    expect(ranks.get(sellNowRowIdentity(third))).toBe(3);
    expect(ranks.has(sellNowRowIdentity(unavailable))).toBe(false);
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

  it("keeps missing numeric values last in both directions", () => {
    const unpriced = row("missing", 0, null, null);
    const priced = row("priced", 10, 5, "neutral");
    for (const sortDirection of ["asc", "desc"] as const) {
      const result = filterAndSortSellNowRows([unpriced, priced], {
        ...filters,
        sortKey: "fair",
        sortDirection,
      });
      expect(result.at(-1)).toBe(unpriced);
    }
  });

  it("sell now preset requires priced SELL or PEAK timing", () => {
    const result = filterAndSortSellNowRows(
      [row("hold", 40, 20, "hold"), row("sell", 60, 20, "sell")],
      { ...filters, preset: "sell_now" },
    );
    expect(result.map((item) => item.inventory.canonicalGameId)).toEqual(["sell"]);
  });

  it("uses one row set for selling and full inventory views", () => {
    const sellable = row("sellable", 60, 20, "sell");
    const reserved = row("reserved", 0, 10, "neutral");
    reserved.inventory.sellableQuantity = 0;
    const attention = row("attention", 0, null, null);
    attention.inventory.sellableQuantity = 0;
    attention.inventory.resolution = "exact_variant_unavailable";

    expect(filterAndSortSellNowRows([sellable, reserved, attention], {
      ...filters,
      preset: "sellable",
    })).toEqual([sellable]);
    expect(filterAndSortSellNowRows([sellable, reserved, attention], {
      ...filters,
      preset: "all",
    })).toHaveLength(3);
    expect(filterAndSortSellNowRows([sellable, reserved, attention], {
      ...filters,
      preset: "attention",
    })).toEqual([attention]);
  });

  it("filters equipped rows independently from sell recommendations", () => {
    const free = row("free", 30, 10, "neutral");
    const equipped = row("equipped", 30, 10, "neutral");
    equipped.inventory.equippedQuantity = 1;

    expect(filterAndSortSellNowRows([free, equipped], {
      ...filters,
      equipped: "free",
    })).toEqual([free]);
    expect(filterAndSortSellNowRows([free, equipped], {
      ...filters,
      equipped: "equipped",
    })).toEqual([equipped]);
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

  it("switches selection to the first row of the newly filtered item type", () => {
    const relic = row("lith_a1_relic", 40, 20, "neutral");
    relic.inventory.tags = ["relic", "lith"];
    const mod = row("primed_flow", 40, 20, "neutral");
    mod.inventory.tags = ["mod"];

    const visibleMods = filterAndSortSellNowRows([relic, mod], {
      ...filters,
      category: "mod",
    });

    expect(resolveSellNowSelection(visibleMods, sellNowRowIdentity(relic))).toBe(mod);
  });

  it("keeps DOM keys unique for unresolved duplicate inventory rows", () => {
    const first = row("veiled_melee_riven_mod", 0, 0, "neutral");
    const second = row("veiled_melee_riven_mod", 0, 0, "neutral");
    first.inventory.key = null;
    second.inventory.key = null;
    first.inventory.canonicalGameId = "/Lotus/Upgrades/Mods/Randomized/PlayerMeleeWeaponRandomModRare";
    second.inventory.canonicalGameId = first.inventory.canonicalGameId;

    expect(sellNowRowIdentity(first)).toBe(sellNowRowIdentity(second));
    expect(sellNowRowDomKey(first, 0)).not.toBe(sellNowRowDomKey(second, 1));
  });

  it("identity preserves exact rank", () => {
    const ranked = row("primed_flow", 40, 70, "neutral");
    ranked.inventory.key!.rank = 10;
    expect(sellNowRowIdentity(ranked)).toContain(":10:");
  });

  it("builds English priority explanations from typed factors", () => {
    const result = priorityReasonMessages(row("flow", 70, 40, "sell"), "en");
    expect(result).toHaveLength(4);
    expect(result.join(" ")).toContain("shows listing order");
    expect(result.join(" ")).not.toContain("70/100");
  });

  it("explains the visible queue position", () => {
    const result = priorityReasonMessages(row("flow", 48, 40, "sell"), "ru", 1);
    expect(result[0]).toContain("№1");
    expect(result[0]).toContain("Чем меньше номер");
  });
});
