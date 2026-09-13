<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { applySceneUpdate, type MissionSceneUpdate } from "./missionSceneUpdates";
  import MemoryRecording from "./MemoryRecording.svelte";
  import MissionFilterEditor from "./MissionFilterEditor.svelte";
  import { MISSION_FILTER_STORAGE, parseMissionFilters, serializeMissionFilters, missionDiscoveryKeys, groupMissionObjects, objectRule, objectFilterEntry, indexMissionFilters, countMissionFilters, filterColor, objectIsVisible, type MissionFilterIndex, type CustomMissionFilter } from "./missionFilters";
  import { MISSION_FILTERS, localAvatar, zoneInHeightSlice, objectMatchesSearch, objectFilter, objectKindLabel, finitePosition, sceneBounds, fitCamera, followFrame, followTargetSignature, mapScale, worldToCanvas, zoomAt, panCamera, objectDistance, distanceLabel, scaledHeight, inHeightSlice, faceInHeightSlice, compareScenes, missionTime, missionBytes,
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
  let filters: Record<MissionFilter, boolean> = { feather: true, pickup: true, players: true, npc: false, other: false, goals: true, lootspots: false, caches: true };
  let customFilters: CustomMissionFilter[] = [];
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
  let showZones = true;
  let heightCenter = 0;
  let heightHalfWidth = 4;
  let camera: MapCamera = { x: 0, z: 0, zoom: 1 };
  let canvas: HTMLCanvasElement | undefined;
  let canvasWidth = 800;
  let canvasHeight = 480;
  let drag: { x: number; y: number; camera: MapCamera; moved: boolean; pointer: number } | null = null;

  $: currentArchive = archives.find(a => a.id === archiveId);
  $: currentSnapshot = currentArchive?.snapshots.find(s => s.sequence === sequence);
  $: bounds = framingBounds ?? (scene ? sceneBounds(scene) : sceneBounds({ objects: [], meshes: [] }));
  $: objects = scene?.objects.filter(o => finitePosition(o.position)) ?? [];
  $: customIndex = indexMissionFilters(customFilters);
  $: customCounts = countMissionFilters(objects, customIndex);
  $: visibleObjects = objects.map(o => o.key === myAvatar?.key ? myAvatar : o).filter(o => objectIsVisible(o, filters, customIndex) && inHeightSlice(o.position[1], sliceEnabled, heightCenter, heightHalfWidth) && objectMatchesSearch(o, query) && (o.kind !== "cache" || cacheState === "all" || o.availability === cacheState)).sort((a, b) => (sortByDistance ? (objectDistance(myAvatar, a) ?? Infinity) - (objectDistance(myAvatar, b) ?? Infinity) : 0) || (a.kind === "feather" ? 0 : 1) - (b.kind === "feather" ? 0 : 1) || a.label.localeCompare(b.label, "ru"));
  $: listEntries = groupMissionObjects(visibleObjects, groupObjects);
  $: sceneAvatar = scene ? localAvatar(scene) : null;
  $: myAvatar = poseReceived && status?.tracking ? (sceneAvatar && fastPose?.avatarKey === sceneAvatar.key && fastPose.position ? { ...sceneAvatar, position: fastPose.position, positionFresh: true } : null) : sceneAvatar;
  $: viewHeading = poseReceived && status?.tracking ? fastPose?.cameraHeading : scene?.cameraHeading;
  $: target = objects.find(o => o.key === targetKey) ?? null;
  $: nearestCache = objects.filter(o => o.kind === "cache" && o.availability === "available" && objectDistance(myAvatar, o) !== null).sort((a, b) => objectDistance(myAvatar, a)! - objectDistance(myAvatar, b)!)[0] ?? null;
  $: selected = selectedKey === myAvatar?.key ? myAvatar : objects.find(o => o.key === selectedKey) ?? null;
  $: pickedObjects = objects.filter(object => pickedKeys.includes(object.key));
  $: filterSelection = pickedKeys.length ? pickedObjects : selected ? [selected] : [];
  $: filterSelectionKey = filterSelection.map(object => object.key).join("|");
  $: if (filterRail && filterSelectionKey !== lastFilterSelection) { filterRail.scrollTop = 0; lastFilterSelection = filterSelectionKey; }
  $: difference = scene && previousScene ? compareScenes(previousScene, scene) : null;
  $: drawState = { scene, liveAvatar: myAvatar, viewHeading, bounds, camera, objects: visibleObjects, selectedKey, targetKey, rotateWithView, pickedKeys, customIndex, showZones, sliceEnabled, heightCenter, heightHalfWidth, width: canvasWidth, height: canvasHeight };
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
    let nearest: MissionObject | null = null, distance = 14;
    for (const raw of visibleObjects) { const object = raw.key === myAvatar?.key ? myAvatar : raw; const [px, py] = worldToCanvas(object.position, camera, scale, canvasWidth, canvasHeight); const d = Math.hypot(px - x, py - y); if (d < distance) { distance = d; nearest = object; } }
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
  interface DrawState { liveAvatar: MissionObject | null; viewHeading: number | null | undefined; scene: MissionScene | null; bounds: MapBounds; camera: MapCamera; objects: MissionObject[]; selectedKey: string; targetKey: string; rotateWithView: boolean; pickedKeys: string[]; customIndex: MissionFilterIndex; showZones: boolean; sliceEnabled: boolean; heightCenter: number; heightHalfWidth: number; width: number; height: number }
  let background: HTMLCanvasElement | undefined;
  let backgroundKey = "";
  let backgroundMeshes: MissionScene["meshes"] | null = null;
  function draw(node: HTMLCanvasElement, state: DrawState) {
    const ctx = node.getContext("2d"); if (!ctx) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2), { width, height } = state;
    if (node.width !== Math.round(width * dpr)) node.width = Math.round(width * dpr); if (node.height !== Math.round(height * dpr)) node.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0); ctx.clearRect(0, 0, width, height); ctx.fillStyle = "#111a23"; ctx.fillRect(0, 0, width, height);
    if (!state.scene) return;
    const scale = mapScale(state.bounds, width, height, state.camera.zoom);
    const key = JSON.stringify([width, height, dpr, state.camera, state.bounds, state.showZones, state.sliceEnabled, state.heightCenter, state.heightHalfWidth, state.scene.zones, state.scene.zonesFresh]);
    if (!background || backgroundKey !== key || backgroundMeshes !== state.scene.meshes) {
      background ??= document.createElement("canvas"); background.width = node.width; background.height = node.height;
      const bg = background.getContext("2d"); if (!bg) return;
      bg.setTransform(dpr, 0, 0, dpr, 0, 0); bg.fillStyle = "#111a23"; bg.fillRect(0, 0, width, height);
      if (state.showZones) for (const zone of state.scene.zones ?? []) {
        if (!zoneInHeightSlice(zone, state.sliceEnabled, state.heightCenter, state.heightHalfWidth)) continue;
        const corners = [[zone.min[0], zone.min[1], zone.min[2]], [zone.max[0], zone.min[1], zone.min[2]], [zone.max[0], zone.min[1], zone.max[2]], [zone.min[0], zone.min[1], zone.max[2]]] as [number, number, number][];
        bg.beginPath(); corners.map(p => worldToCanvas(p, state.camera, scale, width, height)).forEach((p, i) => i ? bg.lineTo(...p) : bg.moveTo(...p)); bg.closePath();
        bg.strokeStyle = state.scene.zonesFresh === false ? "#71818d66" : "#9fb7c780"; bg.lineWidth = 1; bg.setLineDash([5, 5]); bg.stroke(); bg.setLineDash([]);
      }
      for (const mesh of state.scene.meshes) for (const face of mesh.faces) {
        if (!faceInHeightSlice(mesh.vertices, face, state.sliceEnabled, state.heightCenter, state.heightHalfWidth)) continue;
        const points = face.map(i => worldToCanvas(mesh.vertices[i], state.camera, scale, width, height));
        if (points.every(p => p[0] < 0) || points.every(p => p[0] > width) || points.every(p => p[1] < 0) || points.every(p => p[1] > height)) continue;
        const meanY = face.reduce((sum, i) => sum + mesh.vertices[i][1], 0) / face.length; const light = 23 + scaledHeight(meanY, state.bounds) * 18;
        bg.beginPath(); points.forEach((p, i) => i ? bg.lineTo(...p) : bg.moveTo(...p)); bg.closePath(); bg.fillStyle = `hsl(192 19% ${light}%)`; bg.fill(); bg.strokeStyle = "#55737966"; bg.lineWidth = .6; bg.stroke();
      }
      backgroundKey = key; backgroundMeshes = state.scene.meshes;
    }
    ctx.drawImage(background, 0, 0, width, height);
    const mine = state.liveAvatar;
    for (const raw of state.objects) {
      const object = raw.key === mine?.key ? mine : raw;
      const [x, y] = worldToCanvas(object.position, state.camera, scale, width, height); if (x < -15 || y < -15 || x > width + 15 || y > height + 15) continue;
      const chosen = object.key === state.targetKey || object.key === state.selectedKey || state.pickedKeys.includes(object.key); ctx.globalAlpha = object.kind === "cache" && object.availability === "opened" ? .35 : 1; ctx.beginPath(); ctx.fillStyle = filterColor(object, state.customIndex);
      if (object.key === mine?.key) { const heading = Number.isFinite(state.viewHeading) ? state.viewHeading! : state.camera.angle ?? 0;
        const angle = heading - (state.camera.angle ?? 0), c = Math.cos(angle), s = Math.sin(angle);
        [[0, -9], [7, 7], [0, 3], [-7, 7]].forEach(([dx, dy], i) => { const px = x + c * dx - s * dy, py = y + s * dx + c * dy; if (i) ctx.lineTo(px, py); else ctx.moveTo(px, py); }); ctx.closePath(); ctx.fillStyle = "#b5eaff"; }
      else if (object.kind === "extraction") { ctx.rect(x - 6, y - 6, 12, 12); }
      else if (object.kind === "feather") { ctx.moveTo(x, y - 7); ctx.lineTo(x + 5, y); ctx.lineTo(x, y + 7); ctx.lineTo(x - 5, y); ctx.closePath(); } else ctx.arc(x, y, chosen ? 5 : 3.5, 0, Math.PI * 2);
      ctx.fill(); ctx.strokeStyle = "#0c1119"; ctx.lineWidth = 1.5; ctx.stroke();
      ctx.globalAlpha = 1;
      if (chosen) { ctx.beginPath(); ctx.arc(x, y, 11, 0, Math.PI * 2); ctx.strokeStyle = object.key === state.targetKey ? "#ffd66b" : "#fff"; ctx.lineWidth = 2; ctx.stroke(); }
    }
    ctx.fillStyle = "#a8becb"; ctx.font = "12px system-ui"; ctx.fillText(state.rotateWithView ? "Вид сверху · направление взгляда вверх" : "Вид сверху · X / Z", 14, height - 16);
  }
  onMount(() => {
    loadFilters();
    void refresh(); void refreshArchives(); const timer = setInterval(() => void refresh(), 1000); const poseTimer = setInterval(() => void refreshPose(), 50); return () => { alive = false; generation++; clearInterval(timer); clearInterval(poseTimer); }; });
