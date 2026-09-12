import { describe, it, expect } from "vitest";
import { objectFilter, followFrame, followTargetSignature, sceneBounds, fitCamera, mapScale, worldToCanvas, canvasToWorld, zoomAt, scaledHeight, inHeightSlice, faceInHeightSlice, compareScenes, objectSampleKey, type MissionScene, type MissionObject, type Position3 } from "./missionResearch";
import { makeMissionScene, makeMissionResearchMock } from "./missionResearchMock";
import { objectMatchesSearch, localAvatar, zoneInHeightSlice } from "./missionResearch";
const object = (extra: Partial<MissionObject> = {}): MissionObject => ({ key: "one", kind: "feather", label: "Перо", nameEn: "Voidplume", itemPath: "ZarimanDogTagCommon", position: [2, 3, 5], availability: "unknown", details: [], typeNames: ["PickUp"], ...extra });
const scene = (objects: MissionObject[] = []): MissionScene => ({ format: 1, source: "archive", startedAt: "2026-09-12T07:00:00Z", capturedAt: "2026-09-12T07:00:30Z", complete: true, profile: "test", objects, meshes: [], warnings: [], stats: { scannedBytes: 0, objectCount: objects.length, meshCount: 0, vertexCount: 0, faceCount: 0 } });

describe("принадлежность игрока и зоны мини-карты", () => {
  it("не выбирает по движению, при неоднозначной или устаревшей связи", () => {
    const s = scene([object({kind:"avatar"}), object({key:"npc",kind:"npc"})]);
    expect(localAvatar(s)).toBeNull();
    s.players = [{key:"owner",avatarKey:"one",operatorKey:null,local:true}];
    expect(localAvatar(s)?.key).toBe("one");
    s.objects[0].positionFresh = false; expect(localAvatar(s)).toBeNull();
    s.objects[0].positionFresh = true;
    s.players.push({...s.players[0],key:"other"}); expect(localAvatar(s)).toBeNull();
    s.players = [{...s.players[0],avatarKey:"npc"}]; expect(localAvatar(s)).toBeNull();
  });
  it("учитывает границы зон при показе всей карты и срезе высоты", () => {
    const s = scene(); const zone = {key:"zone",min:[-100,-20,-200] as Position3,max:[100,10,200] as Position3}; s.zones=[zone];
    expect(sceneBounds(s)).toEqual({minX:-100,maxX:100,minY:-20,maxY:10,minZ:-200,maxZ:200});
    expect(zoneInHeightSlice(zone,true,12,2)).toBe(true);
    expect(zoneInHeightSlice(zone,true,13,2)).toBe(false);
    expect(zoneInHeightSlice({...zone,min:[NaN,0,0]},false,0,0)).toBe(false);
    expect(zoneInHeightSlice({...zone,min:[101,0,0]},false,0,0)).toBe(false);
  });
  it("отделяет возможные места лута от предметов и эвакуации", () => {
    expect(objectFilter("lootspot")).toBe("lootspots");
    expect(objectFilter("extraction")).toBe("goals");
    expect(objectFilter("terminal")).toBe("goals");
    expect(objectFilter("pickup")).toBe("pickup");
  });
});

describe("проекция карты миссии", () => {
  it("ищет русское и английское имя, тип и несколько слов без различия е/ё", () => {
    const found = object({label:"Чертёж награды", nameEn:"Reward Blueprint"});
    for (const query of ["чертеж", "ЧЕРТЁЖ", "reward blueprint", "перья чертеж", "  "]) expect(objectMatchesSearch(found, query)).toBe(true);
    expect(objectMatchesSearch(found, "шкафчик")).toBe(false);
    expect(objectMatchesSearch(object({kind:"npc"}), "неигровой персонаж")).toBe(true);
  });
  it("учитывает отрицательные координаты объектов и геометрии без привязки к игроку", () => {
    const s = scene([object({ position: [-350, -28, 290] })]); s.meshes = [{ key: "mesh", vertices: [[200, 33, -150], [-400, 0, 0]], faces: [], adjacency: [] }];
    expect(sceneBounds(s)).toEqual({ minX: -400, maxX: 200, minY: -28, maxY: 33, minZ: -150, maxZ: 290 });
  });
  it("даёт конечную рамку при пустых или испорченных данных", () => {
    const bounds = sceneBounds(scene([object({ position: [NaN, 1, 2] })]));
    expect(Object.values(bounds).every(Number.isFinite)).toBe(true);
    const single = sceneBounds(scene([object()])); expect(single.maxX - single.minX).toBeGreaterThan(0); expect(single.maxZ - single.minZ).toBeGreaterThan(0);
  });
  it.each([[1100, 520], [360, 400], [1, 1]])("сохраняет единый масштаб осей в окне %i×%i", (width, height) => {
    const b = { minX: -300, maxX: 200, minZ: -100, maxZ: 300, minY: -30, maxY: 40 }; const camera = fitCamera(b); const scale = mapScale(b, width, height);
    const center: Position3 = [camera.x, 0, camera.z]; const p = worldToCanvas(center, camera, scale, width, height);
    expect(p[0]).toBeCloseTo(width / 2); expect(p[1]).toBeCloseTo(height / 2);
    const dx = worldToCanvas([center[0] + 10, 0, center[2]], camera, scale, width, height)[0] - p[0];
    const dz = p[1] - worldToCanvas([center[0], 0, center[2] + 10], camera, scale, width, height)[1]; expect(dx).toBeCloseTo(dz);
    expect(canvasToWorld(...p, camera, scale, width, height)).toEqual([camera.x, camera.z]);
  });
  it("при увеличении сохраняет точку под курсором и ограничивает масштаб", () => {
    const b = sceneBounds(scene([object({ position: [-100, 0, 40] }), object({ position: [120, 0, -80] })])); const camera = fitCamera(b); const width = 760, height = 480;
    const before = canvasToWorld(137, 260, camera, mapScale(b, width, height), width, height);
    const changed = zoomAt(camera, b, width, height, 137, 260, 2);
    const after = canvasToWorld(137, 260, changed, mapScale(b, width, height, changed.zoom), width, height);
    expect(after[0]).toBeCloseTo(before[0]); expect(after[1]).toBeCloseTo(before[1]);
    expect(zoomAt(camera, b, width, height, 137, 260, 1000).zoom).toBe(32); expect(zoomAt(camera, b, width, height, 137, 260, .0001).zoom).toBe(.2);
  });
});

