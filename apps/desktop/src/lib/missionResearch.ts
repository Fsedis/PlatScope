import { matchesSearch } from "./searchText";
export function objectMatchesSearch(object: MissionObject, query: string): boolean {
  return matchesSearch(query, [object.label, object.nameEn, object.itemPath, objectKindLabel(object.kind), ...object.typeNames,
    object.kind === "feather" ? "перо перья" : object.kind === "npc" ? "неигровой персонаж" : ""]);
}
export type Position3 = [number, number, number];
export type MissionObjectKind = "feather" | "pickup" | "avatar" | "npc" | "hostage" | "spawnpoint" | "panel" | "locker" | "decoration" | "extraction" | "terminal" | "lootspot" | "cache";
export interface MissionObject {
  key: string; kind: MissionObjectKind; label: string; nameEn: string; itemPath: string | null;
  position: Position3; positionFresh?: boolean; variantKey?: string | null; typeNames: string[]; availability: "unknown" | "available" | "opened"; details: { label: string; value: string }[];
}
export interface MissionMesh { key: string; vertices: Position3[]; faces: number[][]; adjacency: [number, number][] }
export interface MissionPlayer { key: string; avatarKey: string; operatorKey: string | null; local: boolean }
export interface MissionZone { key: string; min: Position3; max: Position3 }
export interface MissionScene {
  format: 1; source: "archive" | "live"; startedAt: string; capturedAt: string; complete: boolean; profile: string;
  objects: MissionObject[]; meshes: MissionMesh[]; warnings: string[];
  players?: MissionPlayer[]; zones?: MissionZone[]; zonesFresh?: boolean;
  stats: { scannedBytes: number; objectCount: number; meshCount: number; vertexCount: number; faceCount: number };
}
export interface MissionResearchStatus { busy: boolean; cancelling: boolean; phase: string; error: string | null; revision: number; gameRunning: boolean; tracking: boolean; scannedBytes: number }
export interface MissionArchive { id: string; label: string; createdAt: string; sizeBytes: number; snapshots: { sequence: number; startedAt: string; endedAt: string; complete: boolean; bytes: number; holes: number }[] }
export type MissionFilter = "feather" | "pickup" | "players" | "npc" | "other" | "goals" | "lootspots" | "caches";
export const MISSION_FILTERS: { key: MissionFilter; label: string; color: string }[] = [
  { key: "caches", label: "Тайники", color: "#ffd66b" },
  { key: "feather", label: "Перья", color: "#f4c97e" }, { key: "pickup", label: "Предметы", color: "#78d6b0" },
  { key: "players", label: "Игроки", color: "#83cfff" }, { key: "npc", label: "NPC", color: "#e49baa" }, { key: "other", label: "Прочее", color: "#9eb9e9" },
  { key: "goals", label: "Эвакуация и терминалы", color: "#8de4bb" }, { key: "lootspots", label: "Возможные места лута", color: "#c5a6ee" },
];
export function objectFilter(kind: MissionObjectKind): MissionFilter { return kind === "cache" ? "caches" : kind === "feather" ? "feather" : kind === "pickup" ? "pickup" : kind === "avatar" ? "players" : kind === "extraction" || kind === "terminal" ? "goals" : kind === "lootspot" ? "lootspots" : kind === "npc" || kind === "hostage" || kind === "spawnpoint" ? "npc" : "other"; }
export function objectKindLabel(kind: MissionObjectKind): string { return ({ cache: "Тайник", feather: "Перо", pickup: "Предмет", avatar: "Игрок", npc: "NPC", hostage: "Заложник", spawnpoint: "Точка появления", panel: "Панель", locker: "Шкафчик", decoration: "Объект окружения", extraction: "Эвакуация", terminal: "Терминал", lootspot: "Возможное место появления" })[kind] ?? "Объект"; }
export function localAvatar(scene: Pick<MissionScene, "players" | "objects">): MissionObject | null {
  const players = scene.players?.filter(p => p.local) ?? [];
  if (players.length !== 1) return null;
  return scene.objects.find(o => o.key === players[0].avatarKey && o.kind === "avatar" && o.positionFresh !== false && finitePosition(o.position)) ?? null;
}
export function zoneInHeightSlice(zone: MissionZone, enabled: boolean, center: number, halfWidth: number): boolean {
  return finitePosition(zone.min) && finitePosition(zone.max) && zone.min.every((v, i) => v <= zone.max[i]) && (!enabled || zone.min[1] <= center + halfWidth && zone.max[1] >= center - halfWidth);
}
export const finitePosition = (p: readonly number[]): boolean => p.length >= 3 && p.slice(0, 3).every(Number.isFinite);
export interface MapBounds { minX: number; maxX: number; minZ: number; maxZ: number; minY: number; maxY: number }
export function sceneBounds(scene: Pick<MissionScene, "objects" | "meshes" | "zones">): MapBounds {
  let minX = Infinity, maxX = -Infinity, minZ = Infinity, maxZ = -Infinity, minY = Infinity, maxY = -Infinity;
  const include = (p: Position3) => { if (!finitePosition(p)) return; minX = Math.min(minX, p[0]); maxX = Math.max(maxX, p[0]); minY = Math.min(minY, p[1]); maxY = Math.max(maxY, p[1]); minZ = Math.min(minZ, p[2]); maxZ = Math.max(maxZ, p[2]); };
  scene.objects.forEach(o => include(o.position)); scene.meshes.forEach(m => m.vertices.forEach(include));
  scene.zones?.forEach(zone => { if (zoneInHeightSlice(zone, false, 0, 0)) { include(zone.min); include(zone.max); } });
  if (!Number.isFinite(minX)) return { minX: -1, maxX: 1, minZ: -1, maxZ: 1, minY: 0, maxY: 0 };
  if (maxX - minX < 1) { minX -= .5; maxX += .5; } if (maxZ - minZ < 1) { minZ -= .5; maxZ += .5; }
  return { minX, maxX, minZ, maxZ, minY, maxY };
}
export interface MapCamera { x: number; z: number; zoom: number }
export function followTargetSignature(object: MissionObject): string { return JSON.stringify([object.kind, object.itemPath, object.typeNames]); }
export function followFrame(key: string, signature: string, tracking: boolean, objects: MissionObject[], camera: MapCamera): { key: string; camera: MapCamera; height: number | null; reason: "stopped" | "missing" | null } {
  if (!key) return { key: "", camera, height: null, reason: null };
  if (!tracking) return { key: "", camera, height: null, reason: "stopped" };
  const object = objects.find(o => o.key === key);
  if (!object || object.kind !== "avatar" || followTargetSignature(object) !== signature) return { key: "", camera, height: null, reason: "missing" };
  if (object.positionFresh === false || !finitePosition(object.position)) return { key, camera, height: null, reason: null };
  return { key, camera: { ...camera, x: object.position[0], z: object.position[2] }, height: object.position[1], reason: null };
}
export function fitCamera(bounds: MapBounds): MapCamera { return { x: (bounds.minX + bounds.maxX) / 2, z: (bounds.minZ + bounds.maxZ) / 2, zoom: 1 }; }
export function mapScale(bounds: MapBounds, width: number, height: number, zoom = 1): number {
  const fit = Math.min(Math.max(1, width - 40) / Math.max(1, bounds.maxX - bounds.minX), Math.max(1, height - 40) / Math.max(1, bounds.maxZ - bounds.minZ));
  return Math.max(.000001, fit * Math.max(.2, Math.min(32, zoom)));
}
export function worldToCanvas(p: Position3, camera: MapCamera, scale: number, width: number, height: number): [number, number] { return [width / 2 + (p[0] - camera.x) * scale, height / 2 - (p[2] - camera.z) * scale]; }
export function canvasToWorld(x: number, y: number, camera: MapCamera, scale: number, width: number, height: number): [number, number] { return [camera.x + (x - width / 2) / scale, camera.z - (y - height / 2) / scale]; }
export function zoomAt(camera: MapCamera, bounds: MapBounds, width: number, height: number, x: number, y: number, factor: number): MapCamera {
  const [wx, wz] = canvasToWorld(x, y, camera, mapScale(bounds, width, height, camera.zoom), width, height);
  const zoom = Math.max(.2, Math.min(32, camera.zoom * factor)); const scale = mapScale(bounds, width, height, zoom);
  return { x: wx - (x - width / 2) / scale, z: wz + (y - height / 2) / scale, zoom };
}
export function scaledHeight(y: number, bounds: Pick<MapBounds, "minY" | "maxY">): number { return Math.max(0, Math.min(1, (y - bounds.minY) / Math.max(.001, bounds.maxY - bounds.minY))); }
export function inHeightSlice(y: number, enabled: boolean, center: number, halfWidth: number): boolean { return Number.isFinite(y) && (!enabled || Math.abs(y - center) <= Math.max(0, halfWidth)); }
export function faceInHeightSlice(vertices: Position3[], face: number[], enabled: boolean, center: number, halfWidth: number): boolean {
  if (face.length < 3 || face.some(i => !Number.isInteger(i) || i < 0 || !vertices[i] || !finitePosition(vertices[i]))) return false;
  if (!enabled) return true;
  const ys = face.map(i => vertices[i][1]); return Math.min(...ys) <= center + halfWidth && Math.max(...ys) >= center - halfWidth;
}
export interface SceneDifference { added: MissionObject[]; absent: MissionObject[]; unchanged: number }
/** Адрес и тип недостаточны: прежний адрес может использоваться повторно. Это сравнение выборок, не событий игры. */
export function objectSampleKey(object: MissionObject): string { return JSON.stringify([object.key, object.itemPath, ...object.position]); }
export function compareScenes(previous: MissionScene, current: MissionScene): SceneDifference {
  const before = new Map(previous.objects.map(o => [objectSampleKey(o), o])); const after = new Map(current.objects.map(o => [objectSampleKey(o), o]));
  return { added: [...after].filter(([k]) => !before.has(k)).map(([, o]) => o), absent: [...before].filter(([k]) => !after.has(k)).map(([, o]) => o), unchanged: [...after.keys()].filter(k => before.has(k)).length };
}
export function missionTime(value: string): string { const date = new Date(value); return Number.isNaN(date.valueOf()) ? "Время неизвестно" : date.toLocaleString("ru-RU", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit" }); }
export function missionBytes(bytes: number): string { return bytes >= 1024 ** 3 ? `${(bytes / 1024 ** 3).toFixed(2)} ГиБ` : `${(Math.max(0, bytes) / 1024 ** 2).toFixed(1)} МиБ`; }
