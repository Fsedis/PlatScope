<script lang="ts">
  import { onMount } from "svelte";
  import PersonalGoalImage from "./PersonalGoalImage.svelte";
  import SquadEquipmentBuild from "./SquadEquipmentBuild.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { loadoutText, partName, equipmentMatchesSearch, savedBuildMatchesSearch, type SquadView } from "./squad";

  let view: SquadView | null = null;
  let loading = true;
  let busy = false;
  let error = "";
  let notice = "";
  let mode: "squad" | "saved" | "configuration" = "squad";
  let configurationKey = "";
  let configurationQuery = "";
  let configurationCategory = "";
  let player = "";
  let equipmentKey = "";
  let savedId = "";
  let savedEquipmentKey = "";
  let query = "";
  let editing = false;
  let editingId = "";
  let title = "";
  let note = "";
  let deleting = false;
  let deletingId = "";
  let revision = 0;
  let mounted = true;
  let polling = false;
  $: members = view?.members ?? [];
  $: saved = (view?.saved ?? []).filter(b => savedBuildMatchesSearch(b, query));
  $: member = members.find(m => m.name === player) ?? members[0] ?? null;
  $: selectedBuild = saved.find(b => b.id === savedId) ?? saved[0] ?? null;
  $: if (editing && selectedBuild?.id !== editingId) editing = false;
  $: if (deleting && selectedBuild?.id !== deletingId) deleting = false;
  $: configurationCategories = [...new Set((view?.configurations ?? []).map(e => e.category))];
  $: configurations = (view?.configurations ?? []).filter(e => (!configurationCategory || e.category === configurationCategory) && equipmentMatchesSearch(e, configurationQuery));
  $: configuration = configurations.find(e => e.key === configurationKey) ?? configurations[0] ?? null;
  $: loadout = mode === "configuration" ? (configuration ? [configuration] : []) : mode === "saved" ? selectedBuild?.equipment ?? [] : member?.equipment ?? [];
  $: equipment = loadout.find(e => e.key === (mode === "saved" ? savedEquipmentKey : equipmentKey)) ?? loadout[0] ?? null;
  $: sourcePlayer = mode === "configuration" ? "Владелец не подтверждён" : mode === "saved" ? selectedBuild?.player ?? "" : member?.name ?? "";
  $: capturedAt = mode === "configuration" ? view?.configurationsAt : mode === "saved" ? selectedBuild?.capturedAt : member?.capturedAt;
  $: captureLabel = !view?.enabled ? "Сбор приостановлен" : !view?.running ? "Warframe не запущен" : view.scanning ? "Читаем экипировку…" : "Ожидаем события отряда";

  async function refresh(): Promise<void> {
    if (busy || polling) return;
    polling = true;
    const request = revision;
    try { const next = await invoke<SquadView>("squad_status"); if (mounted && request === revision) { view = next; error = ""; } }
    catch { if (mounted && request === revision) error = "Не удалось загрузить отряд и сохранённые билды. Повторите попытку."; }
    finally { polling = false; loading = false; }
  }
  async function act(command: string, args?: Record<string, unknown>): Promise<boolean> {
    if (busy) return false;
    busy = true; revision++; error = ""; notice = "";
    try { const next = await invoke<SquadView>(command, args); if (mounted) view = next; return mounted; }
    catch (reason) { if (mounted) error = typeof reason === "string" ? reason : "Не удалось выполнить действие. Повторите попытку."; return false; }
    finally { busy = false; }
  }
  function selectMode(next: "squad" | "saved" | "configuration") { mode = next; editing = false; deleting = false; notice = ""; }
  async function save() {
    if (!equipment) return;
    const fromConfiguration = mode === "configuration";
    if (await act("squad_save_build", { player: sourcePlayer, equipmentKey: fromConfiguration ? equipment.key : null, capturedAt, source: fromConfiguration ? "configuration" : null })) notice = fromConfiguration ? "Конфигурация сохранена." : "Вся полученная экипировка сопартийца сохранена одним билдом. Она останется в коллекции после выхода игрока.";
  }
  function edit() { if (selectedBuild) { editingId = selectedBuild.id; title = selectedBuild.title; note = selectedBuild.note; editing = true; } }
  async function saveEdit() {
    if (selectedBuild && await act("squad_edit_build", { id: selectedBuild.id, title, note })) { editing = false; notice = "Название и заметка сохранены."; }
  }
  async function remove() {
    if (selectedBuild && await act("squad_delete_build", { id: selectedBuild.id })) deleting = false;
  }
  async function copy() {
    if (!equipment) return;
    try { await navigator.clipboard.writeText(exportText()); notice = "Список скопирован — можно сверяться с ним в Арсенале."; }
    catch { error = "Не удалось скопировать. Сохраните список в файл."; }
  }
  async function download() {
    if (!equipment) return;
    const text = exportText();
    try { const path = await invoke<string>("squad_export_text", { text }); notice = `Список сохранён: ${path}`; }
    catch { error = "Не удалось сохранить список. Проверьте свободное место и повторите."; }
  }
  function exportText(): string {
    return loadoutText(loadout, sourcePlayer, mode === "saved" ? selectedBuild?.title ?? "Билд" : mode === "squad" ? `Экипировка ${sourcePlayer}` : partName(equipment!.item), mode === "saved" ? selectedBuild?.note : "");
  }
  const date = (value: string | null | undefined) => value ? new Date(value).toLocaleString("ru-RU") : "";
  const memberStatus = (status: string) => status === "matched" ? "Экипировка получена" : status === "ambiguous" ? "Принадлежность не определена" : status === "unavailable" ? "Экипировка не найдена" : "Ожидаем экипировку";
  onMount(() => { void refresh(); const timer = setInterval(() => void refresh(), 2000); return () => { mounted = false; revision++; clearInterval(timer); }; });
