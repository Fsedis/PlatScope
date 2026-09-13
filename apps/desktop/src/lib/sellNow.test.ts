import { describe, expect, it } from "vitest";

import type { InventoryViewItem } from "./inventory";
import type { PriceRecommendation } from "./market";
import { filterAndSortSellNowRows, resolveSellNowSelection, sellNowRowDomKey, sellPriorityRanks, sellNowRowIdentity, inventoryPage, isCheckedPriceCurrent, withCheckedPrice, summarizeInventoryRows, inventoryUnitPrice, type LiveSellNowResult, type SellNowFilters, type SellNowRow } from "./sellNow";

const filters: SellNowFilters = {
  query: "",
  category: "all",
  preset: "all",
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

  it("sellable filter does not hide items because of a trend or missing price", () => {
    const result = filterAndSortSellNowRows(
      [row("hold", 40, 20, "hold"), row("sell", 60, 20, "sell"), row("unpriced", 0, null, null)],
      { ...filters, preset: "sellable" },
    );
    expect(result.map((item) => item.inventory.canonicalGameId)).toEqual(["sell", "hold", "unpriced"]);
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

  it("has one equipment filter, while sellable includes spare equipped mod copies", () => {
    const free = row("free", 30, 10, "neutral");
    const equipped = row("equipped", 30, 10, "neutral");
    equipped.inventory.equippedQuantity = 1;
    const fullyEquipped = row("fully_equipped", 30, 10, "neutral");
    fullyEquipped.inventory.equippedQuantity = fullyEquipped.inventory.ownedQuantity;
    fullyEquipped.inventory.sellableQuantity = 0;

    expect(filterAndSortSellNowRows([free, equipped, fullyEquipped], {
      ...filters,
      preset: "sellable",
    })).toEqual([equipped, free]);
    expect(filterAndSortSellNowRows([free, equipped], {
      ...filters,
      preset: "equipped",
    })).toEqual([equipped]);
  });

  it("searches English slugs as words and treats ё, punctuation and word order consistently", () => {
    const flow = row("primed_flow", 30, 10, "neutral");
    flow.inventory.displayName = "Поток Прайм";
    const blueprint = row("akbronco_prime_blueprint", 20, 4, "neutral");
    blueprint.inventory.displayName = "Чертёж: Акбронко Прайм";
    for (const query of ["Primed Flow", "flow primed", "primed_flow", "прайм поток"]) {
      expect(filterAndSortSellNowRows([flow, blueprint], { ...filters, query })).toEqual([flow]);
    }
    expect(filterAndSortSellNowRows([flow, blueprint], { ...filters, query: "чертеж акбронко" })).toEqual([blueprint]);
    expect(filterAndSortSellNowRows([flow, blueprint], { ...filters, query: "поток акбронко" })).toEqual([]);
  });

  it("sorts by exactly the price shown and treats a live-only estimate as priced", () => {
    const lowFair = row("low_fair", 1, 5, null);
    lowFair.recommendation!.listPrice = 40;
    const highFair = row("high_fair", 1, 30, null);
    highFair.recommendation!.listPrice = 10;
    const liveOnly = row("live_only", 1, null, null);
    liveOnly.recommendation!.listPrice = 20;
    const missing = row("missing", 1, null, null);
    const rows = [missing, lowFair, highFair, liveOnly];
    expect(inventoryUnitPrice(liveOnly)).toBe(20);
    expect(filterAndSortSellNowRows(rows, { ...filters, sortKey: "fair" })).toEqual([lowFair, liveOnly, highFair, missing]);
    expect(filterAndSortSellNowRows(rows, { ...filters, sortKey: "fair", sortDirection: "asc" })).toEqual([highFair, liveOnly, lowFair, missing]);
    expect(filterAndSortSellNowRows(rows, { ...filters, preset: "unpriced" })).toEqual([missing]);
  });

  it("separates duplicates from sellability and keeps the reserve intact", () => {
    const reservedPair = row("pair", 1, 4, null);
    reservedPair.inventory.sellableQuantity = 0;
    const single = row("single", 1, 4, null);
    single.inventory.ownedQuantity = 1;
    const unmatched = row("unmatched", 1, 4, null);
    unmatched.inventory.resolution = "exact_variant_unavailable";
    const rows = [single, reservedPair, unmatched];
    expect(filterAndSortSellNowRows(rows, { ...filters, preset: "duplicates" })).toEqual([reservedPair, unmatched]);
    expect(filterAndSortSellNowRows(rows, { ...filters, preset: "sellable" })).toEqual([single]);
    expect(filterAndSortSellNowRows(rows, { ...filters, preset: "unavailable" })).toEqual([reservedPair, unmatched]);
    expect(reservedPair.inventory.sellableQuantity).toBe(0);
  });

  it("combines only the visible search, type and single filter", () => {
    const mod = row("primed_flow", 1, 4, null);
    mod.inventory.tags = ["mod"];
    mod.inventory.displayName = "Поток Прайм";
    const set = row("nyx_prime_set", 1, 4, null);
    set.inventory.tags = ["warframe", "set"];
    expect(filterAndSortSellNowRows([mod, set], { ...filters, query: "primed flow", category: "mod", preset: "sellable" })).toEqual([mod]);
    expect(filterAndSortSellNowRows([mod, set], { ...filters, query: "primed flow", category: "warframe", preset: "all" })).toEqual([]);
    expect(filterAndSortSellNowRows([mod, set], { ...filters, sortKey: "name", sortDirection: "asc" })).toHaveLength(2);
  });

  it("orders equal values consistently regardless of incoming snapshot order", () => {
    const second = row("same", 1, 4, null);
    second.inventory.key!.rank = 10;
    const first = row("same", 1, 4, null);
    first.inventory.key!.rank = 2;
    for (const sortDirection of ["asc", "desc"] as const) {
      expect(filterAndSortSellNowRows([second, first], { ...filters, sortKey: "fair", sortDirection })).toEqual([first, second]);
      expect(filterAndSortSellNowRows([first, second], { ...filters, sortKey: "fair", sortDirection })).toEqual([first, second]);
    }
  });

  it("keeps the checked item selected if a price change moves it off the current page", () => {
    const checked = row("checked", 1, 100, null);
    const first = row("first", 1, 2, null);
    expect(resolveSellNowSelection([first, checked], sellNowRowIdentity(checked), [first])).toBe(checked);
    expect(resolveSellNowSelection([first, checked], "", [checked])).toBe(checked);
    expect(resolveSellNowSelection([first], sellNowRowIdentity(checked), [first])).toBe(first);
  });

  it("paginates large inventories and clamps after filtering", () => {
    const rows = Array.from({ length: 1206 }, (_, i) => row(`part_${i}`, i, 5, "neutral"));
    const page = inventoryPage(rows, 2);
    expect(page.rows).toEqual(rows.slice(50, 100));
    expect([page.from, page.to, page.count]).toEqual([51, 100, 25]);
    expect(inventoryPage(rows, 99).rows).toHaveLength(6);
    expect(inventoryPage(rows.slice(0, 3), 25).page).toBe(1);
    expect(inventoryPage([], -1)).toMatchObject({ from: 0, to: 0, count: 1, page: 1, rows: [] });
    expect(inventoryPage(rows, NaN, 0).rows).toHaveLength(1);
  });

  function quote(quotedRow: SellNowRow): LiveSellNowResult {
    return { row: quotedRow, fetchedAt: "2026-09-06T12:00:00Z", quoteState: "network", sellOrderCount: 2, buyOrderCount: 1, orders: [], warning: null };
  }

  it("retains a checked price across inventory refresh without restoring old quantities or priority", () => {
    const checked = quote(row("flow", 80, 40, "peak"));
    checked.row.inventory.sellableQuantity = 10;
    const freshInventory = row("flow", 20, 30, "hold");
    freshInventory.inventory.sellableQuantity = 2;
    const merged = withCheckedPrice(freshInventory, checked, Date.parse(checked.fetchedAt) + 10_000);
    expect(merged.recommendation?.fairPrice).toBe(40);
    expect(merged.inventory).toBe(freshInventory.inventory);
    expect(merged.nominalValue).toBe(80);
    expect(merged.priority).toBe(freshInventory.priority);
    expect(merged.trend).toBe(freshInventory.trend);
    freshInventory.inventory.sellableQuantity = 0;
    expect(withCheckedPrice(freshInventory, checked, Date.parse(checked.fetchedAt)).nominalValue).toBe(0);
  });

  it("rejects stale, future, invalid and different-variant checked prices", () => {
    const original = row("flow", 20, 30, "hold");
    const checked = quote(row("flow", 80, 40, "peak"));
    const now = Date.parse(checked.fetchedAt);
    expect(isCheckedPriceCurrent(checked, now + 89_999)).toBe(true);
    for (const time of [now + 90_001, now - 1, NaN]) {
      expect(withCheckedPrice(original, checked, time)).toBe(original);
    }
    expect(withCheckedPrice(original, { ...checked, quoteState: "stale_cache" }, now)).toBe(original);
    expect(withCheckedPrice(original, { ...checked, fetchedAt: "bad date" }, now)).toBe(original);
    expect(isCheckedPriceCurrent(checked, now + 120_000, 300)).toBe(true);
    expect(isCheckedPriceCurrent(checked, now + 30_001, 30)).toBe(false);
    checked.row.inventory.key!.rank = 10;
    expect(withCheckedPrice(original, checked, now)).toBe(original);
  });

  it("counts only sellable entries with known prices in the estimate", () => {
    const priced = row("priced", 60, 10, "sell");
    const reserved = row("reserved", 60, 100, "sell");
    reserved.inventory.sellableQuantity = 0;
    const missing = row("missing", 0, null, null);
    expect(summarizeInventoryRows([priced, reserved, missing])).toEqual({ candidateRows: 2, pricedRows: 1, highPriorityRows: 1, nominalValue: 10 });
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
});
