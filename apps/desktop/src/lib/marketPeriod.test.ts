import { describe, expect, it } from "vitest";
import { marketChangeLabel, summarizeMarketPeriod } from "./marketPeriod";

describe("понятные показатели периода", () => {
  it("различает цену, дневной объём и итог периода", () => {
    const data = summarizeMarketPeriod([9, 8, 10, 9, 9, 9, 8].map((closedMedian, index) => ({
      sourceDate: `2026-09-0${index + 1}`, closedMedian, closedVolume: [23, 13, 14, 18, 23, 15, 15][index],
    })));
    expect(data.median).toBe(9);
    expect(data.low).toBe(8);
    expect(data.high).toBe(10);
    expect(data.volume).toBe(121);
    expect(data.dailyVolume).toBeCloseTo(17.2857);
  });
  it("не усредняет только известные дни при пробелах", () => {
    const data = summarizeMarketPeriod([
      { sourceDate: "2026-09-01", closedMedian: 10, closedVolume: 0 },
      { sourceDate: "2026-09-02", closedMedian: null, closedVolume: null },
    ]);
    expect(data.observedDays).toBe(1);
    expect(data.dailyVolume).toBeNull();
    expect(data.volume).toBeNull();
    expect(summarizeMarketPeriod([]).dailyVolume).toBeNull();
  });
  it("отличает неизвестное изменение от стабильности и небольшого движения", () => {
    expect(marketChangeLabel(null)).toBe("Нет сравнения");
    expect(marketChangeLabel(0)).toBe("Без изменений");
    expect(marketChangeLabel(-7.91)).toBe("↓ 7,9%");
    expect(marketChangeLabel(.01)).toBe("↑ <0,1%");
  });
});
