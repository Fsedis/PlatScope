import { describe, expect, it } from "vitest";

import { bestSellRows, dashboardSummary, liquidityBand, rowsWorthChecking } from "./dashboard";
import type { InventoryView, InventoryViewItem } from "./inventory";
import type { PriceRecommendation } from "./market";
import type { SellNowRow, SellNowView } from "./sellNow";

function row(slug: string, score: number, fairPrice: number | null, volume: number): SellNowRow {
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
    closedVolume: volume,
    liveSellOrderCount: 0,
    liveBuyOrderCount: 0,
    confidence: fairPrice === null ? "unknown" : "medium",
    freshness: "fresh",
    reasons: [],
  } satisfies PriceRecommendation;
  return {
    inventory,
    itemKind: "standard",
    recommendation,
    trend: null,
    priority: {
      score,
      band: score >= 50 ? "high" : "low",
      factors: {
        quantity: 0.5,
        price: 0.5,
        liquidity: 0.5,
        confidenceMultiplier: 1,
        timingMultiplier: 1,
      },
      reasons: [],
    },
    nominalValue: fairPrice,
  };
}

const inventory: InventoryView = {
  metadata: {
    source: "test_fixture",
    observedAt: "2026-08-27T08:30:00Z",
    schemaVersion: 1,
    itemCount: 3,
    checksumSha256: "test",
  },
  keepCopies: 1,
  summary: { ownedQuantity: 7, sellableQuantity: 4, resolvedRows: 2, attentionRows: 1 },
  items: [],
};

describe("dashboard presentation", () => {
  it("keeps nominal value explicitly limited to sellable candidates", () => {
    const sellNow = {
      summary: {
        candidateRows: 3,
        pricedRows: 2,
        highPriorityRows: 1,
        inventoryNominalValue: 910,
        nominalValue: 320,
      },
    } as SellNowView;
    expect(dashboardSummary(inventory, sellNow)).toEqual({
      ownedCopies: 7,
      sellableCopies: 4,
      nominalInventoryValue: 910,
      nominalSellableValue: 320,
      attentionRows: 2,
      pricedCoveragePercent: 67,
    });
  });

  it("ranks best candidates and exposes thin markets", () => {
    const rows = [
      row("thin", 70, 40, 2),
      row("active", 80, 80, 30),
      row("missing", 0, null, 0),
    ];
    expect(bestSellRows(rows, 2).map((candidate) => candidate.inventory.canonicalGameId)).toEqual([
      "active",
      "thin",
    ]);
    expect(liquidityBand(rows[0])).toBe("thin");
    expect(rowsWorthChecking(rows).map((candidate) => candidate.inventory.canonicalGameId)).toEqual([
      "missing",
      "thin",
    ]);
  });
});
