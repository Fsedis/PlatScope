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

export const BOUNTY_AUTO_REFRESH_INTERVAL_MS = 5 * 60 * 1000;
export const BOUNTY_AUTO_RETRY_DELAY_MS = 30 * 1000;

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
  if (!view) return null;
  const fetchedAt = validTimestamp(view.fetchedAt);
  const periodicRefreshAt = fetchedAt === null
    ? null
    : fetchedAt + BOUNTY_AUTO_REFRESH_INTERVAL_MS;
  const rotationAt = bountyRotationAt(view);
  if (periodicRefreshAt === null) return rotationAt;
  if (rotationAt === null) return periodicRefreshAt;
  return Math.min(periodicRefreshAt, rotationAt);
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
