import { describe, expect, it } from "vitest";

import {
  coverageLabel,
  formatPercent,
  formatRatio,
  refinementLabel,
  rivenCategoryLabel,
  setModeLabel,
  vaultLabel,
} from "./insights";

describe("insights presentation", () => {
  it("явно различает incomplete analytics states", () => {
    expect(setModeLabel("insufficient_pricing")).toBe("Не хватает цен");
    expect(coverageLabel("partial")).toBe("Частичный EV");
    expect(formatRatio(null)).toBe("—");
  });

  it("не превращает vaulted в ценовой прогноз", () => {
    expect(vaultLabel("vaulted")).toBe("В хранилище");
    expect(vaultLabel("unknown")).toBe("Статус неизвестен");
  });

  it("форматирует refinement и premium без потери знака", () => {
    expect(refinementLabel("radiant")).toBe("Сияющая");
    expect(formatPercent(12.34)).toBe("+12,3%");
    expect(formatPercent(-5)).toBe("-5%");
  });

  it("localizes typed insight states without translating canonical values", () => {
    expect(setModeLabel("insufficient_pricing", "en")).toBe("Not enough prices");
    expect(coverageLabel("partial", "en")).toBe("Partial EV");
    expect(refinementLabel("radiant", "en")).toBe("Radiant");
    expect(rivenCategoryLabel("arch_gun", "en")).toBe("Arch-gun");
    expect(rivenCategoryLabel("sentinel_weapon")).toBe("Оружие стража");
  });
});
