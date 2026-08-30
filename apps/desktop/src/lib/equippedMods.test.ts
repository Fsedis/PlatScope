import { describe, expect, it } from "vitest";

import type { InventoryViewItem } from "./inventory";
import {
  buildEquippedModEntries,
  configLabel,
  filterEquippedModEntries,
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
  it("groups every placement under the selected mod", () => {
    const entries = buildEquippedModEntries([item]);

    expect(entries).toHaveLength(1);
    expect(entries[0].displayName).toBe("Поток Прайм");
    expect(entries[0].locations).toHaveLength(1);
    expect(entries[0].locations[0].displayName).toBe("Вольт Прайм");
    expect(entries[0].locations[0].configIndexes).toEqual([0, 1]);
    expect(entries[0].equipmentCount).toBe(1);
    expect(entries[0].configCount).toBe(2);
  });

  it("filters the mod list by mod name and placement type", () => {
    const entries = buildEquippedModEntries([item]);

    expect(filterEquippedModEntries(entries, "поток", "all")).toHaveLength(1);
    expect(filterEquippedModEntries(entries, "вольт", "all")).toEqual([]);
    expect(filterEquippedModEntries(entries, "", "primary")).toEqual([]);
    expect(filterEquippedModEntries(entries, "", "warframe")).toHaveLength(1);
  });

  it("counts physical copies separately from loadout placements", () => {
    const entries = buildEquippedModEntries([item]);

    expect(summarizeEquippedMods(entries)).toEqual({
      modVariants: 1,
      modCopies: 1,
      equipmentCount: 1,
      configCount: 2,
    });
    expect(configLabel(0)).toBe("A");
    expect(configLabel(2)).toBe("C");
  });
});
