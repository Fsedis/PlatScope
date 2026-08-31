import { describe, expect, it } from "vitest";

import {
  filterInventory,
  inventoryCategory,
  inventorySourceLabel,
  inventoryVariantLabel,
  resolutionLabel,
  type InventoryFilters,
  type InventoryViewItem,
} from "./inventory";

const base: InventoryViewItem = {
  canonicalGameId: "/Lotus/Items/Test",
  itemId: "wfm-test-item",
  bulkTradable: false,
  displayName: "Тестовый предмет",
  tags: ["component"],
  key: {
    slug: "test_item",
    platform: "pc",
    rank: null,
    subtype: null,
    amberStars: null,
    cyanStars: null,
  },
  rank: null,
  subtype: null,
  ownedQuantity: 3,
  tradeableQuantity: 3,
  untradeableQuantity: 0,
  unknownQuantity: 0,
  leveledQuantity: 0,
  equippedQuantity: 0,
  equippedPlacements: [],
  sellableQuantity: 2,
  resolution: "resolved",
  vaultStatus: "vaulted",
  closedMedian48h: 18,
  hasReliablePrice: true,
};

const allFilters: InventoryFilters = {
  category: "all",
  duplicates: "all" as const,
  vault: "all" as const,
  price: "all" as const,
};

describe("inventory presentation", () => {
  it("filters by localized name and category", () => {
    const filters: InventoryFilters = { ...allFilters, category: "component" };
    expect(filterInventory([base], "тестовый", filters)).toEqual([base]);
    expect(filterInventory([base], "другой", filters)).toEqual([]);
  });

  it("assigns one primary type when tags overlap", () => {
    expect(inventoryCategory({ ...base, tags: ["mod", "warframe", "rare"] })).toBe("mod");
    expect(inventoryCategory({ ...base, tags: ["component", "weapon", "prime"] })).toBe("component");
    expect(inventoryCategory({ ...base, tags: ["set", "prime", "warframe"] })).toBe("warframe");
  });

  it("labels an unavailable exact variant", () => {
    const attention = {
      ...base,
      resolution: "exact_variant_unavailable" as const,
      sellableQuantity: 0,
    };
    expect(resolutionLabel(attention.resolution)).toBe("Нет точного варианта");
  });

  it("recognizes duplicates from owned quantity", () => {
    expect(
      filterInventory([base], "", { ...allFilters, duplicates: "duplicates" }),
    ).toEqual([base]);
  });

  it("combines vault and reliable-price filters", () => {
    expect(
      filterInventory([base], "", {
        ...allFilters,
        vault: "vaulted",
        price: "priced",
      }),
    ).toEqual([base]);
    expect(
      filterInventory([base], "", { ...allFilters, vault: "available" }),
    ).toEqual([]);
    expect(
      filterInventory([{ ...base, closedMedian48h: null, hasReliablePrice: false }], "", {
        ...allFilters,
        price: "unpriced",
      }),
    ).toHaveLength(1);
  });

  it("formats exact rank and subtype", () => {
    expect(inventoryVariantLabel({ ...base, rank: 10, subtype: "radiant" })).toBe(
      "ранг 10 · radiant",
    );
  });

  it("labels the legacy Overwolf source without exposing English text in Russian UI", () => {
    expect(inventorySourceLabel("overwolf_companion", "ru")).toBe("Старый импорт Overwolf");
    expect(inventorySourceLabel("overwolf_companion", "en")).toBe("Legacy Overwolf import");
  });
});
