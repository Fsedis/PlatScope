import { render } from "svelte/server";
import { describe, expect, it, vi } from "vitest";
import MarketItemDetail from "./MarketItemDetail.svelte";
import type { MarketSearchRow, LivePricingResult } from "./market";

const row: MarketSearchRow = {
  itemId: "test-arcane", displayName: "Мистификатор", itemKind: "standard", masteryRequirement: null,
  recommendation: {
    key: { slug: "arcane", platform: "pc", rank: 0, charges: null, subtype: null, amberStars: null, cyanStars: null },
    provider: "relics_run", sourceDate: "2026-09-06", listPrice: 10, fairPrice: 10, quickSell: null,
    lowestAsk: null, depthThree: null, depthPrice: null, closedVolume: 40,
    liveSellOrderCount: 0, liveBuyOrderCount: 0, confidence: "medium", freshness: "fresh", reasons: [],
  },
};
function show(live: LivePricingResult | null): string {
  return render(MarketItemDetail, { props: {
    row, recommendation: live?.recommendation ?? row.recommendation, live, history: null,
    onRefresh: vi.fn(), onHistory: vi.fn(), onMarket: vi.fn(), onInventory: vi.fn(), onBack: vi.fn(),
  } }).body;
}
const live: LivePricingResult = {
  recommendation: row.recommendation, fetchedAt: "2026-09-06T10:00:00Z", quoteState: "network",
  sellOrderCount: 1, buyOrderCount: 0, warning: null,
  orders: [{ side: "sell", platinum: 30, quantity: 15, perTrade: 3, userStatus: "in_game" }],
};

describe("ценовые ориентиры и предложения рынка", () => {
  it("не выдаёт сохранённую оценку за существующую заявку покупателя", () => {
    const html = show(null);
    expect(html).toContain("Нужно проверить");
    expect(html).toContain("Сохранённая оценка от 2026-09-06");
    expect(html).not.toContain("offer-list");
  });
  it("показывает размер партии отдельно от количества и различает отсутствие заявок", () => {
    const html = show(live);
    expect(html).toContain("30 пл.");
    expect(html).toContain("за 3 шт.");
    expect(html).toContain("Всего: 15 шт.");
    expect(html).toContain("Нет заявок");
  });
  it("не представляет устаревшие предложения как текущие", () => {
    const html = show({ ...live, quoteState: "stale_cache" });
    expect(html).toContain("Сохранённые предложения могли устареть");
    expect(html).toContain("Сохранённые предложения</h3>");
    expect(html).not.toContain("Только что обновлено");
  });
});
