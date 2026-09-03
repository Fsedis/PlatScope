import { invoke } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

import {
  BOUNTY_VIEW_AVAILABLE_EVENT,
  BOUNTY_VIEW_REFRESHED_EVENT,
  BOUNTY_WATCHLIST_CHANGED_EVENT,
  currentBountyAppearanceIds,
  detectBountyRewardAlerts,
  loadBountyAppearanceIds,
  loadBountyWatchPreferences,
  saveBountyAppearanceIds,
} from "./bountyWatchlist";
import {
  bountyAutomaticRefreshAt,
  type BountyHunterView,
} from "./bountyHunter";

interface BountyWatchlistChangedDetail {
  view: BountyHunterView | null;
  suppressCurrentKeys: string[];
}

const ROTATION_SETTLE_DELAY_MS = 3_000;
const REFRESH_RETRY_DELAY_MS = 30_000;
const MAX_TIMER_DELAY_MS = 2_147_000_000;

function isDesktopRuntime(): boolean {
  return Boolean(
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__,
  );
}

export async function requestBountyNotificationPermission(): Promise<boolean> {
  if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("mock")) {
    return true;
  }
  if (!isDesktopRuntime()) return true;
  try {
    if (await isPermissionGranted()) return true;
    return await requestPermission() === "granted";
  } catch {
    return false;
  }
}

export function publishBountyView(view: BountyHunterView): void {
  window.dispatchEvent(new CustomEvent<BountyHunterView>(BOUNTY_VIEW_AVAILABLE_EVENT, {
    detail: view,
  }));
}

export function publishBountyWatchlistChange(
  view: BountyHunterView | null,
  suppressCurrentKeys: readonly string[] = [],
): void {
  window.dispatchEvent(new CustomEvent<BountyWatchlistChangedDetail>(
    BOUNTY_WATCHLIST_CHANGED_EVENT,
    { detail: { view, suppressCurrentKeys: [...suppressCurrentKeys] } },
  ));
}

export function startBountyRewardAlerts(): () => void {
  if (!isDesktopRuntime()) return () => undefined;

  let disposed = false;
  let refreshing = false;
  let refreshTimer: number | undefined;
  let currentView: BountyHunterView | null = null;

  const clearRefreshTimer = (): void => {
    if (refreshTimer !== undefined) window.clearTimeout(refreshTimer);
    refreshTimer = undefined;
  };

  const schedule = (view: BountyHunterView | null): void => {
    clearRefreshTimer();
    const preferences = loadBountyWatchPreferences();
    if (!preferences.enabled || preferences.rewards.length === 0 || !view) return;
    const rotationAt = bountyAutomaticRefreshAt(view);
    if (rotationAt === null) return;
    const delay = Math.min(
      MAX_TIMER_DELAY_MS,
      Math.max(1_000, rotationAt - Date.now() + ROTATION_SETTLE_DELAY_MS),
    );
    refreshTimer = window.setTimeout(() => void refresh(true), delay);
  };

  const acceptView = (view: BountyHunterView): void => {
    currentView = view;
    const preferences = loadBountyWatchPreferences();
    const previousAppearanceIds = loadBountyAppearanceIds();
    const result = detectBountyRewardAlerts(view, preferences, previousAppearanceIds);
    saveBountyAppearanceIds(result.currentAppearanceIds);
    schedule(view);
    for (const alert of result.alerts) {
      const extraLocations = Math.max(0, alert.locations.length - 3);
      const visibleLocations = alert.locations.slice(0, 3).join("; ");
      const body = extraLocations > 0
        ? `${visibleLocations}; ещё ${extraLocations}`
        : visibleLocations;
      try {
        sendNotification({
          title: `Награда появилась: ${alert.displayName}`,
          body,
        });
      } catch {
        // Отсутствие системного разрешения не должно мешать обновлению ротации.
      }
    }
  };

  const refresh = async (forceRefresh: boolean): Promise<void> => {
    if (disposed || refreshing) return;
    const preferences = loadBountyWatchPreferences();
    if (!preferences.enabled || preferences.rewards.length === 0) {
      clearRefreshTimer();
      return;
    }
    refreshing = true;
    try {
      const view = await invoke<BountyHunterView | null>("bounty_hunter", { forceRefresh });
      if (!view || disposed) return;
      acceptView(view);
      window.dispatchEvent(new CustomEvent<BountyHunterView>(BOUNTY_VIEW_REFRESHED_EVENT, {
        detail: view,
      }));
    } catch {
      if (!disposed) {
        clearRefreshTimer();
        refreshTimer = window.setTimeout(() => void refresh(true), REFRESH_RETRY_DELAY_MS);
      }
    } finally {
      refreshing = false;
    }
  };

  const handleAvailableView = (event: Event): void => {
    const view = (event as CustomEvent<BountyHunterView>).detail;
    if (view) acceptView(view);
  };

  const handleWatchlistChange = (event: Event): void => {
    const detail = (event as CustomEvent<BountyWatchlistChangedDetail>).detail;
    if (detail?.view) currentView = detail.view;
    const preferences = loadBountyWatchPreferences();
    if (currentView && detail?.suppressCurrentKeys.length) {
      const suppressed = currentBountyAppearanceIds(
        currentView,
        preferences.rewards,
        new Set(detail.suppressCurrentKeys),
      );
      saveBountyAppearanceIds([...loadBountyAppearanceIds(), ...suppressed]);
    }
    if (!preferences.enabled || preferences.rewards.length === 0) {
      clearRefreshTimer();
      return;
    }
    if (currentView) schedule(currentView);
    else void refresh(false);
  };

  window.addEventListener(BOUNTY_VIEW_AVAILABLE_EVENT, handleAvailableView);
  window.addEventListener(BOUNTY_WATCHLIST_CHANGED_EVENT, handleWatchlistChange);
  if (loadBountyWatchPreferences().rewards.length > 0) void refresh(false);

  return () => {
    disposed = true;
    clearRefreshTimer();
    window.removeEventListener(BOUNTY_VIEW_AVAILABLE_EVENT, handleAvailableView);
    window.removeEventListener(BOUNTY_WATCHLIST_CHANGED_EVENT, handleWatchlistChange);
  };
}
