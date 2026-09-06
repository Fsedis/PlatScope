import { describe, expect, it } from "vitest";
import { filterTradeHistory, historyItems, tradeHistoryCsv, tradeHistoryKind, tradeWasClosed } from "./marketTradeHistory";
import type { TradeEvent } from "./tradeShift";
import type { AccountView } from "./account";

const event: TradeEvent = {
  id: 1, occurredAt: "2026-09-06T10:00:00Z", partner: "Market.Tenno", platinumGiven: 0, platinumReceived: 80,
  givenItems: [{ name: "Nyx Prime Set", quantity: 1 }], receivedItems: [], status: "pending", matchedOrderId: null, reconciliationJson: null,
};
const account: AccountView = { connected: false, profile: null, orders: [], orderItems: {
  nyx: { slug: "nyx_prime_set", displayName: "Никс Прайм: комплект", displayNameEn: "Nyx Prime Set", imageUrl: null, itemKind: "standard" },
} };

describe("история сделок рынка", () => {
  it("отличает чистые продажи и покупки от смешанного обмена", () => {
    expect(tradeHistoryKind(event)).toBe("sell");
    expect(tradeHistoryKind({ ...event, platinumReceived: 0, platinumGiven: 80, givenItems: [], receivedItems: event.givenItems })).toBe("buy");
    expect(tradeHistoryKind({ ...event, receivedItems: [{ name: "Forma", quantity: 1 }] })).toBe("exchange");
    expect(tradeHistoryKind({ ...event, platinumReceived: 0, platinumGiven: 20, receivedItems: event.givenItems })).toBe("exchange");
  });

  it("находит предмет по обоим названиям и игрока, не исключает ожидающие продажи", () => {
    expect(historyItems(event, account)[0]).toMatchObject({ name: "Никс Прайм: комплект", english: "Nyx Prime Set" });
    for (const query of ["никс", "nyx prime", "market.tenno", "Никс market"]) {
      expect(filterTradeHistory([event], query, "all", "all", account)).toEqual([event]);
    }
    expect(filterTradeHistory([event], "Rhino", "all", "all", account)).toEqual([]);
  });

  it("фильтрует скользящие семь дней и сортирует всю историю до пагинации", () => {
    const now = Date.parse(event.occurredAt);
    const old = { ...event, id: 2, occurredAt: new Date(now - 8 * 86_400_000).toISOString() };
    const boundary = { ...event, id: 3, occurredAt: new Date(now - 7 * 86_400_000).toISOString() };
    const future = { ...event, id: 4, occurredAt: new Date(now + 1).toISOString() };
    expect(filterTradeHistory([old, boundary, event, future], "", "sell", "7", null, now).map((row) => row.id)).toEqual([1, 3]);
    expect(filterTradeHistory([old, event], "", "all", "all", null, now).map((row) => row.id)).toEqual([1, 2]);
  });

  it("не предлагает отмену закрытой сделки и переживает повреждённое сохранение", () => {
    expect(tradeWasClosed({ ...event, reconciliationJson: JSON.stringify([{ kind: "close", itemName: "Nyx", soldQuantity: 1, before: {} }]) })).toBe(true);
    for (const value of ["broken", "{}", "null", '[{"kind":"close"}]']) {
      expect(tradeWasClosed({ ...event, reconciliationJson: value })).toBe(false);
      expect(() => historyItems({ ...event, reconciliationJson: value }, account)).not.toThrow();
    }
  });

  it("экспортирует обе стороны обмена и экранирует формулы/кавычки в именах", () => {
    const csv = tradeHistoryCsv([{ ...event, partner: '=HYPERLINK("x")', platinumGiven: 10, receivedItems: [{ name: "Forma", quantity: 2 }] }], account);
    expect(csv).toContain('"\'=HYPERLINK(""x"")"');
    expect(csv).toContain('"80";"10"');
    expect(csv).toContain("Никс Прайм: комплект / Nyx Prime Set ×1");
    expect(csv).toContain("Forma ×2");
    expect(csv.startsWith("\uFEFF")).toBe(true);
  });
});
