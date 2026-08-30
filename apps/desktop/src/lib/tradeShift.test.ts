import { describe, expect, it } from "vitest";

import type { AccountView } from "./account";
import type { InventoryView } from "./inventory";
import type { PriceRecommendation } from "./market";
import {
  buildTradeShiftRows,
  pendingSaleEvents,
  normalizeTradeName,
  planTradeReconciliation,
  recommendationIdentity,
  visibleTradeHistory,
  type TradeEvent,
} from "./tradeShift";

const order = {
  id: "order-1", itemId: "item-1", type: "sell" as const, platinum: 140,
  quantity: 3, perTrade: null, rank: null, charges: null, subtype: null,
  amberStars: null, cyanStars: null, visible: true,
  createdAt: "2026-08-20T00:00:00Z", updatedAt: "2026-08-29T00:00:00Z",
};
const account: AccountView = {
  connected: true,
  profile: { id: "u", ingameName: "Tenno", slug: "tenno", platform: "pc", crossplay: true, verification: true },
  orders: [order],
  orderItems: {
    "item-1": { slug: "primed_flow", displayName: "Прайм Поток", displayNameEn: "Primed Flow", imageUrl: null, itemKind: "standard" },
  },
};
const inventory: InventoryView = {
  metadata: { source: "read_only_scan", observedAt: "2026-08-29T00:00:00Z", schemaVersion: 1, itemCount: 1, checksumSha256: "x" },
  keepCopies: 1,
  summary: { ownedQuantity: 3, sellableQuantity: 2, resolvedRows: 1, attentionRows: 0 },
  items: [{
    canonicalGameId: "Primed Flow", itemId: "item-1", bulkTradable: false,
    displayName: "Прайм Поток", tags: ["mod"],
    key: { slug: "primed_flow", platform: "pc", rank: null, subtype: null, amberStars: null, cyanStars: null },
    rank: null, subtype: null, ownedQuantity: 3, tradeableQuantity: 3,
    untradeableQuantity: 0, unknownQuantity: 0, leveledQuantity: 0,
    sellableQuantity: 2, resolution: "resolved", vaultStatus: "unknown",
    closedMedian48h: 130, hasReliablePrice: true,
  }],
};

function recommendation(): PriceRecommendation {
  return {
    key: { slug: "primed_flow", platform: "pc", rank: null, subtype: null, amberStars: null, cyanStars: null },
    provider: "warframe_market", sourceDate: "2026-08-29", fairPrice: 130,
    listPrice: 128, quickSell: 120, lowestAsk: 128, depthThree: 129,
    depthPrice: 130, closedVolume: 50, liveSellOrderCount: 20,
    liveBuyOrderCount: 10, confidence: "high", freshness: "fresh", reasons: [],
  };
}

describe("торговая смена", () => {
  it("ставит расхождение количества выше проверки цены", () => {
    const quote = recommendation();
    const rows = buildTradeShiftRows(account, inventory, new Map([[recommendationIdentity(quote.key), quote]]), new Date("2026-08-29T12:00:00Z"));
    expect(rows[0].health).toBe("inventory_mismatch");
    expect(rows[0].suggestedQuantity).toBe(2);
  });

  it("сопоставляет подтверждённую продажу только с одним безопасным ордером", () => {
    const event: TradeEvent = {
      id: 1, occurredAt: "2026-08-29T12:00:00Z", partner: "Buyer",
      platinumGiven: 0, platinumReceived: 260,
      givenItems: [{ name: "Primed Flow", quantity: 2 }], receivedItems: [],
      status: "pending", matchedOrderId: null, reconciliationJson: null,
    };
    const plan = planTradeReconciliation(event, account);
    expect(plan.unmatched).toEqual([]);
    expect(plan.unsafe).toEqual([]);
    expect(plan.actions[0]).toMatchObject({ kind: "update", soldQuantity: 2 });
  });

  it("не угадывает ранговый вариант", () => {
    const ranked = { ...account, orders: [{ ...order, rank: 10 }] };
    const event: TradeEvent = {
      id: 1, occurredAt: "2026-08-29T12:00:00Z", partner: null,
      platinumGiven: 0, platinumReceived: 100,
      givenItems: [{ name: "Primed Flow", quantity: 1 }], receivedItems: [],
      status: "pending", matchedOrderId: null, reconciliationJson: null,
    };
    expect(planTradeReconciliation(event, ranked).unsafe).toHaveLength(1);
  });

  it("не применяет сделку повторно к уже изменённому ордеру", () => {
    const changedAfterTrade = {
      ...account,
      orders: [{ ...order, updatedAt: "2026-08-29T13:00:00Z" }],
    };
    const event: TradeEvent = {
      id: 1, occurredAt: "2026-08-29T12:00:00Z", partner: null,
      platinumGiven: 0, platinumReceived: 130,
      givenItems: [{ name: "Primed Flow", quantity: 1 }], receivedItems: [],
      status: "pending", matchedOrderId: null, reconciliationJson: null,
    };
    expect(planTradeReconciliation(event, changedAfterTrade).unsafe).toHaveLength(1);
  });

  it("нормализует апострофы и пробелы в английских именах", () => {
    expect(normalizeTradeName("  Tenno’s   Item ")).toBe("tennos item");
  });

  it("показывает покупки и обмены в истории, не отправляя их на сверку ордера", () => {
    const sale: TradeEvent = {
      id: 1, occurredAt: "2026-08-29T12:00:00Z", partner: "Buyer",
      platinumGiven: 0, platinumReceived: 130,
      givenItems: [{ name: "Primed Flow", quantity: 1 }], receivedItems: [],
      status: "pending", matchedOrderId: null, reconciliationJson: null,
    };
    const purchase: TradeEvent = {
      id: 2, occurredAt: "2026-08-29T12:01:00Z", partner: "Seller",
      platinumGiven: 40, platinumReceived: 0,
      givenItems: [], receivedItems: [{ name: "Ash Prime Blueprint", quantity: 1 }],
      status: "pending", matchedOrderId: null, reconciliationJson: null,
    };
    const barter: TradeEvent = {
      id: 3, occurredAt: "2026-08-29T12:02:00Z", partner: "Trader",
      platinumGiven: 0, platinumReceived: 0,
      givenItems: [{ name: "Item A", quantity: 1 }],
      receivedItems: [{ name: "Item B", quantity: 1 }],
      status: "pending", matchedOrderId: null, reconciliationJson: null,
    };

    expect(pendingSaleEvents([sale, purchase, barter])).toEqual([sale]);
    expect(visibleTradeHistory([barter, purchase, sale])).toEqual([barter, purchase]);
  });
});