</script>

<section class="mission" aria-label="Карта миссии">
  <section class="source-panel" aria-label="Источник данных миссии">
    <div class="source-options"><label>Источник<select bind:value={source} disabled={status?.busy || status?.tracking || actionBusy}><option value="live">Игра</option><option value="archive">Сохранённая запись</option></select></label>
      {#if source === "archive"}<label class="archive-choice">Запись<select bind:value={archiveId} onchange={chooseArchive} disabled={loadingArchives || status?.busy || actionBusy || !archives.length}><option value="" disabled>Выберите запись</option>{#each archives as archive}<option value={archive.id}>{archive.label} · {missionTime(archive.createdAt)}</option>{/each}</select></label><label>Снимок<select bind:value={sequence} disabled={!currentArchive || status?.busy || actionBusy}>{#each currentArchive?.snapshots ?? [] as snapshot}<option value={snapshot.sequence}>№ {snapshot.sequence}{snapshot.complete ? "" : " · неполный"}</option>{/each}</select></label><button disabled={loadingArchives || status?.busy} onclick={() => void refreshArchives()}>Обновить записи</button>{/if}
    </div>
    <div class="read-actions">{#if status?.tracking}<button disabled={actionBusy} onclick={() => void stopTracking()}>Остановить карту</button>{:else if status?.busy}<button disabled={actionBusy || status.cancelling} onclick={() => void run(true)}>{status.cancelling ? "Отменяем…" : "Отменить чтение"}</button>{:else}<button class="primary" disabled={actionBusy || !status || (source === "live" ? !status.gameRunning : !currentSnapshot)} onclick={() => void run()}>{actionBusy ? "Запускаем…" : source === "live" ? "Запустить карту" : "Разобрать снимок"}</button>{#if source === "live" && status?.autoStart}<button disabled={actionBusy} onclick={() => void run(true)}>Остановить карту</button>{/if}{/if}</div>
    <div class="live-status"><i class:running={status?.tracking}></i><span>{status?.busy ? status.phase || "Читаем данные…" : scene?.source === "live" ? status?.tracking ? "Карта обновляется" : "Карта остановлена" : scene ? "Просмотр записи" : status?.gameRunning ? status?.autoStart ? "Ожидаем загрузку локации" : "Автозапуск приостановлен" : "Ожидаем Warframe"}</span>{#if scene && !status?.busy}<time>{missionTime(scene.capturedAt)}</time>{/if}</div>
    {#if status?.busy}<div class="progress" role="status"><progress aria-label="Чтение данных миссии"></progress><span>{missionBytes(status.scannedBytes)} · первый поиск может занять время</span></div>{/if}
    {#if source === "archive" && !loadingArchives && !archives.length}<p class="source-note">Сохранённых записей пока нет. Запись можно включить внизу вкладки.</p>{/if}
    {#if archiveError}<p class="error" role="alert">{archiveError}</p>{/if}
  </section>
  {#if error || status?.error}<div class="error" role="alert"><span>{error || status?.error}</span><button onclick={() => { error = ""; loadedRevision = -1; void refresh(); }}>Повторить загрузку</button></div>{/if}
  {#if sceneError}<p class="error" role="status">{sceneError}</p>{/if}
  {#if filterSyncError}<p class="error" role="status">{filterSyncError}</p>{/if}
  <div class="explorer">
    <aside class="filter-rail" bind:this={filterRail} aria-label="Фильтры карты">
      <MissionFilterEditor filters={customFilters} selected={filterSelection} objects={objects} counts={customCounts} onchange={saveFilters} onclear={() => { pickedKeys = []; selectedKey = ""; }} />
      {#if filterStorageError}<p class="error" role="alert">{filterStorageError}<button onclick={() => filterLoadFailed ? loadFilters() : saveFilters(customFilters)}>{filterLoadFailed ? "Повторить загрузку" : "Повторить сохранение"}</button></p>{/if}
      <section class="standard-filters" aria-label="Остальные объекты"><div class="section-heading"><h2>Остальные объекты</h2><button class="text-action" onclick={() => { const enabled = !Object.values(filters).every(Boolean); filters = Object.fromEntries(MISSION_FILTERS.map(filter => [filter.key, enabled])) as Record<MissionFilter, boolean>; }}>{Object.values(filters).every(Boolean) ? "Скрыть все" : "Показать все"}</button></div>
        <div class="filters">{#each MISSION_FILTERS as filter}<label><input type="checkbox" bind:checked={filters[filter.key]} /><i style:background={filter.color}></i><span>{filter.key === "npc" ? "Персонажи и враги" : filter.key === "lootspots" ? "Места находок" : filter.label}</span><b>{objects.filter(o => objectFilter(o.kind) === filter.key && !objectFilterEntry(o, customIndex)).length}</b></label>{/each}</div>
      </section>
      <details class="view-settings"><summary>Вид и высота</summary><div><label>Состояние тайников<select bind:value={cacheState}><option value="all">Все состояния</option><option value="available">Можно открыть</option><option value="unknown">Состояние неизвестно</option><option value="opened">Открытые</option></select></label><label class="check"><input type="checkbox" bind:checked={showZones} />Границы зон</label><label class="check"><input type="checkbox" bind:checked={sliceEnabled} />Ограничить по высоте</label>{#if sliceEnabled}<label>Высота: {heightCenter.toFixed(1)}<input type="range" min={Math.floor(bounds.minY)} max={Math.max(Math.ceil(bounds.maxY), Math.floor(bounds.minY) + 1)} step="0.5" bind:value={heightCenter} /></label><label>Диапазон<select bind:value={heightHalfWidth}><option value={2}>± 2</option><option value={4}>± 4</option><option value={8}>± 8</option><option value={16}>± 16</option></select></label><p>Срез по высоте, а не определённый этаж.</p>{/if}</div></details>
    </aside>
    <section class="map-panel" aria-label="Схема расположения объектов">
      <div class="map-tools"><div class="map-title"><strong>Карта миссии</strong><span>{scene ? `${visibleObjects.length} объектов показано` : "Положение предметов и игроков"}</span></div><div class="map-actions"><button disabled={!myAvatar} onclick={focusMe}>{status?.tracking ? "Следовать за мной" : "Показать меня"}</button><button aria-pressed={rotateWithView} disabled={!rotateWithView && (!myAvatar || !Number.isFinite(viewHeading))} onclick={toggleHeading}>По взгляду</button><button disabled={!scene} onclick={resetMap}>Вся карта</button></div></div>
      {#if scene}
        <div class="canvas-wrap"><canvas bind:this={canvas} use:resizeCanvas tabindex="0" aria-label="Карта объектов. Стрелки перемещают вид, плюс и минус меняют масштаб. Ctrl и щелчок выделяют несколько точек." onwheel={wheel} onpointerdown={pointerDown} onpointermove={pointerMove} onpointerup={pointerUp} onpointercancel={() => drag = null} onkeydown={keyboard}></canvas><div class="zoom-controls"><button aria-label="Увеличить карту" onclick={() => zoom(1.4)}>+</button><button aria-label="Уменьшить карту" onclick={() => zoom(1 / 1.4)}>−</button></div>
          {#if !scene.meshes.length}<p class="map-overlay-note">Показаны точки. Геометрия карты пока не найдена.</p>{/if}
          {#if followKey}<div class="follow-status" role="status"><span>{followHint || (followingMe ? "Следуем за вами" : "Следуем за игроком")}</span><button aria-label="Прекратить следование" onclick={() => { followKey = ""; followingMe = false; followHint = ""; }}>×</button></div>{:else if followHint}<p class="map-overlay-note">{followHint}</p>{/if}
        </div>
        <div class="target-bar">{#if target}<div><small>Текущая цель</small><strong>{target.label}</strong><span>{distanceLabel(myAvatar, target)}{target.kind === "cache" && target.availability === "opened" ? " · уже открыт" : ""}</span></div><button onclick={() => selectObject(target!)}>Показать</button><button aria-label="Убрать цель" onclick={() => targetKey = ""}>×</button>{:else if targetKey}<span>Цель больше не видна.</span><button onclick={() => targetKey = ""}>Убрать цель</button>{:else}<span>Быстрый выбор цели</span><button disabled={!nearestCache} onclick={() => nearestCache && chooseTarget(nearestCache)}>Ближайший закрытый тайник →</button>{/if}</div>
        <div class="map-footer"><span>Колесо — масштаб · Ctrl + щелчок — несколько точек</span><span>{myAvatar ? "Ваш персонаж найден" : "Позиция игрока не определена"}</span></div>
        {#if rotateWithView && !Number.isFinite(viewHeading)}<p class="map-help">Направление взгляда временно недоступно. Поворот сохранён.</p>{/if}
      {:else}<div class="initial"><div class="empty-map-icon">⌖</div><h2>{status?.busy ? "Строим карту миссии" : status?.autoStart ? "Карта запустится автоматически" : "Карта приостановлена"}</h2><p>{status?.busy ? "Находим предметы и проверяем их координаты. Результат появится здесь." : status?.gameRunning ? status?.autoStart ? "После загрузки локации найдём предметы и построим карту. В новой локации карта обновится сама." : "Нажмите «Запустить карту», чтобы возобновить карту и автоматический запуск в следующих локациях." : "Запустите Warframe и войдите на миссию. Здесь появятся предметы, тайники и ваш персонаж."}</p><span>Карта не записывает дампы на диск</span></div>{/if}
    </section>
    <section class="object-panel" aria-label="Найденные объекты">
      <div class="list-heading"><h2>Найти на карте</h2><span>{visibleObjects.length}</span></div><label class="search"><span class="sr-only">Поиск объекта</span><input type="search" bind:value={query} placeholder="Название на русском или английском" /></label>
      <div class="list-options"><label class="check"><input type="checkbox" bind:checked={groupObjects} />По типам</label><label class="check"><input type="checkbox" bind:checked={sortByDistance} disabled={!myAvatar && !sortByDistance} />Ближайшие</label></div>
      {#if query && visibleObjects.length}<button class="select-results" onclick={() => { pickedKeys = listEntries.map(entry => entry.object.key); }}>Выбрать найденное для фильтра</button>{/if}
      {#if selected}<section class="selected-card" aria-label="Выбранный объект"><div class="selected-heading"><span>Выбранный объект</span><button class="text-action" aria-label="Закрыть сведения о предмете" onclick={() => selectedKey = ""}>×</button></div><h3>{selected.label || objectKindLabel(selected.kind)}</h3>{#if selected.nameEn && selected.nameEn !== selected.label}<p class="english-name">{selected.nameEn}</p>{/if}<p>{distanceLabel(myAvatar, selected)}</p><div class="selected-actions">{#if selected.kind !== "avatar"}<button class="primary" onclick={() => chooseTarget(selected!)}>Выбрать целью</button>{/if}<button onclick={() => selectObject(selected!)}>На карте</button>{#if scene?.source === "live" && selected.kind === "avatar" && selected.key !== myAvatar?.key}<button disabled={!status?.tracking} onclick={() => startFollowing(selected!)}>Следовать</button>{/if}</div><p class="assign-reminder">Добавить в фильтр: нажмите + у группы слева.</p>
        <details class="object-data"><summary>Подробнее об объекте</summary><p>{selected.availability === "available" ? "Можно открыть: действие подтверждено." : selected.availability === "opened" ? "Тайник открыт." : selected.kind === "lootspot" ? "Возможное место появления. Наличие предмета не подтверждено." : "Доступность взаимодействия не подтверждена."}</p>{#if selected.positionFresh === false}<p>Показаны последние известные координаты.</p>{/if}<dl><div><dt>Координаты</dt><dd>X {selected.position[0].toFixed(2)} · Y {selected.position[1].toFixed(2)} · Z {selected.position[2].toFixed(2)}</dd></div>{#each selected.details as detail}<div><dt>{detail.label}</dt><dd>{detail.value}</dd></div>{/each}</dl><p>{selected.typeNames.join(" → ")}</p>{#if selected.itemPath}<code>{selected.itemPath}</code>{/if}</details>
      </section>{/if}
      <div class="object-scroll">{#if !scene}<p class="empty">Найденные объекты появятся после запуска карты.</p>{:else if !objects.length}<p class="empty">В этом чтении объекты не определены. Можно запустить карту заново.</p>{:else if !visibleObjects.length}<div class="empty"><strong>Ничего не найдено</strong><p>Проверьте название и включённые группы слева.</p>{#if query}<button onclick={() => query = ""}>Очистить поиск</button>{/if}</div>{:else}<ul class="object-list">{#each listEntries.slice(0, listLimit) as entry, objectIndex (entry.key)}{@const object = entry.object}<li><input type="checkbox" aria-label={`Выбрать для фильтра: ${object.label} · строка ${objectIndex + 1}`} checked={pickedKeys.includes(object.key)} onchange={() => togglePicked(object.key)} /><button class:selected={object.key === selectedKey} onclick={() => selectObject(object)}><i style:background={filterColor(object, customIndex)}></i><span><strong>{object.key === myAvatar?.key ? "Вы" : object.label || objectKindLabel(object.kind)}</strong>{#if object.nameEn && object.nameEn !== object.label}<small>{object.nameEn}</small>{/if}<small>{object.kind === "cache" ? object.availability === "available" ? "Можно открыть · " : object.availability === "opened" ? "Открыт · " : "" : ""}{myAvatar && object.key !== myAvatar.key ? distanceLabel(myAvatar, object) : objectKindLabel(object.kind)}{object.positionFresh === false ? " · прежняя позиция" : ""}</small></span>{#if entry.count > 1}<b class="copy-count" title="Экземпляров этого типа на карте">×{entry.count}</b>{/if}</button></li>{/each}</ul>{#if listEntries.length > listLimit}<button class="more" onclick={() => listLimit += 100}>Показать ещё</button>{/if}{/if}</div>
    </section>
  </div>
  <div class="research-tools">
    {#if scene}<details class="warnings"><summary>{scene.complete ? "О точности карты" : "Карта прочитана частично"}</summary><p>Геометрия может покрывать только часть миссии. Пунктир — границы зон, а не стены. Расстояния указаны по прямой; маршруты не построены.</p>{#if scene.zones?.length && !scene.zonesFresh}<p>Свежие границы зон не подтверждены. Сохранена последняя согласованная карта.</p>{/if}{#if scene.warnings.length}<ul>{#each scene.warnings as warning}<li>{warning}</li>{/each}</ul>{/if}</details>
        {#if difference && previousScene}<details class="comparison"><summary>Изменения относительно {missionTime(previousScene.capturedAt)} · Новых {difference.added.length} · Больше не видны {difference.absent.length}</summary><p>Сравниваются тип, адрес и положение в двух чтениях. Это различие выборок, а не журнал событий миссии.</p><div class="comparison-lists"><div><h3>Появились в выборке</h3>{#if !difference.added.length}<p>Нет новых записей.</p>{:else}<ul>{#each difference.added.slice(0, 30) as object}<li>{object.label || objectKindLabel(object.kind)}</li>{/each}</ul>{#if difference.added.length > 30}<p>И ещё {difference.added.length - 30}.</p>{/if}{/if}</div><div><h3>Больше не видны в выборке</h3>{#if !difference.absent.length}<p>Все прежние записи остаются.</p>{:else}<ul>{#each difference.absent.slice(0, 30) as object}<li>{object.label || objectKindLabel(object.kind)}</li>{/each}</ul>{#if difference.absent.length > 30}<p>И ещё {difference.absent.length - 30}.</p>{/if}{/if}</div></div></details>{/if}
    <details class="export"><summary>Сохранить результат для анализа</summary><p>JSON содержит разобранные объекты и геометрию. OBJ содержит геометрию для просмотра в 3D-редакторе.</p><div class="export-buttons"><button disabled={exporting || status?.busy} onclick={() => void exportScene("json")}>Сохранить JSON</button><button disabled={exporting || status?.busy || !scene.meshes.length} onclick={() => void exportScene("obj")}>Сохранить OBJ</button></div>{#if exportPath}<p role="status">Результат сохранён: <code>{exportPath}</code></p>{/if}<p class="muted">Прочитано {missionBytes(scene.stats.scannedBytes)} · Геометрических частей: {scene.stats.meshCount} · Полигонов: {scene.stats.faceCount.toLocaleString("ru-RU")}</p></details>

    {/if}
    <details class="recording-disclosure" ontoggle={(event) => { if (!(event.currentTarget as HTMLDetailsElement).open) void refreshArchives(); }}><summary>Записать миссию для исследования</summary><div><MemoryRecording /></div></details>
  </div>
</section>

<style>
  .mission{color:var(--text);min-width:0;font-size:.8rem}h2,h3,p{margin:0}h2{font-size:.94rem}h3{font-size:.9rem}p{font-size:.78rem;color:var(--text-muted);line-height:1.5}button,select,input{font:inherit}button{min-height:32px;border:1px solid var(--border);border-radius:6px;background:var(--surface-1);color:var(--text);padding:.45rem .65rem;cursor:pointer;font-size:.76rem}button:hover{background:var(--surface-3);border-color:var(--border-strong)}button:disabled{opacity:.45;cursor:default}.primary{background:#397c67;color:white;border-color:#397c67}.primary:hover{background:#2e6956}button:focus-visible,input:focus-visible,select:focus-visible,summary:focus-visible,canvas:focus-visible{outline:2px solid var(--accent);outline-offset:2px}label{font-size:.74rem;color:var(--text-muted)}select,input[type="search"]{background:var(--surface-1);color:var(--text);border:1px solid var(--border);border-radius:5px;padding:.5rem;min-width:0}input[type="checkbox"]{accent-color:#397c67;width:15px;height:15px;margin:0;flex-shrink:0}i{width:7px;height:7px;border-radius:50%;display:inline-block;flex-shrink:0}.source-panel{display:flex;align-items:center;gap:.75rem;padding:.6rem .75rem;margin-bottom:.85rem;background:var(--surface-1);border:1px solid var(--border);border-radius:9px;flex-wrap:wrap}.source-options{display:flex;align-items:center;gap:.65rem;min-width:0}.source-options label{display:flex;align-items:center;gap:.5rem}.source-options select{max-width:260px;padding:.4rem .5rem}.live-status{display:flex;align-items:center;gap:.5rem;margin-left:auto;font-size:.75rem;color:var(--text-muted)}.live-status i{background:#a8a096}.live-status i.running{background:#479977;box-shadow:0 0 0 3px #47997717}.live-status time{font-size:.67rem;margin-left:.4rem}.progress{flex-basis:100%;display:flex;align-items:center;gap:.65rem;font-size:.74rem;color:var(--text-muted)}progress{height:5px;width:150px;accent-color:#397c67}.source-note{flex-basis:100%}.error{background:var(--danger-soft);color:var(--danger);border:1px solid #a94d4d40;border-radius:6px;padding:.65rem;margin-bottom:.65rem;font-size:.76rem;display:flex;gap:.5rem;align-items:center;justify-content:space-between;overflow-wrap:anywhere}
  .explorer{display:grid;grid-template-columns:254px minmax(0,1fr) 294px;gap:.75rem;height:calc(100vh - 225px);min-height:580px;max-height:1000px}.filter-rail,.object-panel{background:var(--surface-1);border:1px solid var(--border);border-radius:9px;min-width:0;min-height:0}.filter-rail{padding:.9rem;overflow:auto;scrollbar-width:thin}.standard-filters{border-top:1px solid var(--border);padding-top:1rem;margin-top:1rem}.section-heading{display:flex;align-items:center;justify-content:space-between;gap:.3rem}.section-heading h2{font-size:.8rem}.text-action{background:transparent;border:0;font-size:.67rem;padding:.15rem;min-height:25px;color:var(--text-muted)}.filters{display:flex;flex-direction:column;gap:.2rem;margin-top:.6rem}.filters label{display:flex;align-items:center;gap:.5rem;padding:.43rem 0;cursor:pointer;color:var(--text)}.filters label>span{flex:1}.filters b{color:var(--text-muted);font-size:.69rem;font-weight:500}.view-settings{border-top:1px solid var(--border);padding-top:.85rem;margin-top:.9rem;font-size:.78rem}.view-settings>div{display:grid;gap:.7rem;margin-top:.75rem}.view-settings label{display:grid;gap:.35rem}.check,.view-settings .check{display:flex;align-items:center;gap:.4rem}.view-settings p{font-size:.68rem}summary{cursor:pointer;line-height:1.5}.view-settings select{width:100%}
  .map-panel{display:flex;flex-direction:column;background:#101b24;border:1px solid #31454c;border-radius:10px;overflow:hidden;min-width:0;min-height:0;color:#e5eee9}.map-tools{display:flex;align-items:center;justify-content:space-between;gap:.5rem;background:#1a2a34;padding:.75rem;border-bottom:1px solid #31454c}.map-title{display:grid;gap:.2rem;min-width:0}.map-title strong{font-size:.85rem}.map-title span{font-size:.65rem;color:#b1c5c4}.map-actions{display:flex;gap:.25rem}.map-panel button{color:#e6efed;background:#253c47;border-color:#46606b;font-size:.72rem;white-space:nowrap}.map-panel button:hover{background:#35505b}.map-panel button[aria-pressed="true"]{background:#397c67;border-color:#71b49b}.canvas-wrap{flex:1;position:relative;min-height:0;overflow:hidden}canvas{position:absolute;inset:0;display:block;width:100%;height:100%;touch-action:none;cursor:grab}canvas:active{cursor:grabbing}.zoom-controls{position:absolute;right:14px;bottom:14px;display:flex;flex-direction:column;gap:4px}.zoom-controls button{width:33px;height:33px;font-size:1.1rem;padding:0;background:#203641e8}.map-footer{display:flex;justify-content:space-between;gap:.5rem;flex-wrap:wrap;padding:.6rem .75rem;border-top:1px solid #31454c;color:#9fb9bd;font-size:.64rem}.target-bar{padding:.65rem .75rem;display:flex;align-items:center;gap:.5rem;border-top:1px solid #31454c;background:#172832;min-height:52px;font-size:.7rem}.target-bar>span,.target-bar>div{flex:1;min-width:0}.target-bar>div{display:grid;gap:.18rem}.target-bar strong{font-size:.78rem;overflow-wrap:anywhere}.target-bar small{color:#83b89f;font-size:.6rem}.target-bar span{color:#b1c5c4}.map-overlay-note{position:absolute;left:12px;top:12px;max-width:75%;background:#1b303be6;padding:.55rem .7rem;border-radius:6px;color:#d1ded8;font-size:.72rem}.follow-status{position:absolute;left:12px;top:12px;background:#1b303bee;border:1px solid #517f72;border-radius:6px;display:flex;align-items:center;gap:.6rem;padding:.25rem .4rem .25rem .65rem;font-size:.7rem;max-width:85%}.follow-status button{border:0;background:transparent;min-height:24px;padding:0 .3rem}.map-help{padding:.5rem .75rem;color:#d8c49a;font-size:.7rem}.initial{flex:1;display:flex;align-items:center;justify-content:center;flex-direction:column;text-align:center;padding:2rem}.initial h2{font-size:1.2rem;color:#e2eae6;margin:.8rem 0}.initial p{max-width:340px;color:#aabfc1;line-height:1.7}.initial>span{font-size:.67rem;color:#88a7a5;margin-top:1.5rem}.empty-map-icon{font-size:3.2rem;color:#80baa4}
  .object-panel{padding:.9rem;display:flex;flex-direction:column;gap:.65rem;overflow:hidden}.list-heading{display:flex;align-items:center;justify-content:space-between}.list-heading>span{font-size:.7rem;color:var(--text-muted);background:var(--surface-3);border-radius:12px;padding:.15rem .5rem}.search input{width:100%;font-size:.75rem;padding:.65rem}.list-options{display:flex;gap:1rem}.list-options label{font-size:.71rem;cursor:pointer}.object-scroll{flex:1;min-height:120px;overflow:auto;scrollbar-width:thin}.object-list{list-style:none;margin:0;padding:0}.object-list li{display:flex;align-items:flex-start;gap:.35rem;border-bottom:1px solid var(--border)}.object-list li>input{margin-top:.9rem;cursor:pointer}.object-list button{display:flex;align-items:start;gap:.4rem;text-align:left;flex:1;min-width:0;background:transparent;border:1px solid transparent;padding:.7rem .2rem;line-height:1.4}.object-list button:hover{background:var(--surface-2)}.object-list button.selected{border-color:#539c8050;background:#539c8010}.object-list button>span{min-width:0;flex:1}.object-list i{margin-top:.35rem}.object-list strong{display:block;font-size:.77rem;font-weight:600;overflow-wrap:anywhere}.object-list small{display:block;font-size:.66rem;color:var(--text-muted);overflow-wrap:anywhere;margin-top:.2rem;font-weight:400}.copy-count{font-size:.68rem;color:var(--text-muted);white-space:nowrap}.empty{padding:1rem .3rem;font-size:.78rem;color:var(--text-muted)}.empty p{margin:.5rem 0}.more,.select-results{width:100%;font-size:.7rem}.selected-card{background:var(--surface-2);border:1px solid #539c8050;border-radius:7px;padding:.75rem;max-height:46%;overflow:auto;flex-shrink:0;scrollbar-width:thin}.selected-heading{display:flex;align-items:center;justify-content:space-between;color:var(--text-muted);font-size:.65rem;margin-bottom:.25rem}.selected-card h3{font-size:.83rem;line-height:1.4;overflow-wrap:anywhere}.selected-card p{font-size:.69rem;margin-top:.3rem}.selected-card .english-name{font-size:.67rem;overflow-wrap:anywhere}.selected-actions{display:flex;gap:.35rem;flex-wrap:wrap;margin-top:.65rem}.selected-actions button{font-size:.69rem;padding:.4rem .5rem}.selected-card .assign-reminder{color:#33755e;font-size:.65rem;margin-top:.6rem}.object-data{border-top:1px solid var(--border);margin-top:.6rem;padding-top:.5rem;font-size:.69rem}.object-data p,dd,code{overflow-wrap:anywhere}dl{display:grid;gap:.6rem;font-size:.68rem}dt{color:var(--text-muted)}dd{margin:.15rem 0 0}code{font-size:.66rem;white-space:pre-wrap}.sr-only{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0)}
  .research-tools{display:flex;align-items:start;flex-wrap:wrap;gap:.5rem 1.4rem;margin-top:.85rem}.research-tools>details{font-size:.72rem;color:var(--text-muted);min-width:0}.research-tools>details[open]{flex-basis:100%;background:var(--surface-1);border:1px solid var(--border);padding:.8rem;border-radius:7px}.research-tools p{margin:.5rem 0}.research-tools ul{padding-left:1.2rem;line-height:1.5}.export-buttons{display:flex;gap:.5rem;margin:.7rem 0}.comparison-lists{display:grid;grid-template-columns:1fr 1fr;gap:1rem}.recording-disclosure>div{margin-top:.75rem}
</style>
