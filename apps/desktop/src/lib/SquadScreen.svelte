<script lang="ts">
  import { onMount } from "svelte";
  import SquadRiven from "./SquadRiven.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { loadoutText, partName, refinementName, shardColor, equipmentMatchesSearch, savedBuildMatchesSearch, type SquadView } from "./squad";

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
  $: mods = equipment?.upgrades.filter(p => p.kind === "mod") ?? [];
  $: arcanes = equipment?.upgrades.filter(p => p.kind === "arcane") ?? [];
  $: other = equipment?.upgrades.filter(p => !["mod", "arcane"].includes(p.kind)) ?? [];

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
    try { view = await invoke<SquadView>(command, args); return true; }
    catch (reason) { error = typeof reason === "string" ? reason : "Не удалось выполнить действие. Повторите попытку."; return false; }
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
  onMount(() => { void refresh(); const timer = setInterval(() => void refresh(), 2000); return () => { mounted = false; revision++; clearInterval(timer); }; });
</script>

<section class="squad-screen" aria-label="Отряд и билды">
  <div class="toolbar">
    <div class="modes" role="group" aria-label="Источник билдов">
      <button class:active={mode === "squad"} onclick={() => selectMode("squad")}>Текущий отряд</button>
      <button class:active={mode === "configuration"} onclick={() => selectMode("configuration")}>Конфигурации из памяти</button>
      <button class:active={mode === "saved"} onclick={() => selectMode("saved")}>Сохранённые билды {view?.saved.length ? `· ${view.saved.length}` : ""}</button>
    </div>
    {#if view && mode === "squad"}
      <label class="toggle"><input type="checkbox" checked={view.enabled} disabled={busy} onchange={e => void act("squad_set_enabled", { enabled: e.currentTarget.checked })} />Собирать экипировку</label>
    {/if}
  </div>
  {#if error}<p class="error" role="alert">{error} <button disabled={busy} onclick={() => void refresh()}>Повторить</button></p>{/if}
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
  {#if loading && !view}<p class="empty">Загружаем отряд и сохранённые билды…</p>
  {:else if view}
    {#if mode === "squad"}
      <div class="capture-status">
        <span class:live={view.running && view.enabled}><i></i>{!view.enabled ? "Сбор приостановлен" : !view.running ? "Warframe не запущен" : view.scanning ? "Читаем экипировку отряда…" : "Ожидаем события отряда"}</span>
        <button disabled={busy || view.scanning || !view.running || !view.enabled || !members.length} onclick={() => void act("squad_refresh")}>Прочитать снова</button>
      </div>
      <p class="hint">Участники определяются по журналу игры, экипировка читается из памяти Warframe. Сохранённые билды хранятся на этом компьютере.</p>
      {#if view.error}<p class="error" role="alert">{view.error}</p>{/if}
    {/if}
    {#if mode === "configuration"}
      <div class="capture-status"><p class="hint">Откройте Арсенал, затем прочитайте конфигурации. Найденные записи могут быть старыми; их владелец и активный вариант не определены.</p><button class="primary" disabled={busy || view.readingConfigurations || !view.running} onclick={() => void act("squad_read_configurations")}>{busy || view.readingConfigurations ? "Читаем конфигурации…" : "Прочитать конфигурации"}</button></div>
      {#if !view.running}<p class="hint">Запустите Warframe для чтения конфигураций.</p>{/if}
    {/if}
    <div class="workspace">
      <aside class="roster" aria-label={mode === "squad" ? "Участники отряда" : mode === "configuration" ? "Конфигурации снаряжения" : "Коллекция билдов"}>
        {#if mode === "squad"}
          {#if !members.length}<div class="empty"><strong>Сопартийцев пока нет</strong><p>Войдите в отряд в Warframe. Участники появятся здесь после сообщений игры.</p><p>Экипировка игрока, вошедшего до запуска PlatScope, может уже отсутствовать в памяти.</p></div>{/if}
          {#each members as m (m.name)}
            <button class="row" class:selected={member?.name === m.name} onclick={() => { player = m.name; equipmentKey = ""; notice = ""; }}>
              <strong>{m.name}</strong><small>{m.platform}{m.isHost ? " · Хост" : ""}{m.masteryRank !== null ? ` · МР ${m.masteryRank}` : ""}</small>
              <span>{m.status === "matched" ? "Экипировка получена" : m.status === "ambiguous" ? "Неоднозначное совпадение" : m.status === "unavailable" ? "Экипировка не найдена" : "Ожидаем экипировку"}</span>
            </button>
          {/each}
        {:else if mode === "configuration"}
          <label class="search">Категория<select bind:value={configurationCategory}><option value="">Всё снаряжение</option>{#each configurationCategories as category}<option value={category}>{category}</option>{/each}</select></label>
          <label class="search">Найти предмет или состав билда<input type="search" bind:value={configurationQuery} placeholder="Русское или английское название" /></label>
          {#if !configurations.length}<p class="empty">{configurationQuery ? "Предмет не найден. Измените запрос или категорию." : view.configurationsAt ? "Конфигурации не найдены. Откройте Арсенал и повторите чтение." : "Нажмите «Прочитать конфигурации», чтобы получить моды, ранги, осколки и моды разлома."}</p>{/if}
          {#each configurations as e (e.key)}<button class="row" class:selected={equipment?.key === e.key} onclick={() => { configurationKey = e.key; notice = ""; }}><strong>{partName(e.item)}</strong>{#if e.item.nameEn !== e.item.name}<small>{e.item.nameEn}</small>{/if}<span>{e.category}{e.configuration ? ` · Конфигурация ${e.configuration}` : ""}</span>{#if e.item.fingerprint}<small>{e.item.fingerprint.weaponName || e.item.fingerprint.weaponNameEn || "Оружие не определено"}</small>{/if}</button>{/each}
        {:else}
          <label class="search">Найти билд<input type="search" bind:value={query} placeholder="Предмет, мод, способность или игрок" /></label>
          {#if !saved.length}<div class="empty"><strong>{query ? "Билды не найдены" : "Коллекция пока пуста"}</strong><p>{query ? "Измените поисковый запрос." : "Выберите сопартийца в текущем отряде и нажмите «Сохранить экипировку»."}</p></div>{/if}
          {#each saved as b (b.id)}
            <button class="row" class:selected={selectedBuild?.id === b.id} onclick={() => { savedId = b.id; savedEquipmentKey = ""; editing = false; deleting = false; notice = ""; }}><strong>{b.title}</strong><small>{b.player} · Предметов: {b.equipment.length}</small><span>{date(b.savedAt)}</span></button>
          {/each}
        {/if}
      </aside>
      <div class="detail">
        {#if mode === "squad" && member}
          <div class="item-heading"><h2>{member.name}</h2><button class="primary" disabled={busy || member.status !== "matched" || !member.equipment.length || !member.capturedAt} onclick={() => void save()}>Сохранить экипировку</button></div>
          {#if member.status === "ambiguous"}<p class="empty">Найдено несколько подходящих вариантов. Принадлежность билда определить нельзя, поэтому он не показывается.</p>
          {:else if !member.equipment.length}<p class="empty">{view.scanning ? "Получаем экипировку…" : "Данных экипировки пока нет. Попробуйте прочитать снова после входа игрока в миссию."}</p>{/if}
          {#if member.equipment.length}<div class="equipment-tabs" role="group" aria-label="Предметы игрока">{#each member.equipment as e (e.key)}<button class:active={equipment?.key === e.key} onclick={() => { equipmentKey = e.key; notice = ""; }}><small>{e.category}</small>{partName(e.item)}</button>{/each}</div>{/if}
        {/if}
        {#if mode === "saved" && selectedBuild}
          {#if !equipment || selectedBuild.title !== partName(equipment.item)}<h2>{selectedBuild.title}</h2>{/if}
          {#if loadout.length > 1}<div class="equipment-tabs" role="group" aria-label="Сохранённая экипировка">{#each loadout as e (e.key)}<button class:active={equipment?.key === e.key} onclick={() => { savedEquipmentKey = e.key; notice = ""; }}><small>{e.category}</small>{partName(e.item)}{#if e.item.nameEn && e.item.nameEn !== e.item.name}<small lang="en">{e.item.nameEn}</small>{/if}</button>{/each}</div>{/if}
        {/if}
        {#if equipment}
          <div class="item-heading"><div><p class="eyebrow">{equipment.category}</p><h2>{partName(equipment.item)}</h2>{#if equipment.item.nameEn && equipment.item.nameEn !== equipment.item.name}<p class="english" lang="en">{equipment.item.nameEn}</p>{/if}<p class="hint">Уровень: {equipment.level ?? "неизвестен"} · Форм: {equipment.forma ?? "неизвестно"}</p></div>
            {#if mode === "configuration"}<button class="primary" disabled={busy} onclick={() => void save()}>Сохранить конфигурацию</button>{/if}</div>
          <p class="provenance">{sourcePlayer} · Снимок {date(capturedAt)}. {equipment.source === "memoryConfiguration" ? `${equipment.configuration ? `Конфигурация ${equipment.configuration}.` : "Запись инвентаря."} Владелец, активный вариант и актуальность не подтверждены. Несколько вариантов могут относиться к старым состояниям одного предмета.` : "Принадлежность сопоставлена по размеру данных; проверьте её перед использованием."}</p>
          {#if equipment.context?.focus || equipment.context?.relic}<div class="loadout-context">{#if equipment.context.focus}<p><strong>Фокус:</strong> {partName(equipment.context.focus)}{#if equipment.context.focus.nameEn !== equipment.context.focus.name}<small>{equipment.context.focus.nameEn}</small>{/if}</p>{/if}{#if equipment.context.relic}<p><strong>Реликвия:</strong> {partName(equipment.context.relic)} · {refinementName(equipment.context.refinement)}</p>{/if}</div>{/if}
          {#if equipment.category === "Варфрейм"}
            <section class="build-details"><h3>Осколки Архонта {equipment.shards != null ? `· ${equipment.shards.length}` : ""}</h3>
              {#if equipment.shards == null}<p class="hint">Данные об осколках не получены. Это не означает, что осколки не установлены.</p>{:else if !equipment.shards.length}<p class="hint">В этой записи список осколков пуст.</p>{:else}<ul>{#each equipment.shards as shard, i}<li><strong>{i+1}. {shardColor(shard.color)}</strong><div>{partName(shard.effect)}{#if shard.effect.nameEn !== shard.effect.name}<small lang="en">{shard.effect.nameEn}</small>{/if}</div>{#if !shard.effect.name}<code>{shard.effect.path}</code>{/if}</li>{/each}</ul><p class="hint">Показаны выбранные эффекты. Числовые бонусы пока не расшифрованы.</p>{/if}
              <h3 class="ability-title">Заменённая способность</h3>{#if equipment.abilityOverride}<p><strong>{partName(equipment.abilityOverride.ability)}</strong> · {equipment.abilityOverride.slot ? `вместо способности ${equipment.abilityOverride.slot}` : "номер способности неизвестен"}</p>{#if equipment.abilityOverride.ability.nameEn !== equipment.abilityOverride.ability.name}<small lang="en">{equipment.abilityOverride.ability.nameEn}</small>{/if}<details><summary>Проверить исходные значения</summary><code>{equipment.abilityOverride.ability.path}</code><p class="hint">Слоты показаны как 1–4. Сверьте замену с Арсеналом.</p></details>{:else}<p class="hint">Замена не указана в полученных данных.</p>{/if}
            </section>
          {/if}
          {#if equipment.modularParts.length}<section><h3>Состав предмета</h3><ul>{#each equipment.modularParts as p}<li><strong>{partName(p)}</strong>{#if p.nameEn !== p.name}<small>{p.nameEn}</small>{/if}{#if !p.name}<code>{p.path}</code>{/if}</li>{/each}</ul></section>{/if}
          <div class="partial"><strong>Основа для повторения билда</strong><p>{equipment.inventoryResolved ? "Моды и явно указанные ранги сопоставлены внутри одного инвентаря. Неопределённые позиции сохранены. Владелец и активная конфигурация не установлены." : "Данные неполные: видны полученные улучшения. Неизвестные ранги и параметры нельзя считать нулевыми или максимальными."}</p></div>
          {#if equipment.item.fingerprint}<SquadRiven part={equipment.item} />{:else if equipment.category === "Мод разлома"}<p class="hint">Параметры мода разлома не получены. Его свойства и число преобразований неизвестны.</p>{/if}
          {#if equipment.upgradeSlots}
            <section class="upgrades"><h3>Порядок улучшений в конфигурации</h3><p class="hint">Нумерация следует записи игры. Это не схема слотов Арсенала.</p><ul class="ordered-slots">{#each equipment.upgradeSlots as slot}<li><small>Позиция {slot.index + 1}</small>{#if slot.status === "empty"}<span>Пусто</span>{:else if slot.part}<strong>{partName(slot.part)}</strong>{#if slot.part.nameEn && slot.part.nameEn !== slot.part.name}<small lang="en">{slot.part.nameEn}</small>{/if}{#if !slot.part.name && !slot.part.nameEn}<code>{slot.part.path}</code>{/if}<span>{slot.part.rank === null ? "Ранг неизвестен" : `Ранг ${slot.part.rank}`}</span>{#if slot.part.fingerprint}<SquadRiven part={slot.part} />{/if}{:else}<strong>Не удалось определить улучшение</strong><span>Позиция сохранена для проверки в Арсенале.</span>{/if}</li>{/each}</ul>{#if !equipment.upgradeSlots.length}<p class="hint">Список улучшений в этой конфигурации пуст.</p>{/if}</section>
          {:else if equipment.category !== "Мод разлома"}
          {#each [{ title: "Моды", parts: mods }, { title: "Мистификаторы", parts: arcanes }] as group}
            <section class="upgrades"><h3>{group.title} <span>{group.parts.length}</span></h3>{#if !group.parts.length}<p class="hint">Не определены в полученных данных.</p>{:else}<ul>{#each group.parts as p}<li><div><strong>{partName(p)}</strong>{#if p.nameEn && p.nameEn !== p.name}<small lang="en">{p.nameEn}</small>{/if}{#if !p.name && !p.nameEn}<code>{p.path}</code>{/if}</div><span>{p.rank === null ? "Ранг неизвестен" : `Ранг ${p.rank}`}</span></li>{/each}</ul>{/if}</section>
          {/each}
          {#if other.length || equipment.unreadableUpgrades}<details><summary>Другие элементы</summary><p class="hint">Неопределённые элементы могут включать косметику. Они не считаются модами.</p>{#each other as p}<p>{partName(p)}{#if p.nameEn && p.nameEn !== p.name}<small>{p.nameEn}</small>{/if}<code>{p.path}</code></p>{/each}{#if equipment.unreadableUpgrades}<p>Не удалось разобрать элементов: {equipment.unreadableUpgrades}.</p>{/if}</details>{/if}
          {/if}
          {#if mode === "saved" && selectedBuild}
            {#if editing}<form onsubmit={e => { e.preventDefault(); void saveEdit(); }}><label>Название билда<input bind:value={title} required maxlength="100" /></label><label>Заметка<textarea bind:value={note} maxlength="3000" rows="4" placeholder="Для какой миссии подходит, что изменить…"></textarea></label><div class="actions"><button class="primary" disabled={busy}>Сохранить изменения</button><button type="button" onclick={() => editing = false}>Отмена</button></div></form>
            {:else}<div class="notes"><h3>Моя заметка</h3><p>{selectedBuild.note || "Добавьте, чем понравился билд и для какой миссии его попробовать."}</p><button onclick={edit}>Изменить название и заметку</button></div>{/if}
          {/if}
          <div class="actions"><button onclick={() => void copy()}>Скопировать список</button><button onclick={() => void download()}>Сохранить список в файл</button>{#if mode === "saved"}<button class="delete" disabled={busy} onclick={() => { deletingId = selectedBuild?.id ?? ""; deleting = true; }}>Удалить билд</button>{/if}</div>
          {#if deleting}<div class="delete-confirm"><p>Удалить «{selectedBuild?.title}» из коллекции?</p><button disabled={busy} onclick={() => void remove()}>Удалить из коллекции</button><button onclick={() => deleting = false}>Оставить</button></div>{/if}
        {:else if mode === "saved"}<p class="empty">Выберите сохранённый билд, чтобы посмотреть состав и повторить его в Арсенале.</p>{/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .loadout-context { display:grid; gap:.6rem; padding:.8rem; background:var(--surface-2); border-radius:.5rem; font-size:.85rem; } .ability-title { margin-top:1rem; } .build-details { border-top:1px solid var(--border); padding-top:1rem; }
  .ordered-slots { grid-template-columns:1fr; margin-top:.6rem; }
  .squad-screen { display:grid; gap:1rem; min-width:0; }
  .toolbar,.capture-status,.actions,.item-heading { display:flex; gap:.75rem; align-items:center; justify-content:space-between; flex-wrap:wrap; }
  button,input,textarea,select { font:inherit; } button { padding:.55rem .85rem; border:1px solid var(--border); border-radius:.55rem; background:var(--surface-2); color:var(--text); cursor:pointer; } button:disabled { opacity:.5; cursor:default; } button:hover:not(:disabled) { border-color:var(--accent); } button:focus-visible,input:focus-visible,textarea:focus-visible,summary:focus-visible { outline:2px solid var(--accent); outline-offset:3px; }
  .modes { display:flex; flex-wrap:wrap; gap:.35rem; } .active,.primary { background:var(--accent-soft, #234239); border-color:var(--accent); } .toggle { display:flex; align-items:center; gap:.5rem; font-size:.85rem; }
  .capture-status { font-size:.86rem; } .capture-status span { display:flex; align-items:center; gap:.5rem; } i { width:.5rem; height:.5rem; background:var(--text-muted); border-radius:50%; } .live i { background:var(--accent); }
  .hint,.english,.provenance { color:var(--text-muted); font-size:.85rem; line-height:1.55; margin:.2rem 0; } .provenance { font-size:.78rem; }
  .workspace { display:grid; grid-template-columns:minmax(13rem, .8fr) minmax(0, 2fr); gap:1rem; align-items:start; }
  .roster,.detail { min-width:0; border:1px solid var(--border); border-radius:.8rem; background:var(--surface); padding:1rem; } .roster { display:grid; gap:.5rem; max-height:65vh; overflow:auto; } .detail { display:grid; gap:1rem; }
  .row { display:grid; gap:.25rem; text-align:left; overflow-wrap:anywhere; width:100%; } .row.selected { border-color:var(--accent); background:var(--accent-soft, #234239); } .row small,.row span { color:var(--text-muted); font-size:.76rem; }
  .empty { padding:1rem 0; color:var(--text-muted); font-size:.9rem; line-height:1.6; } .empty strong { color:var(--text); } .empty p { margin:.6rem 0; }
  h2,h3,p { margin:0; } h2 { font-size:1.25rem; overflow-wrap:anywhere; } h3 { font-size:.95rem; margin-bottom:.6rem; } h3 span { color:var(--text-muted); font-weight:400; margin-left:.3rem; }
  .eyebrow { color:var(--text-muted); font-size:.75rem; margin-bottom:.3rem; } .item-heading { align-items:flex-start; }
  .equipment-tabs { display:flex; gap:.4rem; flex-wrap:wrap; } .equipment-tabs button { text-align:left; font-size:.85rem; max-width:100%; overflow-wrap:anywhere; } .equipment-tabs small { display:block; font-size:.7rem; color:var(--text-muted); }
  .partial { padding:.8rem 1rem; border-left:3px solid var(--accent); background:var(--surface-2); border-radius:.3rem; font-size:.85rem; line-height:1.55; } .partial p { color:var(--text-muted); margin-top:.3rem; }
  ul { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:.45rem; margin:0; padding:0; list-style:none; } li { display:flex; flex-direction:column; gap:.5rem; padding:.65rem; background:var(--surface-2); border:1px solid var(--border); border-radius:.5rem; min-width:0; overflow-wrap:anywhere; } li strong { font-size:.85rem; } small { display:block; color:var(--text-muted); font-size:.75rem; } li>span { font-size:.7rem; color:var(--text-muted); }
  code { display:block; font-size:.7rem; overflow-wrap:anywhere; color:var(--text-muted); margin-top:.3rem; } details { border-top:1px solid var(--border); padding-top:.8rem; } summary { cursor:pointer; font-size:.85rem; } details p { margin-top:.8rem; }
  .actions { justify-content:flex-start; } .actions button { font-size:.8rem; } form,label { display:grid; gap:.4rem; } form { gap:.8rem; } input:not([type=checkbox]),textarea,select { width:100%; box-sizing:border-box; padding:.65rem; background:var(--surface-2); border:1px solid var(--border); border-radius:.4rem; color:var(--text); } textarea { resize:vertical; } .search { font-size:.8rem; margin-bottom:.5rem; }
  .notes { border-top:1px solid var(--border); padding-top:1rem; } .notes p { white-space:pre-wrap; overflow-wrap:anywhere; font-size:.85rem; line-height:1.6; margin-bottom:.6rem; } .notes button { font-size:.8rem; }
  .error,.notice,.delete-confirm { overflow-wrap:anywhere; padding:.8rem; border:1px solid var(--border); border-radius:.5rem; line-height:1.5; font-size:.85rem; } .error { color:var(--danger, #ef9e9e); } .notice { color:var(--accent); } .delete { margin-left:auto; } .delete-confirm button { margin:.5rem .4rem 0 0; }
  @media (max-width:1100px) { .workspace { grid-template-columns:minmax(11rem,.8fr) minmax(0,2fr); } .roster,.detail { padding:.75rem; } ul { grid-template-columns:1fr; } }
  @media (max-width:780px) { .workspace { grid-template-columns:1fr; } .roster { max-height:20rem; overflow:auto; } .toggle { width:100%; } .delete { margin-left:0; } }
</style>
