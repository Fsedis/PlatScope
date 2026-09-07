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
  expiry?: string | null;
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
  refreshFailed?: boolean;
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
          return price === undefined || !Number.isFinite(price) || price <= 0 ? reward : {
            ...reward, unitPrice: price, expectedPlatinum: price * reward.expectedQuantity,
          };
        });
        const pricedRewardCount = bountyEstimate({ ...job, rewards }).priced;
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
    .flatMap((region) => [validTimestamp(region.expiry), ...region.jobs.map(job => validTimestamp(job.expiry))])
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

function normalized(value: string): string {
  return value.trim().toLocaleLowerCase("ru").replaceAll("ё", "е");
}

export function activeBountyView(view: BountyHunterView | null, now: number): BountyHunterView | null {
  return view ? { ...view, regions: view.regions.filter(region => (validTimestamp(region.expiry) ?? 0) > now)
    .map(region => ({ ...region, jobs: region.jobs.filter(job => !job.expiry || (validTimestamp(job.expiry) ?? 0) > now) }))
    .filter(region => region.jobs.length > 0) } : null;
}

export function bountyJobIdentity(row: RankedBountyJob): string {
  return JSON.stringify([row.regionKey, row.expiry, row.job.id]);
}

export function bountyFeaturedReward(job: BountyJobView, query = "", targetKey = "", byChance = false): BountyRewardView | null {
  const matched = job.rewards.filter(reward => targetKey
    ? reward.trackingKey === targetKey : !!normalized(query) && normalized(reward.displayName).includes(normalized(query)));
  return [...(matched.length ? matched : job.rewards)].sort((a, b) =>
    matched.length || byChance ? b.chancePercent - a.chancePercent
      : (b.expectedPlatinum ?? -1) - (a.expectedPlatinum ?? -1) || b.chancePercent - a.chancePercent,
  )[0] ?? null;
}

export function bountyEstimate(job: BountyJobView): { value: number | null; priced: number; total: number } {
  const market = job.rewards.filter(reward => reward.marketKey);
  const priced = market.filter(reward => reward.expectedPlatinum != null && Number.isFinite(reward.expectedPlatinum) && reward.expectedPlatinum >= 0);
  return { value: priced.length ? priced.reduce((sum, reward) => sum + reward.expectedPlatinum!, 0) : null, priced: priced.length, total: market.length };
}

export function rankedBountyJobs(
  view: BountyHunterView | null,
  options: {
    region: string;
    onlyPriced: boolean;
    query: string;
    sort: BountySortKey;
    now?: number;
    targetKey?: string;
  },
): RankedBountyJob[] {
  if (!view) return [];
  const query = normalized(options.query);
  const activeView = options.now === undefined ? view : activeBountyView(view, options.now)!;
  const rows = activeView.regions.flatMap((region) => region.jobs.map((job) => ({
    regionKey: region.key,
    regionName: region.displayName,
    expiry: job.expiry ?? region.expiry,
    job,
  }))).filter((row) => {
    if (options.region !== "all" && row.regionKey !== options.region) return false;
    if (options.onlyPriced && bountyEstimate(row.job).value === null) return false;
    if (options.targetKey) return row.job.rewards.some(reward => reward.trackingKey === options.targetKey);
    if (!query) return true;
    return normalized(row.job.title).includes(query)
      || normalized(row.regionName).includes(query)
      || row.job.rewards.some((reward) => normalized(reward.displayName).includes(query));
  });
  rows.sort((left, right) => {
    const leftEstimate = bountyEstimate(left.job), rightEstimate = bountyEstimate(right.job);
    const byValue = (rightEstimate.value ?? -1) - (leftEstimate.value ?? -1);
    if (options.sort === "reward_chance") {
      const chance = (job: BountyJobView) => {
        const reward = bountyFeaturedReward(job, query, options.targetKey, true);
        if (query && !options.targetKey && reward && !normalized(reward.displayName).includes(query)) return -1;
        return reward?.chancePercent ?? -1;
      };
      return chance(right.job) - chance(left.job)
        || byValue;
    }
    if (options.sort === "level") {
      return left.job.minLevel - right.job.minLevel
        || byValue;
    }
    if (options.sort === "rotation") {
      return new Date(left.expiry).getTime() - new Date(right.expiry).getTime()
        || byValue;
    }
    return byValue
      || (rightEstimate.total ? rightEstimate.priced / rightEstimate.total : 0) - (leftEstimate.total ? leftEstimate.priced / leftEstimate.total : 0)
      || left.job.minLevel - right.job.minLevel;
  });
  return rows;
}

export function bountyRewardCount(job: BountyJobView): number {
  return job.rewards.filter((reward) => reward.unitPrice != null).length;
}
