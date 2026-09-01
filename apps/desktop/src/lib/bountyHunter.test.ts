import { describe, expect, it } from "vitest";

import {
  bestBountyJob,
  visibleBountyRegions,
  type BountyHunterView,
} from "./bountyHunter";

const view: BountyHunterView = {
  fetchedAt: "2026-09-02T10:00:00Z",
  marketSourceDate: "2026-09-01",
  regions: [
    {
      key: "cetus",
      displayName: "Цетус",
      expiry: "2026-09-02T12:00:00Z",
      jobs: [
        {
          id: "low",
          title: "Заказ",
          minLevel: 5,
          maxLevel: 15,
          minMasteryRank: 0,
          stageCount: 3,
          totalStanding: 1000,
          expectedPlatinum: 0,
          pricedRewardCount: 0,
          rewards: [],
        },
        {
          id: "priced",
          title: "Заказ",
          minLevel: 40,
          maxLevel: 60,
          minMasteryRank: 0,
          stageCount: 5,
          totalStanding: 5000,
          expectedPlatinum: 4.2,
          pricedRewardCount: 2,
          rewards: [],
        },
      ],
    },
  ],
};

describe("bounty hunter view helpers", () => {
  it("keeps only jobs with market rewards when requested", () => {
    const rows = visibleBountyRegions(view, "all", true);
    expect(rows[0]?.jobs.map((job) => job.id)).toEqual(["priced"]);
  });

  it("selects the highest expected platinum job", () => {
    expect(bestBountyJob(view)?.id).toBe("priced");
  });
});
