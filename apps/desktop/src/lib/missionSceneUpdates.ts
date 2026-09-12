import type { MissionScene } from "./missionResearch";
export interface MissionSceneUpdate {
  revision: number; epoch: number; resetObjects: boolean; geometryIncluded: boolean; removed: string[]; scene: MissionScene | null;
}
/** Геометрия сохраняет ссылку между кадрами; пропущенная ревизия восстанавливается полным списком объектов. */
export function applySceneUpdate(previous: MissionScene | null, epoch: number, update: MissionSceneUpdate): MissionScene | null {
  if (!update.scene) return null;
  if ((!update.geometryIncluded || !update.resetObjects) && (!previous || epoch !== update.epoch)) throw new Error("Основа карты потеряна. Загружаем её повторно.");
  let objects = update.scene.objects;
  if (!update.resetObjects && previous) {
    const merged = new Map(previous.objects.map(object => [object.key, object]));
    update.removed.forEach(key => merged.delete(key));
    objects.forEach(object => merged.set(object.key, object));
    objects = [...merged.values()];
  }
  return { ...update.scene, objects, meshes: update.geometryIncluded ? update.scene.meshes : previous!.meshes };
}
