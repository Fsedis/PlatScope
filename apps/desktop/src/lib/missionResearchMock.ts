import type { MissionScene, MissionObject, MissionMesh, MissionResearchStatus, MissionArchive } from "./missionResearch";
import geometryFixture from "./__fixtures__/missionResearchGeometry.json";
const recordedMeshes = geometryFixture as MissionMesh[];
// Настоящие координаты перьев и все 30 извлечённых частей геометрии из записи Заримана 12.09.2026.
// Fixture используется только моками: полные полигоны для проверки отображения, координаты округлены до 0,001.
const recordedFeathers = [{"number":1,"base":"0x26196ac1430","type":"ZarimanDogTagCommon","position":[-113.54986572265625,-6.750004291534424,-76.49998474121094],"presentInSecondSnapshot":false},{"number":2,"base":"0x261baa53740","type":"ZarimanDogTagCommon","position":[-106.47724151611328,1.2499804496765137,170.3057861328125],"presentInSecondSnapshot":true},{"number":3,"base":"0x261ce40ba30","type":"ZarimanDogTagCommon","position":[-94.16438293457031,-0.8789792060852051,-55.890594482421875],"presentInSecondSnapshot":false},{"number":4,"base":"0x261a8c5d200","type":"ZarimanDogTagUncommon","position":[-158.31591796875,-21.074867248535156,166.50100708007812],"presentInSecondSnapshot":true},{"number":5,"base":"0x261ad0de070","type":"ZarimanDogTagUncommon","position":[-341.0060729980469,-16.35940170288086,48.033355712890625],"presentInSecondSnapshot":true},{"number":6,"base":"0x2620ccf9bb0","type":"ZarimanDogTagUncommon","position":[3.128045082092285,-0.5103771686553955,-22.51422882080078],"presentInSecondSnapshot":true},{"number":7,"base":"0x2620ce047d0","type":"ZarimanDogTagUncommon","position":[-367.0001220703125,-28.500015258789062,61.0699462890625],"presentInSecondSnapshot":true},{"number":8,"base":"0x261b20bcdc0","type":"ZarimanDogTagRare","position":[-291.2008361816406,-15.672859191894531,289.55279541015625],"presentInSecondSnapshot":true}];
export function makeMissionScene(sequence = 1, source: "archive" | "live" = "archive"): MissionScene {
  const names: Record<string, [string, string]> = { ZarimanDogTagCommon: ["Перо Бездны · обычное", "Voidplume Down"], ZarimanDogTagUncommon: ["Перо Бездны · необычное", "Voidplume Vane"], ZarimanDogTagRare: ["Перо Бездны · редкое", "Voidplume Crest"] };
  const objects: MissionObject[] = recordedFeathers.filter(f => sequence === 1 || f.presentInSecondSnapshot).map(f => ({ key: f.base, kind: "feather", label: names[f.type][0], nameEn: names[f.type][1], itemPath: f.type, position: [...f.position] as [number, number, number], typeNames: ["PickUp"], availability: "unknown", details: [{ label: "Источник типа", value: "Предмет, связанный с объектом подбора" }] }));
  objects.push({ key: "preview-pickup", kind: "pickup", label: "Ресурс с длинным названием для проверки переноса текста", nameEn: "Resource with a long name for layout verification", itemPath: null, position: [-130, -6, -65], typeNames: ["PickUp"], availability: "unknown", details: [{ label: "Пример", value: "Проверка интерфейса" }] }, { key: "preview-avatar", kind: "avatar", label: "Персонаж игрока", nameEn: "TennoAvatar", itemPath: null, position: [-120, -6, -61], typeNames: ["TennoAvatar"], availability: "unknown", details: [] }, { key: "preview-panel", kind: "panel", label: "Панель", nameEn: "Panel", itemPath: null, position: [-128, -6, -68], typeNames: ["Panel"], availability: "unknown", details: [] });
  objects.push({ key: "preview-corpus-npc", kind: "npc", label: "Боец Корпуса", nameEn: "Corpus Crewman", itemPath: "/Lotus/Types/Enemies/Corpus/Spaceman/RifleSpacemanAvatar", position: [-115, -6, -64], positionFresh: true, typeNames: ["LotusNpcAvatar"], availability: "unknown", details: [] }, { key: "preview-hostage", kind: "hostage", label: "Заложник", nameEn: "HostageAvatar", itemPath: null, position: [-118, -6, -63], positionFresh: true, typeNames: ["HostageAvatar"], availability: "unknown", details: [] });
  objects.push(...([['preview-exit','extraction','Эвакуация','ExtractionTrigger',[-110,-6,-50]],['preview-terminal','terminal','Терминал союзного МОА','BipedSpawner',[-125,-6,-60]],['preview-lootspot','lootspot','Возможное место редкого контейнера','RareLootCrateWaypoint',[-132,-6,-68]]] as const).map(([key,kind,label,nameEn,position]) => ({key,kind,label,nameEn,position:[...position] as [number,number,number],itemPath:null,typeNames:[nameEn],availability:'unknown' as const,details:[]})));
  // Реальные позиции трёх тайников из записи Гринир; состояния для проверки интерфейса.
  for (const [index, position] of ([[-154.61235,2.33101,8.88806],[285.71234,-23.51897,230.61194],[338.04999,-3.341,38.21152]] as [number,number,number][]).entries()) {
    objects.push({key:`preview-cache-${index}`,kind:"cache",label:"Тайник Гринир",nameEn:"Grineer Resource Cache",itemPath:null,variantKey:"type-v1:grineer-cache",position,typeNames:["Decoration *"],availability:index < 2 ? "opened" : "available",details:[{label:"Ресурс",value:"/Lotus/Objects/Grineer/Structural/Doors/GrnStorageLocker_skel.fbx"},{label:"Материалы варианта",value:"/Lotus/Objects/Grineer/Structural/Doors/GrineerDoorHatchWhiteCache"}]});
  }
  for (let index=0;index<3;index++) objects.push({key:`preview-crate-${index}`,kind:"decoration",label:"Ящик Гринир",nameEn:"GrnLootCrateARareB.fbx",itemPath:null,variantKey:index===2?"type-v1:crate-b":"type-v1:crate-a",position:[20+index*15,0,30],typeNames:["Decoration *"],availability:"unknown",details:[{label:"Ресурс",value:index===0?"/Lotus/Objects/Grineer/Props/GrnLootCrateARareB.fbx":"GrnLootCrateARareB.fbx"},{label:"Материалы варианта",value:index===2?"Материал B":"Материал A"}]});
  const meshes = structuredClone(recordedMeshes);
  return { format: 1, source, startedAt: `2026-09-12T07:${sequence === 1 ? "10" : "14"}:00Z`, capturedAt: `2026-09-12T07:${sequence === 1 ? "10" : "14"}:30Z`, complete: true, profile: "Проверка интерфейса на данных Заримана", objects, meshes,
    cameraHeading: 0, players:[{key:'preview-owner',avatarKey:'preview-avatar',operatorKey:'preview-operator',local:true}],
    zones:[{key:'preview-zone',min:[-145,-12,-80],max:[-100,4,-40]}],zonesFresh:true,
    warnings: ["Демонстрационные данные: настоящие координаты перьев и все извлечённые полигоны из записи. Дополнительные примеры объектов созданы для проверки интерфейса.", "Принадлежность всех частей активной миссии и доступность предметов не подтверждены."],
    stats: { scannedBytes: 7_000_000_000, objectCount: objects.length, meshCount: meshes.length, vertexCount: meshes.reduce((sum, m) => sum + m.vertices.length, 0), faceCount: meshes.reduce((sum, m) => sum + m.faces.length, 0) } };
}
export function makeMissionResearchMock(variant: string | null = null) {
  let scene: MissionScene | null = variant === "empty" || variant === "offline" || variant === "error" ? null : makeMissionScene(1, variant?.startsWith("tracking") ? "live" : "archive");
  if (scene && variant === "partial") { scene.complete = false; scene.warnings.unshift("Не все области памяти удалось прочитать."); }
  let status: MissionResearchStatus = { busy: false, cancelling: false, phase: "", error: variant === "error" ? "Не удалось открыть сохранённый блок. Проверьте целостность папки записи." : null, revision: scene ? 1 : 0, gameRunning: variant !== "offline", tracking: Boolean(variant?.startsWith("tracking")), scannedBytes: 0 };
  let poseTicks = 0; let epoch = 1; let updateCalls = 0; let previous: { revision: number; objects: MissionObject[] } | null = null;
  let polls = 0; let liveTicks = 0; let nextSequence = 1; let nextSource: "archive" | "live" = "live";
  const archive: MissionArchive = { id: "preview-zariman", label: "Зариман · проверочная запись", createdAt: "2026-09-12T07:10:00Z", sizeBytes: 4_400_000_000, snapshots: [1, 2].map(sequence => ({ sequence, startedAt: `2026-09-12T07:${sequence === 1 ? "10" : "14"}:00Z`, endedAt: `2026-09-12T07:${sequence === 1 ? "10" : "14"}:30Z`, complete: true, bytes: 7_000_000_000, holes: 0 })) };
  return async (command: string, args: Record<string, unknown> = {}): Promise<unknown> => {
    if (command === "mission_research_archives") return variant === "empty" ? [] : structuredClone([archive]);
    if (command === "mission_research_scene") return structuredClone(scene);
    if (command === "mission_research_pose") {
      const avatar = scene?.objects.find(o => o.key === "preview-avatar"); poseTicks++;
      return { epoch, pose: avatar && status.tracking && args.epoch === epoch ? { avatarKey: avatar.key, position: [avatar.position[0] + (poseTicks % 20) * .04, avatar.position[1], avatar.position[2]], cameraHeading: (poseTicks * .008) % (Math.PI * 2) } : null };
    }
    if (command === "mission_research_update") {
      if (variant === "retry" && ++updateCalls === 1) throw "Проверочный временный сбой загрузки.";
      const geometryIncluded = args.geometryEpoch !== epoch;
      const baseline = !geometryIncluded && args.afterRevision === previous?.revision ? previous : null;
      const result = scene ? structuredClone(scene) : null;
      const removed = baseline?.objects.filter(old => !scene?.objects.some(o => o.key === old.key)).map(o => o.key) ?? [];
      if (result) {
        if (!geometryIncluded) result.meshes = [];
        if (baseline) result.objects = result.objects.filter(o => JSON.stringify(o) !== JSON.stringify(baseline.objects.find(old => old.key === o.key)));
      }
      return { revision: status.revision, epoch, resetObjects: !baseline, geometryIncluded, removed, scene: result };
    }
    if (command === "mission_research_status") {
      if (status.busy && ++polls >= 3) { scene = makeMissionScene(nextSequence, nextSource); epoch++; previous = null; status = { ...status, busy: false, phase: "Чтение завершено", revision: status.revision + 1, scannedBytes: scene.stats.scannedBytes }; }
      else if (status.busy) status.scannedBytes += 1_300_000_000;
      else if (status.tracking && scene) {
        previous = { revision: status.revision, objects: structuredClone(scene.objects) };
        liveTicks++; scene.cameraHeading = variant === "tracking-stale" && (liveTicks === 2 || liveTicks === 3) ? null : (liveTicks * .08) % (2 * Math.PI); scene.capturedAt = new Date(Date.parse(scene.capturedAt) + 1000).toISOString();
        const stale = variant === "tracking-stale" && (liveTicks === 2 || liveTicks === 3);
        scene.objects = scene.objects.map(o => o.kind === "avatar" ? stale ? { ...o, positionFresh: false } : { ...o, positionFresh: true, position: [o.position[0] + .8, o.position[1], o.position[2] + .3] } : o);
        if (liveTicks === 5) scene.objects = scene.objects.filter(o => o.key !== recordedFeathers[0].base);
        scene.stats.objectCount = scene.objects.length; status.revision++;
      }
      return structuredClone(status);
    }
    if (command === "mission_research_scan_live" || command === "mission_research_analyze_archive") {
      if (status.busy) throw "Чтение уже выполняется.";
      if (command === "mission_research_scan_live" && !status.gameRunning) throw "Warframe не запущен.";
      if (command === "mission_research_analyze_archive" && (args.id !== archive.id || !archive.snapshots.some(s => s.sequence === args.sequence))) throw "Снимок не найден.";
      if (variant === "error") throw "Не удалось прочитать данные. Предыдущий результат сохранён.";
      nextSequence = command === "mission_research_analyze_archive" ? Number(args.sequence) : scene ? 2 : 1; nextSource = command === "mission_research_analyze_archive" ? "archive" : "live";
      polls = 0; status = { ...status, busy: true, cancelling: false, error: null, phase: "Читаем сохранённые области", scannedBytes: 0 }; return structuredClone(status);
    }
    if (command === "mission_research_cancel") { status = { ...status, busy: false, cancelling: false, phase: "Чтение отменено" }; return structuredClone(status); }
    if (command === "mission_research_export") { if (!scene) throw "Сначала прочитайте миссию."; if (args.format !== "json" && args.format !== "obj") throw "Неизвестный формат."; return `C:\\PlatScope\\diagnostics\\mission-research\\mission-preview.${args.format}`; }
    if (command === "mission_research_filters") return null;
    if (command === "mission_research_track") { if (args.enabled && (!scene || scene.source !== "live")) throw "Сначала прочитайте миссию из игры."; status.tracking = Boolean(args.enabled); return structuredClone(status); }
    throw `Неизвестная команда исследования: ${command}`;
  };
}
