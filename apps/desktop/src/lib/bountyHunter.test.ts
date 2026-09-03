import { describe, expect, it } from "vitest";

import {
  BOUNTY_AUTO_REFRESH_INTERVAL_MS,
  bestBountyJob,
  bountyAutomaticRefreshAt,
  bountyRotationAt,
  rankedBountyJobs,
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
          marketRewardCount: 0,
          pricedRewardCount: 0,
          priceCoveragePercent: 0,
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
          marketRewardCount: 2,
          pricedRewardCount: 2,
          priceCoveragePercent: 100,
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

  it("uses the nearest region rotation for the live countdown", () => {
    const withDifferentRotations: BountyHunterView = {
      ...view,
      regions: [
        view.regions[0]!,
        { ...view.regions[0]!, key: "fortuna", expiry: "2026-09-02T11:30:00Z" },
      ],
    };

    expect(bountyRotationAt(withDifferentRotations)).toBe(
      new Date("2026-09-02T11:30:00Z").getTime(),
    );
  });

  it("refreshes automatically every minute or at rotation, whichever comes first", () => {
    expect(bountyAutomaticRefreshAt(view)).toBe(
      new Date(view.fetchedAt).getTime() + BOUNTY_AUTO_REFRESH_INTERVAL_MS,
    );

    const soonRotation: BountyHunterView = {
      ...view,
      fetchedAt: "2026-09-02T11:59:30Z",
    };
    expect(bountyAutomaticRefreshAt(soonRotation)).toBe(
      new Date("2026-09-02T12:00:00Z").getTime(),
    );
  });

  it("builds one ranked list and filters it by Russian text", () => {
    const rows = rankedBountyJobs(view, {
      region: "all",
      onlyPriced: true,
      query: "заказ",
      sort: "platinum",
    });
    expect(rows.map((row) => row.job.id)).toEqual(["priced"]);
    expect(rows[0]?.regionKey).toBe("cetus");
  });
});
