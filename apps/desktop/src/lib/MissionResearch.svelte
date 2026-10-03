<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { applySceneUpdate, type MissionSceneUpdate } from "./missionSceneUpdates";
  import MemoryRecording from "./MemoryRecording.svelte";
  import MissionFilterEditor from "./MissionFilterEditor.svelte";
  import MissionHiddenNames from "./MissionHiddenNames.svelte";
  import MissionObjectIcon from "./MissionObjectIcon.svelte";
  import MissionMapSettings from "./MissionMapSettings.svelte";
  import { createMissionMapRenderer } from "./missionMapRenderer";
  import { defaultMissionMapPreferences, loadMissionMapPreferences, saveMissionMapPreferences, missionMarkerSize, missionMarkerHitRadius, type MissionMapPreferences } from "./missionMapPreferences";
  import { MISSION_HIDDEN_STORAGE, hiddenMissionNameKey, hiddenMissionNameLabel, indexHiddenMissionNames, objectIsHidden, parseHiddenMissionNames, serializeHiddenMissionNames, type HiddenMissionName } from "./missionFilters";
  import { MISSION_FILTER_STORAGE, parseMissionFilters, serializeMissionFilters, missionDiscoveryKeys, groupMissionObjects, objectRule, objectFilterEntry, indexMissionFilters, countMissionFilters, filterColor, objectIsVisible, type MissionFilterIndex, type CustomMissionFilter } from "./missionFilters";
  import { MISSION_FILTERS, localAvatar, objectMatchesSearch, objectFilter, objectKindLabel, finitePosition, sceneBounds, fitCamera, followFrame, followTargetSignature, mapScale, worldToCanvas, zoomAt, panCamera, objectDistance, distanceLabel, inHeightSlice, compareScenes, missionTime, missionBytes,
    type MissionScene, type MissionObject, type MissionResearchStatus, type MissionArchive, type MissionFilter, type MapCamera, type MapBounds } from "./missionResearch";

  let source: "live" | "archive" = "live";
  let status: MissionResearchStatus | null = null;
  let scene: MissionScene | null = null;
  let previousScene: MissionScene | null = null;
  let archives: MissionArchive[] = [];
  let archiveId = "";
  let sequence = 0;
  let loadingArchives = false;
  let archiveError = "";
  let error = "";
  let exportPath = "";
  let exporting = false;
  let actionBusy = false;
  let startTrackingAfterScan = false;
  let frameNextScene = true;
  let polling = false;
  let loadedRevision = -1;
  let loadedEpoch = -1;
  let retryAt = 0;
  let retryAttempts = 0;
  let sceneError = "";
  let rotateWithView = false;
  let posePolling = false;
  let poseReceived = false;
  let fastPose: { avatarKey: string; position: [number, number, number] | null; cameraHeading: number | null } | null = null;
  let targetKey = "";
  let cacheState = "all";
  let sortByDistance = false;
  let groupObjects = true;
  let generation = 0;
  let alive = true;
  let filters: Record<MissionFilter, boolean> = { feather: true, pickup: true, decree_fragments: true, players: true, npc: false, other: false, goals: true, lootspots: false, caches: true, dragon_doors: true };
  let customFilters: CustomMissionFilter[] = [];
  let hiddenNames: HiddenMissionName[] = [];
  let hiddenStorageError = "";
  let hiddenLoadFailed = false;
  let filterRail: HTMLElement | undefined;
  let lastFilterSelection = "";
  let filterStorageError = "";
  let filterLoadFailed = false;
  let filterSyncBusy = false;
  let syncedFilterKeys = "";
  let filterSyncError = "";
  let pickedKeys: string[] = [];
  let query = "";
  let selectedKey = "";
  let followKey = "";
  let followingMe = false;
  let followSignature = "";
  let followHint = "";
  let framingBounds: MapBounds | null = null;
  let listLimit = 100;
  let sliceEnabled = false;
  let showZones = false;
  let heightCenter = 0;
  let heightHalfWidth = 4;
  let markerPreferences = defaultMissionMapPreferences();
  let markerStorageError = "";
  let markerSettings: MissionMapSettings | undefined;
  let camera: MapCamera = { x: 0, z: 0, zoom: 1 };
  let canvas: HTMLCanvasElement | undefined;
  let canvasWidth = 800;
  let canvasHeight = 480;
  let drag: { x: number; y: number; camera: MapCamera; moved: boolean; pointer: number } | null = null;
  const filterIcons: Record<MissionFilter, MissionObject["kind"]> = {
    feather: "feather", pickup: "pickup", decree_fragments: "decree_fragment", players: "avatar",
    npc: "npc", other: "decoration", goals: "extraction", lootspots: "lootspot", caches: "cache", dragon_doors: "dragon_door",
  };

  $: currentArchive = archives.find(a => a.id === archiveId);
  $: currentSnapshot = currentArchive?.snapshots.find(s => s.sequence === sequence);
  $: bounds = framingBounds ?? (scene ? sceneBounds(scene) : sceneBounds({ objects: [], meshes: [] }));
  $: objects = scene?.objects.filter(o => finitePosition(o.position)) ?? [];
  $: customIndex = indexMissionFilters(customFilters);
  $: hiddenIndex = indexHiddenMissionNames(hiddenNames);
  $: availableObjects = objects.filter(o => !objectIsHidden(o, hiddenIndex));
  $: customCounts = countMissionFilters(availableObjects, customIndex);
  $: visibleObjects = availableObjects.map(o => o.key === myAvatar?.key ? myAvatar : o).filter(o => objectIsVisible(o, filters, customIndex) && inHeightSlice(o.position[1], sliceEnabled, heightCenter, heightHalfWidth) && objectMatchesSearch(o, query) && (o.kind !== "cache" || cacheState === "all" || o.availability === cacheState)).sort((a, b) => (sortByDistance ? (objectDistance(myAvatar, a) ?? Infinity) - (objectDistance(myAvatar, b) ?? Infinity) : 0) || (a.kind === "feather" ? 0 : 1) - (b.kind === "feather" ? 0 : 1) || a.label.localeCompare(b.label, "ru"));
  $: listEntries = groupMissionObjects(visibleObjects, groupObjects);
  $: sceneAvatar = scene ? localAvatar(scene) : null;
  $: myAvatar = poseReceived && status?.tracking ? (sceneAvatar && fastPose?.avatarKey === sceneAvatar.key && fastPose.position ? { ...sceneAvatar, position: fastPose.position, positionFresh: true } : null) : sceneAvatar;
  $: viewHeading = poseReceived && status?.tracking ? fastPose?.cameraHeading : scene?.cameraHeading;
  $: target = availableObjects.find(o => o.key === targetKey) ?? null;
  $: nearestCache = availableObjects.filter(o => o.kind === "cache" && o.availability === "available" && objectDistance(myAvatar, o) !== null).sort((a, b) => objectDistance(myAvatar, a)! - objectDistance(myAvatar, b)!)[0] ?? null;
  $: selected = selectedKey === myAvatar?.key && !objectIsHidden(myAvatar, hiddenIndex) ? myAvatar : availableObjects.find(o => o.key === selectedKey) ?? null;
  $: pickedObjects = availableObjects.filter(object => pickedKeys.includes(object.key));
  $: filterSelection = pickedKeys.length ? pickedObjects : selected ? [selected] : [];
  $: filterSelectionKey = filterSelection.map(object => object.key).join("|");
  $: if (filterRail && filterSelectionKey !== lastFilterSelection) { filterRail.scrollTop = 0; lastFilterSelection = filterSelectionKey; }
  $: difference = scene && previousScene ? compareScenes(previousScene, scene) : null;
  $: drawState = { scene, liveAvatar: myAvatar, viewHeading, bounds, camera, objects: visibleObjects, selectedKey, targetKey, rotateWithView, pickedKeys, customIndex, showZones, sliceEnabled, heightCenter, heightHalfWidth, markerPreferences, width: canvasWidth, height: canvasHeight };
  $: if (canvas) draw(canvas, drawState);

  async function receive(next: MissionResearchStatus, version: number) {
    if (!alive || version !== generation) return;
    status = next;
    if (!next.tracking && followKey) { followKey = ""; followingMe = false; followHint = ""; }
    if (loadedRevision >= next.revision || Date.now() < retryAt) return;
    try {
      const update = await invoke<MissionSceneUpdate>("mission_research_update", { afterRevision: loadedRevision < 0 ? null : loadedRevision, geometryEpoch: loadedEpoch < 0 ? null : loadedEpoch });
      if (!alive || version !== generation || update.revision < loadedRevision) return;
      let nextScene: MissionScene | null;
      try { nextScene = applySceneUpdate(scene, loadedEpoch, update); }
      catch (reason) { loadedEpoch = -1; throw reason; }
      const sameEpoch = loadedEpoch === update.epoch;
      loadedRevision = update.revision; loadedEpoch = update.epoch;
      retryAt = 0; retryAttempts = 0; sceneError = "";
      if (nextScene) {
        const continuingLive = sameEpoch && scene?.source === "live" && nextScene.source === "live" && !frameNextScene;
        previousScene = scene?.source === "archive" && nextScene.source === "archive" && scene.capturedAt !== nextScene.capturedAt ? scene : nextScene.source === "live" ? null : previousScene;
        if (!scene && !startTrackingAfterScan) source = nextScene.source;
        scene = nextScene;
        if (!continuingLive) {
          poseReceived = false; fastPose = null; selectedKey = ""; targetKey = ""; pickedKeys = []; followKey = ""; followingMe = false; followHint = ""; listLimit = 100; query = "";
          const nextBounds = sceneBounds(nextScene); framingBounds = nextBounds; camera = fitCamera(nextBounds); heightCenter = (nextBounds.minY + nextBounds.maxY) / 2;
        } else if (!nextScene.objects.some(o => o.key === selectedKey)) selectedKey = "";
        frameNextScene = false;
        if (rotateWithView && !poseReceived && Number.isFinite(nextScene.cameraHeading)) camera = { ...camera, angle: nextScene.cameraHeading! };
        if (followKey && followingMe && !localAvatar(nextScene)) {
          followHint = "Связь с вашим персонажем временно не подтверждена. Вид сохранён.";
        } else if (followKey && !(followingMe && poseReceived)) {
          const mine = followingMe ? localAvatar(nextScene) : null;
          if (mine && mine.key !== followKey) { followKey = mine.key; followSignature = followTargetSignature(mine); }
          followHint = "";
          const frame = followFrame(followKey, followSignature, next.tracking, nextScene.objects, camera);
          followKey = frame.key; camera = frame.camera;
          if (sliceEnabled && frame.height !== null) heightCenter = frame.height;
          if (frame.reason === "missing") followHint = "Выбранный игрок больше не виден. Следование остановлено.";
        }
        if (startTrackingAfterScan && !next.busy && nextScene.source === "live") {
          startTrackingAfterScan = false;
          const tracked = await invoke<MissionResearchStatus>("mission_research_track", { enabled: true });
          if (alive && version === generation) status = tracked;
        }
      } else {
        scene = null; previousScene = null; poseReceived = false; fastPose = null; selectedKey = ""; targetKey = ""; pickedKeys = []; followKey = ""; followingMe = false; followHint = ""; framingBounds = null; frameNextScene = true;
      }
    } catch (reason) { if (alive && version === generation) { retryAt = Date.now() + Math.min(8000, 1000 * 2 ** retryAttempts++); sceneError = `${typeof reason === "string" ? reason : reason instanceof Error ? reason.message : "Не удалось загрузить карту."} Повторим автоматически.${scene ? " Прежний вид сохранён." : ""}`; } }
  }
  async function refreshPose() {
    if (posePolling || actionBusy || !status?.tracking || scene?.source !== "live" || loadedEpoch < 0 || document.hidden) return;
    posePolling = true; const version = generation, epoch = loadedEpoch;
    try {
      const update = await invoke<{ epoch: number; pose: typeof fastPose }>("mission_research_pose", { epoch });
      if (!alive || version !== generation || epoch !== loadedEpoch || update.epoch !== epoch || !status?.tracking) return;
      fastPose = update.pose; poseReceived = true;
      if (rotateWithView && Number.isFinite(update.pose?.cameraHeading)) camera = { ...camera, angle: update.pose!.cameraHeading! };
      if (followingMe && followKey && update.pose?.position && update.pose.avatarKey === sceneAvatar?.key) {
        camera = { ...camera, x: update.pose.position[0], z: update.pose.position[2] };
        if (sliceEnabled) heightCenter = update.pose.position[1];
        followHint = "";
      } else if (followingMe && followKey) followHint = "Свежая позиция временно не подтверждена. Вид сохранён.";
    } catch {
      if (alive && version === generation && epoch === loadedEpoch) { fastPose = null; poseReceived = true; }
    } finally { posePolling = false; }
  }
  async function refresh() {
    void syncDiscoveryFilters();
    if (polling || actionBusy) return;
    polling = true; const version = generation;
    try { await receive(await invoke<MissionResearchStatus>("mission_research_status"), version); }
    catch (reason) { if (alive && version === generation) error = typeof reason === "string" ? reason : "Не удалось получить состояние исследования."; }
    finally { polling = false; }
  }
  async function refreshArchives() {
    if (loadingArchives) return;
    loadingArchives = true; archiveError = "";
    try {
      const next = await invoke<MissionArchive[]>("mission_research_archives");
      if (!alive) return;
      archives = next;
      if (!next.some(a => a.id === archiveId)) archiveId = next[0]?.id ?? "";
      chooseArchive();
    } catch (reason) { if (alive) archiveError = typeof reason === "string" ? reason : "Не удалось прочитать список записей."; }
    finally { loadingArchives = false; }
  }
  function chooseArchive() {
    const archive = archives.find(a => a.id === archiveId);
    if (!archive?.snapshots.some(s => s.sequence === sequence)) sequence = archive?.snapshots.at(-1)?.sequence ?? 0;
  }
  async function run(cancel = false) {
    if (actionBusy) return;
    actionBusy = true; const version = ++generation; error = ""; exportPath = "";
    startTrackingAfterScan = !cancel && source === "live";
    if (!cancel) frameNextScene = true;
    try {
      const command = cancel ? "mission_research_cancel" : source === "live" ? "mission_research_scan_live" : "mission_research_analyze_archive";
      const args = !cancel && source === "archive" ? { id: archiveId, sequence } : undefined;
      await receive(await invoke<MissionResearchStatus>(command, args), version);
    } catch (reason) { startTrackingAfterScan = false; if (alive && version === generation) error = typeof reason === "string" ? reason : "Не удалось выполнить чтение. Попробуйте ещё раз."; }
    finally { actionBusy = false; }
  }
  async function stopTracking() {
    if (actionBusy) return;
    actionBusy = true; startTrackingAfterScan = false; followKey = ""; followingMe = false; followHint = ""; const version = ++generation; error = "";
    try { await receive(await invoke<MissionResearchStatus>("mission_research_track", { enabled: false }), version); }
    catch (reason) { if (alive && version === generation) error = typeof reason === "string" ? reason : "Не удалось остановить обновление карты."; }
    finally { actionBusy = false; }
  }
  async function exportScene(format: "json" | "obj") {
    if (exporting) return;
    exporting = true; error = ""; exportPath = "";
    try { const path = await invoke<string>("mission_research_export", { format }); if (alive) exportPath = path; }
    catch (reason) { if (alive) error = typeof reason === "string" ? reason : "Не удалось сохранить результат."; }
    finally { exporting = false; }
  }
  function loadFilters() {
    try { customFilters = parseMissionFilters(localStorage.getItem(MISSION_FILTER_STORAGE)); filterStorageError = ""; filterLoadFailed = false; }
    catch { filterLoadFailed = true; filterStorageError = "Не удалось загрузить свои фильтры. Сохранённые данные не изменены."; }
    void syncDiscoveryFilters();
  }
  function saveMarkerPreferences(next: MissionMapPreferences) {
    markerPreferences = next;
    markerStorageError = saveMissionMapPreferences(next) ? "" : "Настройки значков действуют в этом окне, но не сохранены.";
  }
  function loadHiddenNames() {
    try { hiddenNames = parseHiddenMissionNames(localStorage.getItem(MISSION_HIDDEN_STORAGE)); hiddenStorageError = ""; hiddenLoadFailed = false; }
    catch { hiddenLoadFailed = true; hiddenStorageError = "Не удалось загрузить скрытые названия. Сохранённый список не изменён."; }
  }
  function saveHiddenNames(next: HiddenMissionName[]) {
    if (hiddenLoadFailed) return false;
    let raw: string;
    try { raw = serializeHiddenMissionNames(next); }
    catch { hiddenStorageError = "Не удалось изменить скрытые названия. Можно сохранить до 500 названий."; return false; }
    hiddenNames = next;
    try { localStorage.setItem(MISSION_HIDDEN_STORAGE, raw); hiddenStorageError = ""; }
    catch { hiddenStorageError = "Скрытые названия действуют в этом окне, но не сохранены. Повторите сохранение."; }
    return true;
  }
  function hideByName(object: MissionObject) {
    if (!hiddenMissionNameKey(object) || objectIsHidden(object, hiddenIndex)) return;
    if (!saveHiddenNames([...hiddenNames, { label: object.label, nameEn: object.nameEn }])) return;
    const index = indexHiddenMissionNames(hiddenNames);
    const removed = new Set(objects.filter(o => objectIsHidden(o, index)).map(o => o.key));
    pickedKeys = pickedKeys.filter(key => !removed.has(key));
    if (removed.has(selectedKey)) selectedKey = "";
    if (removed.has(targetKey)) targetKey = "";
    if (removed.has(followKey)) { followKey = ""; followingMe = false; followHint = ""; }
  }
  function saveFilters(next: CustomMissionFilter[]) {
    filterLoadFailed = false;
    customFilters = next;
    try { localStorage.setItem(MISSION_FILTER_STORAGE, serializeMissionFilters(next)); filterStorageError = ""; }
    catch { filterStorageError = "Фильтры работают в этом окне, но не сохранены. Проверьте доступное место и повторите сохранение."; }
    void syncDiscoveryFilters();
  }
  async function syncDiscoveryFilters() {
    if (!alive || filterSyncBusy || filterLoadFailed) return;
    const keys = missionDiscoveryKeys(customFilters), signature = JSON.stringify(keys);
    if (signature === syncedFilterKeys) return;
    filterSyncBusy = true;
    try {
      await invoke("mission_research_filters", { keys });
      if (alive) { syncedFilterKeys = signature; filterSyncError = ""; }
    } catch {
      if (alive) filterSyncError = "Не удалось ускорить поиск предметов из своих фильтров. Повторим автоматически.";
    } finally { filterSyncBusy = false; }
  }
  function togglePicked(key: string) { pickedKeys = pickedKeys.includes(key) ? pickedKeys.filter(item => item !== key) : [...pickedKeys, key]; }
  function selectObject(object: MissionObject) {
    followKey = ""; followingMe = false; followHint = ""; selectedKey = object.key; camera = { ...camera, x: object.position[0], z: object.position[2], zoom: Math.max(3, camera.zoom) };
    if (sliceEnabled) heightCenter = object.position[1];
  }
  function startFollowing(object: MissionObject) { if (object.kind !== "avatar") return; if (object.positionFresh === false) { selectedKey = object.key; followHint = ""; } else selectObject(object); followKey = object.key; followSignature = followTargetSignature(object); }
  function focusMe() {
    if (!myAvatar) return;
    filters.players = true; query = "";
    const groups = objectFilterEntry(myAvatar, customIndex)?.groups ?? [];
    if (groups.length) saveFilters(customFilters.map(f => groups.includes(f.id) ? { ...f, enabled: true } : f));
    if (status?.tracking) { startFollowing(myAvatar); followingMe = true; } else selectObject(myAvatar);
  }
  function chooseTarget(object: MissionObject) {
    targetKey = object.key; selectedKey = object.key;
    filters[objectFilter(object.kind)] = true; query = "";
    if (object.kind === "cache" && cacheState !== "all" && cacheState !== object.availability) cacheState = "all";
    const groups = objectFilterEntry(object, customIndex)?.groups ?? [];
    if (groups.length) saveFilters(customFilters.map(f => groups.includes(f.id) ? { ...f, enabled: true } : f));
    if (!followingMe) selectObject(object);
  }
  function toggleHeading() {
    rotateWithView = !rotateWithView;
    if (rotateWithView) { focusMe(); if (Number.isFinite(viewHeading)) camera = { ...camera, angle: viewHeading! }; }
    else camera = { ...camera, angle: 0 };
  }
  function resetMap() { rotateWithView = false; followKey = ""; followingMe = false; followHint = ""; if (scene) framingBounds = sceneBounds(scene); camera = fitCamera(framingBounds ?? bounds); }
  function zoom(factor: number) { camera = zoomAt(camera, bounds, canvasWidth, canvasHeight, canvasWidth / 2, canvasHeight / 2, factor); }
  function resizeCanvas(node: HTMLCanvasElement) {
    const update = () => { const rect = node.getBoundingClientRect(); canvasWidth = Math.max(1, rect.width); canvasHeight = Math.max(1, rect.height); };
    const observer = new ResizeObserver(update); observer.observe(node); update(); return { destroy() { observer.disconnect(); } };
  }
  function wheel(event: WheelEvent) {
    event.preventDefault(); const rect = canvas!.getBoundingClientRect();
    camera = zoomAt(camera, bounds, canvasWidth, canvasHeight, event.clientX - rect.left, event.clientY - rect.top, Math.exp(-Math.max(-200, Math.min(200, event.deltaY)) * .003));
  }
  function pointerDown(event: PointerEvent) { if (event.button !== 0) return; canvas?.setPointerCapture(event.pointerId); drag = { x: event.clientX, y: event.clientY, camera: { ...camera }, moved: false, pointer: event.pointerId }; }
  function pointerMove(event: PointerEvent) {
    if (!drag || drag.pointer !== event.pointerId) return;
    const dx = event.clientX - drag.x, dy = event.clientY - drag.y; if (Math.hypot(dx, dy) > 3) { drag.moved = true; followKey = ""; followingMe = false; followHint = ""; }
    const scale = mapScale(bounds, canvasWidth, canvasHeight, drag.camera.zoom); camera = panCamera(drag.camera, dx, dy, scale);
  }
  function pointerUp(event: PointerEvent) {
    if (!drag || drag.pointer !== event.pointerId) return;
    const click = !drag.moved; drag = null; canvas?.releasePointerCapture(event.pointerId);
    if (!click || !canvas) return;
    const rect = canvas.getBoundingClientRect(), x = event.clientX - rect.left, y = event.clientY - rect.top; const scale = mapScale(bounds, canvasWidth, canvasHeight, camera.zoom);
    let nearest: MissionObject | null = null, distance = Infinity;
    for (const raw of visibleObjects) {
      const object = raw.key === myAvatar?.key ? myAvatar : raw;
      const [px, py] = worldToCanvas(object.position, camera, scale, canvasWidth, canvasHeight);
      const d = Math.hypot(px - x, py - y), hitRadius = missionMarkerHitRadius(missionMarkerSize(object, customIndex, markerPreferences));
      if (d < hitRadius && d < distance) { distance = d; nearest = object; }
    }
    if (nearest) { if (event.ctrlKey || event.metaKey) togglePicked(nearest.key); selectedKey = nearest.key; followKey = ""; followingMe = false; followHint = ""; }
  }
  function keyboard(event: KeyboardEvent) {
    const scale = mapScale(bounds, canvasWidth, canvasHeight, camera.zoom);
    if (event.key === "+" || event.key === "=") zoom(1.4); else if (event.key === "-") zoom(1 / 1.4); else if (event.key === "Home") resetMap();
    else if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) {
      camera = panCamera(camera, event.key === "ArrowLeft" ? 60 : event.key === "ArrowRight" ? -60 : 0, event.key === "ArrowUp" ? 60 : event.key === "ArrowDown" ? -60 : 0, scale);
      followKey = ""; followingMe = false;
    } else return;
    event.preventDefault();
  }
  const draw = createMissionMapRenderer();
  onMount(() => {
    loadFilters();
    loadHiddenNames();
    markerPreferences = loadMissionMapPreferences();
    void refresh(); void refreshArchives(); const timer = setInterval(() => void refresh(), 1000); const poseTimer = setInterval(() => void refreshPose(), 50); return () => { alive = false; generation++; clearInterval(timer); clearInterval(poseTimer); }; });
