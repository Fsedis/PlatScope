import { describe, expect, it } from "vitest";

import type { BountyHunterView } from "./bountyHunter";
import {
  currentBountyAppearanceIds,
  detectBountyRewardAlerts,
  loadBountyAppearanceIds,
  loadBountyWatchPreferences,
  saveBountyAppearanceIds,
  saveBountyWatchPreferences,
  type BountyWatchPreferences,
} from "./bountyWatchlist";

class MemoryStorage {
  private readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

const preferences: BountyWatchPreferences = {
  enabled: true,
  rewards: [
    { key: "worldstate:aya", displayName: "Айя", imageUrl: null },
  ],
};

function bountyView(expiry = "2026-09-04T12:00:00Z"): BountyHunterView {
  return {
    fetchedAt: "2026-09-04T10:00:00Z",
    marketSourceDate: "2026-09-03",
    regions: [
      {
        key: "cetus",
        displayName: "Цетус",
        expiry,
        jobs: [
          {
            id: "tier-five",
            title: "Заказ 40–60",
            minLevel: 40,
            maxLevel: 60,
            minMasteryRank: 0,
            stageCount: 5,
            totalStanding: 7_420,
            expectedPlatinum: 0,
            marketRewardCount: 0,
            pricedRewardCount: 0,
            priceCoveragePercent: 0,
            rewards: [
              {
                trackingKey: "worldstate:aya",
                displayName: "Айя",
                imageUrl: null,
                slug: null,
                marketKey: null,
                ownedQuantity: 1,
                rarity: "Редкая",
                expectedQuantity: 0.2,
                chancePercent: 20,
                unitPrice: null,
                expectedPlatinum: null,
              },
            ],
          },
        ],
      },
    ],
  };
}

describe("bounty reward watchlist", () => {
  it("persists watched rewards and notification state", () => {
    const storage = new MemoryStorage();
    expect(saveBountyWatchPreferences(preferences, storage)).toBe(true);
    expect(loadBountyWatchPreferences(storage)).toEqual(preferences);

    expect(saveBountyAppearanceIds(["one", "one", "two"], storage)).toBe(true);
    expect(loadBountyAppearanceIds(storage)).toEqual(["one", "two"]);
  });

  it("notifies once for the same reward appearance", () => {
    const first = detectBountyRewardAlerts(bountyView(), preferences, []);
    expect(first.alerts).toEqual([
      {
        key: "worldstate:aya",
        displayName: "Айя",
        locations: ["Цетус — Заказ 40–60"],
      },
    ]);

    const repeated = detectBountyRewardAlerts(
      bountyView(),
      preferences,
      first.currentAppearanceIds,
    );
    expect(repeated.alerts).toEqual([]);
  });

  it("notifies again when the reward returns in another rotation", () => {
    const previous = detectBountyRewardAlerts(bountyView(), preferences, []);
    const next = detectBountyRewardAlerts(
      bountyView("2026-09-04T15:00:00Z"),
      preferences,
      previous.currentAppearanceIds,
    );
    expect(next.alerts).toHaveLength(1);
  });

  it("keeps the current appearance as a baseline while notifications are disabled", () => {
    const result = detectBountyRewardAlerts(
      bountyView(),
      { ...preferences, enabled: false },
      [],
    );
    expect(result.alerts).toEqual([]);
    expect(result.currentAppearanceIds).toHaveLength(1);
  });

  it("can baseline only a newly watched reward", () => {
    expect(currentBountyAppearanceIds(
      bountyView(),
      preferences.rewards,
      new Set(["worldstate:aya"]),
    )).toHaveLength(1);
    expect(currentBountyAppearanceIds(
      bountyView(),
      preferences.rewards,
      new Set(["market:other"]),
    )).toEqual([]);
  });
});