</script>

<section class="squad-screen" aria-label="Отряд и билды">
  <div class="modes" role="group" aria-label="Источник билдов">
    <button class:active={mode === "squad"} aria-pressed={mode === "squad"} onclick={() => selectMode("squad")}>Текущий отряд</button>
    <button class:active={mode === "saved"} aria-pressed={mode === "saved"} onclick={() => selectMode("saved")}>Сохранённые билды{#if view?.saved.length}<span class="count">{view.saved.length}</span>{/if}</button>
    <button class:active={mode === "configuration"} aria-pressed={mode === "configuration"} onclick={() => selectMode("configuration")}>Конфигурации из памяти</button>
  </div>

  {#if error}<div class="message error" role="alert"><span>{error}</span><button disabled={busy} onclick={() => void refresh()}>Повторить</button></div>{/if}
  {#if notice}<p class="message notice" role="status">{notice}</p>{/if}
  {#if loading && !view}
    <div class="loading" role="status"><span class="loading-mark" aria-hidden="true"></span><div><strong>Загружаем отряд и билды</strong><p>Получаем экипировку и вашу коллекцию.</p></div></div>
  {:else if view}
    {#if mode === "squad"}
      <div class="capture-bar">
        <span class="capture-state" class:live={view.running && view.enabled}><i aria-hidden="true"></i>{captureLabel}</span>
        <div class="capture-actions"><label class="toggle"><input type="checkbox" checked={view.enabled} disabled={busy} onchange={e => void act("squad_set_enabled", { enabled: e.currentTarget.checked })} />Собирать экипировку</label><button disabled={busy || view.scanning || !view.running || !view.enabled || !members.length} onclick={() => void act("squad_refresh")}>Прочитать снова</button></div>
      </div>
      {#if view.error}<p class="message error" role="alert">{view.error}</p>{/if}
    {:else if mode === "configuration"}
      <div class="capture-bar"><p>{view.running ? "Откройте Арсенал, чтобы прочитать конфигурации снаряжения." : "Запустите Warframe для чтения конфигураций."}</p><button class="primary" disabled={busy || view.readingConfigurations || !view.running} onclick={() => void act("squad_read_configurations")}>{busy || view.readingConfigurations ? "Читаем конфигурации…" : "Прочитать конфигурации"}</button></div>
      {#if view.error}<p class="message error" role="alert">{view.error}</p>{/if}
    {/if}

    <div class="workspace">
      <aside class="roster" aria-label={mode === "squad" ? "Участники отряда" : mode === "saved" ? "Коллекция билдов" : "Конфигурации снаряжения"}>
        <div class="roster-heading"><h2>{mode === "squad" ? "Сопартийцы" : mode === "saved" ? "Моя коллекция" : "Снаряжение"}</h2><span class="count">{mode === "squad" ? members.length : mode === "saved" ? saved.length : configurations.length}</span></div>
        {#if mode === "squad"}
          {#if !members.length}<div class="sidebar-empty"><strong>Отряд пока пуст</strong><p>Войдите в отряд в Warframe. Участники появятся после событий игры.</p></div>{/if}
          <div class="roster-list">
            {#each members as m (m.name)}
              <button class="roster-row member-row" class:selected={member?.name === m.name} aria-pressed={member?.name === m.name} onclick={() => { player = m.name; equipmentKey = ""; notice = ""; }}>
                <span class="row-main"><PersonalGoalImage src={m.equipment.find(e => e.category === "Варфрейм")?.item.imageUrl ?? null} /><span class="row-copy"><strong>{m.name}</strong><small>{m.platform}{m.isHost ? " · Хост" : ""}{m.masteryRank !== null ? ` · МР ${m.masteryRank}` : ""}</small></span></span>
                <span class="member-state" class:matched={m.status === "matched"}><i aria-hidden="true"></i>{memberStatus(m.status)}</span>
              </button>
            {/each}
          </div>
        {:else if mode === "saved"}
          <label class="search"><span>Найти билд</span><input type="search" bind:value={query} placeholder="Игрок, предмет или мод" /></label>
          {#if !saved.length}<div class="sidebar-empty"><strong>{query ? "Билды не найдены" : "Коллекция пока пуста"}</strong><p>{query ? "Попробуйте другое название или имя игрока." : "Выберите сопартийца и сохраните его экипировку."}</p></div>{/if}
          <div class="roster-list">
            {#each saved as b (b.id)}
              <button class="roster-row" class:selected={selectedBuild?.id === b.id} aria-pressed={selectedBuild?.id === b.id} onclick={() => { savedId = b.id; savedEquipmentKey = ""; editing = false; deleting = false; notice = ""; }}>
                <strong>{b.title}</strong><small>{b.player} · Предметов: {b.equipment.length}</small>
                <span class="loadout-preview" aria-hidden="true">{#each b.equipment.slice(0, 4) as e}<PersonalGoalImage src={e.item.imageUrl ?? null} />{/each}{#if b.equipment.length > 4}<span>+{b.equipment.length - 4}</span>{/if}</span>
                <small class="saved-date">{date(b.savedAt)}</small>
              </button>
            {/each}
          </div>
        {:else}
          <label class="search"><span>Категория</span><select bind:value={configurationCategory}><option value="">Всё снаряжение</option>{#each configurationCategories as category}<option value={category}>{category}</option>{/each}</select></label>
          <label class="search"><span>Найти предмет или мод</span><input type="search" bind:value={configurationQuery} placeholder="Название на русском или английском" /></label>
          {#if !configurations.length}<div class="sidebar-empty"><strong>{configurationQuery || configurationCategory ? "Ничего не найдено" : "Конфигураций пока нет"}</strong><p>{configurationQuery || configurationCategory ? "Измените запрос или категорию." : view.configurationsAt ? "Откройте Арсенал и повторите чтение." : "Нажмите «Прочитать конфигурации»."}</p></div>{/if}
          <div class="roster-list">
            {#each configurations as e (e.key)}
              <button class="roster-row" class:selected={equipment?.key === e.key} aria-pressed={equipment?.key === e.key} onclick={() => { configurationKey = e.key; notice = ""; }}>
                <span class="row-main"><PersonalGoalImage src={e.item.imageUrl ?? null} /><span class="row-copy"><strong>{partName(e.item)}</strong>{#if e.item.nameEn && e.item.nameEn !== e.item.name}<small lang="en">{e.item.nameEn}</small>{/if}</span></span>
                <small>{e.category}{e.configuration ? ` · Конфигурация ${e.configuration}` : ""}</small>
                {#if e.item.fingerprint}<small>{e.item.fingerprint.weaponName || e.item.fingerprint.weaponNameEn || "Оружие не определено"}</small>{#if e.item.fingerprint.weaponName && e.item.fingerprint.weaponNameEn && e.item.fingerprint.weaponNameEn !== e.item.fingerprint.weaponName}<small lang="en">{e.item.fingerprint.weaponNameEn}</small>{/if}{/if}
              </button>
            {/each}
          </div>
        {/if}
      </aside>

      <div class="detail">
        {#if (mode === "squad" && member) || (mode === "saved" && selectedBuild) || (mode === "configuration" && equipment)}
          <header class="loadout-header">
            <div class="loadout-title"><p class="eyebrow">{mode === "squad" ? "Экипировка сопартийца" : mode === "saved" ? "Сохранённый билд" : "Конфигурация из памяти"}</p>
              <h2>{mode === "squad" ? member?.name : mode === "saved" ? selectedBuild?.title : equipment?.configuration ? `Конфигурация ${equipment.configuration}` : "Запись снаряжения"}</h2>
              <p class="header-meta">{mode === "saved" ? `${selectedBuild?.player} · ` : ""}{mode === "configuration" ? "Владелец и актуальность не подтверждены" : `Предметов: ${loadout.length}`}{capturedAt ? ` · ${date(capturedAt)}` : ""}</p>
            </div>
            <div class="header-actions">
              {#if mode === "squad"}<button class="primary" disabled={busy || member?.status !== "matched" || !loadout.length || !capturedAt} onclick={() => void save()}>Сохранить экипировку</button>
              {:else if mode === "configuration"}<button class="primary" disabled={busy || !equipment || !capturedAt} onclick={() => void save()}>Сохранить конфигурацию</button>{/if}
              {#if equipment}<div class="export-actions"><button onclick={() => void copy()}>Скопировать список</button><button onclick={() => void download()}>Сохранить список в файл</button></div>{/if}
            </div>
          </header>
        {/if}

        {#if mode === "saved" && selectedBuild}
          {#key selectedBuild.id}
            <details class="notes" open={editing}>
              <summary>Название и заметка{selectedBuild.note ? " · есть заметка" : ""}</summary>
              {#if editing}<form onsubmit={e => { e.preventDefault(); void saveEdit(); }}>
                <label>Название билда<input bind:value={title} required maxlength="100" /></label><label>Заметка<textarea bind:value={note} maxlength="3000" rows="3" placeholder="Для какой миссии подходит, что изменить…"></textarea></label>
                <div class="actions"><button class="primary" disabled={busy}>Сохранить изменения</button><button type="button" disabled={busy} onclick={() => editing = false}>Отмена</button></div>
              </form>{:else}<p>{selectedBuild.note || "Добавьте, чем понравился билд и где его попробовать."}</p><button onclick={edit}>Изменить название и заметку</button>{/if}
            </details>
          {/key}
        {/if}

        {#if mode !== "configuration" && loadout.length}
          <div class="equipment-selector" role="group" aria-label="Предметы экипировки">
            {#each loadout as e (e.key)}
              <button class="equipment-choice" class:selected={equipment?.key === e.key} aria-pressed={equipment?.key === e.key} onclick={() => { if (mode === "saved") savedEquipmentKey = e.key; else equipmentKey = e.key; notice = ""; }}>
                <PersonalGoalImage src={e.item.imageUrl ?? null} /><span><small class="category">{e.category}</small><strong>{partName(e.item)}</strong>{#if e.item.nameEn && e.item.nameEn !== e.item.name}<small lang="en">{e.item.nameEn}</small>{/if}</span>
              </button>
            {/each}
          </div>
        {/if}

        {#if equipment}
          {#key `${mode}:${mode === "saved" ? selectedBuild?.id : sourcePlayer}:${equipment.key}`}
            <SquadEquipmentBuild {equipment} player={sourcePlayer} {capturedAt} />
          {/key}
        {:else}
          <div class="main-empty">
            <div class="empty-emblem" aria-hidden="true"><svg viewBox="0 0 64 64" fill="none"><path d="m32 8 22 12v24L32 56 10 44V20Z M10 20l22 12 22-12 M32 32v24" stroke="currentColor" stroke-width="1.3"/><path d="m21 26 11-6 11 6" stroke="currentColor" stroke-width="1.3"/></svg></div>
            {#if mode === "squad"}
              <h2>{member?.status === "ambiguous" ? "Принадлежность не определена" : member ? "Экипировка ещё не получена" : "Здесь появится экипировка отряда"}</h2>
              <p>{member?.status === "ambiguous" ? "Найдено несколько подходящих снимков. Показывать один из них как билд этого игрока пока нельзя." : !view.running ? "Запустите Warframe и войдите в отряд. Сохранённые билды доступны и без игры." : member ? "Попробуйте прочитать экипировку снова после входа игрока в миссию." : "Выберите сопартийца после его появления в отряде, чтобы рассмотреть снаряжение и сохранить комплект."}</p>
              {#if !members.length && view.running}<small>Если игрок вошёл до запуска PlatScope, его снимок может уже отсутствовать в памяти.</small>{/if}
            {:else if mode === "saved"}<h2>{query ? "Нет подходящих билдов" : "Ваша коллекция экипировки"}</h2><p>{query ? "Измените поиск, чтобы найти сохранённого игрока, предмет или мод." : "Сохраните экипировку сопартийца — она останется здесь после его выхода и перезапуска приложения."}</p>
            {:else}<h2>{view.readingConfigurations ? "Читаем конфигурации…" : "Выберите конфигурацию"}</h2><p>Здесь появятся предмет и его улучшения. Владелец и активный вариант конфигураций из памяти не определены.</p>{/if}
          </div>
        {/if}

        {#if mode === "saved" && selectedBuild}
          <footer class="collection-footer"><span>Билд хранится на этом компьютере</span><button class="delete" disabled={busy} onclick={() => { deletingId = selectedBuild?.id ?? ""; deleting = true; }}>Удалить билд</button></footer>
          {#if deleting}<div class="delete-confirm" role="group" aria-label="Подтверждение удаления"><p>Удалить «{selectedBuild.title}» из коллекции?</p><div class="actions"><button class="delete" disabled={busy} onclick={() => void remove()}>Удалить из коллекции</button><button disabled={busy} onclick={() => deleting = false}>Оставить</button></div></div>{/if}
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .squad-screen { display:grid; gap:.9rem; min-width:0; color:var(--text); }
  button,input,textarea,select { font:inherit; } button { min-width:0; padding:.55rem .8rem; border:1px solid var(--border); border-radius:.5rem; background:var(--surface-1); color:var(--text); font-size:.78rem; font-weight:600; cursor:pointer; box-shadow:none; } button:hover:not(:disabled) { border-color:var(--border-strong); background:var(--surface-hover); } button:disabled { opacity:.55; cursor:default; } button:focus-visible,input:focus-visible,textarea:focus-visible,select:focus-visible,summary:focus-visible { outline:2px solid var(--accent); outline-offset:3px; } button.primary { color:var(--surface-1); background:var(--accent); border-color:var(--accent); } button.primary:hover:not(:disabled) { background:var(--accent-strong); border-color:var(--accent-strong); }
  .modes { display:flex; gap:.25rem; border:1px solid var(--border); border-radius:.7rem; padding:.3rem; background:var(--surface-1); box-shadow:var(--shadow-sm); } .modes button { flex:1; border-color:transparent; background:transparent; display:flex; align-items:center; justify-content:center; gap:.5rem; min-height:2.6rem; } .modes button.active { border-color:var(--accent); background:var(--accent-soft); color:var(--accent-strong); }
  .count { display:inline-flex; justify-content:center; align-items:center; font-size:.67rem; line-height:1.5; padding:.05rem .4rem; border-radius:.35rem; background:var(--surface-3); color:var(--text-muted); font-variant-numeric:tabular-nums; }
  .capture-bar { display:flex; gap:.75rem; flex-wrap:wrap; align-items:center; justify-content:space-between; padding:.1rem .1rem .2rem; font-size:.78rem; color:var(--text-muted); } .capture-bar p { margin:0; } .capture-actions { display:flex; gap:1rem; align-items:center; flex-wrap:wrap; } .capture-bar button { padding:.4rem .7rem; } .capture-state,.toggle { display:flex; gap:.5rem; align-items:center; } .toggle { cursor:pointer; } input[type="checkbox"] { accent-color:var(--accent); width:1rem; height:1rem; margin:0; }
  .capture-state i,.member-state i { width:.4rem; height:.4rem; border-radius:50%; flex:none; background:var(--text-muted); } .capture-state.live i,.member-state.matched i { background:var(--success); }
  .workspace { display:grid; grid-template-columns:260px minmax(0,1fr); gap:1.1rem; align-items:start; min-width:0; }
  .roster { min-width:0; display:grid; gap:.8rem; padding:.85rem; border:1px solid var(--border); border-radius:.85rem; background:var(--surface-1); box-shadow:var(--shadow-sm); } .roster-heading { display:flex; align-items:center; gap:.5rem; justify-content:space-between; padding:.1rem .15rem; } .roster-heading h2 { font-size:.84rem; margin:0; }
  .roster-list { display:grid; gap:.5rem; max-height:70vh; overflow-y:auto; padding:.2rem; margin:-.2rem; } .roster-row { width:100%; display:grid; gap:.4rem; text-align:left; padding:.65rem; overflow-wrap:anywhere; } .roster-row.selected { background:var(--accent-soft); border-color:var(--accent); box-shadow:inset 3px 0 0 var(--accent); } .roster-row strong { font-size:.83rem; line-height:1.35; } small { display:block; color:var(--text-muted); font-size:.7rem; line-height:1.4; font-weight:400; }
  .row-main { display:flex; align-items:center; gap:.6rem; min-width:0; } .row-copy { display:grid; gap:.2rem; min-width:0; } .member-state { display:flex; gap:.35rem; align-items:center; font-size:.65rem; color:var(--text-muted); font-weight:400; } .member-state.matched { color:var(--success); }
  .loadout-preview { display:flex; gap:.15rem; align-items:center; flex-wrap:wrap; } .loadout-preview :global(.artwork) { width:2.1rem; height:2.1rem; border-radius:.3rem; } .loadout-preview>span { color:var(--text-muted); font-size:.68rem; } .saved-date { font-size:.66rem; }
  .search { display:grid; gap:.35rem; font-size:.72rem; color:var(--text-muted); } input:not([type="checkbox"]),select,textarea { display:block; width:100%; min-width:0; box-sizing:border-box; border:1px solid var(--border); padding:.6rem; border-radius:.45rem; background:var(--surface-1); color:var(--text); font-size:.8rem; } .search input,.search select { font-size:.75rem; } textarea { resize:vertical; }
  .detail { min-width:0; display:grid; gap:1rem; padding:1.1rem; border:1px solid var(--border); border-radius:.85rem; background:var(--surface-1); box-shadow:var(--shadow-sm); }
  .loadout-header { display:flex; flex-wrap:wrap; gap:.8rem; align-items:center; justify-content:space-between; } .loadout-title { min-width:0; flex:1 1 14rem; overflow-wrap:anywhere; } .loadout-title h2 { font-size:1.15rem; line-height:1.3; margin:.2rem 0; } .eyebrow { font-size:.65rem; letter-spacing:.08em; text-transform:uppercase; font-weight:700; color:var(--accent-strong); margin:0; } .header-meta { font-size:.7rem; color:var(--text-muted); margin:.2rem 0 0; line-height:1.5; }
  .header-actions { display:flex; align-items:flex-end; flex-direction:column; gap:.4rem; max-width:100%; } .export-actions { display:flex; gap:.35rem; flex-wrap:wrap; } .export-actions button { font-size:.69rem; padding:.25rem .45rem; min-height:1.8rem; background:transparent; border-color:transparent; color:var(--accent-strong); } .export-actions button:hover { border-color:var(--border); background:var(--surface-2); }
  .equipment-selector { display:grid; grid-template-columns:repeat(auto-fit,minmax(min(100%,170px),1fr)); gap:.5rem; } .equipment-choice { display:flex; align-items:center; gap:.6rem; text-align:left; padding:.65rem; overflow-wrap:anywhere; background:var(--surface-2); } .equipment-choice>span { min-width:0; display:grid; gap:.16rem; } .equipment-choice strong { font-size:.78rem; line-height:1.3; } .equipment-choice small { font-size:.65rem; } .equipment-choice .category { font-size:.59rem; text-transform:uppercase; letter-spacing:.04em; } .equipment-choice.selected { border-color:var(--accent); background:var(--accent-soft); box-shadow:inset 0 -2px 0 var(--accent); }
  .notes { border:1px solid var(--border); background:var(--surface-2); padding:.65rem .8rem; border-radius:.5rem; font-size:.8rem; } summary { cursor:pointer; font-size:.76rem; font-weight:600; } .notes p { margin:.8rem 0; white-space:pre-wrap; overflow-wrap:anywhere; line-height:1.6; } .notes>button { font-size:.73rem; } form { display:grid; gap:.7rem; margin-top:.8rem; } form label { display:grid; gap:.35rem; font-size:.75rem; } .actions { display:flex; flex-wrap:wrap; gap:.5rem; }
  .collection-footer { display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:.7rem; border-top:1px solid var(--border); padding-top:.9rem; font-size:.7rem; color:var(--text-muted); } button.delete { border-color:var(--border); color:var(--danger); background:transparent; } button.delete:hover:not(:disabled) { border-color:var(--danger); background:var(--danger-soft); } .delete-confirm { border:1px solid var(--danger); border-radius:.5rem; padding:.8rem; font-size:.8rem; overflow-wrap:anywhere; } .delete-confirm p { margin:0 0 .7rem; }
  .sidebar-empty { padding:.65rem .15rem; color:var(--text-muted); font-size:.79rem; line-height:1.55; } .sidebar-empty strong { color:var(--text); } .sidebar-empty p { margin:.4rem 0 0; }
  .main-empty { display:grid; justify-items:center; align-content:center; gap:.8rem; text-align:center; min-height:330px; padding:2rem 1rem; color:var(--text-muted); } .main-empty h2 { font-size:1.15rem; color:var(--text); margin:0; } .main-empty p { max-width:31rem; font-size:.87rem; line-height:1.65; margin:0; } .main-empty small { max-width:28rem; } .empty-emblem { color:var(--gold); width:64px; height:64px; margin-bottom:.4rem; } svg { width:100%; height:100%; }
  .message { display:flex; align-items:center; justify-content:space-between; gap:.8rem; padding:.75rem 1rem; margin:0; border:1px solid var(--border); border-radius:.6rem; font-size:.8rem; line-height:1.5; overflow-wrap:anywhere; } .message.error { background:var(--danger-soft); color:var(--danger); } .message.notice { color:var(--success); background:var(--success-soft); } .message button { flex:none; }
  .loading { display:flex; align-items:center; gap:1rem; padding:2rem; border:1px solid var(--border); border-radius:.8rem; background:var(--surface-1); } .loading p { color:var(--text-muted); font-size:.85rem; margin:.4rem 0 0; } .loading-mark { width:1.25rem; height:1.25rem; border:2px solid var(--border); border-top-color:var(--accent); border-radius:50%; } @media (prefers-reduced-motion:no-preference) { .loading-mark { animation:turn 1s linear infinite; } } @keyframes turn { to { transform:rotate(360deg); } }
  @media (max-width:1100px) { .workspace { grid-template-columns:215px minmax(0,1fr); gap:.75rem; } .detail { padding:.85rem; } .roster { padding:.65rem; } .equipment-selector { grid-template-columns:repeat(auto-fit,minmax(min(100%,155px),1fr)); } .header-actions { align-items:flex-start; } }
  @media (max-width:780px) { .workspace { grid-template-columns:minmax(0,1fr); } .roster-list { max-height:19rem; } .modes { flex-wrap:wrap; } .modes button { flex:1 1 10rem; padding:.45rem; font-size:.74rem; } .capture-actions { justify-content:space-between; width:100%; gap:.6rem; } .detail { padding:.75rem; } .loadout-header { align-items:flex-start; } .header-actions { width:100%; } .main-empty { min-height:240px; } }
  @media (max-width:440px) { .equipment-selector { grid-template-columns:minmax(0,1fr); } .message { align-items:flex-start; flex-direction:column; } .loadout-title h2 { font-size:1.05rem; } }
</style>
