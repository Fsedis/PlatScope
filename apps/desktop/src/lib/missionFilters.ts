import { MISSION_FILTERS, objectFilter, objectKindLabel, type MissionFilter, type MissionObject, type MissionObjectKind } from "./missionResearch";

export interface MissionFilterRule { key: string; label: string; nameEn: string }
export interface CustomMissionFilter { id: string; name: string; color: string; enabled: boolean; rules: MissionFilterRule[] }
export const MISSION_FILTER_STORAGE = "platscope.mission-filters.v1";
export const MISSION_HIDDEN_STORAGE = "platscope.mission-hidden-names.v1";
export interface HiddenMissionName { label: string; nameEn: string }
const normalizeName = (name: string) => name.trim().toLowerCase().replaceAll("ё", "е").replace(/\s+/g, " ");
const categoryNames = new Set([
  ...MISSION_FILTERS.map(filter => filter.label),
  ...(["feather", "pickup", "avatar", "npc", "hostage", "spawnpoint", "panel", "locker", "decoration", "extraction", "terminal", "lootspot", "cache"] as MissionObjectKind[]).map(objectKindLabel),
  "Объект", "Деталь окружения", "Варфрейм или оператор", "Warframe / Operator",
].map(normalizeName));
/** Одно конкретное имя вместо объединения с общей подписью вроде NPC. */
export function hiddenMissionNameKey(name: HiddenMissionName): string {
  const en = normalizeName(name.nameEn), label = normalizeName(name.label);
  if (en && !categoryNames.has(en)) return `en:${en}`;
  return label && !categoryNames.has(label) ? `label:${label}` : "";
}
export function hiddenMissionNameLabel(name: HiddenMissionName): string {
  return categoryNames.has(normalizeName(name.label)) ? name.nameEn || name.label : name.label || name.nameEn;
}
export function indexHiddenMissionNames(names: HiddenMissionName[]): Set<string> {
  // Старые записи v1 уже содержат nameEn: применяем уточнённое правило без
  // перезаписи хранилища. Запись без конкретного имени больше ничего не скрывает.
  return new Set(names.map(hiddenMissionNameKey).filter(Boolean));
}
export function objectIsHidden(object: HiddenMissionName, names: Set<string>): boolean {
  return names.has(hiddenMissionNameKey(object));
}
export function parseHiddenMissionNames(raw: string | null): HiddenMissionName[] {
  if (!raw) return [];
  if (raw.length > 1_000_000) throw new Error("Список скрытых слишком большой.");
  const data = JSON.parse(raw);
  if (data?.version !== 1 || !Array.isArray(data.names) || data.names.length > 500
    || data.names.some((name: HiddenMissionName) => !name || typeof name.label !== "string" || typeof name.nameEn !== "string"
      || name.label.length > 1024 || name.nameEn.length > 1024 || !(name.label.trim() || name.nameEn.trim()))) {
    throw new Error("Не удалось прочитать список скрытых названий.");
  }
  return data.names.map((name: HiddenMissionName) => ({ label: name.label, nameEn: name.nameEn }));
}
export function serializeHiddenMissionNames(names: HiddenMissionName[]): string {
  const raw = JSON.stringify({ version: 1, names });
  parseHiddenMissionNames(raw);
  return raw;
}
export type MissionFilterIndex = Map<string, { visible: boolean; color: string | null; groups: string[] }>;
export type FilterScope = "variant" | "model";
const resourceName = (value: string) => value.trim().replace(/^"|"$/g, "").replaceAll("\\", "/").split("/").pop()!.toLowerCase();
function canonicalRuleKey(key: string): string {
  try {
    const parts = JSON.parse(key);
    if (Array.isArray(parts) && parts[1] === "resource" && typeof parts[2] === "string") return JSON.stringify([parts[0], "resource", resourceName(parts[2])]);
  } catch { /* Старые неизвестные правила сохраняются без потери данных. */ }
  return key;
}
export function objectFilterEntry(object: MissionObject, index: MissionFilterIndex) {
  return index.get(objectRule(object).key) ?? index.get(objectRule(object, "model").key);
}
export function indexMissionFilters(filters: CustomMissionFilter[]): MissionFilterIndex {
  const index: MissionFilterIndex = new Map();
  for (const filter of filters) for (const rule of filter.rules) {
    const key = canonicalRuleKey(rule.key);
    const entry = index.get(key) ?? { visible:false, color:null, groups:[] };
    entry.visible ||= filter.enabled;
    if (filter.enabled && !entry.color) entry.color = filter.color;
    if (!entry.groups.includes(filter.id)) entry.groups.push(filter.id);
    index.set(key, entry);
  }
  return index;
}
/** Выключенный фильтр скрывает точки, но сохраняет поиск назначенных ему типов. */
export function missionDiscoveryKeys(filters: CustomMissionFilter[]): string[] {
  return [...new Set(filters.flatMap(filter => filter.rules.map(rule => canonicalRuleKey(rule.key))))].sort();
}
/** Одна строка на вариант; первый объект сохраняет выбранную сортировку списка. */
export function groupMissionObjects(objects: MissionObject[], grouped: boolean): { key: string; object: MissionObject; count: number }[] {
  const groups = new Map<string, { key: string; object: MissionObject; count: number }>();
  for (const object of objects) {
    const key = grouped && object.kind !== "avatar"
      ? JSON.stringify([objectRule(object).key, object.availability, object.positionFresh !== false]) : object.key;
    const entry = groups.get(key);
    if (entry) entry.count++; else groups.set(key, { key, object, count: 1 });
  }
  return [...groups.values()];
}
export function countMissionFilters(objects: MissionObject[], index: MissionFilterIndex): Map<string, number> {
  const counts = new Map<string, number>();
  for (const object of objects) for (const id of objectFilterEntry(object, index)?.groups ?? []) counts.set(id, (counts.get(id) ?? 0) + 1);
  return counts;
}
const normalizePath = (value: string) => value.replace(/^\/Lotus\/StoreItems\//, "/Lotus/");

/** Правило описывает тип/ресурс, а не адрес экземпляра или его координаты. */
export function objectRule(object: MissionObject, scope: FilterScope = "variant"): MissionFilterRule {
  const resource = object.details.find(detail => detail.label === "Ресурс")?.value;
  const identity = scope === "variant" && object.variantKey ? ["variant", object.variantKey]
    : object.itemPath ? ["item", normalizePath(object.itemPath)]
    : resource ? ["resource", resourceName(resource)]
    : ["type", [...object.typeNames].sort(), object.nameEn];
  return { key: JSON.stringify([object.kind, ...identity]), label: object.label, nameEn: object.nameEn };
}

export function filterMatches(filter: CustomMissionFilter, object: MissionObject): boolean {
  const keys = [objectRule(object).key, objectRule(object, "model").key];
  return filter.rules.some(rule => keys.includes(canonicalRuleKey(rule.key)));
}

export function addFilterObjects(filter: CustomMissionFilter, objects: MissionObject[], scope: FilterScope = "variant"): CustomMissionFilter {
  const rules = new Map(filter.rules.map(rule => [canonicalRuleKey(rule.key), {...rule, key: canonicalRuleKey(rule.key)}]));
  for (const object of objects) { const rule = objectRule(object, scope); rules.set(rule.key, rule); }
  if (rules.size > 200) throw new Error("В одном фильтре можно сохранить до 200 типов объектов.");
  return { ...filter, rules: [...rules.values()] };
}

export function objectIsVisible(object: MissionObject, standard: Record<MissionFilter, boolean>, custom: CustomMissionFilter[] | MissionFilterIndex, hidden?: Set<string>): boolean {
  if (hidden && objectIsHidden(object, hidden)) return false;
  const index = Array.isArray(custom) ? indexMissionFilters(custom) : custom;
  return objectFilterEntry(object, index)?.visible ?? standard[objectFilter(object.kind)];
}

export function filterColor(object: MissionObject, custom: CustomMissionFilter[] | MissionFilterIndex): string {
  const index = Array.isArray(custom) ? indexMissionFilters(custom) : custom;
  return objectFilterEntry(object, index)?.color
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
