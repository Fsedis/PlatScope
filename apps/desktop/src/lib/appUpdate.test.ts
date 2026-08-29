import { describe, expect, it } from "vitest";

import { updateProgressPercent } from "./appUpdate";

describe("updateProgressPercent", () => {
  it("calculates and clamps known download progress", () => {
    expect(updateProgressPercent(25, 100)).toBe(25);
    expect(updateProgressPercent(125, 100)).toBe(100);
    expect(updateProgressPercent(-5, 100)).toBe(0);
  });

  it("keeps progress indeterminate when content length is unknown", () => {
    expect(updateProgressPercent(25, null)).toBeNull();
    expect(updateProgressPercent(25, 0)).toBeNull();
  });
});
