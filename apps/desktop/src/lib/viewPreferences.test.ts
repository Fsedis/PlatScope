import { describe, expect, it } from "vitest";

import {
  DEFAULT_INSIGHTS_VIEW,
  DEFAULT_MARKET_VIEW,
  DEFAULT_SELL_NOW_VIEW,
  loadInsightsViewPreferences,
  loadMarketViewPreferences,
  loadSellNowViewPreferences,
  saveInsightsViewPreferences,
  saveMarketViewPreferences,
  saveSellNowViewPreferences,
  type ViewPreferenceStorage,
} from "./viewPreferences";

class MemoryStorage implements ViewPreferenceStorage {
  readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

describe("saved working views", () => {
  it("returns stable defaults when nothing was saved", () => {
    const storage = new MemoryStorage();
    expect(loadMarketViewPreferences(storage)).toEqual(DEFAULT_MARKET_VIEW);
    expect(loadSellNowViewPreferences(storage)).toEqual(DEFAULT_SELL_NOW_VIEW);
    expect(loadInsightsViewPreferences(storage)).toEqual(DEFAULT_INSIGHTS_VIEW);
  });

  it("round-trips each screen independently", () => {
    const storage = new MemoryStorage();
    expect(saveMarketViewPreferences({
      priceFilter: "unpriced",
      sortKey: "name",
      sortDirection: "asc",
    }, storage)).toBe(true);
    expect(saveSellNowViewPreferences({
      category: "arcane_enhancement",
      preset: "sell_now",
      equipped: "all",
      sortKey: "fair",
      sortDirection: "desc",
    }, storage)).toBe(true);
    expect(saveInsightsViewPreferences({ mode: "complete_sets" }, storage)).toBe(true);

    expect(loadMarketViewPreferences(storage)).toEqual({
      priceFilter: "unpriced",
      sortKey: "name",
      sortDirection: "asc",
    });
    expect(loadSellNowViewPreferences(storage).preset).toBe("sell_now");
    expect(loadSellNowViewPreferences(storage).category).toBe("arcane_enhancement");
    expect(loadInsightsViewPreferences(storage)).toEqual({ mode: "complete_sets" });
    expect(storage.values.size).toBe(3);
  });

  it("fails closed for corrupt, stale, and out-of-domain values", () => {
    const corrupt = new MemoryStorage();
    corrupt.values.set("platscope.market-view.v1", "not json");
    expect(loadMarketViewPreferences(corrupt)).toEqual(DEFAULT_MARKET_VIEW);

    const stale = new MemoryStorage();
    stale.values.set("platscope.sell-now-view.v1", JSON.stringify({
      version: 0,
      preset: "sell_now",
    }));
    expect(loadSellNowViewPreferences(stale)).toEqual(DEFAULT_SELL_NOW_VIEW);

    const invalid = new MemoryStorage();
    invalid.values.set("platscope.sell-now-view.v1", JSON.stringify({
      version: 1,
      category: "<script>",
      preset: "everything",
    }));
    expect(loadSellNowViewPreferences(invalid)).toEqual(DEFAULT_SELL_NOW_VIEW);

    const removedConfidenceSort = new MemoryStorage();
    removedConfidenceSort.values.set("platscope.market-view.v1", JSON.stringify({
      version: 1,
      priceFilter: "all",
      sortKey: "confidence",
      sortDirection: "desc",
    }));
    expect(loadMarketViewPreferences(removedConfidenceSort).sortKey).toBe("volume");

    const staleInsights = new MemoryStorage();
    staleInsights.values.set("platscope.insights-view.v1", JSON.stringify({
      version: 0,
      mode: "relics",
    }));
    expect(loadInsightsViewPreferences(staleInsights)).toEqual(DEFAULT_INSIGHTS_VIEW);

    const invalidInsights = new MemoryStorage();
    invalidInsights.values.set("platscope.insights-view.v1", JSON.stringify({
      version: 1,
      mode: "anything",
    }));
    expect(loadInsightsViewPreferences(invalidInsights)).toEqual(DEFAULT_INSIGHTS_VIEW);
  });

  it("round-trips every supported insights mode", () => {
    const storage = new MemoryStorage();
    const modes = [
      "overview",
      "resources",
      "relics",
      "complete_sets",
      "sell_sets",
      "ducats",
    ] as const;

    for (const mode of modes) {
      expect(saveInsightsViewPreferences({ mode }, storage)).toBe(true);
      expect(loadInsightsViewPreferences(storage)).toEqual({ mode });
    }
  });

  it("does not throw when storage is unavailable", () => {
    const unavailable: ViewPreferenceStorage = {
      getItem: () => { throw new Error("blocked"); },
      setItem: () => { throw new Error("full"); },
    };
    expect(loadMarketViewPreferences(unavailable)).toEqual(DEFAULT_MARKET_VIEW);
    expect(saveMarketViewPreferences(DEFAULT_MARKET_VIEW, unavailable)).toBe(false);
    expect(loadInsightsViewPreferences(unavailable)).toEqual(DEFAULT_INSIGHTS_VIEW);
    expect(saveInsightsViewPreferences(DEFAULT_INSIGHTS_VIEW, unavailable)).toBe(false);
  });
});
