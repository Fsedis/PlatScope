import { describe, expect, it, vi } from "vitest";
import type { InventoryView, InventoryViewItem } from "./inventory";
import { createResurgenceInventory, type ResurgenceInventoryState } from "./resurgenceInventory";
import { rotationRelicOwned, rotationRelics } from "./primeResurgence";
import { makeWorldActivityMock } from "./worldActivityMock";

function inventory(): InventoryView {
  const item = (slug: string, subtype: string, quantity: number): InventoryViewItem => ({
    canonicalGameId: `${slug}/${subtype}`, itemId: null, bulkTradable: false, displayName: "Реликвия",
    tags: ["relic"], key: { slug, subtype, platform: "pc", rank: null, charges: null, amberStars: null, cyanStars: null },
    rank: null, subtype, ownedQuantity: quantity, tradeableQuantity: quantity, untradeableQuantity: 0,
    unknownQuantity: 0, leveledQuantity: 0, equippedQuantity: 0, equippedPlacements: [], sellableQuantity: 0,
    resolution: "resolved", vaultStatus: "unknown",
  });
  return { metadata: { source: "test_fixture", observedAt: "2026-09-06T00:00:00Z", schemaVersion: 1, itemCount: 5, checksumSha256: "test" },
    keepCopies: 1, modUsageScanned: false, summary: { ownedQuantity: 110, sellableQuantity: 0, resolvedRows: 5, attentionRows: 0 },
    items: [item("lith_k5_relic", "intact", 4), item("lith_k5_relic", "exceptional", 1), item("lith_k5_relic", "flawless", 2), item("lith_k5_relic", "radiant", 3), item("lith_k50_relic", "intact", 100)] };
}
const flush = async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); };

describe("запас реликвий текущей ротации", () => {
  it("считает все улучшения и оставленные себе копии, исключая похожие реликвии", () => {
    const relic = rotationRelics(makeWorldActivityMock("real").resurgenceOffers)[0];
    expect(rotationRelicOwned(relic, inventory())).toBe(10);
    expect(rotationRelicOwned({ ...relic, relicSlug: "lith_m7_relic" }, inventory())).toBe(0);
    expect(rotationRelicOwned(relic, null)).toBeNull();
    expect(rotationRelicOwned({ ...relic, relicSlug: null }, inventory())).toBeNull();
  });
  it("сбрасывает старый аккаунт и игнорирует его запоздалый ответ", async () => {
    let finish!: (view: InventoryView) => void;
    const latest = { ...inventory(), items: [] };
    const load = vi.fn<() => Promise<InventoryView | null>>().mockImplementationOnce(() => new Promise(resolve => { finish = resolve; })).mockResolvedValueOnce(latest);
    const handlers = new Map<string, () => void>();
    const cleanup = vi.fn();
    const store = createResurgenceInventory({ load, listen: async (event, handler) => { handlers.set(event, handler); return cleanup; } });
    const states: ResurgenceInventoryState[] = [];
    const stop = store.subscribe(state => states.push(state));
    try {
      handlers.get("inventory-updated")!();
      finish(inventory());
      await flush();
      expect(load).toHaveBeenCalledTimes(2);
      expect(states.some(state => state.view?.items.length === 5)).toBe(false);
      expect(states.at(-1)).toEqual({ view: latest, loading: false, error: false });
    } finally { stop(); }
    expect(cleanup).toHaveBeenCalledTimes(2);
  });
  it("после ошибки не подменяет неизвестное количество нулём и восстанавливается по событию", async () => {
    const load = vi.fn<() => Promise<InventoryView | null>>().mockResolvedValueOnce(inventory()).mockRejectedValueOnce(new Error("read failed")).mockResolvedValueOnce(null);
    const handlers = new Map<string, () => void>();
    const store = createResurgenceInventory({ load, listen: async (event, handler) => { handlers.set(event, handler); return () => {}; } });
    let state!: ResurgenceInventoryState;
    const stop = store.subscribe(value => { state = value; });
    try {
      await flush();
      expect(state.view?.items).toHaveLength(5);
      handlers.get("inventory-updated")!();
      expect(state.view).toBeNull();
      await flush();
      expect(state).toEqual({ view: null, error: true, loading: false });
      handlers.get("game-metadata-updated")!();
      await flush();
      expect(state).toEqual({ view: null, error: false, loading: false });
    } finally { stop(); }
  });
});
