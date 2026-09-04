import { describe, expect, it } from "vitest";
import { checkedLivePrice, priceCheckSummary } from "./livePriceCheck";
import type { LivePricingResult } from "./market";

const quote = (quoteState: LivePricingResult["quoteState"], price: number | null) => ({
  quoteState, recommendation: { listPrice: price, fairPrice: price },
} as LivePricingResult);

describe("результат проверки текущих цен", () => {
  it("различает проверенную цену, пустой ответ и устаревший запасной ответ", () => {
    expect(checkedLivePrice(quote("network", 20))).toEqual({ state: "priced", price: 20 });
    expect(checkedLivePrice(quote("cache", 20)).state).toBe("priced");
    expect(checkedLivePrice(null).state).toBe("empty");
    expect(checkedLivePrice(quote("network", null)).state).toBe("empty");
    expect(checkedLivePrice(quote("stale_cache", 20)).state).toBe("failed");
  });
  it("не называет сетевую ошибку отсутствием предложений", () => {
    expect(priceCheckSummary(0, 0, 3)).toContain("Не удалось проверить");
    expect(priceCheckSummary(0, 0, 3)).not.toContain("без подходящих");
    expect(priceCheckSummary(2, 1, 3)).toBe("Цены проверены: 2 · без подходящих предложений: 1 · не удалось проверить: 3.");
    expect(priceCheckSummary(0, 3, 0)).toContain("без подходящих предложений: 3");
  });
});
