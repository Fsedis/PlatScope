import { describe, expect, it } from "vitest";

import type { AccountView } from "./account";
import type { InventoryView } from "./inventory";
import type { PriceRecommendation } from "./market";
import {
  applyPriceCheckFailures,
  buildTradeShiftRows,
  filterTradeShiftRows,
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
  modUsageScanned: false,
  summary: { ownedQuantity: 3, sellableQuantity: 2, resolvedRows: 1, attentionRows: 0 },
  items: [{
    canonicalGameId: "Primed Flow", itemId: "item-1", bulkTradable: false,
    displayName: "Прайм Поток", tags: ["mod"],
    key: { slug: "primed_flow", platform: "pc", rank: null, charges: null, subtype: null, amberStars: null, cyanStars: null },
    rank: null, subtype: null, ownedQuantity: 3, tradeableQuantity: 3,
    untradeableQuantity: 0, unknownQuantity: 0, leveledQuantity: 0,
    equippedQuantity: 0, equippedPlacements: [],
    sellableQuantity: 2, resolution: "resolved", vaultStatus: "unknown",
  }],
};

function recommendation(): PriceRecommendation {
  return {
    key: { slug: "primed_flow", platform: "pc", rank: null, charges: null, subtype: null, amberStars: null, cyanStars: null },
    provider: "warframe_market", sourceDate: "2026-08-29", fairPrice: 130,
    listPrice: 128, quickSell: 120, lowestAsk: 128, depthThree: 129,
    depthPrice: 130, closedVolume: 50, liveSellOrderCount: 20,
    liveBuyOrderCount: 10, confidence: "high", freshness: "fresh", reasons: [],
  };
}

