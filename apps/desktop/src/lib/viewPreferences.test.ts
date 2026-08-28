import { describe, expect, it } from "vitest";

import {
  DEFAULT_INVENTORY_VIEW,
  DEFAULT_MARKET_VIEW,
  DEFAULT_SELL_NOW_VIEW,
  loadInventoryViewPreferences,
  loadMarketViewPreferences,
  loadSellNowViewPreferences,
  saveInventoryViewPreferences,
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
    expect(loadInventoryViewPreferences(storage)).toEqual(DEFAULT_INVENTORY_VIEW);
    expect(loadSellNowViewPreferences(storage)).toEqual(DEFAULT_SELL_NOW_VIEW);
  });

  it("round-trips each screen independently", () => {
    const storage = new MemoryStorage();
    expect(saveMarketViewPreferences({
      priceFilter: "unpriced",
      sortKey: "name",
      sortDirection: "asc",
    }, storage)).toBe(true);
    expect(saveInventoryViewPreferences({
      category: "arcane_enhancement",
      duplicates: "duplicates",
      vault: "vaulted",
      price: "unpriced",
    }, storage)).toBe(true);
    expect(saveSellNowViewPreferences({
      category: "arcane_enhancement",
      preset: "sell_now",
      confidence: "high",
      timing: "peak",
      sortKey: "fair",
      sortDirection: "desc",
    }, storage)).toBe(true);

    expect(loadMarketViewPreferences(storage)).toEqual({
      priceFilter: "unpriced",
      sortKey: "name",
      sortDirection: "asc",
    });
    expect(loadInventoryViewPreferences(storage).category).toBe("arcane_enhancement");
    expect(loadSellNowViewPreferences(storage).preset).toBe("sell_now");
    expect(loadSellNowViewPreferences(storage).category).toBe("arcane_enhancement");
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
    invalid.values.set("platscope.inventory-view.v1", JSON.stringify({
      version: 1,
      category: "<script>",
      duplicates: true,
      vault: "soon",
      price: "free",
    }));
    expect(loadInventoryViewPreferences(invalid)).toEqual(DEFAULT_INVENTORY_VIEW);
  });

  it("does not throw when storage is unavailable", () => {
    const unavailable: ViewPreferenceStorage = {
      getItem: () => { throw new Error("blocked"); },
      setItem: () => { throw new Error("full"); },
    };
    expect(loadMarketViewPreferences(unavailable)).toEqual(DEFAULT_MARKET_VIEW);
    expect(saveMarketViewPreferences(DEFAULT_MARKET_VIEW, unavailable)).toBe(false);
  });
});
