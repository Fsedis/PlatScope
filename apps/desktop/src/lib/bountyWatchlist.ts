import type {
  BountyHunterView,
  BountyRewardView,
} from "./bountyHunter";

export interface BountyWatchedReward {
  key: string;
  displayName: string;
  imageUrl: string | null;
}

export interface BountyWatchPreferences {
  enabled: boolean;
  rewards: BountyWatchedReward[];
}

export interface BountyRewardAlert {
  key: string;
  displayName: string;
  locations: string[];
}

export interface BountyAlertResult {
  alerts: BountyRewardAlert[];
  currentAppearanceIds: string[];
}

interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

interface BountyRewardAppearance {
  id: string;
  key: string;
  displayName: string;
  location: string;
}

const WATCHLIST_KEY = "platscope.bounty-watchlist.v1";
const APPEARANCES_KEY = "platscope.bounty-watch-appearances.v1";
const MAX_WATCHED_REWARDS = 200;
const MAX_APPEARANCES = 1_000;

export const BOUNTY_WATCHLIST_CHANGED_EVENT = "platscope:bounty-watchlist-changed";
export const BOUNTY_VIEW_AVAILABLE_EVENT = "platscope:bounty-view-available";
export const BOUNTY_VIEW_REFRESHED_EVENT = "platscope:bounty-view-refreshed";

export const DEFAULT_BOUNTY_WATCH_PREFERENCES: BountyWatchPreferences = {
  enabled: true,
  rewards: [],
};

export function watchedReward(reward: BountyRewardView): BountyWatchedReward {
  return {
    key: reward.trackingKey,
    displayName: reward.displayName,
    imageUrl: reward.imageUrl ?? null,
  };
}

export function loadBountyWatchPreferences(
  storage: StorageLike | null = defaultStorage(),
): BountyWatchPreferences {
  const value = readRecord(WATCHLIST_KEY, storage);
  const rewards = Array.isArray(value?.rewards)
    ? sanitizeRewards(value.rewards)
    : [];
  return {
    enabled: typeof value?.enabled === "boolean" ? value.enabled : true,
    rewards,
  };
}

export function saveBountyWatchPreferences(
  preferences: BountyWatchPreferences,
  storage: StorageLike | null = defaultStorage(),
): boolean {
  if (!storage) return false;
  try {
    storage.setItem(WATCHLIST_KEY, JSON.stringify({
      version: 1,
      enabled: preferences.enabled,
      rewards: sanitizeRewards(preferences.rewards),
    }));
    return true;
  } catch {
    return false;
  }
}

export function loadBountyAppearanceIds(
  storage: StorageLike | null = defaultStorage(),
): string[] {
  const value = readRecord(APPEARANCES_KEY, storage);
  if (!Array.isArray(value?.appearanceIds)) return [];
  return value.appearanceIds
    .filter((item): item is string => typeof item === "string" && item.length <= 1_024)
    .slice(-MAX_APPEARANCES);
}

export function saveBountyAppearanceIds(
  appearanceIds: readonly string[],
  storage: StorageLike | null = defaultStorage(),
): boolean {
  if (!storage) return false;
  try {
    storage.setItem(APPEARANCES_KEY, JSON.stringify({
      version: 1,
      appearanceIds: [...new Set(appearanceIds)].slice(-MAX_APPEARANCES),
    }));
    return true;
  } catch {
    return false;
  }
}

export function detectBountyRewardAlerts(
  view: BountyHunterView,
  preferences: BountyWatchPreferences,
  previousAppearanceIds: readonly string[],
): BountyAlertResult {
  const appearances = collectAppearances(view, preferences.rewards);
  const previous = new Set(previousAppearanceIds);
  const grouped = new Map<string, BountyRewardAlert>();

  if (preferences.enabled) {
    for (const appearance of appearances) {
      if (previous.has(appearance.id)) continue;
      const alert = grouped.get(appearance.key) ?? {
        key: appearance.key,
        displayName: appearance.displayName,
        locations: [],
      };
      if (!alert.locations.includes(appearance.location)) {
        alert.locations.push(appearance.location);
      }
      grouped.set(appearance.key, alert);
    }
  }

  return {
    alerts: [...grouped.values()],
    currentAppearanceIds: appearances.map((appearance) => appearance.id),
  };
}

export function currentBountyAppearanceIds(
  view: BountyHunterView,
  watched: readonly BountyWatchedReward[],
  onlyKeys?: ReadonlySet<string>,
): string[] {
  return collectAppearances(
    view,
    onlyKeys ? watched.filter((reward) => onlyKeys.has(reward.key)) : watched,
  ).map((appearance) => appearance.id);
}

function collectAppearances(
  view: BountyHunterView,
  watched: readonly BountyWatchedReward[],
): BountyRewardAppearance[] {
  const watchedByKey = new Map(watched.map((reward) => [reward.key, reward]));
  const appearances: BountyRewardAppearance[] = [];
  for (const region of view.regions) {
    for (const job of region.jobs) {
      for (const reward of job.rewards) {
        const favorite = watchedByKey.get(reward.trackingKey);
        if (!favorite) continue;
        appearances.push({
          id: JSON.stringify([reward.trackingKey, region.key, region.expiry, job.id]),
          key: reward.trackingKey,
          displayName: reward.displayName || favorite.displayName,
          location: `${region.displayName} — ${job.title}`,
        });
      }
    }
  }
  return appearances;
}

function sanitizeRewards(values: unknown[]): BountyWatchedReward[] {
  const rewards: BountyWatchedReward[] = [];
  const keys = new Set<string>();
  for (const value of values) {
    if (!isRecord(value)) continue;
    const key = typeof value.key === "string" ? value.key.trim() : "";
    const displayName = typeof value.displayName === "string"
      ? value.displayName.trim()
      : "";
    if (!key || key.length > 512 || !displayName || displayName.length > 512 || keys.has(key)) {
      continue;
    }
    keys.add(key);
    rewards.push({
      key,
      displayName,
      imageUrl: typeof value.imageUrl === "string" && value.imageUrl.length <= 2_048
        ? value.imageUrl
        : null,
    });
    if (rewards.length >= MAX_WATCHED_REWARDS) break;
  }
  return rewards;
}

function defaultStorage(): StorageLike | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function readRecord(key: string, storage: StorageLike | null): Record<string, unknown> | null {
  if (!storage) return null;
  try {
    const raw = storage.getItem(key);
    if (!raw) return null;
    const value: unknown = JSON.parse(raw);
    return isRecord(value) && value.version === 1 ? value : null;
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
