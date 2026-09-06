import { describe, expect, it } from "vitest";
import { buyListingInput, existingBuyOrder } from "./marketBuyOrder";
import type { MarketSearchRow } from "./market";
import type { AccountOrder } from "./account";

const row: MarketSearchRow = {
  itemId: "item-id", displayName: "Предмет", displayNameEn: "Item", itemKind: "standard", masteryRequirement: null,
  recommendation: {
    key: { slug: "item", platform: "pc", rank: null, charges: null, subtype: null, amberStars: null, cyanStars: null },
    provider: "relics_run", sourceDate: "2026-09-06", fairPrice: 20, listPrice: 21, quickSell: 18, lowestAsk: null,
    depthThree: null, depthPrice: null, closedVolume: 5, liveSellOrderCount: 0, liveBuyOrderCount: 0,
    confidence: "medium", freshness: "fresh", reasons: [],
  },
};

describe("заявка на покупку из поиска", () => {
  it("сохраняет точные ранг, заряды, подтип и установленные звёзды", () => {
    const variants = [
      { rank: 0, charges: null, subtype: null, amberStars: null, cyanStars: null },
      { rank: 10, charges: null, subtype: null, amberStars: null, cyanStars: null },
      { rank: null, charges: 3, subtype: null, amberStars: null, cyanStars: null },
      { rank: null, charges: null, subtype: "radiant", amberStars: null, cyanStars: null },
      { rank: null, charges: null, subtype: null, amberStars: 0, cyanStars: 2 },
    ];
    for (const variant of variants) {
      const input = buyListingInput({ ...row, recommendation: { ...row.recommendation, key: { ...row.recommendation.key, ...variant } } }, 35, 6, false);
      expect(input).toMatchObject({ ...variant, itemId: "item-id", type: "buy", platinum: 35, quantity: 6, visible: false, perTrade: null });
    }
  });

  it("отличает уже созданную покупку от продажи и других вариантов предмета", () => {
    const order: AccountOrder = { ...buyListingInput(row, 30, 1, true), id: "order", createdAt: "", updatedAt: "" };
    expect(existingBuyOrder(row, [{ ...order, type: "sell" }, { ...order, rank: 0 }, { ...order, charges: 1 }])).toBeNull();
    expect(existingBuyOrder(row, [{ ...order, visible: false }])?.id).toBe("order");
  });
});
