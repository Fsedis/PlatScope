import { describe, expect, it } from "vitest";

import type { InventoryViewItem } from "./inventory";
import {
  buildEquippedEquipmentGroups,
  configLabel,
  summarizeEquippedMods,
} from "./equippedMods";

const item: InventoryViewItem = {
  canonicalGameId: "/Lotus/Upgrades/Mods/PrimedFlow",
  itemId: "primed-flow",
  bulkTradable: false,
  displayName: "Поток Прайм",
  imageUrl: null,
  tags: ["mod"],
  key: null,
  rank: 10,
  subtype: null,
  ownedQuantity: 3,
  tradeableQuantity: 3,
  untradeableQuantity: 0,
  unknownQuantity: 0,
  leveledQuantity: 3,
  equippedQuantity: 1,
  equippedPlacements: [
    {
      equipmentInstanceKey: "volt-instance",
      equipmentGameId: "/Lotus/Powersuits/Volt/VoltPrime",
      equipmentDisplayName: "Вольт Прайм",
      equipmentImageUrl: null,
      equipmentKind: "warframe",
      configIndex: 0,
    },
    {
      equipmentInstanceKey: "volt-instance",
      equipmentGameId: "/Lotus/Powersuits/Volt/VoltPrime",
      equipmentDisplayName: "Вольт Прайм",
      equipmentImageUrl: null,
      equipmentKind: "warframe",
      configIndex: 1,
    },
  ],
  sellableQuantity: 2,
  resolution: "resolved",
  vaultStatus: "unknown",
  closedMedian48h: 40,
  hasReliablePrice: true,
};

describe("equipped mods presentation", () => {
  it("groups one physical item by its loadout configurations", () => {
    const groups = buildEquippedEquipmentGroups([item]);
    expect(groups).toHaveLength(1);
    expect(groups[0].displayName).toBe("Вольт Прайм");
    expect(groups[0].configs.map((config) => config.index)).toEqual([0, 1]);
    expect(summarizeEquippedMods([item], groups)).toEqual({
      modCopies: 1,
      equipmentCount: 1,
      configCount: 2,
    });
  });

  it("finds groups by mod name and filters equipment kind", () => {
    expect(buildEquippedEquipmentGroups([item], "поток", "all")).toHaveLength(1);
    expect(buildEquippedEquipmentGroups([item], "", "primary")).toEqual([]);
    expect(configLabel(0)).toBe("A");
    expect(configLabel(2)).toBe("C");
  });
});
