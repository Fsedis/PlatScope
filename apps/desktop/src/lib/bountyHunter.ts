import type { MarketVariantKey } from "./market";

export interface BountyRewardView {
  trackingKey: string;
  displayName: string;
  imageUrl?: string | null;
  slug?: string | null;
  marketKey?: MarketVariantKey | null;
  ownedQuantity?: number | null;
  rarity: string;
  expectedQuantity: number;
  chancePercent: number;
  unitPrice?: number | null;
  expectedPlatinum?: number | null;
}

export interface BountyJobView {
  id: string;
  title: string;
  minLevel: number;
  maxLevel: number;
  minMasteryRank: number;
  stageCount: number;
  totalStanding: number;
  timeBound?: string | null;
  expectedPlatinum: number;
  marketRewardCount: number;
  pricedRewardCount: number;
  priceCoveragePercent: number;
  rewards: BountyRewardView[];
}

export interface BountyRegionView {
  key: string;
  displayName: string;
  expiry: string;
  jobs: BountyJobView[];
}

export interface BountyHunterView {
  fetchedAt: string;
  marketSourceDate?: string | null;
  regions: BountyRegionView[];
}

export type BountySortKey = "platinum" | "reward_chance" | "level" | "rotation";

export interface RankedBountyJob {
  regionKey: string;
  regionName: string;
  expiry: string;
  job: BountyJobView;
}

export const BOUNTY_AUTO_RETRY_DELAY_MS = 30 * 1000;

export function withBountyLivePrices(
  view: BountyHunterView | null,
  prices: ReadonlyMap<string, number>,
): BountyHunterView | null {
  if (!view || prices.size === 0) return view;
  return {
    ...view,
    regions: view.regions.map((region) => ({
      ...region,
      jobs: region.jobs.map((job) => {
        if (!job.rewards.some((reward) => reward.slug && prices.has(reward.slug))) return job;
        const rewards = job.rewards.map((reward) => {
          const price = reward.slug ? prices.get(reward.slug) : undefined;
          return price === undefined ? reward : {
            ...reward, unitPrice: price, expectedPlatinum: price * reward.expectedQuantity,
          };
        });
        const pricedRewardCount = rewards.filter((reward) => reward.marketKey && reward.unitPrice != null).length;
        return {
          ...job, rewards, pricedRewardCount,
          expectedPlatinum: rewards.reduce((total, reward) => total + (reward.expectedPlatinum ?? 0), 0),
          priceCoveragePercent: job.marketRewardCount > 0 ? pricedRewardCount / job.marketRewardCount * 100 : 0,
        };
      }),
    })),
  };
}

function validTimestamp(value: string | null | undefined): number | null {
  if (!value) return null;
  const timestamp = new Date(value).getTime();
  return Number.isFinite(timestamp) ? timestamp : null;
}

export function bountyRotationAt(view: BountyHunterView | null): number | null {
  if (!view) return null;
  const expiries = view.regions
    .map((region) => validTimestamp(region.expiry))
    .filter((value): value is number => value !== null);
  return expiries.length > 0 ? Math.min(...expiries) : null;
}

export function bountyAutomaticRefreshAt(view: BountyHunterView | null): number | null {
  return bountyRotationAt(view);
}

export function visibleBountyRegions(
  view: BountyHunterView | null,
  region: string,
  onlyPriced: boolean,
): BountyRegionView[] {
  if (!view) return [];
  return view.regions
    .filter((row) => region === "all" || row.key === region)
    .map((row) => ({
      ...row,
      jobs: onlyPriced
        ? row.jobs.filter((job) => job.pricedRewardCount > 0)
        : row.jobs,
    }))
    .filter((row) => row.jobs.length > 0);
}

export function bestBountyJob(view: BountyHunterView | null): BountyJobView | null {
  if (!view) return null;
  return view.regions
    .flatMap((region) => region.jobs)
    .sort((left, right) => right.expectedPlatinum - left.expectedPlatinum)[0] ?? null;
}

function topPricedRewardChance(job: BountyJobView): number {
  return job.rewards.find((reward) => reward.unitPrice != null)?.chancePercent ?? 0;
}

export function rankedBountyJobs(
  view: BountyHunterView | null,
  options: {
    region: string;
    onlyPriced: boolean;
    query: string;
    sort: BountySortKey;
  },
): RankedBountyJob[] {
  if (!view) return [];
  const query = options.query.trim().toLocaleLowerCase("ru");
  const rows = view.regions.flatMap((region) => region.jobs.map((job) => ({
    regionKey: region.key,
    regionName: region.displayName,
    expiry: region.expiry,
    job,
  }))).filter((row) => {
    if (options.region !== "all" && row.regionKey !== options.region) return false;
    if (options.onlyPriced && row.job.pricedRewardCount === 0) return false;
    if (!query) return true;
    return row.job.title.toLocaleLowerCase("ru").includes(query)
      || row.regionName.toLocaleLowerCase("ru").includes(query)
      || row.job.rewards.some((reward) => reward.displayName.toLocaleLowerCase("ru").includes(query));
  });
  rows.sort((left, right) => {
    if (options.sort === "reward_chance") {
      return topPricedRewardChance(right.job) - topPricedRewardChance(left.job)
        || right.job.expectedPlatinum - left.job.expectedPlatinum;
    }
    if (options.sort === "level") {
      return left.job.minLevel - right.job.minLevel
        || right.job.expectedPlatinum - left.job.expectedPlatinum;
    }
    if (options.sort === "rotation") {
      return new Date(left.expiry).getTime() - new Date(right.expiry).getTime()
        || right.job.expectedPlatinum - left.job.expectedPlatinum;
    }
    return right.job.expectedPlatinum - left.job.expectedPlatinum
      || right.job.priceCoveragePercent - left.job.priceCoveragePercent
      || left.job.minLevel - right.job.minLevel;
  });
  return rows;
}

export function bountyRewardCount(job: BountyJobView): number {
  return job.rewards.filter((reward) => reward.unitPrice != null).length;
}
