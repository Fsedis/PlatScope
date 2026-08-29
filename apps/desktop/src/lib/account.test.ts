import { describe, expect, it } from "vitest";

import {
  accountActionErrorMessage,
  createListingInput,
  createListingInputFromInventory,
  matchingSellOrder,
  orderEnglishName,
  validateListingNumbers,
  type AccountView,
} from "./account";
import type { InventoryViewItem } from "./inventory";
import type { MarketSearchRow } from "./market";

const row: MarketSearchRow = {
  itemId: "wfm-item-id",
  displayName: "Поток Прайм",
  itemKind: "standard",
  masteryRequirement: null,
  recommendation: {
    key: {
      slug: "primed_flow",
      platform: "pc",
      rank: 10,
      subtype: null,
      amberStars: null,
      cyanStars: null,
    },
    provider: "relics_run",
    sourceDate: "2026-08-26",
    fairPrice: 73,
    listPrice: 75,
    quickSell: 65,
    lowestAsk: null,
    depthThree: null,
    depthPrice: null,
    closedVolume: 52,
    liveSellOrderCount: 0,
    liveBuyOrderCount: 0,
    confidence: "medium",
    freshness: "fresh",
    reasons: [],
  },
};

describe("account listing drafts", () => {
  it("shows the English market name only when it adds useful context", () => {
    expect(orderEnglishName({
      slug: "primed_flow",
      displayName: "Поток Прайм",
      displayNameEn: "Primed Flow",
      imageUrl: null,
    })).toBe("Primed Flow");
    expect(orderEnglishName({
      slug: "primed_flow",
      displayName: "Primed Flow",
      displayNameEn: "Primed Flow",
      imageUrl: null,
    })).toBeNull();
  });

  it("preserves the exact selected market variant", () => {
    expect(createListingInput(row, 75, 1, false, null)).toMatchObject({
      itemId: "wfm-item-id",
      type: "sell",
      rank: 10,
      visible: false,
    });
  });

  it("rejects invalid bounds and bulk quantities", () => {
    expect(validateListingNumbers(0, 1, null)).not.toBeNull();
    expect(validateListingNumbers(10, 5, 2)).not.toBeNull();
    expect(validateListingNumbers(10, 6, 2)).toBeNull();
  });

  it("creates a sell draft directly from an exact inventory variant", () => {
    const item = inventoryItem();
    expect(createListingInputFromInventory(item, 75, 2, true, null)).toMatchObject({
      itemId: "wfm-item-id",
      type: "sell",
      platinum: 75,
      quantity: 2,
      visible: true,
      rank: 10,
    });
  });

  it("adds per-trade quantity for a bulk inventory item", () => {
    const item = { ...inventoryItem(), bulkTradable: true };
    expect(createListingInputFromInventory(item, 2, 15, true, 1)).toMatchObject({
      quantity: 15,
      perTrade: 1,
      rank: 10,
    });
  });

  it("turns known WFM failures into a useful instruction", () => {
    expect(accountActionErrorMessage("field perTrade is required", "ru"))
      .toContain("от 1 до 6");
    expect(accountActionErrorMessage("WFM returned HTTP 400 Bad Request", "ru"))
      .not.toContain("HTTP");
  });

  it("matches only the exact current sell order", () => {
    const item = inventoryItem();
    const account: AccountView = {
      connected: true,
      profile: null,
      orders: [
        {
          id: "wrong-rank",
          itemId: "wfm-item-id",
          type: "sell",
          platinum: 60,
          quantity: 1,
          perTrade: null,
          rank: 0,
          charges: null,
          subtype: null,
          amberStars: null,
          cyanStars: null,
          visible: true,
          createdAt: "2026-08-27T00:00:00Z",
          updatedAt: "2026-08-27T00:00:00Z",
        },
        {
          id: "exact",
          itemId: "wfm-item-id",
          type: "sell",
          platinum: 75,
          quantity: 2,
          perTrade: null,
          rank: 10,
          charges: null,
          subtype: null,
          amberStars: null,
          cyanStars: null,
          visible: true,
          createdAt: "2026-08-27T00:00:00Z",
          updatedAt: "2026-08-27T00:00:00Z",
        },
      ],
    };
    expect(matchingSellOrder(item, account)?.id).toBe("exact");
  });
});

function inventoryItem(): InventoryViewItem {
  return {
    canonicalGameId: "/Lotus/Upgrades/Mods/PrimedFlow",
    itemId: "wfm-item-id",
    bulkTradable: false,
    displayName: "Поток Прайм",
    tags: ["mod"],
    key: row.recommendation.key,
    rank: 10,
    subtype: null,
    ownedQuantity: 3,
    tradeableQuantity: 3,
    untradeableQuantity: 0,
    unknownQuantity: 0,
    leveledQuantity: 3,
    sellableQuantity: 2,
    resolution: "resolved",
    vaultStatus: "unknown",
    closedMedian48h: 73,
    hasReliablePrice: true,
  };
}
