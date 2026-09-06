import { describe, expect, it } from "vitest";
import type { PriceRecommendation } from "./market";
import type { TradeShiftRow } from "./tradeShift";
import { orderChange, orderMarketPrice, orderUnchanged, reviewedChanges, reviewedQuantitiesMatch, salesFilterRows } from "./marketSales";

function row(overrides: Partial<TradeShiftRow> = {}): TradeShiftRow {
  const key = { slug: "primary_deadhead", platform: "pc", rank: 0, charges: null, subtype: null, amberStars: null, cyanStars: null };
  const recommendation: PriceRecommendation = {
    key, provider: "warframe_market", sourceDate: "2026-09-06", listPrice: 10, fairPrice: 10,
    quickSell: null, lowestAsk: 10, depthThree: null, depthPrice: null, closedVolume: 20,
    liveSellOrderCount: 2, liveBuyOrderCount: 0, confidence: "medium", freshness: "fresh", reasons: [],
  };
  return {
    order: { id: "one", itemId: "arcane", type: "sell", platinum: 45, quantity: 15, perTrade: 3,
      rank: 0, charges: null, subtype: null, amberStars: null, cyanStars: null, visible: true,
      createdAt: "2026-09-01", updatedAt: "2026-09-06" },
    item: { slug: key.slug, displayName: "Мистификатор", displayNameEn: "Arcane", imageUrl: null, itemKind: "standard" },
    key, itemKind: "standard", inventory: null, recommendation,
    health: "overpriced", suggestedPrice: 30, suggestedQuantity: null, needsAction: true,
    ...overrides,
  };
}

describe("изменения объявлений на рынке", () => {
  it("сравнивает цену партии с ценой такой же партии и не выдумывает отсутствующую оценку", () => {
    expect(orderMarketPrice(row())).toBe(30);
    expect(orderMarketPrice(row({ recommendation: null }))).toBeNull();
  });

  it("сохраняет показанные пользователю значения при смене рекомендации и исходного объявления", () => {
    const original = row();
    const review = reviewedChanges([original]);
    original.order.platinum = 90;
    original.order.quantity = 6;
    original.suggestedPrice = 120;
    expect(review[0].before).toMatchObject({ platinum: 45, quantity: 15 });
    expect(review[0].change).toEqual({ price: 30, quantity: null, delete: false });
    expect(orderUnchanged(review[0].before, original.order)).toBe(false);
  });

  it("не сохраняет поверх удалённого или обновлённого объявления", () => {
    const before = row().order;
    expect(orderUnchanged(before, { ...before })).toBe(true);
    expect(orderUnchanged(before, undefined)).toBe(false);
    expect(orderUnchanged(before, { ...before, visible: false })).toBe(false);
    expect(orderUnchanged(before, { ...before, updatedAt: "later" })).toBe(false);
  });

  it("различает отсутствие изменений, изменение цены и снятие объявления без свободных копий", () => {
    expect(orderChange(row({ suggestedPrice: 45 }))).toBeNull();
    expect(orderChange(row({ suggestedQuantity: 0 }))).toEqual({ price: null, quantity: 0, delete: true });
    expect(reviewedChanges([row({ suggestedPrice: null })])).toEqual([]);
  });

  it("отменяет подготовленное уменьшение или снятие, когда остатки уже изменились", () => {
    const reduction = row({ suggestedQuantity: 6 });
    const reviewed = reviewedChanges([reduction]);
    expect(reviewedQuantitiesMatch(reviewed, [row({ suggestedQuantity: 6 })])).toBe(true);
    expect(reviewedQuantitiesMatch(reviewed, [row({ suggestedQuantity: 3 })])).toBe(false);
    expect(reviewedQuantitiesMatch(reviewed, [])).toBe(false);
    const removal = reviewedChanges([row({ suggestedQuantity: 0 })]);
    expect(reviewedQuantitiesMatch(removal, [row({ suggestedQuantity: null })])).toBe(false);
    expect(reviewedQuantitiesMatch(reviewedChanges([row()]), [row()])).toBe(true);
  });

  it("сохраняет все объявления и отдельно отбирает скрытые и требующие решения", () => {
    const healthy = row({ needsAction: false, health: "healthy", suggestedPrice: null });
    const hidden = row({ order: { ...healthy.order, id: "hidden", visible: false }, needsAction: false, health: "hidden", suggestedPrice: null });
    const source = [healthy, hidden, row()];
    expect(salesFilterRows(source, "all")).toHaveLength(3);
    expect(salesFilterRows(source, "hidden")).toEqual([hidden]);
    expect(salesFilterRows(source, "attention")).toEqual([source[2]]);
  });
});
