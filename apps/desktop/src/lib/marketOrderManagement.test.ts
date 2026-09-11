import { describe, expect, it } from "vitest";
import { validateListingNumbers, type AccountOrder, type AccountView } from "./account";
import { inventoryListingQuantity, listingReserveWarning, type InventoryView } from "./inventory";
import type { PriceRecommendation } from "./market";
import { competingOffers, orderChange, orderMarketPrice, reviewedChanges, reviewedQuantitiesMatch, salesFilterRows, sortSalesRows } from "./marketSales";
import type { LiveOrderView } from "./market";
import { applyPriceCheckFailures, buildTradeShiftRows, recommendationIdentity, updateInput } from "./tradeShift";

const key = {
  slug: "primary_deadhead", platform: "pc", rank: 0, charges: null,
  subtype: null, amberStars: null, cyanStars: null,
};

function order(id: string, type: "sell" | "buy", overrides: Partial<AccountOrder> = {}): AccountOrder {
  return {
    id, type, itemId: "arcane", platinum: 45, quantity: 12, perTrade: 3,
    rank: 0, charges: null, subtype: null, amberStars: null, cyanStars: null,
    visible: true, createdAt: "2026-09-01T00:00:00Z", updatedAt: "2026-09-05T00:00:00Z",
    ...overrides,
  };
}

function account(orders: AccountOrder[]): AccountView {
  return {
    connected: true,
    profile: { id: "player", ingameName: "Tenno", slug: "tenno", platform: "pc", crossplay: true, verification: true },
    orders,
    orderItems: {
      arcane: { slug: key.slug, displayName: "Основной Выстрел в Голову", displayNameEn: "Primary Deadhead", imageUrl: null, itemKind: "standard" },
    },
  };
}

function inventory(quantity: number): InventoryView {
  return {
    metadata: { source: "read_only_scan", observedAt: "2026-09-06T00:00:00Z", schemaVersion: 1, itemCount: 1, checksumSha256: "inventory" },
    keepCopies: 0, modUsageScanned: true,
    summary: { ownedQuantity: quantity, sellableQuantity: quantity, resolvedRows: 1, attentionRows: 0 },
    items: [{
      canonicalGameId: "Primary Deadhead", itemId: "arcane", bulkTradable: true,
      displayName: "Основной Выстрел в Голову", tags: ["arcane"], key, rank: 0, subtype: null,
      ownedQuantity: quantity, tradeableQuantity: quantity, untradeableQuantity: 0,
      unknownQuantity: 0, leveledQuantity: 0, equippedQuantity: 0, equippedPlacements: [],
      sellableQuantity: quantity, resolution: "resolved", vaultStatus: "unknown",
    }],
  };
}

function quotes(price = 10): Map<string, PriceRecommendation> {
  return new Map([[recommendationIdentity(key), {
    key, provider: "warframe_market", sourceDate: "2026-09-05", listPrice: price, fairPrice: price,
    quickSell: 9, lowestAsk: 10, depthThree: 11, depthPrice: 11, closedVolume: 90,
    liveSellOrderCount: 8, liveBuyOrderCount: 5, confidence: "high", freshness: "fresh", reasons: [],
  }]]);
}

