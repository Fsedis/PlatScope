import { describe, expect, it } from "vitest";

import {
  inventoryCategory,
  inventorySourceLabel,
  inventoryVariantLabel,
  resolutionLabel,
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
    charges: null,
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
};

describe("inventory presentation", () => {
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
