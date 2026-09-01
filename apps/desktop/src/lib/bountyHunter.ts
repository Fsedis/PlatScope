export interface BountyRewardView {
  displayName: string;
  imageUrl?: string | null;
  slug?: string | null;
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
  pricedRewardCount: number;
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

export function bountyRewardCount(job: BountyJobView): number {
  return job.rewards.filter((reward) => reward.unitPrice != null).length;
}
