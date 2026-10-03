import { objectFilterEntry, objectRule, type MissionFilterIndex } from "./missionFilters";
import { MISSION_FILTERS, objectFilter, type MissionFilter, type MissionObject } from "./missionResearch";
import type { ViewPreferenceStorage } from "./viewPreferences";

export const DEFAULT_MISSION_MARKER_SIZE = 18;
export const MIN_MISSION_MARKER_SIZE = 12;
export const MAX_MISSION_MARKER_SIZE = 48;

export interface MissionMapPreferences {
  markerSize: number;
  categorySizes: Partial<Record<MissionFilter, number>>;
  groupSizes: Record<string, number>;
  typeSizes: Record<string, number>;
  showHeightIndicators: boolean;
  heightArrowsOnly: boolean;
}

const STORAGE_KEY = "platscope.mission-map-view.v1";
const categories = new Set<string>(MISSION_FILTERS.map(filter => filter.key));
const objectKinds = new Set([
  "feather", "pickup", "decree_fragment", "avatar", "npc", "hostage", "spawnpoint",
  "panel", "locker", "decoration", "extraction", "terminal", "lootspot", "cache", "dragon_door",
]);

export function defaultMissionMapPreferences(): MissionMapPreferences {
  return {
    markerSize: DEFAULT_MISSION_MARKER_SIZE,
    categorySizes: Object.create(null),
    groupSizes: Object.create(null),
    typeSizes: Object.create(null),
    showHeightIndicators: true,
    heightArrowsOnly: false,
  };
}

/** Размер конкретного типа важнее размера группы и общей категории. */
export function missionMarkerSize(
  object: MissionObject,
  customIndex: MissionFilterIndex,
  preferences: MissionMapPreferences,
): number {
  const variantSize = ownSize(preferences.typeSizes, objectRule(object).key);
  if (variantSize !== undefined) return variantSize;
  const modelSize = ownSize(preferences.typeSizes, objectRule(object, "model").key);
  if (modelSize !== undefined) return modelSize;
  for (const groupId of objectFilterEntry(object, customIndex)?.enabledGroups ?? []) {
    const groupSize = ownSize(preferences.groupSizes, groupId);
    if (groupSize !== undefined) return groupSize;
  }
  return ownSize(preferences.categorySizes, objectFilter(object.kind))
    ?? validSize(preferences.markerSize)
    ?? DEFAULT_MISSION_MARKER_SIZE;
}

export function missionMarkerRadius(size: number): number {
  return 10 * (validSize(size) ?? DEFAULT_MISSION_MARKER_SIZE) / DEFAULT_MISSION_MARKER_SIZE;
}

export function missionMarkerHitRadius(size: number): number {
  return 14 * (validSize(size) ?? DEFAULT_MISSION_MARKER_SIZE) / DEFAULT_MISSION_MARKER_SIZE;
}

export function loadMissionMapPreferences(
  storage: ViewPreferenceStorage | null = defaultStorage(),
): MissionMapPreferences {
  if (!storage) return defaultMissionMapPreferences();
  try {
    const raw = storage.getItem(STORAGE_KEY);
    if (!raw || raw.length > 1_000_000) return defaultMissionMapPreferences();
    const value: unknown = JSON.parse(raw);
    if (!isRecord(value) || value.version !== 1) return defaultMissionMapPreferences();
    return normalizePreferences(value);
  } catch {
    return defaultMissionMapPreferences();
  }
}

export function saveMissionMapPreferences(
  preferences: MissionMapPreferences,
  storage: ViewPreferenceStorage | null = defaultStorage(),
): boolean {
  if (!storage) return false;
  try {
    const raw = JSON.stringify({ version: 1, ...normalizePreferences(preferences) });
    if (raw.length > 1_000_000) return false;
    storage.setItem(STORAGE_KEY, raw);
    return true;
  } catch {
    return false;
  }
}

function normalizePreferences(value: unknown): MissionMapPreferences {
  const defaults = defaultMissionMapPreferences();
  if (!isRecord(value)) return defaults;
  return {
    markerSize: validSize(value.markerSize) ?? defaults.markerSize,
    categorySizes: sizeOverrides(value.categorySizes, key => categories.has(key)),
    groupSizes: sizeOverrides(value.groupSizes, key => safeKey(key) && key.length <= 100),
    typeSizes: sizeOverrides(value.typeSizes, validTypeKey),
    showHeightIndicators: typeof value.showHeightIndicators === "boolean"
      ? value.showHeightIndicators : defaults.showHeightIndicators,
    heightArrowsOnly: typeof value.heightArrowsOnly === "boolean"
      ? value.heightArrowsOnly : defaults.heightArrowsOnly,
  };
}

function sizeOverrides(value: unknown, validKey: (key: string) => boolean): Record<string, number> {
  const result: Record<string, number> = Object.create(null);
  if (!isRecord(value)) return result;
  for (const [key, size] of Object.entries(value)) {
    const normalizedSize = validSize(size);
    if (validKey(key) && normalizedSize !== undefined) result[key] = normalizedSize;
  }
  return result;
}

function ownSize(value: object, key: string): number | undefined {
  return Object.prototype.hasOwnProperty.call(value, key)
    ? validSize((value as Record<string, unknown>)[key]) : undefined;
}

function validSize(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.max(MIN_MISSION_MARKER_SIZE, Math.min(MAX_MISSION_MARKER_SIZE, value))
    : undefined;
}

function safeKey(key: string): boolean {
  return key.length > 0 && key.trim().length > 0
    && key !== "__proto__" && key !== "prototype" && key !== "constructor";
}

function validTypeKey(key: string): boolean {
  if (!safeKey(key) || key.length > 4096) return false;
  try {
    const parts: unknown = JSON.parse(key);
    if (!Array.isArray(parts) || typeof parts[0] !== "string" || !objectKinds.has(parts[0])) return false;
    if (parts[1] === "type") {
      return parts.length === 4 && Array.isArray(parts[2])
        && parts[2].every(name => typeof name === "string") && typeof parts[3] === "string";
    }
    return parts.length === 3 && ["variant", "item", "resource"].includes(parts[1])
      && typeof parts[2] === "string" && parts[2].length > 0;
  } catch {
    return false;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function defaultStorage(): ViewPreferenceStorage | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}
