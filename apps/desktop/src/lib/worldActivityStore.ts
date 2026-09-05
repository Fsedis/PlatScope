import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { get, writable } from "svelte/store";
import { markWorldAlertsDelivered, nextWorldRefresh, parseWorldPreferences, WORLD_PREFERENCES_KEY, worldAlertCandidates,
  type WorldActivityView, type WorldPreferences } from "./worldActivity";

export interface WorldActivityState { view: WorldActivityView | null; loading: boolean; error: boolean; nextRefreshAt: number; manualRefreshAt: number }
const browserStorage = () => { try { return window.localStorage; } catch { return null; } };
export const worldPreferences = writable(parseWorldPreferences(browserStorage()?.getItem(WORLD_PREFERENCES_KEY) ?? null));
export const worldNow = writable(Date.now());

export function saveWorldPreferences(preferences: WorldPreferences): boolean {
  try {
    const storage = browserStorage();
    if (!storage) return false;
    storage.setItem(WORLD_PREFERENCES_KEY, JSON.stringify(preferences));
    worldPreferences.set(preferences);
    return true;
  } catch { return false; }
}

export function createWorldActivityStore(load: (force: boolean) => Promise<WorldActivityView>, now = Date.now) {
  let state: WorldActivityState = { view: null, loading: false, error: false, nextRefreshAt: 0, manualRefreshAt: 0 };
  const store = writable(state);
  let inFlight: Promise<void> | null = null;
  let invalidated = false;
  function publish(update: Partial<WorldActivityState>) { state = { ...state, ...update }; store.set(state); }
  function refresh(force = false): Promise<void> {
    if (inFlight) return inFlight;
    if (now() < state.manualRefreshAt) return Promise.resolve();
    publish({ loading: true, manualRefreshAt: now() + 15_000 });
    // Изменение локального справочника не требует нового запроса к источнику.
    const refreshSource = force && !invalidated;
    invalidated = false;
    inFlight = (async () => {
      try {
        const view = await load(refreshSource);
        publish({ view, error: view.refreshFailed, nextRefreshAt: nextWorldRefresh(view, now()),
          manualRefreshAt: now() + (view.refreshFailed ? 45_000 : 15_000) });
      } catch {
        publish({ error: true, nextRefreshAt: now() + 45_000, manualRefreshAt: now() + 45_000 });
      } finally { publish({ loading: false, ...(invalidated ? { nextRefreshAt: 0 } : {}) }); }
    })().finally(() => { inFlight = null; });
    return inFlight;
  }
  function invalidate() { invalidated = true; publish({ nextRefreshAt: 0 }); }
  return { subscribe: store.subscribe, refresh, invalidate };
}

function loadWithDeadline(forceRefresh: boolean): Promise<WorldActivityView> {
  return new Promise((resolve, reject) => {
    const deadline = window.setTimeout(() => reject(new Error("worldstate deadline exceeded")), 20_000);
    invoke<WorldActivityView>("world_activity", { forceRefresh }).then(resolve, reject)
      .finally(() => window.clearTimeout(deadline));
  });
}
export const worldActivityStore = createWorldActivityStore(loadWithDeadline);
let screenReaders = 0;
export function retainWorldActivityScreen(): () => void {
  screenReaders++;
  worldNow.set(Date.now());
  const state = get(worldActivityStore);
  if (!state.view || Date.now() >= state.nextRefreshAt) void worldActivityStore.refresh();
  return () => { screenReaders = Math.max(0, screenReaders - 1); };
}
function isMock(): boolean {
  return import.meta.env.DEV && new URLSearchParams(window.location.search).has("mock");
}
export async function requestWorldNotificationPermission(): Promise<boolean> {
  if (isMock()) return true;
  try { return await isPermissionGranted() || await requestPermission() === "granted"; }
  catch { return false; }
}

/** Один фоновой планировщик для экрана и всех напоминаний, без запроса на каждый таймер. */
export function startWorldActivityAlerts(): () => void {
  let previousTick = Date.now();
  let eligibleSince = previousTick;
  let disposed = false;
  let preferences = get(worldPreferences);
  const tick = () => {
    if (disposed) return;
    const now = Date.now();
    if (now < previousTick || now - previousTick > 2 * 60_000) eligibleSince = now;
    const watched = preferences.rules.some(rule => rule.enabled);
    if (screenReaders || watched) {
      worldNow.set(now);
      const state = get(worldActivityStore);
      if (!state.loading && now >= state.nextRefreshAt) void worldActivityStore.refresh(Boolean(state.view?.catalogAvailable));
      const candidates = worldAlertCandidates(state.error ? null : state.view, preferences, now, previousTick, eligibleSince);
      if (candidates.length) {
        const next = markWorldAlertsDelivered(preferences, candidates);
        // Если дедупликацию нельзя сохранить, не спамим одним событием каждую секунду.
        if (saveWorldPreferences(next) && !isMock()) {
          for (const alert of candidates) {
            try { sendNotification({ title: alert.title, body: alert.body }); } catch { /* Запрет ОС не мешает экрану. */ }
          }
        }
      }
    }
    previousTick = now;
  };
  const cleanup = worldPreferences.subscribe(value => {
    preferences = value;
    if (value.rules.some(rule => rule.enabled) && !get(worldActivityStore).view) void worldActivityStore.refresh();
  });
  const timer = window.setInterval(tick, 1_000);
  // Названия и состав обновляются после загрузки справочников, не только при
  // следующей смене цикла мира. В демонстрационном браузере IPC не запускается.
  const listeners = isMock() ? [] : ["game-metadata-updated", "market-data-updated"].map(event =>
    listen(event, () => worldActivityStore.invalidate()).catch(() => () => undefined));
  window.addEventListener("focus", tick);
  document.addEventListener("visibilitychange", tick);
  tick();
  return () => { disposed = true; cleanup(); window.clearInterval(timer);
    for (const listener of listeners) void listener.then(unlisten => unlisten());
    window.removeEventListener("focus", tick); document.removeEventListener("visibilitychange", tick); };
}