describe("высота геометрии", () => {
  it("нормирует отрицательные и положительные высоты, включая плоский уровень", () => {
    expect(scaledHeight(-20, { minY: -20, maxY: 20 })).toBe(0); expect(scaledHeight(0, { minY: -20, maxY: 20 })).toBe(.5); expect(scaledHeight(20, { minY: -20, maxY: 20 })).toBe(1);
    expect(scaledHeight(5, { minY: 5, maxY: 5 })).toBe(0); expect(scaledHeight(99, { minY: -20, maxY: 20 })).toBe(1);
  });
  it("проверяет высоту Y, а не экранную координату Z", () => {
    expect(inHeightSlice(-20, true, -18, 2)).toBe(true); expect(inHeightSlice(-20.01, true, -18, 2)).toBe(false); expect(inHeightSlice(800, false, -18, 2)).toBe(true); expect(inHeightSlice(NaN, false, 0, 4)).toBe(false);
  });
  it("оставляет пересекающий срез наклонный полигон и исключает повреждённые индексы", () => {
    const vertices: Position3[] = [[0, -10, 0], [10, 10, 0], [10, 10, 10]];
    expect(faceInHeightSlice(vertices, [0, 1, 2], true, 0, 1)).toBe(true); expect(faceInHeightSlice(vertices, [0, 1, 2], true, 20, 1)).toBe(false);
    expect(faceInHeightSlice(vertices, [0, 1, 99], false, 0, 1)).toBe(false); expect(faceInHeightSlice(vertices, [0, 1], false, 0, 1)).toBe(false);
  });
});

describe("сравнение выборок", () => {
  it("не считает изменение текста новым объектом", () => { const diff = compareScenes(scene([object()]), scene([object({ label: "Локализованное перо" })])); expect(diff).toEqual({ added: [], absent: [], unchanged: 1 }); });
  it("различает повторное использование адреса под другой предмет и положение", () => {
    const before = scene([object()]);
    for (const replacement of [object({ itemPath: "ДругойПредмет" }), object({ position: [2, 4, 5] })]) { const diff = compareScenes(before, scene([replacement])); expect(diff.added).toHaveLength(1); expect(diff.absent).toHaveLength(1); expect(diff.unchanged).toBe(0); }
  });
  it("не смешивает пропавшую запись с подтверждением подбора", () => { const diff = compareScenes(scene([object()]), scene()); expect(diff.absent).toHaveLength(1); expect(Object.keys(diff)).toEqual(["added", "absent", "unchanged"]); });
  it("ключ не создаёт коллизии из разделителей в строковых полях", () => { expect(objectSampleKey(object({ key: "a|b", itemPath: "c" }))).not.toBe(objectSampleKey(object({ key: "a", itemPath: "b|c" }))); });
  it("реальная выборка перьев меняется с восьми до шести", () => { const diff = compareScenes(makeMissionScene(1), makeMissionScene(2)); expect(diff.absent).toHaveLength(2); expect(diff.added).toHaveLength(0); expect(diff.absent.every(o => o.kind === "feather")).toBe(true); });
});

