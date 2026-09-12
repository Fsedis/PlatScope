<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import MemoryRecording from "./MemoryRecording.svelte";
  import MissionFilterEditor from "./MissionFilterEditor.svelte";
  import { MISSION_FILTER_STORAGE, parseMissionFilters, serializeMissionFilters, objectRule, indexMissionFilters, countMissionFilters, filterColor, objectIsVisible, type MissionFilterIndex, type CustomMissionFilter } from "./missionFilters";
  import { MISSION_FILTERS, objectMatchesSearch, objectFilter, objectKindLabel, finitePosition, sceneBounds, fitCamera, followFrame, followTargetSignature, mapScale, worldToCanvas, zoomAt, scaledHeight, inHeightSlice, faceInHeightSlice, compareScenes, missionTime, missionBytes,
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
  let generation = 0;
  let alive = true;
  let filters: Record<MissionFilter, boolean> = { feather: true, pickup: true, players: true, npc: false, other: false };
  let customFilters: CustomMissionFilter[] = [];
  let filterStorageError = "";
  let filterLoadFailed = false;
  let pickedKeys: string[] = [];
  let query = "";
  let selectedKey = "";
  let followKey = "";
  let followSignature = "";
  let followHint = "";
  let framingBounds: MapBounds | null = null;
  let listLimit = 100;
  let sliceEnabled = false;
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
  $: visibleObjects = objects.filter(o => objectIsVisible(o, filters, customIndex) && inHeightSlice(o.position[1], sliceEnabled, heightCenter, heightHalfWidth) && objectMatchesSearch(o, query)).sort((a, b) => (a.kind === "feather" ? 0 : 1) - (b.kind === "feather" ? 0 : 1) || a.label.localeCompare(b.label, "ru"));
  $: selected = objects.find(o => o.key === selectedKey) ?? null;
  $: pickedObjects = objects.filter(object => pickedKeys.includes(object.key));
  $: filterSelection = pickedKeys.length ? pickedObjects : selected ? [selected] : [];
  $: difference = scene && previousScene ? compareScenes(previousScene, scene) : null;
  $: drawState = { scene, bounds, camera, objects: visibleObjects, selectedKey, pickedKeys, customIndex, sliceEnabled, heightCenter, heightHalfWidth, width: canvasWidth, height: canvasHeight };
  $: if (canvas) draw(canvas, drawState);

  async function receive(next: MissionResearchStatus, version: number) {
    if (!alive || version !== generation) return;
    status = next;
    if (!next.tracking && followKey) { followKey = ""; followHint = ""; }
    if (loadedRevision === next.revision) return;
    try {
      const nextScene = await invoke<MissionScene | null>("mission_research_scene");
      if (!alive || version !== generation || status?.revision !== next.revision) return;
      loadedRevision = next.revision;
      if (nextScene) {
        const continuingLive = scene?.source === "live" && nextScene.source === "live" && !frameNextScene;
        previousScene = scene?.source === "archive" && nextScene.source === "archive" && scene.capturedAt !== nextScene.capturedAt ? scene : nextScene.source === "live" ? null : previousScene;
        scene = nextScene;
        if (!continuingLive) {
          selectedKey = ""; pickedKeys = []; followKey = ""; followHint = ""; listLimit = 100; query = "";
          const nextBounds = sceneBounds(nextScene); framingBounds = nextBounds; camera = fitCamera(nextBounds); heightCenter = (nextBounds.minY + nextBounds.maxY) / 2;
        } else if (!nextScene.objects.some(o => o.key === selectedKey)) selectedKey = "";
        frameNextScene = false;
        if (followKey) {
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
        scene = null; previousScene = null; selectedKey = ""; pickedKeys = []; followKey = ""; followHint = ""; framingBounds = null; frameNextScene = true;
      }
    } catch (reason) { if (alive && version === generation) { loadedRevision = next.revision; error = typeof reason === "string" ? reason : "Не удалось загрузить результат. Повторите загрузку."; } }
  }
  async function refresh() {
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
    actionBusy = true; startTrackingAfterScan = false; followKey = ""; followHint = ""; const version = ++generation; error = "";
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
  }
  function saveFilters(next: CustomMissionFilter[]) {
    filterLoadFailed = false;
    customFilters = next;
    try { localStorage.setItem(MISSION_FILTER_STORAGE, serializeMissionFilters(next)); filterStorageError = ""; }
    catch { filterStorageError = "Фильтры работают в этом окне, но не сохранены. Проверьте доступное место и повторите сохранение."; }
  }
  function togglePicked(key: string) { pickedKeys = pickedKeys.includes(key) ? pickedKeys.filter(item => item !== key) : [...pickedKeys, key]; }
  function selectObject(object: MissionObject) {
    followKey = ""; followHint = ""; selectedKey = object.key; camera = { ...camera, x: object.position[0], z: object.position[2], zoom: Math.max(3, camera.zoom) };
    if (sliceEnabled) heightCenter = object.position[1];
  }
  function startFollowing(object: MissionObject) { if (object.kind !== "avatar") return; if (object.positionFresh === false) { selectedKey = object.key; followHint = ""; } else selectObject(object); followKey = object.key; followSignature = followTargetSignature(object); }
  function resetMap() { followKey = ""; followHint = ""; if (scene) framingBounds = sceneBounds(scene); camera = fitCamera(framingBounds ?? bounds); }
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
    const dx = event.clientX - drag.x, dy = event.clientY - drag.y; if (Math.hypot(dx, dy) > 3) { drag.moved = true; followKey = ""; followHint = ""; }
    const scale = mapScale(bounds, canvasWidth, canvasHeight, drag.camera.zoom); camera = { ...drag.camera, x: drag.camera.x - dx / scale, z: drag.camera.z + dy / scale };
  }
  function pointerUp(event: PointerEvent) {
    if (!drag || drag.pointer !== event.pointerId) return;
    const click = !drag.moved; drag = null; canvas?.releasePointerCapture(event.pointerId);
    if (!click || !canvas) return;
    const rect = canvas.getBoundingClientRect(), x = event.clientX - rect.left, y = event.clientY - rect.top; const scale = mapScale(bounds, canvasWidth, canvasHeight, camera.zoom);
    let nearest: MissionObject | null = null, distance = 14;
    for (const object of visibleObjects) { const [px, py] = worldToCanvas(object.position, camera, scale, canvasWidth, canvasHeight); const d = Math.hypot(px - x, py - y); if (d < distance) { distance = d; nearest = object; } }
    if (nearest) { if (event.ctrlKey || event.metaKey) togglePicked(nearest.key); selectedKey = nearest.key; followKey = ""; followHint = ""; }
  }
  function keyboard(event: KeyboardEvent) {
    const scale = mapScale(bounds, canvasWidth, canvasHeight, camera.zoom), step = 60 / scale;
    if (event.key === "+" || event.key === "=") zoom(1.4); else if (event.key === "-") zoom(1 / 1.4); else if (event.key === "Home") resetMap();
    else if (event.key === "ArrowLeft") { camera = { ...camera, x: camera.x - step }; followKey = ""; } else if (event.key === "ArrowRight") { camera = { ...camera, x: camera.x + step }; followKey = ""; }
    else if (event.key === "ArrowUp") { camera = { ...camera, z: camera.z + step }; followKey = ""; } else if (event.key === "ArrowDown") { camera = { ...camera, z: camera.z - step }; followKey = ""; } else return;
    event.preventDefault();
  }
  interface DrawState { scene: MissionScene | null; bounds: MapBounds; camera: MapCamera; objects: MissionObject[]; selectedKey: string; pickedKeys: string[]; customIndex: MissionFilterIndex; sliceEnabled: boolean; heightCenter: number; heightHalfWidth: number; width: number; height: number }
  function draw(node: HTMLCanvasElement, state: DrawState) {
    const ctx = node.getContext("2d"); if (!ctx) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2), { width, height } = state;
    if (node.width !== Math.round(width * dpr)) node.width = Math.round(width * dpr); if (node.height !== Math.round(height * dpr)) node.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0); ctx.clearRect(0, 0, width, height); ctx.fillStyle = "#111a23"; ctx.fillRect(0, 0, width, height);
    if (!state.scene) return;
    const scale = mapScale(state.bounds, width, height, state.camera.zoom);
    for (const mesh of state.scene.meshes) for (const face of mesh.faces) {
      if (!faceInHeightSlice(mesh.vertices, face, state.sliceEnabled, state.heightCenter, state.heightHalfWidth)) continue;
      const points = face.map(i => worldToCanvas(mesh.vertices[i], state.camera, scale, width, height));
      if (points.every(p => p[0] < 0) || points.every(p => p[0] > width) || points.every(p => p[1] < 0) || points.every(p => p[1] > height)) continue;
      const meanY = face.reduce((sum, i) => sum + mesh.vertices[i][1], 0) / face.length; const light = 23 + scaledHeight(meanY, state.bounds) * 18;
      ctx.beginPath(); points.forEach((p, i) => i ? ctx.lineTo(...p) : ctx.moveTo(...p)); ctx.closePath(); ctx.fillStyle = `hsl(192 19% ${light}%)`; ctx.fill(); ctx.strokeStyle = "#55737966"; ctx.lineWidth = .6; ctx.stroke();
    }
    for (const object of state.objects) {
      const [x, y] = worldToCanvas(object.position, state.camera, scale, width, height); if (x < -15 || y < -15 || x > width + 15 || y > height + 15) continue;
      const chosen = object.key === state.selectedKey || state.pickedKeys.includes(object.key); ctx.beginPath(); ctx.fillStyle = filterColor(object, state.customIndex);
      if (object.kind === "feather") { ctx.moveTo(x, y - 7); ctx.lineTo(x + 5, y); ctx.lineTo(x, y + 7); ctx.lineTo(x - 5, y); ctx.closePath(); } else ctx.arc(x, y, chosen ? 5 : 3.5, 0, Math.PI * 2);
      ctx.fill(); ctx.strokeStyle = "#0c1119"; ctx.lineWidth = 1.5; ctx.stroke();
      if (chosen) { ctx.beginPath(); ctx.arc(x, y, 11, 0, Math.PI * 2); ctx.strokeStyle = "#fff"; ctx.lineWidth = 2; ctx.stroke(); }
    }
    ctx.fillStyle = "#a8becb"; ctx.font = "12px system-ui"; ctx.fillText("Вид сверху · X / Z", 14, height - 16);
  }
  onMount(() => {
    loadFilters();
    void refresh(); void refreshArchives(); const timer = setInterval(() => void refresh(), 1000); return () => { alive = false; generation++; clearInterval(timer); }; });
