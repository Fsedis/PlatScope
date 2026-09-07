import { describe, expect, it } from "vitest";

import {
  bestBountyJob,
  activeBountyView,
  bountyEstimate,
  bountyFeaturedReward,
  bountyJobIdentity,
  bountyAutomaticRefreshAt,
  bountyRotationAt,
  rankedBountyJobs,
  visibleBountyRegions,
  withBountyLivePrices,
  type BountyHunterView,
} from "./bountyHunter";
import { makeBountyHunterMock } from "./bountyHunterMock";

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
          rewards: [{ trackingKey: "market:saved", displayName: "Награда", slug: "saved", rarity: "rare",
            marketKey: { slug: "saved", platform: "pc", rank: null, charges: null, subtype: null, amberStars: null, cyanStars: null }, expectedQuantity: .5, chancePercent: 40, unitPrice: 8.4, expectedPlatinum: 4.2 }],
        },
      ],
    },
  ],
};

describe("bounty hunter view helpers", () => {
  it("убирает заказ Нармер при собственной смене раньше конца ротации региона", () => {
    const now = Date.parse("2026-09-02T11:10:00Z");
    const saved = structuredClone(view);
    saved.regions[0]!.jobs[0]!.expiry = "2026-09-02T11:10:00Z";
    expect(bountyRotationAt(saved)).toBe(now);
    const active = activeBountyView(saved, now)!;
    expect(active.regions[0]!.jobs.map(job => job.id)).toEqual(["priced"]);
    expect(bountyRotationAt(active)).toBe(Date.parse("2026-09-02T12:00:00Z"));
    expect(saved.regions[0]!.jobs).toHaveLength(2);
  });
  it("сортирует по шансу искомой непродаваемой награды, а не первой награды с ценой", () => {
    const mock = makeBountyHunterMock();
    const rows = rankedBountyJobs(mock, { region: "all", onlyPriced: false, query: "аЙя", sort: "reward_chance" });
    expect(rows[0]!.job.title).toBe("Ослабить позиции Гринир");
    expect(bountyFeaturedReward(rows[0]!.job, "Айя")?.chancePercent).toBe(78.2);
    const chances = rows.map(row => bountyFeaturedReward(row.job, "Айя")!.chancePercent);
    expect(chances).toEqual([...chances].sort((a, b) => b - a));
  });

  it("отслеживание ищет точный ключ и нормализует ё в текстовом поиске", () => {
    const mock = makeBountyHunterMock();
    const job = mock.regions[0]!.jobs[0]!;
    job.title = "Чертёж";
    expect(rankedBountyJobs(mock, { region: "all", onlyPriced: false, query: "чертеж", sort: "platinum" })).toHaveLength(1);
    const result = rankedBountyJobs(mock, { region: "all", onlyPriced: false, query: "старое имя", targetKey: job.rewards[0]!.trackingKey, sort: "reward_chance" });
    expect(result.length).toBeGreaterThan(0);
    expect(result.every(row => row.job.rewards.some(reward => reward.trackingKey === job.rewards[0]!.trackingKey))).toBe(true);
  });

  it("скрывает завершённую ротацию, сохраняя заказы других регионов", () => {
    const now = Date.parse("2026-09-06T10:00:00Z");
    const mock = makeBountyHunterMock("partial-expired", now);
    const rows = rankedBountyJobs(mock, { region: "all", onlyPriced: false, query: "", sort: "platinum", now });
    expect(rows).toHaveLength(10);
    expect(rows.some(row => row.regionKey === "cetus")).toBe(false);
    mock.regions[1]!.expiry = "invalid";
    expect(activeBountyView(mock, now)?.regions.map(region => region.key)).toEqual(["necralisk"]);
    expect(mock.regions).toHaveLength(3);
  });

  it("различает неизвестную стоимость и нулевую, не считает ненадёжную цену учтённой", () => {
    const mock = makeBountyHunterMock();
    const job = mock.regions[0]!.jobs[0]!;
    job.rewards[0]!.expectedPlatinum = null;
    expect(bountyEstimate(job)).toEqual({ value: null, total: 1, priced: 0 });
    const options = { region: "cetus", onlyPriced: true, query: job.title, sort: "platinum" as const };
    expect(rankedBountyJobs(mock, options)).toHaveLength(0);
    job.rewards[0]!.expectedPlatinum = 0;
    expect(bountyEstimate(job).value).toBe(0);
    expect(rankedBountyJobs(mock, options)).toHaveLength(1);
  });

  it("после проверки цен меняет главную награду и рейтинг, не смешивая заказы разных регионов", () => {
    const mock = makeBountyHunterMock();
    const job = mock.regions[0]!.jobs[3]!;
    const updated = withBountyLivePrices(mock, new Map([["necramech_continuity", 1000], ["augur_reach", NaN]]))!;
    const updatedJob = updated.regions[0]!.jobs[3]!;
    expect(bountyFeaturedReward(updatedJob)?.slug).toBe("necramech_continuity");
    expect(updatedJob.rewards[0]!.unitPrice).toBe(job.rewards[0]!.unitPrice);
    const rows = rankedBountyJobs(updated, { region: "all", onlyPriced: false, query: "", sort: "platinum" });
    expect(rows[0]!.job.id).toBe(job.id);
    expect(new Set(rows.map(bountyJobIdentity)).size).toBe(rows.length);
    expect(bountyJobIdentity(rows[0]!)).not.toBe(bountyJobIdentity({ ...rows[0]!, expiry: "2027-01-01" }));
  });
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