describe("мок исследования миссии", () => {
  it("не меняет ревизию при чтении готовой сцены", async () => { const mock = makeMissionResearchMock(); const initial = await mock("mission_research_status"); await mock("mission_research_scene"); await mock("mission_research_scene"); expect(await mock("mission_research_status")).toEqual(initial); });
  it("поддерживает чтение, отмену и повторное чтение второго снимка", async () => {
    const mock = makeMissionResearchMock(); await mock("mission_research_analyze_archive", { id: "preview-zariman", sequence: 2 }); expect(await mock("mission_research_cancel")).toMatchObject({ busy: false });
    expect(await mock("mission_research_scene")).toMatchObject({ capturedAt: makeMissionScene(1).capturedAt });
    await mock("mission_research_analyze_archive", { id: "preview-zariman", sequence: 2 }); for (let i = 0; i < 3; i++) await mock("mission_research_status");
    expect(await mock("mission_research_scene")).toMatchObject({ capturedAt: makeMissionScene(2).capturedAt });
  });
  it("обновляет движущегося персонажа только при включённой живой карте", async () => {
    const mock = makeMissionResearchMock("tracking"); const before = await mock("mission_research_scene") as MissionScene; await mock("mission_research_status"); const after = await mock("mission_research_scene") as MissionScene;
    expect(after.objects.find(o => o.kind === "avatar")?.position).not.toEqual(before.objects.find(o => o.kind === "avatar")?.position);
    await mock("mission_research_track", { enabled: false }); await mock("mission_research_status"); expect(await mock("mission_research_scene")).toEqual(after);
  });
  it("не запускает живую карту без первого чтения игры", async () => { const mock = makeMissionResearchMock(); await expect(mock("mission_research_track", { enabled: true })).rejects.toContain("Сначала прочитайте"); });
  it("показывает отсутствие игры и неизвестный снимок как ошибки", async () => { const mock = makeMissionResearchMock("offline"); await expect(mock("mission_research_scan_live")).rejects.toContain("не запущен"); await expect(mock("mission_research_analyze_archive", { id: "missing", sequence: 1 })).rejects.toContain("не найден"); });
});


describe("игроки и NPC на карте", () => {
  it("относит к игрокам только avatar, исключая Корпус и заложника", () => {
    expect(objectFilter("avatar")).toBe("players");
    expect(objectFilter("npc")).toBe("npc"); expect(objectFilter("hostage")).toBe("npc"); expect(objectFilter("spawnpoint")).toBe("npc");
    const players = makeMissionScene().objects.filter(o => objectFilter(o.kind) === "players");
    expect(players.map(o => o.key)).toEqual(["preview-avatar"]);
    const npcs = makeMissionScene().objects.filter(o => objectFilter(o.kind) === "npc");
    expect(npcs.map(o => o.key)).toContain("preview-corpus-npc"); expect(npcs.map(o => o.key)).toContain("preview-hostage");
  });
  it("держит follow и камеру при временной потере координат, затем продолжает без изменения масштаба", () => {
    const avatar = object({ kind: "avatar", position: [100, 7, 300] }); const camera = { x: 10, z: 20, zoom: 8 }; const signature = followTargetSignature(avatar);
    const waiting = followFrame(avatar.key, signature, true, [{ ...avatar, positionFresh: false }], camera);
    expect(waiting).toEqual({ key: avatar.key, camera, height: null, reason: null });
    const restored = followFrame(waiting.key, signature, true, [{ ...avatar, positionFresh: true }], waiting.camera);
    expect(restored.camera).toEqual({ x: 100, z: 300, zoom: 8 }); expect(restored.height).toBe(7); expect(restored.key).toBe(avatar.key);
  });
  it("считает positionFresh без поля свежим для старых снимков", () => {
    const avatar = object({ kind: "avatar" }); const result = followFrame(avatar.key, followTargetSignature(avatar), true, [avatar], { x: 99, z: 99, zoom: 4 });
    expect(result.camera).toEqual({ x: 2, z: 5, zoom: 4 });
  });
  it("останавливает follow при tracking=false без перемещения камеры", () => {
    const avatar = object({ kind: "avatar" }); const camera = { x: 42, z: 43, zoom: 7 };
    expect(followFrame(avatar.key, followTargetSignature(avatar), false, [avatar], camera)).toEqual({ key: "", camera, height: null, reason: "stopped" });
  });
  it("не начинает или не продолжает следование за NPC под прежним адресом", () => {
    const avatar = object({ kind: "avatar" }); const camera = { x: 1, z: 2, zoom: 3 };
    for (const kind of ["npc", "hostage", "spawnpoint"] as const) {
      const npc = object({ kind }); expect(followFrame(npc.key, followTargetSignature(npc), true, [npc], camera).key).toBe("");
      expect(followFrame(avatar.key, followTargetSignature(avatar), true, [npc], camera).camera).toEqual(camera);
    }
  });
  it("мок воспроизводит временно устаревшую позицию без удаления игрока", async () => {
    const mock = makeMissionResearchMock("tracking-stale"); await mock("mission_research_status"); const first = await mock("mission_research_scene") as MissionScene;
    await mock("mission_research_status"); const second = await mock("mission_research_scene") as MissionScene;
    expect(second.objects.find(o => o.kind === "avatar")).toMatchObject({ positionFresh: false, position: first.objects.find(o => o.kind === "avatar")!.position });
    await mock("mission_research_status"); await mock("mission_research_status"); const fourth = await mock("mission_research_scene") as MissionScene;
    expect(fourth.objects.find(o => o.kind === "avatar")?.positionFresh).toBe(true);
  });
});