</script>

<section class="mission" aria-label="Карта миссии">
  <section class="source-panel" aria-label="Источник данных миссии">
    <div class="source-options"><label>Источник<select bind:value={source} disabled={status?.busy || status?.tracking || actionBusy}><option value="live">Игра</option><option value="archive">Сохранённая запись</option></select></label>
      {#if source === "archive"}<label class="archive-choice">Запись<select bind:value={archiveId} onchange={chooseArchive} disabled={loadingArchives || status?.busy || actionBusy || !archives.length}><option value="" disabled>Выберите запись</option>{#each archives as archive}<option value={archive.id}>{archive.label} · {missionTime(archive.createdAt)}</option>{/each}</select></label>
        <label>Снимок<select bind:value={sequence} disabled={!currentArchive || status?.busy || actionBusy}>{#each currentArchive?.snapshots ?? [] as snapshot}<option value={snapshot.sequence}>№ {snapshot.sequence} · {missionTime(snapshot.endedAt)}{snapshot.complete ? "" : " · неполный"}</option>{/each}</select></label>
        <button class="subtle" disabled={loadingArchives || status?.busy} onclick={() => void refreshArchives()}>{loadingArchives ? "Ищем записи…" : "Обновить список"}</button>
      {/if}
    </div>
    <div class="read-actions">{#if status?.tracking}<button disabled={actionBusy} onclick={() => void stopTracking()}>Остановить карту</button>{:else if status?.busy}<button disabled={actionBusy || status.cancelling} onclick={() => void run(true)}>{status.cancelling ? "Отменяем…" : "Отменить чтение"}</button>{:else}<button class="primary" disabled={actionBusy || !status || (source === "live" ? !status.gameRunning : !currentSnapshot)} onclick={() => void run()}>{actionBusy ? "Запускаем…" : source === "live" ? "Запустить карту" : "Разобрать снимок"}</button>{/if}
      {#if source === "live" && (!status || !status.gameRunning)}<p>{!status ? "Проверяем доступность игры…" : "Warframe не запущен. Запустите игру или выберите запись."}</p>{:else if source === "archive" && currentSnapshot && !currentSnapshot.complete}<p>Неполный снимок · Непрочитанных участков: {currentSnapshot.holes}</p>{/if}
    </div>
    {#if scene}
    <div class="capture"><strong>{scene.source === "live" ? status?.tracking ? "Живая карта" : "Обновление карты остановлено" : "Из сохранённой записи"}</strong><span>{scene.source === "live" ? `Последнее обновление: ${missionTime(scene.capturedAt)}` : `${missionTime(scene.startedAt)} — ${missionTime(scene.capturedAt)}`}</span>{#if !scene.complete}<span class="partial">Чтение неполное</span>{/if}</div>
    {/if}
    {#if source === "live"}<p class="disk-note">Карта не сохраняет дампы на диск. Запись включается отдельно.</p>{/if}
    {#if source === "archive" && !loadingArchives && !archives.length && !archiveError}<p class="notice">Сохранённых записей пока нет. Ниже можно начать запись исследования миссии.</p>{/if}
    {#if archiveError}<p class="error" role="alert">{archiveError}</p>{/if}
    {#if status?.busy}<div class="progress" role="status"><progress aria-label="Чтение данных миссии"></progress><span>{status.cancelling ? "Завершаем отмену…" : status.phase || "Читаем данные…"} · {missionBytes(status.scannedBytes)}</span></div>{/if}
  </section>
  {#if error || status?.error}<div class="error" role="alert"><span>{error || status?.error}</span><button onclick={() => { error = ""; loadedRevision = -1; void refresh(); }}>Повторить загрузку</button></div>{/if}
  <MissionFilterEditor filters={customFilters} selected={filterSelection} onchange={saveFilters} />
  {#if filterStorageError}<p class="error" role="alert">{filterStorageError}<button onclick={() => filterLoadFailed ? loadFilters() : saveFilters(customFilters)}>{filterLoadFailed ? "Повторить загрузку фильтров" : "Повторить сохранение"}</button></p>{/if}
  {#if scene}
    {#if followKey}<p class="follow-status" role="status">Вид следует за выбранным игроком. Принадлежность вам не подтверждена. <button onclick={() => { followKey = ""; followHint = ""; }}>Прекратить следование</button></p>{:else if followHint}<p role="status">{followHint}</p>{/if}
    <details class="warnings"><summary>Частичная карта · локальный персонаж не определён · о данных</summary><p>Геометрия может покрывать только часть миссии. Ваша точка, расстояния от вас и маршруты пока не определены. Можно выбрать персонажа из списка и следить за ним.</p>{#if scene.warnings.length}<ul>{#each scene.warnings as warning}<li>{warning}</li>{/each}</ul>{/if}</details>
    <div class="display-controls"><div class="filters" aria-label="Фильтры объектов">{#each MISSION_FILTERS as filter}<label><input type="checkbox" bind:checked={filters[filter.key]} /><i style:background={filter.color}></i>{filter.label}<span>{objects.filter(o => objectFilter(o.kind) === filter.key && !customIndex.has(objectRule(o).key)).length}</span></label>{/each}{#each customFilters as custom (custom.id)}<label><input type="checkbox" checked={custom.enabled} onchange={(event) => saveFilters(customFilters.map(filter => filter.id === custom.id ? { ...filter, enabled: event.currentTarget.checked } : filter))} /><i style:background={custom.color}></i>{custom.name}<span>{customCounts.get(custom.id) ?? 0}</span></label>{/each}</div>
    <div class="height-controls"><label class="slice"><input type="checkbox" bind:checked={sliceEnabled} />Ограничить по высоте</label>{#if sliceEnabled}<label class="height-range">Высота Y: {heightCenter.toFixed(1)}<input type="range" min={Math.floor(bounds.minY)} max={Math.max(Math.ceil(bounds.maxY), Math.floor(bounds.minY) + 1)} step="0.5" bind:value={heightCenter} /></label><label>Диапазон<select bind:value={heightHalfWidth}><option value={2}>± 2</option><option value={4}>± 4</option><option value={8}>± 8</option><option value={16}>± 16</option></select></label><span class="muted">Это срез координат, а не определённый этаж.</span>{/if}</div>
    </div>
    <div class="explorer">
      <section class="map-panel" aria-label="Схема расположения объектов"><div class="map-tools"><span>Схема по прочитанным данным</span><div><button aria-label="Уменьшить карту" onclick={() => zoom(1 / 1.4)}>−</button><button aria-label="Увеличить карту" onclick={() => zoom(1.4)}>+</button><button onclick={resetMap}>Показать всё</button></div></div>
        <canvas bind:this={canvas} use:resizeCanvas tabindex="0" aria-label="Схема объектов. Стрелки перемещают вид, плюс и минус меняют масштаб, Home показывает всё. Объекты доступны также списком рядом." onwheel={wheel} onpointerdown={pointerDown} onpointermove={pointerMove} onpointerup={pointerUp} onpointercancel={() => drag = null} onkeydown={keyboard}></canvas>
        {#if !scene.meshes.length}<p class="map-empty">Геометрия не извлечена. Точки показывают только расположение объектов в общей системе координат.</p>{/if}
        <p class="map-help">Перетаскивайте схему и меняйте масштаб колёсиком. Светлые поверхности расположены выше. Выберите точку или объект в списке. Ctrl + щелчок — выделить несколько точек для фильтра.</p>
      </section>
      <section class="object-panel" aria-label="Найденные объекты"><div class="list-heading"><h2>Объекты</h2><span>{visibleObjects.length} из {objects.length}</span></div>{#if filterSelection.length}<button class="clear-selection" onclick={() => { pickedKeys = []; selectedKey = ""; }}>Снять выделение · {filterSelection.length}</button>{/if}<label class="search">Поиск объекта<input type="search" bind:value={query} placeholder="Русское или английское название" /></label>
        {#if !objects.length}<p class="empty">В этом чтении объекты не определены. Это не означает, что на миссии нет предметов.</p>{:else if !visibleObjects.length}<p class="empty">Нет объектов с выбранными фильтрами. Измените поиск, категории или диапазон высоты.</p>{:else}<ul class="object-list">{#each visibleObjects.slice(0, listLimit) as object, objectIndex (object.key)}<li class="selectable-object"><input type="checkbox" aria-label={`Выбрать для фильтра: ${object.label || objectKindLabel(object.kind)} · объект ${objectIndex + 1}`} checked={pickedKeys.includes(object.key)} onchange={() => togglePicked(object.key)} /><button class:selected={object.key === selectedKey} onclick={() => selectObject(object)}><i style:background={filterColor(object, customIndex)}></i><span><strong>{object.label || objectKindLabel(object.kind)}</strong>{#if object.nameEn && object.nameEn !== object.label}<small>{object.nameEn}</small>{/if}<small>{objectKindLabel(object.kind)} · Высота {object.position[1].toFixed(1)}{object.positionFresh === false ? " · прежние координаты" : ""}</small></span></button></li>{/each}</ul>{#if visibleObjects.length > listLimit}<button class="more" onclick={() => listLimit += 100}>Показать ещё {Math.min(100, visibleObjects.length - listLimit)}</button>{/if}{/if}
      </section>
    </div>
    {#if selected}<section class="object-detail" aria-label="Выбранный объект"><div><h2>{selected.label || objectKindLabel(selected.kind)}</h2>{#if selected.nameEn && selected.nameEn !== selected.label}<p>{selected.nameEn}</p>{/if}<p>Доступность не подтверждена. Объект может сохраняться в памяти после изменения его состояния.</p>{#if selected.positionFresh === false}<p>Свежие координаты пока не прочитаны. Показано последнее положение.</p>{/if}</div><div class="selected-actions"><button onclick={() => selectObject(selected!)}>Центрировать на объекте</button>{#if scene.source === "live" && selected.kind === "avatar"}<button disabled={!status?.tracking} onclick={() => startFollowing(selected!)}>Следить за игроком</button>{/if}</div><dl><div><dt>Координаты</dt><dd>X {selected.position[0].toFixed(2)} · Y {selected.position[1].toFixed(2)} · Z {selected.position[2].toFixed(2)}</dd></div><div><dt>Тип</dt><dd>{objectKindLabel(selected.kind)}</dd></div>{#each selected.details as detail}<div><dt>{detail.label}</dt><dd>{detail.value}</dd></div>{/each}</dl><details><summary>Данные для исследования</summary><p>{selected.typeNames.join(" → ") || "Название класса не определено"}</p>{#if selected.itemPath}<code>{selected.itemPath}</code>{/if}</details></section>{/if}
    {#if difference && previousScene}<details class="comparison"><summary>Изменения относительно {missionTime(previousScene.capturedAt)} · Новых {difference.added.length} · Больше не видны {difference.absent.length}</summary><p>Сравниваются тип, адрес и положение в двух чтениях. Это различие выборок, а не журнал событий миссии.</p><div class="comparison-lists"><div><h3>Появились в выборке</h3>{#if !difference.added.length}<p>Нет новых записей.</p>{:else}<ul>{#each difference.added.slice(0, 30) as object}<li>{object.label || objectKindLabel(object.kind)}</li>{/each}</ul>{#if difference.added.length > 30}<p>И ещё {difference.added.length - 30}.</p>{/if}{/if}</div><div><h3>Больше не видны в выборке</h3>{#if !difference.absent.length}<p>Все прежние записи остаются.</p>{:else}<ul>{#each difference.absent.slice(0, 30) as object}<li>{object.label || objectKindLabel(object.kind)}</li>{/each}</ul>{#if difference.absent.length > 30}<p>И ещё {difference.absent.length - 30}.</p>{/if}{/if}</div></div></details>{/if}
    <details class="export"><summary>Сохранить результат для анализа</summary><p>JSON содержит разобранные объекты и геометрию. OBJ содержит геометрию для просмотра в 3D-редакторе.</p><div class="export-buttons"><button disabled={exporting || status?.busy} onclick={() => void exportScene("json")}>Сохранить JSON</button><button disabled={exporting || status?.busy || !scene.meshes.length} onclick={() => void exportScene("obj")}>Сохранить OBJ</button></div>{#if exportPath}<p role="status">Результат сохранён: <code>{exportPath}</code></p>{/if}<p class="muted">Прочитано {missionBytes(scene.stats.scannedBytes)} · Геометрических частей: {scene.stats.meshCount} · Полигонов: {scene.stats.faceCount.toLocaleString("ru-RU")}</p></details>
  {:else if !status?.busy}<div class="initial"><h2>Запустите карту на миссии</h2><p>После первого чтения схема будет обновляться во время игры. Для разбора прошлой миссии выберите сохранённый снимок.</p></div>{/if}
  <details class="recording-disclosure" ontoggle={(event) => { if (!(event.currentTarget as HTMLDetailsElement).open) void refreshArchives(); }}><summary>Запись для исследования после миссии</summary><div><MemoryRecording /></div></details>
</section>

<style>
  .object-list .selectable-object { display:flex; align-items:flex-start; gap:.2rem; }
  .selectable-object > input { flex-shrink:0; margin-top:1rem; cursor:pointer; }
  .object-list .selectable-object > button { flex:1; width:auto; }
  .clear-selection { margin-top:.7rem; }

  .mission { padding:1.5rem; color:var(--text); max-width:1700px; margin:0 auto; min-width:0; } h2 { font-size:1rem; margin:0; } h3 { font-size:.9rem; } p { margin:.4rem 0; line-height:1.5; color:var(--text-muted); font-size:.86rem; } .source-panel,.object-detail,.initial { background:var(--surface); border:1px solid var(--border); border-radius:.75rem; padding:1rem; } .source-options,.read-actions,.capture,.filters,.height-controls,.map-tools,.export-buttons { display:flex; align-items:center; gap:.75rem; flex-wrap:wrap; } .source-options { align-items:end; } label { display:flex; flex-direction:column; gap:.35rem; font-size:.78rem; color:var(--text-muted); min-width:0; } select,input[type="search"] { color:var(--text); background:var(--bg); border:1px solid var(--border); padding:.55rem .65rem; border-radius:.4rem; min-width:0; width:100%; font:inherit; } select { max-width:100%; } .archive-choice { flex:1; min-width:180px; max-width:420px; } button { font:inherit; font-size:.8rem; color:var(--text); background:var(--surface); border:1px solid var(--border); border-radius:.4rem; padding:.55rem .75rem; cursor:pointer; } button:hover { border-color:var(--text-muted); } button:disabled { opacity:.5; cursor:default; } button.primary { background:#397c67; border-color:#529b84; color:#fff; font-weight:600; } button:focus-visible,select:focus-visible,input:focus-visible,summary:focus-visible,canvas:focus-visible { outline:2px solid #e4bb75; outline-offset:3px; } .read-actions { margin-top:.9rem; } .read-actions p { flex:1; min-width:180px; } .progress { display:flex; flex-wrap:wrap; align-items:center; gap:.75rem; margin-top:1rem; font-size:.83rem; } progress { max-width:180px; height:7px; accent-color:#80cbae; } .error { color:#f1b4b4; background:#a94d4d15; border:1px solid #a94d4d50; border-radius:.5rem; padding:.75rem; margin:.75rem 0; display:flex; gap:.75rem; align-items:center; justify-content:space-between; overflow-wrap:anywhere; } .capture { margin:1.25rem 0 .5rem; font-size:.8rem; color:var(--text-muted); } .capture strong { color:var(--text); } .partial { color:#e1b874; } .notice { background:#b9904010; border-left:2px solid #ac8e51; padding:.65rem .85rem; } details { font-size:.83rem; border:1px solid var(--border); border-radius:.55rem; margin:.85rem 0; padding:.75rem .9rem; min-width:0; } summary { cursor:pointer; color:var(--text); line-height:1.5; } details p,li { overflow-wrap:anywhere; } details ul { color:var(--text-muted); line-height:1.6; padding-left:1.3rem; } .filters { margin:1.1rem 0 .75rem; gap:.55rem; } .filters label { display:flex; flex-direction:row; align-items:center; gap:.4rem; background:var(--surface); border:1px solid var(--border); border-radius:2rem; padding:.5rem .7rem; cursor:pointer; color:var(--text); } input[type="checkbox"] { accent-color:#72bda0; } i { display:inline-block; width:7px; height:7px; border-radius:50%; flex-shrink:0; } .filters span { color:var(--text-muted); font-size:.73rem; } .height-controls { margin:.75rem 0; min-height:35px; } .height-controls .slice { flex-direction:row; align-items:center; } .height-range { width:220px; } .height-range input { accent-color:#83baa6; } .muted { color:var(--text-muted); font-size:.78rem; } .explorer { display:grid; grid-template-columns:minmax(0,1fr) 310px; gap:1rem; align-items:start; } .map-panel { border:1px solid var(--border); border-radius:.65rem; overflow:hidden; min-width:0; background:#111a23; } .map-tools { justify-content:space-between; padding:.65rem .75rem; border-bottom:1px solid #314350; background:var(--surface); font-size:.77rem; } .map-tools > div { display:flex; gap:.35rem; } .map-tools button { padding:.35rem .6rem; } canvas { display:block; width:100%; height:520px; touch-action:none; cursor:grab; } canvas:active { cursor:grabbing; } .map-help,.map-empty { padding:.6rem .8rem; margin:0; color:#afc0ce; font-size:.76rem; } .map-empty { color:#e3c394; } .object-panel { min-width:0; background:var(--surface); border:1px solid var(--border); border-radius:.65rem; padding:.85rem; } .list-heading { display:flex; justify-content:space-between; align-items:center; gap:1rem; } .list-heading span { font-size:.76rem; color:var(--text-muted); } .search { margin:.75rem 0; } .object-list { list-style:none; padding:0; margin:0; max-height:475px; overflow:auto; } .object-list li { margin:.25rem 0; } .object-list button { width:100%; display:flex; align-items:flex-start; gap:.6rem; text-align:left; background:transparent; border-color:transparent; padding:.65rem .5rem; min-width:0; } .object-list button:hover { background:#8ab8aa10; } .object-list button.selected { background:#8ab8aa18; border-color:#74a18f77; } .object-list i { margin-top:.35rem; } .object-list span { min-width:0; } .object-list strong,.object-list small { display:block; overflow-wrap:anywhere; } .object-list strong { font-size:.82rem; font-weight:600; line-height:1.45; } .object-list small { margin-top:.2rem; font-size:.72rem; color:var(--text-muted); } .empty { padding:1rem .25rem; } .more { width:100%; margin-top:.65rem; } .object-detail { margin-top:1rem; display:flex; gap:.8rem; flex-wrap:wrap; align-items:start; } .object-detail > div { flex:1; min-width:180px; } .selected-actions { display:flex; gap:.5rem; flex-wrap:wrap; justify-content:flex-end; } .follow-status { display:flex; flex-wrap:wrap; align-items:center; gap:.7rem; color:var(--text); } .object-detail dl { flex-basis:100%; display:flex; flex-wrap:wrap; gap:1rem 2rem; margin:.25rem 0; font-size:.8rem; } dt { color:var(--text-muted); margin-bottom:.3rem; } dd { margin:0; overflow-wrap:anywhere; } .object-detail details { flex-basis:100%; margin:.25rem 0 0; } code { font-size:.75rem; overflow-wrap:anywhere; white-space:pre-wrap; } .comparison-lists { display:grid; grid-template-columns:1fr 1fr; gap:1rem; } .export-buttons { margin:.75rem 0; } .recording-disclosure { margin-top:1.25rem; } .recording-disclosure > div { margin-top:.8rem; } .initial { padding:2rem; margin-top:1rem; text-align:center; } .initial p { max-width:520px; margin:.7rem auto; }
  .mission { padding:.25rem 0 1rem; }
  .source-panel { display:flex; align-items:center; flex-wrap:wrap; gap:.45rem .8rem; padding:.65rem .8rem; }
  .source-options { align-items:center; gap:.6rem; }
  .source-options > label { flex-direction:row; align-items:center; gap:.5rem; }
  .source-options select { padding:.4rem .55rem; width:auto; max-width:360px; }
  .read-actions { margin:0; }
  .read-actions p { margin:0; font-size:.75rem; }
  .capture { margin:0 0 0 auto; gap:.35rem .65rem; font-size:.74rem; }
  .disk-note { flex-basis:100%; margin:0; font-size:.7rem; }
  .progress { flex-basis:100%; margin:.25rem 0 0; }
  .warnings { border:0; padding:.35rem 0; margin:.3rem 0; font-size:.75rem; color:var(--text-muted); }
  .warnings summary { color:var(--text-muted); }
  .display-controls { display:flex; align-items:center; justify-content:space-between; gap:.6rem 1rem; flex-wrap:wrap; margin:.2rem 0 .55rem; }
  .filters { margin:0; gap:.4rem; }
  .filters label { padding:.4rem .6rem; }
  .height-controls { margin:0; min-height:30px; }
  .map-tools { background:#1c2a35; color:#dfebf2; padding:.5rem .7rem; }
  .map-tools button { background:#273d4b; border-color:#567386; color:#f4f8fb; }
  .map-tools button:hover { background:#355365; border-color:#8baaba; }
  canvas { height:500px; }
  .map-help { padding:.5rem .75rem; }
  @media(max-width:1000px) { .explorer { grid-template-columns:minmax(0,1fr) 270px; } .mission { padding:1rem; } canvas { height:470px; } }
  @media(max-width:760px) { .explorer { grid-template-columns:1fr; } canvas { height:400px; } .object-list { max-height:320px; } .source-options > label { flex:1 1 160px; max-width:none; } .read-actions > button { width:100%; } .capture { gap:.4rem .7rem; } .comparison-lists { grid-template-columns:1fr; } .error { flex-wrap:wrap; } .map-tools > span { flex-basis:100%; } .height-range { flex:1; min-width:160px; } }
</style>
