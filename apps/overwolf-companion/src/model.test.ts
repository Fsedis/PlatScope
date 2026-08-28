import { describe, expect, it } from "vitest";
import {
  analyzeInventoryValue,
  createCompanionEnvelope,
  englishPlural,
  extractInventoryValue,
  isAbsoluteJsonPath,
  isWarframeRunning,
  russianPlural,
  serializeEnvelope,
  SnapshotError,
} from "./model";

const inventory = {
  Inventory: {
    MiscItems: [
      { ItemType: "/Lotus/Test/Part", ItemCount: 2 },
      { ItemType: "/Lotus/Test/Part", ItemCount: 1 },
    ],
    Upgrades: [{ ItemType: "/Lotus/Test/Mod", ItemCount: 1, Rank: 10 }],
  },
};

describe("Overwolf inventory boundary", () => {
  it("extracts documented event and getInfo wrapper shapes", () => {
    expect(
      extractInventoryValue({
        feature: "match_info",
        info: JSON.stringify({ match_info: { inventory: JSON.stringify(inventory) } }),
      }),
    ).toBe(JSON.stringify(inventory));
    expect(extractInventoryValue({ info: { match_info: { inventory } } })).toEqual(inventory);
    expect(
      extractInventoryValue({ category: "game_info", key: "inventory", value: inventory }),
    ).toEqual(inventory);
    expect(extractInventoryValue({ feature: "chat", key: "inventory", value: inventory })).toBeNull();
  });

  it("builds a bounded summary without treating duplicate rows as distinct items", () => {
    const result = analyzeInventoryValue(inventory);
    expect(result.rowCount).toBe(3);
    expect(result.distinctItemCount).toBe(2);
    expect(result.totalQuantity).toBe(4);
    expect(result.categories).toEqual([
      { name: "MiscItems", rows: 2, quantity: 3 },
      { name: "Upgrades", rows: 1, quantity: 1 },
    ]);
  });

  it("fails closed for malformed, empty and invalid item payloads", () => {
    expect(() => analyzeInventoryValue("{"))
      .toThrowError(expect.objectContaining<Partial<SnapshotError>>({ code: "invalid_json" }));
    expect(() => analyzeInventoryValue({ Inventory: { Credits: 100 } }))
      .toThrowError(expect.objectContaining<Partial<SnapshotError>>({ code: "items_missing" }));
    expect(() => analyzeInventoryValue({ rows: [{ ItemType: "/Lotus/Test", ItemCount: 0 }] }))
      .toThrowError(expect.objectContaining<Partial<SnapshotError>>({ code: "invalid_item" }));
  });

  it("creates only the allowlisted v1 envelope fields", () => {
    const analysis = analyzeInventoryValue(inventory);
    const envelope = createCompanionEnvelope(analysis, new Date("2026-08-27T10:15:30Z"));
    expect(Object.keys(envelope)).toEqual([
      "schemaVersion",
      "producer",
      "observedAt",
      "gameId",
      "feature",
      "key",
      "complete",
      "value",
    ]);
    expect(envelope).toMatchObject({
      schemaVersion: 1,
      producer: "platscope-overwolf-companion",
      observedAt: "2026-08-27T10:15:30.000Z",
      gameId: 8954,
      feature: "match_info",
      key: "inventory",
      complete: true,
    });
    expect(serializeEnvelope(envelope)).not.toContain("username");
  });

  it("accepts only explicit absolute Windows JSON destinations", () => {
    expect(isAbsoluteJsonPath("C:\\Users\\Dmitrii\\inventory.json")).toBe(true);
    expect(isAbsoluteJsonPath("\\\\server\\share\\PlatScope\\inventory.json")).toBe(true);
    expect(isAbsoluteJsonPath("inventory.json")).toBe(false);
    expect(isAbsoluteJsonPath("C:\\Users\\Dmitrii\\inventory.txt")).toBe(false);
    expect(isAbsoluteJsonPath("C:\\bad|name\\inventory.json")).toBe(false);
  });

  it("recognizes Warframe launcher-suffixed and direct game IDs", () => {
    expect(isWarframeRunning({ isRunning: true, id: 8954 })).toBe(true);
    expect(isWarframeRunning({ isRunning: true, id: 89541 })).toBe(true);
    expect(isWarframeRunning({ isRunning: false, id: 8954 })).toBe(false);
    expect(isWarframeRunning({ isRunning: true, id: 5426 })).toBe(false);
  });

  it("uses correct Russian row forms", () => {
    expect(russianPlural(1, "строка", "строки", "строк")).toBe("строка");
    expect(russianPlural(2, "строка", "строки", "строк")).toBe("строки");
    expect(russianPlural(4, "строка", "строки", "строк")).toBe("строки");
    expect(russianPlural(5, "строка", "строки", "строк")).toBe("строк");
    expect(russianPlural(11, "строка", "строки", "строк")).toBe("строк");
    expect(russianPlural(21, "строка", "строки", "строк")).toBe("строка");
  });

  it("uses the English singular only for one row", () => {
    expect(englishPlural(1, "row", "rows")).toBe("row");
    expect(englishPlural(0, "row", "rows")).toBe("rows");
    expect(englishPlural(2, "row", "rows")).toBe("rows");
  });
});
