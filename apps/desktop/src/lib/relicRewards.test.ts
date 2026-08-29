import { describe, expect, it } from "vitest";
import type { PriceRecommendation } from "./market";
import {
  confidencePercent,
  overlayContentScale,
  ownedSetParts,
  rewardPrice,
  type RelicRewardChoice,
} from "./relicRewards";

function recommendation(overrides: Partial<PriceRecommendation> = {}): PriceRecommendation {
  return {
    key: { slug: "test", platform: "pc", rank: null, subtype: null, amberStars: null, cyanStars: null },
    provider: "relics_run",
    sourceDate: "2026-08-29",
    fairPrice: null,
    listPrice: null,
    quickSell: null,
    lowestAsk: null,
    depthThree: null,
    depthPrice: null,
    closedVolume: null,
    liveSellOrderCount: 0,
    liveBuyOrderCount: 0,
    confidence: "unknown",
    freshness: "fresh",
    reasons: [],
    ...overrides,
  };
}

function choice(price: PriceRecommendation): RelicRewardChoice {
  return {
    slot: 0,
    rawText: "Test Prime Blueprint",
    confidence: 1,
    itemId: "test",
    slug: "test_prime_blueprint",
    displayName: "Test Prime Blueprint",
    market: {
      itemId: "test",
      displayName: "Test Prime Blueprint",
      itemKind: "standard",
      masteryRequirement: null,
      recommendation: price,
    },
    ducats: 45,
    ownedQuantity: 2,
    set: null,
    completesSet: null,
    choiceValue: null,
    recommended: false,
  };
}

describe("relic reward helper", () => {
  it("shows fair price before listing and quick-sale fallbacks", () => {
    expect(rewardPrice(choice(recommendation({ fairPrice: 44, listPrice: 42, quickSell: 35 })))).toBe(44);
    expect(rewardPrice(choice(recommendation({ listPrice: 42, quickSell: 35 })))).toBe(42);
    expect(rewardPrice(choice(recommendation({ quickSell: 35 })))).toBe(35);
  });

  it("clamps OCR confidence for display", () => {
    expect(confidencePercent(-1)).toBe(0);
    expect(confidencePercent(0.736)).toBe(74);
    expect(confidencePercent(2)).toBe(100);
  });

  it("scales overlay contents together with the native window and Windows DPI", () => {
    expect(overlayContentScale(0.86, 1)).toBeCloseTo(0.86);
    expect(overlayContentScale(0.86, 1.25)).toBeCloseTo(0.688);
    expect(overlayContentScale(Number.NaN, 0)).toBe(1);
  });

  it("summarizes collected set parts and required duplicate pieces", () => {
    const reward = choice(recommendation());
    reward.set = {
      setName: "Test Prime Set",
      setPrice: 90,
      readyComponents: 2,
      totalComponents: 4,
      parts: [
        { name: "Blueprint", imageUrl: null, ownedQuantity: 1, requiredQuantity: 1, isReward: true },
        { name: "Blade", imageUrl: null, ownedQuantity: 1, requiredQuantity: 2, isReward: false },
        { name: "Handle", imageUrl: null, ownedQuantity: 0, requiredQuantity: 1, isReward: false },
      ],
    };
    expect(ownedSetParts(reward)).toBe("Blueprint · Blade 1/2");
  });
});