describe("торговая смена", () => {
  it("не выдаёт возраст публикации ордера за возраст проверки цены", () => {
    const quote = { ...recommendation(), listPrice: 140, fairPrice: 140 };
    const matchingInventory = {
      ...inventory,
      items: [{ ...inventory.items[0], sellableQuantity: 3 }],
    };
    const rows = buildTradeShiftRows(
      account,
      matchingInventory,
      new Map([[recommendationIdentity(quote.key), quote]]),
      new Date("2026-09-04T12:00:00Z"),
    );

    expect(rows[0].health).toBe("healthy");
  });

  it("не предлагает менять ордер по цене, которую не удалось проверить", () => {
    const quote = { ...recommendation(), listPrice: 100, fairPrice: 100 };
    const matchingInventory = {
      ...inventory,
      items: [{ ...inventory.items[0], sellableQuantity: 3 }],
    };
    const rows = buildTradeShiftRows(
      account,
      matchingInventory,
      new Map([[recommendationIdentity(quote.key), quote]]),
    );
    const failed = applyPriceCheckFailures(
      rows,
      new Set([recommendationIdentity(quote.key)]),
    );

    expect(rows[0].health).toBe("overpriced");
    expect(failed[0]).toMatchObject({
      health: "price_check_failed",
      recommendation: null,
      suggestedPrice: null,
      needsAction: true,
      priceCheckFailed: true,
    });
  });

  it("сбой цены не скрывает лишнее количество в ордере", () => {
    const quote = recommendation();
    const rows = buildTradeShiftRows(account, inventory, new Map([[recommendationIdentity(quote.key), quote]]));
    const failed = applyPriceCheckFailures(rows, new Set([recommendationIdentity(quote.key)]));
    expect(failed[0]).toMatchObject({
      health: "inventory_mismatch", suggestedQuantity: 2, suggestedPrice: null,
      recommendation: null, needsAction: true, priceCheckFailed: true,
    });
    expect(rows[0].priceCheckFailed).toBeUndefined();
  });

  it("ищет ордера по русскому и английскому названию", () => {
    const rows = buildTradeShiftRows(account, inventory, new Map());

    expect(filterTradeShiftRows(rows, "Прайм Поток")).toHaveLength(1);
    expect(filterTradeShiftRows(rows, "primed flow")).toHaveLength(1);
    expect(filterTradeShiftRows(rows, "поток prime")).toHaveLength(1);
    expect(filterTradeShiftRows(rows, "Рино")).toHaveLength(0);
  });

  it("ставит расхождение количества выше проверки цены", () => {
    const quote = recommendation();
    const rows = buildTradeShiftRows(account, inventory, new Map([[recommendationIdentity(quote.key), quote]]), new Date("2026-08-29T12:00:00Z"));
    expect(rows[0].health).toBe("inventory_mismatch");
    expect(rows[0].suggestedQuantity).toBe(2);
  });

  it("суммирует одинаковые точные варианты инвентаря", () => {
    const quote = recommendation();
    const splitInventory: InventoryView = {
      ...inventory,
      items: [
        { ...inventory.items[0], canonicalGameId: "stack-a", sellableQuantity: 1 },
        { ...inventory.items[0], canonicalGameId: "stack-b", sellableQuantity: 2 },
      ],
    };
    const rows = buildTradeShiftRows(
      account,
      splitInventory,
      new Map([[recommendationIdentity(quote.key), quote]]),
      new Date("2026-08-29T12:00:00Z"),
    );
    expect(rows[0].health).not.toBe("inventory_mismatch");
    expect(rows[0].inventory?.sellableQuantity).toBe(3);
  });

  it("считает полный комплект по доступным деталям, а не по несуществующей строке инвентаря сета", () => {
    const setOrder = { ...order, id: "set-order", itemId: "set-id", quantity: 1 };
    const setAccount: AccountView = {
      ...account,
      orders: [setOrder],
      orderItems: {
        "set-id": {
          slug: "test_prime_set",
          displayName: "Тест Прайм: комплект",
          displayNameEn: "Test Prime Set",
          imageUrl: null,
          itemKind: "standard",
          setComponents: [
            { slug: "part_a", requiredQuantity: 1, displayName: "Деталь A", displayNameEn: "Part A" },
            { slug: "part_b", requiredQuantity: 2, displayName: "Деталь B", displayNameEn: "Part B" },
          ],
        },
      },
    };
    const setInventory: InventoryView = {
      ...inventory,
      items: [
        {
          ...inventory.items[0],
          canonicalGameId: "part-a",
          itemId: "part-a-id",
          key: { ...inventory.items[0].key!, slug: "part_a" },
          sellableQuantity: 1,
        },
        {
          ...inventory.items[0],
          canonicalGameId: "part-b",
          itemId: "part-b-id",
          key: { ...inventory.items[0].key!, slug: "part_b" },
          sellableQuantity: 2,
        },
      ],
    };

    const rows = buildTradeShiftRows(setAccount, setInventory, new Map());

    expect(rows[0].health).not.toBe("inventory_mismatch");
    expect(rows[0].inventory?.sellableQuantity).toBe(1);
  });

  it("не предлагает повторно выставить деталь, занятую активным ордером на сет", () => {
    const setOrder = { ...order, id: "set-order", itemId: "set-id", quantity: 1 };
    const partOrder = { ...order, id: "part-order", itemId: "part-a-id", quantity: 1 };
    const reservedAccount: AccountView = {
      ...account,
      orders: [setOrder, partOrder],
      orderItems: {
        "set-id": {
          slug: "test_prime_set",
          displayName: "Тест Прайм: комплект",
          displayNameEn: "Test Prime Set",
          imageUrl: null,
          itemKind: "standard",
          setComponents: [{
            slug: "part_a",
            requiredQuantity: 1,
            displayName: "Деталь A",
            displayNameEn: "Part A",
          }],
        },
        "part-a-id": {
          slug: "part_a",
          displayName: "Деталь A",
          displayNameEn: "Part A",
          imageUrl: null,
          itemKind: "standard",
        },
      },
    };
    const reservedInventory: InventoryView = {
      ...inventory,
      items: [{
        ...inventory.items[0],
        canonicalGameId: "part-a",
        itemId: "part-a-id",
        key: { ...inventory.items[0].key!, slug: "part_a" },
        sellableQuantity: 1,
      }],
    };

    const rows = buildTradeShiftRows(reservedAccount, reservedInventory, new Map());
    const partRow = rows.find((row) => row.order.id === "part-order");

    expect(partRow?.health).toBe("inventory_mismatch");
    expect(partRow?.suggestedQuantity).toBe(0);
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

  it("сопоставляет мод по русской подписи точного ранга из журнала игры", () => {
    const rankedAccount: AccountView = {
      ...account,
      orders: [
        {
          ...order,
          id: "transient-fortitude-rank-0",
          itemId: "transient-fortitude",
          platinum: 10,
          quantity: 1,
          rank: 0,
          subtype: "regular",
        },
        {
          ...order,
          id: "transient-fortitude-rank-5",
          itemId: "transient-fortitude",
          platinum: 40,
          quantity: 1,
          rank: 5,
          subtype: "regular",
        },
      ],
      orderItems: {
        "transient-fortitude": {
          slug: "transient_fortitude",
          displayName: "Кратковременное усиление",
          displayNameEn: "Transient Fortitude",
          imageUrl: null,
          itemKind: "standard",
        },
      },
    };
    const event: TradeEvent = {
      id: 19,
      occurredAt: "2026-09-01T14:38:01Z",
      partner: "rebo2808",
      platinumGiven: 0,
      platinumReceived: 10,
      givenItems: [{ name: "Кратковременное усиление (РЕДКИЙ РАНГ 0)", quantity: 1 }],
      receivedItems: [],
      status: "pending",
      matchedOrderId: null,
      reconciliationJson: null,
    };

    const plan = planTradeReconciliation(event, rankedAccount);

    expect(plan.unmatched).toEqual([]);
    expect(plan.unsafe).toEqual([]);
    expect(plan.actions).toEqual([expect.objectContaining({
      kind: "delete",
      itemName: "Кратковременное усиление",
      soldQuantity: 1,
      before: expect.objectContaining({ id: "transient-fortitude-rank-0" }),
    })]);
  });

  it("считает отсутствующий ранг WFM нулевым для явного РАНГ 0 из журнала", () => {
    const defaultRank = {
      ...order,
      id: "toxic-flight-default-rank",
      itemId: "toxic-flight",
      platinum: 3,
      quantity: 1,
      rank: null,
      subtype: null,
    };
    const wrongRank = { ...defaultRank, id: "toxic-flight-rank-5", rank: 5 };
    const toxicFlightAccount: AccountView = {
      ...account,
      orders: [defaultRank, wrongRank],
      orderItems: {
        "toxic-flight": {
          slug: "toxic_flight",
          displayName: "Токсичный Полёт",
          displayNameEn: "Toxic Flight",
          imageUrl: null,
          itemKind: "standard",
          setComponents: [],
        },
      },
    };
    const event: TradeEvent = {
      id: 9,
      occurredAt: "2026-09-02T11:27:12Z",
      partner: "Noobpromaster3000",
      platinumGiven: 0,
      platinumReceived: 3,
      givenItems: [{ name: "Токсичный полёт (РЕДКИЙ РАНГ 0)", quantity: 1 }],
      receivedItems: [],
      status: "pending",
      matchedOrderId: null,
      reconciliationJson: null,
    };

    const plan = planTradeReconciliation(event, toxicFlightAccount);

    expect(plan.unmatched).toEqual([]);
    expect(plan.unsafe).toEqual([]);
    expect(plan.actions).toEqual([expect.objectContaining({
      itemName: "Токсичный Полёт",
      soldQuantity: 1,
      before: expect.objectContaining({ id: "toxic-flight-default-rank" }),
    })]);
  });

  it("сопоставляет четыре русских чертежа с одним проданным полным комплектом", () => {
    const hildrynOrder = {
      ...order,
      id: "hildryn-set-order",
      itemId: "hildryn-set-id",
      platinum: 69,
      quantity: 1,
    };
    const hildrynAccount: AccountView = {
      ...account,
      orders: [hildrynOrder],
      orderItems: {
        "hildryn-set-id": {
          slug: "hildryn_prime_set",
          displayName: "Хильдрин Прайм: Комплект",
          displayNameEn: "Hildryn Prime Set",
          imageUrl: null,
          itemKind: "standard",
          setComponents: [
            {
              slug: "hildryn_prime_chassis_blueprint",
              requiredQuantity: 1,
              displayName: "Хильдрин Прайм: Каркас (Чертеж)",
              displayNameEn: "Hildryn Prime Chassis Blueprint",
            },
            {
              slug: "hildryn_prime_neuroptics_blueprint",
              requiredQuantity: 1,
              displayName: "Хильдрин Прайм: Нейрооптика (Чертеж)",
              displayNameEn: "Hildryn Prime Neuroptics Blueprint",
            },
            {
              slug: "hildryn_prime_systems_blueprint",
              requiredQuantity: 1,
              displayName: "Хильдрин Прайм: Система (Чертеж)",
              displayNameEn: "Hildryn Prime Systems Blueprint",
            },
            {
              slug: "hildryn_prime_blueprint",
              requiredQuantity: 1,
              displayName: "Хильдрин Прайм (Чертеж)",
              displayNameEn: "Hildryn Prime Blueprint",
            },
          ],
        },
      },
    };
    const event: TradeEvent = {
      id: 7,
      occurredAt: "2026-09-01T00:50:33Z",
      partner: "RuralAnimals",
      platinumGiven: 0,
      platinumReceived: 69,
      givenItems: [
        { name: "ЧЕРТЁЖ: Хильдрин Прайм: Каркас", quantity: 1 },
        { name: "ЧЕРТЁЖ: Хильдрин Прайм: Нейрооптика", quantity: 1 },
        { name: "ЧЕРТЁЖ: Хильдрин Прайм: Система", quantity: 1 },
        { name: "ЧЕРТЁЖ: Хильдрин Прайм", quantity: 1 },
      ],
      receivedItems: [],
      status: "pending",
      matchedOrderId: null,
      reconciliationJson: null,
    };

    const plan = planTradeReconciliation(event, hildrynAccount);

    expect(plan.unmatched).toEqual([]);
    expect(plan.unsafe).toEqual([]);
    expect(plan.actions).toEqual([expect.objectContaining({
      kind: "delete",
      itemName: "Хильдрин Прайм: Комплект",
      soldQuantity: 1,
    })]);
  });

  it("не принимает неполный набор деталей за проданный комплект", () => {
    const setAccount: AccountView = {
      ...account,
      orders: [{ ...order, id: "set-order", itemId: "set-id", quantity: 1 }],
      orderItems: {
        "set-id": {
          slug: "test_prime_set",
          displayName: "Тест Прайм: Комплект",
          displayNameEn: "Test Prime Set",
          imageUrl: null,
          itemKind: "standard",
          setComponents: [
            { slug: "a", requiredQuantity: 1, displayName: "Тест: A (Чертеж)", displayNameEn: "Test A Blueprint" },
            { slug: "b", requiredQuantity: 1, displayName: "Тест: B (Чертеж)", displayNameEn: "Test B Blueprint" },
            { slug: "c", requiredQuantity: 1, displayName: "Тест: C (Чертеж)", displayNameEn: "Test C Blueprint" },
            { slug: "main", requiredQuantity: 1, displayName: "Тест (Чертеж)", displayNameEn: "Test Blueprint" },
          ],
        },
      },
    };
    const event: TradeEvent = {
      id: 8,
      occurredAt: "2026-09-01T00:50:33Z",
      partner: "Buyer",
      platinumGiven: 0,
      platinumReceived: 30,
      givenItems: [
        { name: "ЧЕРТЁЖ: Тест: A", quantity: 1 },
        { name: "ЧЕРТЁЖ: Тест: B", quantity: 1 },
        { name: "ЧЕРТЁЖ: Тест: C", quantity: 1 },
      ],
      receivedItems: [],
      status: "pending",
      matchedOrderId: null,
      reconciliationJson: null,
    };

    const plan = planTradeReconciliation(event, setAccount);

    expect(plan.actions).toEqual([]);
    expect(plan.unmatched).toHaveLength(3);
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

  it("сводит русские варианты названия чертежа к одному имени", () => {
    expect(normalizeTradeName("ЧЕРТЁЖ: Хильдрин Прайм: Каркас"))
      .toBe(normalizeTradeName("Хильдрин Прайм: Каркас (Чертеж)"));
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

  it("не считает обмен с полученным предметом чистой продажей", () => {
    const mixed: TradeEvent = {
      id: 4, occurredAt: "2026-08-29T12:03:00Z", partner: "Trader",
      platinumGiven: 0, platinumReceived: 50,
      givenItems: [{ name: "Primed Flow", quantity: 1 }],
      receivedItems: [{ name: "Ayatan Star", quantity: 1 }],
      status: "pending", matchedOrderId: null, reconciliationJson: null,
    };
    expect(pendingSaleEvents([mixed])).toEqual([]);
    expect(planTradeReconciliation(mixed, account).unsafe).toHaveLength(1);
  });

  it("отклоняет продажу больше остатка ордера", () => {
    const event: TradeEvent = {
      id: 5, occurredAt: "2026-08-29T12:00:00Z", partner: "Buyer",
      platinumGiven: 0, platinumReceived: 520,
      givenItems: [{ name: "Primed Flow", quantity: 4 }], receivedItems: [],
      status: "pending", matchedOrderId: null, reconciliationJson: null,
    };
    expect(planTradeReconciliation(event, account).unsafe).toHaveLength(1);
  });

  it("учитывает цену и остаток пакетного ордера", () => {
    const bulkAccount: AccountView = {
      ...account,
      orders: [{ ...order, platinum: 30, quantity: 6, perTrade: 3 }],
    };
    const bulkInventory: InventoryView = {
      ...inventory,
      items: [{ ...inventory.items[0], sellableQuantity: 5 }],
    };
    const quote = { ...recommendation(), listPrice: 10, fairPrice: 10 };
    const rows = buildTradeShiftRows(
      bulkAccount,
      bulkInventory,
      new Map([[recommendationIdentity(quote.key), quote]]),
      new Date("2026-08-29T12:00:00Z"),
    );
    expect(rows[0].suggestedPrice).toBe(30);
    expect(rows[0].suggestedQuantity).toBe(3);
  });
});
