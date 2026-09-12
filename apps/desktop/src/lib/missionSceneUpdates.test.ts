import { expect, it } from "vitest";
import { applySceneUpdate, type MissionSceneUpdate } from "./missionSceneUpdates";
import { makeMissionScene } from "./missionResearchMock";
import { canvasToWorld, mapScale, objectDistance, panCamera, sceneBounds, worldToCanvas, zoomAt } from "./missionResearch";

it("применяет изменения и удаления, сохраняя геометрию и неизменённые объекты", () => {
  const scene = makeMissionScene(); const removed = scene.objects[0], unchanged = scene.objects[1];
  const changed = { ...scene.objects[2], position: [10, 20, 30] as [number, number, number] };
  const added = { ...changed, key: "new" };
  const update: MissionSceneUpdate = { revision: 2, epoch: 1, resetObjects: false, geometryIncluded: false, removed: [removed.key], scene: { ...scene, meshes: [], objects: [changed, added], cameraHeading: 1.2 } };
  const next = applySceneUpdate(scene, 1, update)!;
  expect(next.meshes).toBe(scene.meshes); expect(next.cameraHeading).toBe(1.2);
  expect(next.objects.find(o => o.key === unchanged.key)).toBe(unchanged);
  expect(next.objects.find(o => o.key === changed.key)).toBe(changed);
  expect(next.objects.some(o => o.key === removed.key)).toBe(false);
  expect(next.objects.at(-1)).toBe(added);
  expect(() => applySceneUpdate(null, -1, update)).toThrow();
  expect(() => applySceneUpdate(scene, 0, update)).toThrow();
  const reset = applySceneUpdate(scene, 1, { ...update, resetObjects: true })!;
  expect(reset.objects).toEqual([changed, added]); expect(reset.meshes).toBe(scene.meshes);
  expect(applySceneUpdate(scene, 1, { ...update, scene: null })).toBeNull();
});

it("поворот сохраняет попадание в объект, точку масштабирования и направление перетаскивания", () => {
  const bounds = sceneBounds(makeMissionScene()), width = 1200, height = 500;
  for (const angle of [0, .8, Math.PI / 2, -3.1]) {
    const camera = { x: 10, z: -20, zoom: 3, angle }, point: [number, number, number] = [35, 4, 62];
    const scale = mapScale(bounds, width, height, camera.zoom), pixel = worldToCanvas(point, camera, scale, width, height);
    const restored = canvasToWorld(...pixel, camera, scale, width, height);
    expect(restored[0]).toBeCloseTo(point[0]); expect(restored[1]).toBeCloseTo(point[2]);
    const zoomed = zoomAt(camera, bounds, width, height, ...pixel, 1.4);
    const moved = worldToCanvas(point, zoomed, mapScale(bounds, width, height, zoomed.zoom), width, height);
    expect(moved[0]).toBeCloseTo(pixel[0]); expect(moved[1]).toBeCloseTo(pixel[1]);
    const dragged = worldToCanvas(point, panCamera(camera, 35, -12, scale), scale, width, height);
    expect(dragged[0]).toBeCloseTo(pixel[0] + 35); expect(dragged[1]).toBeCloseTo(pixel[1] - 12);
    const forward = worldToCanvas([camera.x + Math.sin(angle) * 10, 0, camera.z + Math.cos(angle) * 10], camera, scale, width, height);
    expect(forward[0]).toBeCloseTo(width / 2); expect(forward[1]).toBeLessThan(height / 2);
  }
});

it("не выдаёт старые координаты за актуальное расстояние до цели", () => {
  const player = makeMissionScene().objects[0], target = { ...player, position: [player.position[0] + 3, player.position[1] + 4, player.position[2]] as [number, number, number] };
  expect(objectDistance(player, target)).toBe(5);
  expect(objectDistance(null, target)).toBeNull();
  expect(objectDistance(player, { ...target, positionFresh: false })).toBeNull();
});