</script>

<section class="mission" aria-label="Карта миссии">
  <section class="source-panel" aria-label="Источник данных миссии">
    <div class="source-options"><label>Источник<select bind:value={source} disabled={status?.busy || status?.tracking || actionBusy}><option value="live">Игра</option><option value="archive">Сохранённая запись</option></select></label>
      {#if source === "archive"}<label class="archive-choice">Запись<select bind:value={archiveId} onchange={chooseArchive} disabled={loadingArchives || status?.busy || actionBusy || !archives.length}><option value="" disabled>Выберите запись</option>{#each archives as archive}<option value={archive.id}>{archive.label} · {missionTime(archive.createdAt)}</option>{/each}</select></label><label>Снимок<select bind:value={sequence} disabled={!currentArchive || status?.busy || actionBusy}>{#each currentArchive?.snapshots ?? [] as snapshot}<option value={snapshot.sequence}>№ {snapshot.sequence}{snapshot.complete ? "" : " · неполный"}</option>{/each}</select></label><button disabled={loadingArchives || status?.busy} onclick={() => void refreshArchives()}>Обновить записи</button>{/if}
    </div>
    <div class="read-actions">{#if status?.tracking}<button disabled={actionBusy} onclick={() => void stopTracking()}>Остановить карту</button>{:else if status?.busy}<button disabled={actionBusy || status.cancelling} onclick={() => void run(true)}>{status.cancelling ? "Отменяем…" : "Отменить чтение"}</button>{:else}<button class="primary" disabled={actionBusy || !status || (source === "live" ? !status.gameRunning : !currentSnapshot)} onclick={() => void run()}>{actionBusy ? "Запускаем…" : source === "live" ? "Запустить карту" : "Разобрать снимок"}</button>{#if source === "live" && status?.autoStart}<button disabled={actionBusy} onclick={() => void run(true)}>Остановить карту</button>{/if}{/if}</div>
    <div class="live-status"><i class:running={status?.tracking}></i><span>{status?.busy ? status.phase || "Читаем данные…" : scene?.source === "live" ? status?.tracking ? "Карта обновляется" : "Карта остановлена" : scene ? "Просмотр записи" : status?.gameRunning ? status?.autoStart ? status?.inOrbiter ? "Орбитер · ожидаем вылета" : "Ожидаем загрузку локации" : "Автозапуск приостановлен" : "Ожидаем Warframe"}</span>{#if scene && !status?.busy}<time>{missionTime(scene.capturedAt)}</time>{/if}</div>
    {#if status?.busy}<div class="progress" role="status"><progress aria-label="Чтение данных миссии"></progress><span>{missionBytes(status.scannedBytes)} · первый поиск может занять время</span></div>{/if}
    {#if source === "archive" && !loadingArchives && !archives.length}<p class="source-note">Сохранённых записей пока нет.</p>{/if}
    {#if archiveError}<p class="error" role="alert">{archiveError}</p>{/if}
  </section>
  {#if error || status?.error}<div class="error" role="alert"><span>{error || status?.error}</span><button onclick={() => { error = ""; loadedRevision = -1; void refresh(); }}>Повторить загрузку</button></div>{/if}
  {#if sceneError}<p class="error" role="status">{sceneError}</p>{/if}
  {#if filterSyncError}<p class="error" role="status">{filterSyncError}</p>{/if}
  <div class="explorer">
    <aside class="filter-rail" bind:this={filterRail} aria-label="Фильтры карты">
      <MissionFilterEditor filters={customFilters} selected={filterSelection} objects={availableObjects} counts={customCounts} onchange={saveFilters} onclear={() => { pickedKeys = []; selectedKey = ""; }} />
      <MissionHiddenNames names={hiddenNames} count={objects.length - availableObjects.length} disabled={hiddenLoadFailed} onrestore={name => saveHiddenNames(hiddenNames.filter(item => item !== name))} onrestoreall={() => saveHiddenNames([])} />
      {#if hiddenStorageError}<p class="error" role="alert">{hiddenStorageError}<button onclick={() => hiddenLoadFailed ? loadHiddenNames() : saveHiddenNames(hiddenNames)}>{hiddenLoadFailed ? "Повторить загрузку" : "Повторить сохранение"}</button></p>{/if}
      {#if filterStorageError}<p class="error" role="alert">{filterStorageError}<button onclick={() => filterLoadFailed ? loadFilters() : saveFilters(customFilters)}>{filterLoadFailed ? "Повторить загрузку" : "Повторить сохранение"}</button></p>{/if}
      <section class="standard-filters" aria-label="Показать на карте"><div class="section-heading"><h2>Показать на карте</h2><button class="text-action" onclick={() => { const enabled = !Object.values(filters).every(Boolean); filters = Object.fromEntries(MISSION_FILTERS.map(filter => [filter.key, enabled])) as Record<MissionFilter, boolean>; }}>{Object.values(filters).every(Boolean) ? "Скрыть все" : "Показать все"}</button></div>
        <div class="filters">{#each MISSION_FILTERS as filter}<label><input type="checkbox" bind:checked={filters[filter.key]} /><span class="filter-icon"><MissionObjectIcon kind={filterIcons[filter.key]} color={filter.color} /></span><span>{filter.key === "npc" ? "Персонажи и враги" : filter.key === "lootspots" ? "Места находок" : filter.label}</span><b>{availableObjects.filter(o => objectFilter(o.kind) === filter.key && !objectFilterEntry(o, customIndex)).length}</b></label>{/each}</div>
      </section>
      <MissionMapSettings bind:this={markerSettings} preferences={markerPreferences} filters={customFilters} {customIndex} {selected} heightKnown={!!myAvatar} onchange={saveMarkerPreferences} />
      {#if markerStorageError}<p class="error" role="alert">{markerStorageError}<button onclick={() => saveMarkerPreferences(markerPreferences)}>Повторить сохранение</button></p>{/if}
      <details class="view-settings"><summary>Вид и высота</summary><div><label>Состояние тайников<select bind:value={cacheState}><option value="all">Все состояния</option><option value="available">Можно открыть</option><option value="unknown">Состояние неизвестно</option><option value="opened">Открытые</option></select></label><label class="check"><input type="checkbox" bind:checked={showZones} />Границы зон</label><label class="check"><input type="checkbox" bind:checked={sliceEnabled} />Ограничить по высоте</label>{#if sliceEnabled}<label>Высота: {heightCenter.toFixed(1)}<input type="range" min={Math.floor(bounds.minY)} max={Math.max(Math.ceil(bounds.maxY), Math.floor(bounds.minY) + 1)} step="0.5" bind:value={heightCenter} /></label><label>Диапазон<select bind:value={heightHalfWidth}><option value={2}>± 2</option><option value={4}>± 4</option><option value={8}>± 8</option><option value={16}>± 16</option></select></label><p>Срез по высоте, а не определённый этаж.</p>{/if}</div></details>
    </aside>
    <section class="map-panel" aria-label="Схема расположения объектов">
      <div class="map-tools">
        <div class="map-title"><strong>Карта миссии</strong><span>{scene ? `${visibleObjects.length} объектов показано` : "Положение предметов и игроков"}</span></div>
        <div class="map-actions">
          <button aria-pressed={markerPreferences.heightArrowsOnly} title="Заменить значки объектов стрелками высоты" onclick={() => saveMarkerPreferences({ ...markerPreferences, heightArrowsOnly: !markerPreferences.heightArrowsOnly })}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 19V5m-4 4 4-4 4 4m6-4v14m-4-4 4 4 4-4" /></svg>Только стрелки</button>
          <button disabled={!myAvatar} onclick={focusMe}><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="6" /><path d="M12 2v4m0 12v4M2 12h4m12 0h4" /></svg>{status?.tracking ? "Следовать за мной" : "Показать меня"}</button>
          <button aria-pressed={rotateWithView} disabled={!rotateWithView && (!myAvatar || !Number.isFinite(viewHeading))} onclick={toggleHeading}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z" /><circle cx="12" cy="12" r="3" /></svg>По взгляду</button>
          <button disabled={!scene} onclick={resetMap}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m3 5 6-2 6 2 6-2v16l-6 2-6-2-6 2ZM9 3v16M15 5v16" /></svg>Вся карта</button>
        </div>
      </div>
      {#if markerPreferences.heightArrowsOnly}<div class="marker-mode-note">↑ выше вас · ↓ ниже вас · • на вашей высоте · ? высота неизвестна</div>{/if}
      {#if scene}
        <div class="canvas-wrap"><canvas bind:this={canvas} use:resizeCanvas tabindex="0" aria-label="Карта объектов. Стрелки перемещают вид, плюс и минус меняют масштаб. Ctrl и щелчок выделяют несколько точек." onwheel={wheel} onpointerdown={pointerDown} onpointermove={pointerMove} onpointerup={pointerUp} onpointercancel={() => drag = null} onkeydown={keyboard}></canvas><div class="zoom-controls"><button aria-label="Увеличить карту" onclick={() => zoom(1.4)}>+</button><button aria-label="Уменьшить карту" onclick={() => zoom(1 / 1.4)}>−</button></div>
          {#if !scene.meshes.length}<p class="map-overlay-note">Показаны точки. Геометрия карты пока не найдена.</p>{/if}
          {#if followKey}<div class="follow-status" role="status"><span>{followHint || (followingMe ? "Следуем за вами" : "Следуем за игроком")}</span><button aria-label="Прекратить следование" onclick={() => { followKey = ""; followingMe = false; followHint = ""; }}>×</button></div>{:else if followHint}<p class="map-overlay-note">{followHint}</p>{/if}
        </div>
        <div class="target-bar">{#if target}<div><small>Текущая цель</small><strong>{target.label}</strong><span>{distanceLabel(myAvatar, target)}{target.kind === "cache" && target.availability === "opened" ? " · уже открыт" : ""}</span></div><button onclick={() => selectObject(target!)}>Показать на карте</button><button aria-label="Убрать цель" onclick={() => targetKey = ""}>×</button>{:else if targetKey}<span>Цель больше не видна.</span><button onclick={() => targetKey = ""}>Убрать цель</button>{:else}<span>Быстрый выбор цели</span><button disabled={!nearestCache} onclick={() => nearestCache && chooseTarget(nearestCache)}>Ближайший закрытый тайник →</button>{/if}</div>
        <div class="map-footer"><span>Колесо — масштаб · Ctrl + щелчок — несколько точек</span><span>{myAvatar ? "Ваш персонаж найден" : "Позиция игрока не определена"}</span></div>
        {#if rotateWithView && !Number.isFinite(viewHeading)}<p class="map-help">Направление взгляда временно недоступно. Поворот сохранён.</p>{/if}
      {:else}<div class="initial"><div class="empty-map-icon">⌖</div><h2>{!status?.gameRunning ? "Warframe не запущен" : status?.busy ? "Строим карту миссии" : status?.autoStart ? status?.inOrbiter ? "В Орбитере карта не запускается" : "Карта запустится автоматически" : "Карта приостановлена"}</h2></div>{/if}
    </section>
    <section class="object-panel" aria-label="Найденные объекты">
      <div class="list-heading"><h2>Найти на карте</h2><span>{visibleObjects.length}</span></div>
      <label class="search"><span class="sr-only">Поиск объекта</span><input type="search" bind:value={query} placeholder="Название на русском или английском" /></label>
      <div class="list-options"><label class="check"><input type="checkbox" bind:checked={groupObjects} />По типам</label><label class="check"><input type="checkbox" bind:checked={sortByDistance} disabled={!myAvatar && !sortByDistance} />Ближайшие</label></div>
      {#if query && visibleObjects.length}<button class="select-results" onclick={() => { pickedKeys = listEntries.map(entry => entry.object.key); }}>Выбрать найденное для фильтра</button>{/if}
      {#if selected}
        <section class="selected-card" class:is-target={selected.key === targetKey} aria-label="Выбранный объект">
          <div class="selected-heading"><span>{selected.key === targetKey ? "Выбранная цель" : "Выбранный объект"}</span><button class="text-action" aria-label="Закрыть сведения о предмете" onclick={() => selectedKey = ""}>×</button></div>
          <div class="selected-title"><span class="object-icon"><MissionObjectIcon kind={selected.kind} color={filterColor(selected, customIndex)} size={24} /></span><div><h3>{selected.label || objectKindLabel(selected.kind)}</h3>{#if selected.nameEn && selected.nameEn !== selected.label}<p class="english-name">{selected.nameEn}</p>{/if}</div></div>
          <p class="selected-distance">{distanceLabel(myAvatar, selected)}</p>
          {#if selected.kind === "cache"}<p class="object-state" class:available={selected.availability === "available"}>{selected.availability === "available" ? "Можно открыть" : selected.availability === "opened" ? "Тайник открыт" : "Состояние неизвестно"}</p>{/if}
          <div class="selected-actions">
            {#if selected.kind !== "avatar" && selected.key !== targetKey}<button class="primary" onclick={() => chooseTarget(selected!)}>Выбрать целью</button>{/if}
            <button onclick={() => selectObject(selected!)}>Показать на карте</button>
            <button onclick={() => void markerSettings?.configureSelected()}>Размер значка</button>

            {#if selected.kind !== "avatar"}<button disabled={hiddenLoadFailed || !hiddenMissionNameKey(selected)} title={`Скрыть все экземпляры: ${hiddenMissionNameLabel(selected)}`} onclick={() => hideByName(selected!)}>Скрыть по названию</button>{/if}
            {#if scene?.source === "live" && selected.kind === "avatar" && selected.key !== myAvatar?.key}<button disabled={!status?.tracking} onclick={() => startFollowing(selected!)}>Следовать</button>{/if}
          </div>
          <details class="object-data"><summary>Подробнее об объекте</summary><p>{selected.availability === "available" ? "Можно открыть: действие подтверждено." : selected.availability === "opened" ? "Тайник открыт." : selected.kind === "lootspot" ? "Возможное место появления. Наличие предмета не подтверждено." : "Доступность взаимодействия не подтверждена."}</p>{#if selected.positionFresh === false}<p>Показаны последние известные координаты.</p>{/if}<dl><div><dt>Координаты</dt><dd>X {selected.position[0].toFixed(2)} · Y {selected.position[1].toFixed(2)} · Z {selected.position[2].toFixed(2)}</dd></div>{#each selected.details as detail}<div><dt>{detail.label}</dt><dd>{detail.value}</dd></div>{/each}</dl><p>{selected.typeNames.join(" → ")}</p>{#if selected.itemPath}<code>{selected.itemPath}</code>{/if}</details>
        </section>
      {/if}
      <div class="object-scroll">
        {#if !scene}<p class="empty">Карта ещё не запущена.</p>
        {:else if !objects.length}<p class="empty">Объекты не найдены.</p>
        {:else if !visibleObjects.length}<div class="empty"><strong>Ничего не найдено</strong>{#if query}<button onclick={() => query = ""}>Очистить поиск</button>{/if}</div>
        {:else}
          <ul class="object-list">
            {#each listEntries.slice(0, listLimit) as entry, objectIndex (entry.key)}
              {@const object = entry.object}
              <li>
                <input type="checkbox" aria-label={`Выбрать для фильтра: ${object.label} · строка ${objectIndex + 1}`} checked={pickedKeys.includes(object.key)} onchange={() => togglePicked(object.key)} />
                <button class:selected={object.key === selectedKey} class:is-target={object.key === targetKey} onclick={() => selectObject(object)}>
                  <span class="object-icon"><MissionObjectIcon kind={object.kind} color={filterColor(object, customIndex)} /></span>
                  <span class="object-copy"><strong>{object.key === myAvatar?.key ? "Вы" : object.label || objectKindLabel(object.kind)}</strong>{#if object.nameEn && object.nameEn !== object.label}<small>{object.nameEn}</small>{/if}<small>{object.kind === "cache" ? object.availability === "available" ? "Можно открыть · " : object.availability === "opened" ? "Открыт · " : "" : ""}{myAvatar && object.key !== myAvatar.key ? distanceLabel(myAvatar, object) : objectKindLabel(object.kind)}{object.positionFresh === false ? " · прежняя позиция" : ""}</small></span>
                  {#if entry.count > 1}<b class="copy-count" title="Экземпляров этого типа на карте">×{entry.count}</b>{/if}
                </button>
                {#if object.kind !== "avatar"}<button class="hide-name" disabled={hiddenLoadFailed || !hiddenMissionNameKey(object)} aria-label={`Скрыть по названию: ${hiddenMissionNameLabel(object)}`} title={`Скрыть все экземпляры: ${hiddenMissionNameLabel(object)}`} onclick={() => hideByName(object)}>Скрыть</button>{/if}
              </li>
            {/each}
          </ul>
          {#if listEntries.length > listLimit}<button class="more" onclick={() => listLimit += 100}>Показать ещё</button>{/if}
        {/if}
      </div>
    </section>
  </div>
  <div class="research-tools">
    {#if scene}<details class="warnings"><summary>{scene.complete ? "О точности карты" : "Карта прочитана частично"}</summary><p>Геометрия может покрывать только часть миссии. Пунктир — границы зон, а не стены. Расстояния указаны по прямой; маршруты не построены.</p>{#if scene.zones?.length && !scene.zonesFresh}<p>Свежие границы зон не подтверждены. Сохранена последняя согласованная карта.</p>{/if}{#if scene.warnings.length}<ul>{#each scene.warnings as warning}<li>{warning}</li>{/each}</ul>{/if}</details>
        {#if difference && previousScene}<details class="comparison"><summary>Изменения относительно {missionTime(previousScene.capturedAt)} · Новых {difference.added.length} · Больше не видны {difference.absent.length}</summary><div class="comparison-lists"><div><h3>Появились в выборке</h3>{#if !difference.added.length}<p>Нет новых записей.</p>{:else}<ul>{#each difference.added.slice(0, 30) as object}<li>{object.label || objectKindLabel(object.kind)}</li>{/each}</ul>{#if difference.added.length > 30}<p>И ещё {difference.added.length - 30}.</p>{/if}{/if}</div><div><h3>Больше не видны в выборке</h3>{#if !difference.absent.length}<p>Все прежние записи остаются.</p>{:else}<ul>{#each difference.absent.slice(0, 30) as object}<li>{object.label || objectKindLabel(object.kind)}</li>{/each}</ul>{#if difference.absent.length > 30}<p>И ещё {difference.absent.length - 30}.</p>{/if}{/if}</div></div></details>{/if}
    <details class="export"><summary>Сохранить результат для анализа</summary><div class="export-buttons"><button disabled={exporting || status?.busy} onclick={() => void exportScene("json")}>Сохранить JSON</button><button disabled={exporting || status?.busy || !scene.meshes.length} onclick={() => void exportScene("obj")}>Сохранить OBJ</button></div>{#if exportPath}<p role="status">Результат сохранён: <code>{exportPath}</code></p>{/if}<p class="muted">Прочитано {missionBytes(scene.stats.scannedBytes)} · Геометрических частей: {scene.stats.meshCount} · Полигонов: {scene.stats.faceCount.toLocaleString("ru-RU")}</p></details>

    {/if}
    <details class="recording-disclosure" ontoggle={(event) => { if (!(event.currentTarget as HTMLDetailsElement).open) void refreshArchives(); }}><summary>Записать миссию для исследования</summary><div><MemoryRecording /></div></details>
  </div>
</section>

<style>
  .mission { color: var(--text); min-width: 0; font-size: .8rem; container: mission / inline-size; }
  h2, h3, p { margin: 0; }
  h2 { font-size: .9rem; }
  h3 { font-size: .88rem; }
  p { font-size: .78rem; color: var(--text-muted); line-height: 1.5; }
  button, select, input { font: inherit; }
  button { min-height: 32px; border: 1px solid var(--border); border-radius: 7px; background: var(--surface-1); color: var(--text); padding: .45rem .65rem; cursor: pointer; font-size: .76rem; }
  button:hover { background: var(--surface-3); border-color: var(--border-strong); }
  button:disabled { opacity: .45; cursor: default; }
  .primary { background: var(--accent); color: var(--surface-1); border-color: var(--accent); }
  .primary:hover { background: var(--accent-strong); }
  button:focus-visible, input:focus-visible, select:focus-visible, summary:focus-visible, canvas:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  label { font-size: .74rem; color: var(--text-muted); }
  select, input[type="search"] { background: var(--surface-1); color: var(--text); border: 1px solid var(--border); border-radius: 6px; padding: .5rem; min-width: 0; }
  input[type="checkbox"] { accent-color: var(--accent); width: 15px; height: 15px; margin: 0; flex-shrink: 0; }
  summary { cursor: pointer; line-height: 1.5; }

  .source-panel { display: flex; align-items: center; gap: .75rem; padding: .6rem .75rem; margin-bottom: .75rem; background: var(--surface-1); border: 1px solid var(--border); border-radius: 10px; flex-wrap: wrap; }
  .source-options, .read-actions { display: flex; align-items: center; gap: .5rem; min-width: 0; flex-wrap: wrap; }
  .source-options label { display: flex; align-items: center; gap: .5rem; min-width: 0; }
  .source-options select { max-width: 260px; padding: .4rem .5rem; }
  .live-status { display: flex; align-items: center; gap: .5rem; margin-left: auto; font-size: .75rem; color: var(--text-muted); min-width: 0; }
  .live-status i { width: 8px; height: 8px; border-radius: 50%; flex: none; background: #a8a096; }
  .live-status i.running { background: #479977; box-shadow: 0 0 0 3px #47997717; }
  .live-status span { overflow-wrap: anywhere; }
  .live-status time { font-size: .67rem; margin-left: .4rem; white-space: nowrap; }
  .progress { flex-basis: 100%; display: flex; align-items: center; gap: .65rem; font-size: .74rem; color: var(--text-muted); }
  progress { height: 5px; width: 150px; accent-color: var(--accent); flex-shrink: 0; }
  .source-note { flex-basis: 100%; }
  .error { background: var(--danger-soft); color: var(--danger); border: 1px solid #a94d4d40; border-radius: 7px; padding: .65rem; margin-bottom: .65rem; font-size: .76rem; display: flex; gap: .5rem; align-items: center; justify-content: space-between; overflow-wrap: anywhere; }

  .explorer { display: grid; grid-template-columns: 220px minmax(0, 1fr) 272px; gap: .75rem; height: calc(100dvh - 215px); min-height: 600px; }
  .filter-rail, .object-panel { background: var(--surface-1); border: 1px solid var(--border); border-radius: 10px; min-width: 0; min-height: 0; }
  .filter-rail { padding: .85rem; overflow: auto; scrollbar-width: thin; }
  .standard-filters { border-top: 1px solid var(--border); padding-top: .9rem; margin-top: .9rem; }
  .section-heading { display: flex; align-items: center; justify-content: space-between; gap: .3rem; }
  .section-heading h2 { font-size: .78rem; }
  .text-action { background: transparent; border: 0; font-size: .67rem; padding: .15rem; min-height: 25px; color: var(--text-muted); }
  .filters { display: flex; flex-direction: column; gap: .1rem; margin-top: .6rem; }
  .filters label { display: flex; align-items: center; gap: .45rem; padding: .4rem 0; cursor: pointer; color: var(--text); }
  .filters label > span:not(.filter-icon) { flex: 1; min-width: 0; line-height: 1.4; }
  .filters b { color: var(--text-muted); font-size: .69rem; font-weight: 500; }
  .filter-icon, .object-icon { display: grid; place-items: center; flex: none; width: 27px; height: 27px; border-radius: 7px; background: #1b2b33; border: 1px solid #344b53; }
  .view-settings { border-top: 1px solid var(--border); padding-top: .85rem; margin-top: .9rem; font-size: .78rem; }
  .view-settings > div { display: grid; gap: .7rem; margin-top: .75rem; }
  .view-settings label { display: grid; gap: .35rem; }
  .check, .view-settings .check { display: flex; align-items: center; gap: .4rem; }
  .view-settings p { font-size: .68rem; }
  .view-settings select { width: 100%; }

  .map-panel { display: flex; flex-direction: column; background: #101b24; border: 1px solid #31454c; border-radius: 12px; overflow: hidden; min-width: 0; min-height: 0; color: #e5eee9; box-shadow: 0 3px 12px #14202615; }
  .map-tools { display: flex; align-items: center; justify-content: space-between; gap: .65rem; flex-wrap: wrap; background: #182831; padding: .65rem .8rem; border-bottom: 1px solid #31454c; }
  .map-title { display: grid; gap: .2rem; min-width: 0; }
  .map-title strong { font-size: .85rem; }
  .map-title span { font-size: .65rem; color: #a5bcbe; }
  .map-actions { display: flex; gap: .3rem; flex-wrap: wrap; }
  .map-panel button { color: #e6efed; background: #223640; border-color: #3b535d; font-size: .72rem; white-space: nowrap; }
  .map-panel button:hover { background: #2d4651; border-color: #5a7881; }
  .map-panel button[aria-pressed="true"] { background: #30584f; border-color: #71b49b; }
  .map-actions button { display: inline-flex; align-items: center; justify-content: center; gap: .35rem; padding: .4rem .55rem; }
  .map-actions svg { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.5; stroke-linejoin: round; flex: none; }
  .marker-mode-note { padding: .4rem .8rem; background: #16252e; border-bottom: 1px solid #31454c; color: #b7cfcb; font-size: .68rem; line-height: 1.5; overflow-wrap: anywhere; }
  .canvas-wrap { flex: 1; position: relative; min-height: 0; overflow: hidden; }
  canvas { position: absolute; inset: 0; display: block; width: 100%; height: 100%; touch-action: none; cursor: grab; }
  canvas:active { cursor: grabbing; }
  .zoom-controls { position: absolute; right: 14px; bottom: 16px; display: flex; flex-direction: column; gap: 5px; }
  .zoom-controls button { width: 33px; height: 33px; font-size: 1.1rem; padding: 0; background: #1a2c36ed; border-color: #52707a; box-shadow: 0 2px 6px #080e1433; }
  .map-footer { display: flex; justify-content: space-between; gap: .5rem; flex-wrap: wrap; padding: .5rem .8rem; border-top: 1px solid #263e48; color: #9fb9bd; font-size: .64rem; }
  .target-bar { padding: .6rem .8rem; display: flex; align-items: center; gap: .5rem; border-top: 1px solid #31454c; background: #162832; min-height: 52px; font-size: .7rem; }
  .target-bar > span, .target-bar > div { flex: 1; min-width: 0; }
  .target-bar > div { display: grid; gap: .18rem; }
  .target-bar strong { font-size: .78rem; color: #edc581; overflow-wrap: anywhere; }
  .target-bar small { color: #bdac89; font-size: .61rem; }
  .target-bar span { color: #b1c5c4; }
  .target-bar button { white-space: normal; }
  .map-overlay-note { position: absolute; left: 12px; top: 12px; max-width: min(75%, 420px); background: #1b303be6; border: 1px solid #3c5862; padding: .55rem .7rem; border-radius: 7px; color: #d1ded8; font-size: .72rem; }
  .follow-status { position: absolute; left: 12px; top: 12px; background: #172c36ed; border: 1px solid #517f72; border-radius: 7px; display: flex; align-items: center; gap: .6rem; padding: .25rem .4rem .25rem .65rem; font-size: .7rem; max-width: 85%; }
  .follow-status button { border: 0; background: transparent; min-height: 24px; padding: 0 .3rem; }
  .map-help { padding: .5rem .75rem; color: #d8c49a; font-size: .7rem; }
  .initial { flex: 1; display: flex; align-items: center; justify-content: center; flex-direction: column; text-align: center; padding: 2rem; }
  .initial h2 { font-size: 1.2rem; color: #e2eae6; margin: .8rem 0; }
  .empty-map-icon { font-size: 3.2rem; color: #80baa4; }

  .object-panel { padding: .85rem; display: flex; flex-direction: column; gap: .65rem; overflow: hidden; }
  .list-heading { display: flex; align-items: center; justify-content: space-between; }
  .list-heading > span { font-size: .7rem; color: var(--text-muted); background: var(--surface-3); border-radius: 12px; padding: .15rem .5rem; }
  .search input { width: 100%; font-size: .75rem; padding: .65rem; }
  .list-options { display: flex; gap: .9rem; flex-wrap: wrap; }
  .list-options label { font-size: .71rem; cursor: pointer; }
  .object-scroll { flex: 1; min-height: 120px; overflow: auto; scrollbar-width: thin; }
  .object-list { list-style: none; margin: 0; padding: 0; }
  .object-list li { display: flex; align-items: flex-start; gap: .25rem; border-bottom: 1px solid var(--border); padding: .15rem 0; }
  .object-list li > input { margin-top: .85rem; cursor: pointer; }
  .object-list button:not(.hide-name) { display: flex; align-items: start; gap: .5rem; text-align: left; flex: 1; min-width: 0; background: transparent; border: 1px solid transparent; padding: .6rem .2rem; line-height: 1.4; }
  .object-list button:hover { background: var(--surface-2); }
  .object-list button.selected { border-color: var(--border-strong); background: var(--surface-2); }
  .object-list button.is-target { border-color: #b98b4770; background: #c59e5710; }
  .object-copy { min-width: 0; flex: 1; }
  .object-list .object-icon { margin-top: .1rem; }
  .object-list strong { display: block; font-size: .76rem; font-weight: 600; overflow-wrap: anywhere; }
  .object-list small { display: block; font-size: .65rem; color: var(--text-muted); overflow-wrap: anywhere; margin-top: .2rem; font-weight: 400; }
  .copy-count { font-size: .68rem; color: var(--text-muted); white-space: nowrap; }
  .object-list button.hide-name { flex: 0 0 auto; margin-top: .55rem; padding: .3rem .15rem; font-size: .66rem; min-height: 28px; color: var(--text-muted); border-color: transparent; background: transparent; }
  .empty { padding: 1rem .3rem; font-size: .78rem; color: var(--text-muted); }
  .more, .select-results { width: 100%; font-size: .7rem; }
  .selected-card { background: var(--surface-2); border: 1px solid var(--border); border-radius: 9px; padding: .75rem; max-height: 48%; overflow: auto; flex-shrink: 0; scrollbar-width: thin; }
  .selected-card.is-target { border-color: #b98b4770; }
  .selected-heading { display: flex; align-items: center; justify-content: space-between; color: var(--text-muted); font-size: .65rem; margin-bottom: .4rem; }
  .selected-title { display: flex; align-items: start; gap: .6rem; }
  .selected-title > div { min-width: 0; }
  .selected-title .object-icon { width: 35px; height: 35px; }
  .selected-card h3 { font-size: .85rem; line-height: 1.4; overflow-wrap: anywhere; }
  .selected-card p { font-size: .69rem; margin-top: .3rem; }
  .selected-card .english-name { font-size: .67rem; overflow-wrap: anywhere; }
  .selected-card .selected-distance { margin-top: .65rem; }
  .selected-card .object-state { color: var(--text-muted); font-weight: 600; }
  .selected-card .object-state.available { color: var(--success); }
  .selected-actions { display: flex; gap: .35rem; flex-wrap: wrap; margin-top: .65rem; }
  .selected-actions button { font-size: .69rem; padding: .4rem .5rem; }
  .object-data { border-top: 1px solid var(--border); margin-top: .6rem; padding-top: .5rem; font-size: .69rem; }
  .object-data p, dd, code { overflow-wrap: anywhere; }
  dl { display: grid; gap: .6rem; font-size: .68rem; }
  dt { color: var(--text-muted); }
  dd { margin: .15rem 0 0; }
  code { font-size: .66rem; white-space: pre-wrap; }
  .sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0, 0, 0, 0); }

  .research-tools { display: flex; align-items: start; flex-wrap: wrap; gap: .5rem 1.4rem; margin-top: .85rem; }
  .research-tools > details { font-size: .72rem; color: var(--text-muted); min-width: 0; }
  .research-tools > details[open] { flex-basis: 100%; background: var(--surface-1); border: 1px solid var(--border); padding: .8rem; border-radius: 7px; }
  .research-tools p { margin: .5rem 0; }
  .research-tools ul { padding-left: 1.2rem; line-height: 1.5; }
  .export-buttons { display: flex; gap: .5rem; margin: .7rem 0; }
  .comparison-lists { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }
  .recording-disclosure > div { margin-top: .75rem; }

  @container mission (max-width: 1080px) {
    .explorer { grid-template-columns: 200px minmax(0, 1fr); height: auto; min-height: 0; }
    .map-panel { min-height: 620px; height: calc(100dvh - 215px); }
    .filter-rail { max-height: max(620px, calc(100dvh - 215px)); }
    .object-panel { grid-column: 1 / -1; min-height: 300px; max-height: 460px; }
    .object-scroll { min-height: 0; }
    .selected-card { max-height: 250px; }
  }
  @container mission (max-width: 620px) {
    .explorer { display: flex; flex-direction: column; }
    .map-panel { order: 0; height: 70dvh; min-height: 460px; }
    .filter-rail { order: 1; max-height: none; }
    .object-panel { order: 2; max-height: 620px; }
    .map-title { flex: 1 0 100%; }
    .map-actions { width: 100%; }
    .map-actions button { flex: 1; min-width: 0; white-space: normal; }
    .map-actions svg { display: none; }
    .target-bar { flex-wrap: wrap; }
    .target-bar > div { flex-basis: calc(100% - 90px); }
    .live-status { margin-left: 0; flex-wrap: wrap; }
    .live-status time { margin-left: 0; }
    .source-options { width: 100%; }
    .source-options label { flex-wrap: wrap; }
    .source-options select { max-width: 100%; }
    .progress { flex-wrap: wrap; }
    .comparison-lists { grid-template-columns: 1fr; }
  }
</style>
