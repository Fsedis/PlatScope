import { describe, expect, it } from "vitest";

import {
  bestBountyJob,
  bountyAutomaticRefreshAt,
  bountyRotationAt,
  rankedBountyJobs,
  visibleBountyRegions,
  withBountyLivePrices,
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
  it("updates ranking and coverage from live prices without changing saved data", () => {
    const saved: BountyHunterView = {
      ...view,
      regions: [{ ...view.regions[0]!, jobs: [
        { ...view.regions[0]!.jobs[1]!, id: "saved", expectedPlatinum: 4.2 },
        { ...view.regions[0]!.jobs[0]!, id: "live", marketRewardCount: 1, rewards: [{
          trackingKey: "reward", displayName: "Награда", slug: "reward", rarity: "rare",
          marketKey: { slug: "reward", platform: "pc", rank: null, charges: null, subtype: null, amberStars: null, cyanStars: null }, expectedQuantity: 0.5, chancePercent: 50,
          unitPrice: null, expectedPlatinum: null,
        }] },
      ] }],
    };
    const updated = withBountyLivePrices(saved, new Map([["reward", 20]]));
    const rows = rankedBountyJobs(updated, { region: "cetus", onlyPriced: true, query: "", sort: "platinum" });
    expect(rows.map((row) => row.job.id)).toEqual(["live", "saved"]);
    expect(rows[0]!.job).toMatchObject({ expectedPlatinum: 10, pricedRewardCount: 1, priceCoveragePercent: 100 });
    expect(saved.regions[0]!.jobs[1]!.rewards[0]!.unitPrice).toBeNull();
    expect(withBountyLivePrices(saved, new Map())).toBe(saved);
    expect(rankedBountyJobs(updated, { region: "fortuna", onlyPriced: true, query: "", sort: "platinum" })).toEqual([]);
    expect(rankedBountyJobs(updated, { region: "all", onlyPriced: true, query: "не существует", sort: "platinum" })).toEqual([]);
  });
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

  it("refreshes automatically only when the nearest rotation changes", () => {
    expect(bountyAutomaticRefreshAt(view)).toBe(
      new Date("2026-09-02T12:00:00Z").getTime(),
    );

    expect(bountyAutomaticRefreshAt({ ...view, fetchedAt: "2026-09-02T11:59:30Z" })).toBe(
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
