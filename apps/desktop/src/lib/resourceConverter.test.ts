import { describe, expect, it } from "vitest";

import { compactNumber, visibleArcaneDecisions, type ArcaneConversionDecision } from "./resourceConverter";

function row(slug: string, value: number): ArcaneConversionDecision {
  return {
    decision: "sell",
    slug,
    displayName: slug,
    rank: 0,
    quantity: 1,
    marketPriceEach: value,
    vosforEach: 20,
    vosforTotal: 20,
    equivalentPlatinumEach: 1,
    estimatedPlatinum: value,
  };
}

describe("resource converter presentation", () => {
  it("keeps the compact arcane list bounded until the user expands it", () => {
    const rows = [row("a", 5), row("b", 4), row("c", 3), row("d", 2), row("e", 1)];
    expect(visibleArcaneDecisions(rows, false).map((item) => item.slug)).toEqual(["a", "b", "c", "d"]);
    expect(visibleArcaneDecisions(rows, true)).toHaveLength(5);
  });

  it("formats balances without misleading decimals", () => {
    expect(compactNumber(25_000, "ru-RU").replace(/\s/g, " ")).toBe("25 000");
  });
});