describe("управление продажами и заявками на покупку", () => {
  it("разрешает выставить последнюю копию после предупреждения вместо запрета", () => {
    const stock = inventory(1);
    stock.keepCopies = 1;
    stock.items[0].sellableQuantity = 0;
    const row = buildTradeShiftRows(account([order("sale", "sell", { quantity: 1, perTrade: null })]), stock, quotes())[0];
    expect(inventoryListingQuantity(row.inventory)).toBe(1);
    expect(validateListingNumbers(10, 1, null, "ru", inventoryListingQuantity(row.inventory))).toBeNull();
    expect(listingReserveWarning(row.inventory, 1)).toContain("Оставлять копий");
    expect(row.health).not.toBe("inventory_mismatch");
    expect(row.suggestedQuantity).toBeNull();
  });

  it("учитывает другие объявления в пределе и предупреждает только при использовании резерва", () => {
    const stock = inventory(5);
    stock.keepCopies = 2;
    stock.items[0].sellableQuantity = 3;
    const row = buildTradeShiftRows(account([
      order("edited", "sell", { quantity: 1, perTrade: null }),
      order("other", "sell", { quantity: 2, perTrade: null }),
    ]), stock, quotes()).find(row => row.order.id === "edited")!;
    expect(inventoryListingQuantity(row.inventory)).toBe(3);
    expect(listingReserveWarning(row.inventory, 1)).toBeNull();
    expect(listingReserveWarning(row.inventory, 3)).toContain("вы выбрали 3");
    expect(validateListingNumbers(10, 4, null, "ru", inventoryListingQuantity(row.inventory))).not.toBeNull();
  });

  it("не снимает защиту надетых копий, личных целей и неопределённых вариантов", () => {
    const item = inventory(6).items[0];
    Object.assign(item, { tradeableQuantity: 4, untradeableQuantity: 1, unknownQuantity: 1, equippedQuantity: 1, personalReservedQuantity: 2 });
    expect(inventoryListingQuantity(item)).toBe(2);
    expect(inventoryListingQuantity({ ...item, resolution: "ambiguous_item" })).toBe(0);
    expect(listingReserveWarning(inventory(4).items[0], 4)).toBeNull();
  });

  it("сравнивает объявления по цене одной копии, даже если цены указаны за разные партии", () => {
    const source = account([
      order("single", "sell", { platinum: 20, perTrade: 1 }),
      order("lot", "sell", { platinum: 45, perTrade: 3 }),
    ]);
    const rows = buildTradeShiftRows(source, inventory(12), quotes());
    expect(sortSalesRows(rows, "cheap").map(row => row.order.id)).toEqual(["lot", "single"]);
    expect(sortSalesRows(rows, "expensive").map(row => row.order.id)).toEqual(["single", "lot"]);
    expect(rows.map(row => row.order.id)).toEqual(["single", "lot"]);
  });

  it("показывает конкурентов в игре с нужной стороны и исключает собственные объявления", () => {
    const profile = account([]).profile;
    const offer = (name: string, price: number, overrides: Partial<LiveOrderView> = {}): LiveOrderView => ({
      userIngameName: name, side: "sell", platinum: price, quantity: 12, perTrade: 1, userStatus: "in_game", ...overrides,
    });
    const offers = [
      offer("tenno", 1), offer("Другой ник", 2, { userSlug: "TENNO" }),
      offer("Отошёл", 3, { userStatus: "online" }),
      offer("Партия", 30, { perTrade: 3 }), offer("Одна копия", 15),
      offer("Покупатель А", 24, { side: "buy", perTrade: 3 }),
      offer("Покупатель Б", 12, { side: "buy" }),
    ];
    expect(competingOffers(offers, "sell", profile).map(offer => offer.userIngameName)).toEqual(["Партия", "Одна копия"]);
    expect(competingOffers(offers, "buy", profile).map(offer => offer.userIngameName)).toEqual(["Покупатель Б", "Покупатель А"]);
    expect(competingOffers([], "sell", profile)).toEqual([]);
  });

  it("показывает стороны отдельно и не исправляет покупку по правилу продажи", () => {
    const source = account([order("sell", "sell"), order("buy", "buy", { quantity: 900 })]);
    const sales = buildTradeShiftRows(source, inventory(12), quotes());
    const purchases = buildTradeShiftRows(source, inventory(0), quotes(), new Date(), "buy");
    expect(sales.map((row) => row.order.id)).toEqual(["sell"]);
    expect(purchases.map((row) => row.order.id)).toEqual(["buy"]);
    expect(sales[0].suggestedPrice).toBe(30);
    expect(purchases[0]).toMatchObject({ health: "healthy", needsAction: false, suggestedPrice: null, suggestedQuantity: null });
    expect(purchases[0].recommendation?.listPrice).toBe(10);
    expect(reviewedChanges(purchases)).toEqual([]);
  });

  it("заявка на покупку не резервирует уже имеющиеся копии для продажи", () => {
    const source = account([order("sell", "sell"), order("buy", "buy", { quantity: 900 })]);
    const [sale] = buildTradeShiftRows(source, inventory(12), quotes(15));
    expect(sale.inventory?.sellableQuantity).toBe(12);
    expect(sale.health).toBe("healthy");
    expect(orderChange(sale)).toBeNull();
  });

  it("скрытая покупка остаётся скрытой без предложения снять её из-за нулевого инвентаря", () => {
    const source = account([order("hidden-buy", "buy", { visible: false })]);
    const rows = buildTradeShiftRows(source, inventory(0), quotes(), new Date(), "buy");
    expect(salesFilterRows(rows, "hidden")).toHaveLength(1);
    expect(salesFilterRows(rows, "attention")).toEqual([]);
    expect(rows[0].health).toBe("hidden");
    expect(orderChange(rows[0])).toBeNull();
  });

  it("сбой котировки покупки требует повторной проверки, но не создаёт изменение цены или количества", () => {
    const source = account([order("buy", "buy")]);
    const rows = buildTradeShiftRows(source, inventory(0), quotes(), new Date(), "buy");
    const failed = applyPriceCheckFailures(rows, new Set([recommendationIdentity(key)]));
    expect(failed[0]).toMatchObject({ priceCheckFailed: true, suggestedPrice: null, suggestedQuantity: null, recommendation: null });
    expect(reviewedChanges(failed)).toEqual([]);
  });

  it("при неизвестном инвентаре не предлагает удалить продажу, а известный ноль обрабатывает явно", () => {
    const source = account([order("sell", "sell"), order("buy", "buy")]);
    const [unknown] = buildTradeShiftRows(source, null, quotes(15));
    const [missing] = buildTradeShiftRows(source, inventory(0), quotes(15));
    expect(unknown.suggestedQuantity).toBeNull();
    expect(orderChange(unknown)).toBeNull();
    expect(orderChange(missing)).toEqual({ price: null, quantity: 0, delete: true });
    const [purchase] = buildTradeShiftRows(source, inventory(0), quotes(15), new Date(), "buy");
    expect(orderChange(purchase)).toBeNull();
  });

  it("уменьшает продажу целыми лотами и сравнивает цену с тем же размером лота", () => {
    const source = account([order("sell", "sell")]);
    const [sale] = buildTradeShiftRows(source, inventory(10), quotes());
    expect(sale.suggestedQuantity).toBe(9);
    expect(sale.suggestedPrice).toBe(30);
    expect(orderMarketPrice(sale)).toBe(30);
    expect(orderChange(sale)).toEqual({ price: 30, quantity: 9, delete: false });
  });

  it("замораживает выбранные продажи и отклоняет уменьшение после изменения остатков", () => {
    const source = account([order("sell", "sell"), order("buy", "buy")]);
    const selectedSales = buildTradeShiftRows(source, inventory(10), quotes());
    const reviewed = reviewedChanges(selectedSales);
    const currentSales = buildTradeShiftRows(source, inventory(6), quotes());
    expect(reviewed.map((change) => change.before.id)).toEqual(["sell"]);
    expect(reviewed[0].change.quantity).toBe(9);
    expect(reviewedQuantitiesMatch(reviewed, currentSales)).toBe(false);
    selectedSales[0].order.quantity = 99;
    expect(reviewed[0].before.quantity).toBe(12);
  });

  it("быстрый показ и скрытие передают только видимость, сохраняя цену, количество и точный вариант", () => {
    for (const visible of [false, true]) {
      expect(updateInput({ visible })).toEqual({
        visible, platinum: null, quantity: null, perTrade: null, rank: null, charges: null,
        subtype: null, amberStars: null, cyanStars: null,
      });
    }
  });

  it("позволяет заказать больше имеющегося и продолжает проверять целый размер лота", () => {
    expect(validateListingNumbers(45, 900, 3, "ru", null)).toBeNull();
    expect(validateListingNumbers(45, 901, 3, "ru", null)).toContain("делить общее количество");
    expect(validateListingNumbers(45, 900, 3, "ru", 12)).toContain("12 подтверждённых копий");
  });
});
