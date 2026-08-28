import { describe, expect, it } from "vitest";

import { buildHistoryChart, formatChange, timingLabel } from "./history";

describe("history presentation", () => {
  it("не строит линию по одной точке", () => {
    expect(
      buildHistoryChart([
        {
          sourceDate: "2026-08-26",
          closedMedian: 30,
          closedVolume: 5,
          sellMedian: null,
          buyMedian: null,
        },
      ]),
    ).toBeNull();
  });

  it("строит конечный SVG path для одинаковых цен", () => {
    const model = buildHistoryChart([
      { sourceDate: "2026-08-25", closedMedian: 30, closedVolume: 5, sellMedian: null, buyMedian: null },
      { sourceDate: "2026-08-26", closedMedian: 30, closedVolume: 8, sellMedian: null, buyMedian: null },
    ]);
    expect(model?.path).not.toContain("NaN");
    expect(model?.dots).toHaveLength(2);
  });

  it("локализует изменение и timing", () => {
    expect(formatChange(12.34)).toBe("+12,3%");
    expect(timingLabel("peak")).toContain("PEAK");
  });
});
