import { MISSION_FILTERS, objectFilter, type MissionFilter, type MissionObject } from "./missionResearch";

export interface MissionFilterRule { key: string; label: string; nameEn: string }
export interface CustomMissionFilter { id: string; name: string; color: string; enabled: boolean; rules: MissionFilterRule[] }
export const MISSION_FILTER_STORAGE = "platscope.mission-filters.v1";
export type MissionFilterIndex = Map<string, { visible: boolean; color: string | null; groups: string[] }>;
export function indexMissionFilters(filters: CustomMissionFilter[]): MissionFilterIndex {
  const index: MissionFilterIndex = new Map();
  for (const filter of filters) for (const rule of filter.rules) {
    const entry = index.get(rule.key) ?? { visible:false, color:null, groups:[] };
    entry.visible ||= filter.enabled;
    if (filter.enabled && !entry.color) entry.color = filter.color;
    if (!entry.groups.includes(filter.id)) entry.groups.push(filter.id);
    index.set(rule.key, entry);
  }
  return index;
}
export function countMissionFilters(objects: MissionObject[], index: MissionFilterIndex): Map<string, number> {
  const counts = new Map<string, number>();
  for (const object of objects) for (const id of index.get(objectRule(object).key)?.groups ?? []) counts.set(id, (counts.get(id) ?? 0) + 1);
  return counts;
}
const normalizePath = (value: string) => value.replace(/^\/Lotus\/StoreItems\//, "/Lotus/");

/** Правило описывает тип/ресурс, а не адрес экземпляра или его координаты. */
export function objectRule(object: MissionObject): MissionFilterRule {
  const resource = object.details.find(detail => detail.label === "Ресурс")?.value;
  const identity = object.itemPath ? ["item", normalizePath(object.itemPath)]
    : resource ? ["resource", resource]
    : ["type", [...object.typeNames].sort(), object.nameEn];
  return { key: JSON.stringify([object.kind, ...identity]), label: object.label, nameEn: object.nameEn };
}

export function filterMatches(filter: CustomMissionFilter, object: MissionObject): boolean {
  const key = objectRule(object).key;
  return filter.rules.some(rule => rule.key === key);
}

export function addFilterObjects(filter: CustomMissionFilter, objects: MissionObject[]): CustomMissionFilter {
  const rules = new Map(filter.rules.map(rule => [rule.key, rule]));
  for (const object of objects) { const rule = objectRule(object); rules.set(rule.key, rule); }
  if (rules.size > 200) throw new Error("В одном фильтре можно сохранить до 200 типов объектов.");
  return { ...filter, rules: [...rules.values()] };
}

export function objectIsVisible(object: MissionObject, standard: Record<MissionFilter, boolean>, custom: CustomMissionFilter[] | MissionFilterIndex): boolean {
  const index = Array.isArray(custom) ? indexMissionFilters(custom) : custom;
  return index.get(objectRule(object).key)?.visible ?? standard[objectFilter(object.kind)];
}

export function filterColor(object: MissionObject, custom: CustomMissionFilter[] | MissionFilterIndex): string {
  const index = Array.isArray(custom) ? indexMissionFilters(custom) : custom;
  return index.get(objectRule(object).key)?.color
    ?? MISSION_FILTERS.find(filter => filter.key === objectFilter(object.kind))!.color;
}

export function parseMissionFilters(raw: string | null): CustomMissionFilter[] {
  if (!raw) return [];
  if (raw.length > 1_000_000) throw new Error("Файл фильтров слишком большой.");
  const data = JSON.parse(raw);
  if (data?.version !== 1 || !Array.isArray(data.filters) || data.filters.length > 40) throw new Error("Не удалось прочитать сохранённые фильтры.");
  const ids = new Set<string>();
  for (const filter of data.filters) {
    if (!filter || typeof filter.id !== "string" || !filter.id || filter.id.length > 100 || ids.has(filter.id)
      || typeof filter.name !== "string" || !filter.name.trim() || filter.name.length > 60
      || typeof filter.color !== "string" || !/^#[\da-f]{6}$/i.test(filter.color) || typeof filter.enabled !== "boolean"
      || !Array.isArray(filter.rules) || filter.rules.length > 200) throw new Error("Сохранённый фильтр повреждён.");
    ids.add(filter.id);
    for (const rule of filter.rules) if (!rule || typeof rule.key !== "string" || !rule.key || rule.key.length > 4096
      || typeof rule.label !== "string" || rule.label.length > 1024 || typeof rule.nameEn !== "string" || rule.nameEn.length > 1024) throw new Error("Тип объекта в фильтре повреждён.");
  }
  return data.filters;
}

export function serializeMissionFilters(filters: CustomMissionFilter[]): string {
  const raw = JSON.stringify({ version: 1, filters });
  parseMissionFilters(raw);
  return raw;
}
