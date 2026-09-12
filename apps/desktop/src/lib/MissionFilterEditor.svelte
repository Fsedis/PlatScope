<script lang="ts">
  import { addFilterObjects, filterMatches, objectRule, type FilterScope, type CustomMissionFilter } from "./missionFilters";
  import type { MissionObject } from "./missionResearch";
  export let filters: CustomMissionFilter[] = [];
  export let selected: MissionObject[] = [];
  export let objects: MissionObject[] = [];
  export let counts: Map<string, number> = new Map();
  export let onchange: (filters: CustomMissionFilter[]) => void;
  export let onclear: () => void;
  const colors = ["#bf78cf", "#4ba989", "#cf994c", "#679cce", "#d37887", "#929664"];
  let scope: FilterScope = "variant";
  let name = "";
  let selectionKey = "";
  let error = "";
  let notice = "";
  let editingId = "";
  let editName = "";
  let editColor = colors[0];
  let undo: CustomMissionFilter[] | null = null;
  $: nextSelectionKey = selected.map(object => object.key).join("|");
  $: if (nextSelectionKey !== selectionKey) {
    selectionKey = nextSelectionKey; scope = "variant"; error = "";
    name = suggestedName(selected.length === 1 ? selected[0].label : "Мои находки", filters);
  }
  $: preview = { id: "preview", name: "", color: "#ffffff", enabled: true, rules: selected.map(object => objectRule(object, scope)) };
  $: matchingCount = objects.filter(object => filterMatches(preview, object)).length;
  $: includedIds = new Set(filters.filter(filter => preview.rules.length > 0 && preview.rules.every(rule => filter.rules.some(existing => existing.key === rule.key))).map(filter => filter.id));
  function suggestedName(base: string, current: CustomMissionFilter[]) {
    const label = (base || "Мои находки").slice(0, 54);
    let candidate = label, index = 2;
    while (current.some(filter => filter.name.toLocaleLowerCase("ru") === candidate.toLocaleLowerCase("ru"))) candidate = `${label} ${index++}`;
    return candidate;
  }
  function commit(next: CustomMissionFilter[]) { undo = filters; onchange(next); error = ""; }
  function add(existing?: CustomMissionFilter) {
    if (!selected.length) return;
    if (!existing && (!name.trim() || filters.some(filter => filter.name.toLocaleLowerCase("ru") === name.trim().toLocaleLowerCase("ru")))) { error = "Укажите название, отличающееся от других групп."; return; }
    if (!existing && filters.length >= 40) { error = "Уже создано 40 групп. Добавьте предметы в существующую."; return; }
    try {
      const next = addFilterObjects(existing ? { ...existing, enabled: true } : { id: crypto.randomUUID(), name: name.trim(), color: colors[filters.length % colors.length], enabled: true, rules: [] }, selected, scope);
      commit(existing ? filters.map(filter => filter.id === next.id ? next : filter) : [...filters, next]);
      notice = `Добавлено в «${next.name}».`;
      name = suggestedName(name, existing ? filters : [...filters, next]);
    } catch (reason) { error = reason instanceof Error ? reason.message : "Не удалось добавить предметы."; }
  }
  function edit(filter: CustomMissionFilter) { editingId = editingId === filter.id ? "" : filter.id; editName = filter.name; editColor = filter.color; error = ""; }
  function saveEdit() {
    if (!editName.trim() || filters.some(filter => filter.id !== editingId && filter.name.toLocaleLowerCase("ru") === editName.trim().toLocaleLowerCase("ru"))) { error = "Укажите название, отличающееся от других групп."; return; }
    commit(filters.map(filter => filter.id === editingId ? { ...filter, name: editName.trim(), color: editColor } : filter)); editingId = "";
    notice = "Группа обновлена.";
  }
  function ruleScope(key: string) { try { return JSON.parse(key)[1] === "variant" ? "Этот вариант" : "Все экземпляры"; } catch { return "Сохранённое правило"; } }
</script>

<section class="custom-editor" aria-label="Свои фильтры">
  <header><h2>Свои фильтры</h2><span title="Группы сохраняются между запусками">Автосохранение</span></header>
  {#if selected.length}
    <div class="selection-box">
      <div class="selection-heading"><strong>{selected.length === 1 ? selected[0].label : `Выбрано объектов: ${selected.length}`}</strong><button class="icon" aria-label="Снять выделение" onclick={onclear}>×</button></div>
      <label class="scope">Добавлять<select aria-label="Какие экземпляры добавлять" bind:value={scope}><option value="variant">Этот вариант</option><option value="model">Все варианты модели</option></select></label>
      <p>Все подходящие экземпляры · сейчас {matchingCount}</p>
      <div class="assign-hint">{filters.length ? "Нажмите + у нужной группы ниже" : "Создайте первую группу ниже"}</div>
    </div>
  {:else}
    <p class="instruction">Выберите предмет на карте или в списке, чтобы добавить его в группу.</p>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="group-list">
    {#each filters as filter (filter.id)}
      <div class="group" class:editing={editingId === filter.id}>
        <div class="group-row">
          <label title={filter.enabled ? "Скрыть эту группу на карте" : "Показать эту группу на карте"}><input type="checkbox" checked={filter.enabled} onchange={(event) => commit(filters.map(item => item.id === filter.id ? { ...item, enabled: event.currentTarget.checked } : item))} /><i style:background={filter.color}></i><span>{filter.name}</span><b>{counts.get(filter.id) ?? 0}</b></label>
          {#if selected.length}<button class="icon add" aria-label={`Добавить выбранное в ${filter.name}`} title={includedIds.has(filter.id) ? "Уже добавлено" : `Добавить в «${filter.name}»`} disabled={includedIds.has(filter.id) && filter.enabled} onclick={() => add(filter)}>{includedIds.has(filter.id) ? "✓" : "+"}</button>{/if}
          <button class="icon" aria-label={`Настроить группу ${filter.name}`} aria-expanded={editingId === filter.id} onclick={() => edit(filter)}>⋯</button>
        </div>
        {#if editingId === filter.id}
          <div class="group-edit">
            <label>Название<input maxlength="60" bind:value={editName} onkeydown={(event) => { if (event.key === "Enter") saveEdit(); }} /></label>
            <div class="edit-actions"><label class="color-label">Цвет<input type="color" bind:value={editColor} /></label><button onclick={saveEdit}>Сохранить</button></div>
            <h3>Состав группы · {filter.rules.length}</h3>
            <ul>{#each filter.rules as rule}<li><span>{rule.label || "Предмет"}{#if rule.nameEn && rule.nameEn !== rule.label}<small>{rule.nameEn}</small>{/if}<small class="rule-scope">{ruleScope(rule.key)}</small></span><button class="icon" aria-label={`Убрать ${rule.label || rule.nameEn} из ${filter.name}`} onclick={() => commit(filters.map(item => item.id === filter.id ? { ...item, rules: item.rules.filter(entry => entry.key !== rule.key) } : item))}>×</button></li>{/each}</ul>
            {#if !filter.rules.length}<p>Пока пусто. Выберите предмет и нажмите + у группы.</p>{/if}
            <button class="delete" onclick={() => { commit(filters.filter(item => item.id !== filter.id)); editingId = ""; notice = `Группа «${filter.name}» удалена.`; }}>Удалить группу</button>
          </div>
        {/if}
      </div>
    {/each}
  </div>
  {#if selected.length}
    <form class="create" onsubmit={(event) => { event.preventDefault(); add(); }}>
      <label for="mission-new-filter">Новая группа</label><input id="mission-new-filter" maxlength="60" bind:value={name} placeholder="Название группы" />
      <button class="primary" disabled={!name.trim()}>Создать из выбранного</button>
    </form>
  {:else if !filters.length}<div class="no-groups"><span>＋</span><strong>Соберите свои находки</strong><p>Перья, скульптуры или нужные тайники — в отдельных группах.</p></div>{/if}
  {#if notice || undo}<div class="feedback" role="status">{#if notice}<p>{notice}</p>{/if}{#if undo}<button onclick={() => { const previous = undo!; undo = null; onchange(previous); notice = "Изменение отменено."; }}>Отменить изменение</button>{/if}</div>{/if}
</section>

<style>
  .custom-editor{font-size:.8rem;color:var(--text)}header{display:flex;align-items:center;justify-content:space-between;gap:.5rem;margin-bottom:1rem}h2{font-size:.94rem;margin:0}header>span{font-size:.65rem;color:var(--text-muted)}p{font-size:.76rem;color:var(--text-muted);line-height:1.5;margin:.5rem 0}button,input,select{font:inherit}button{border:1px solid var(--border);border-radius:6px;background:var(--surface-1);color:var(--text);padding:.45rem .6rem;cursor:pointer;min-height:30px}button:hover{background:var(--surface-3);border-color:var(--border-strong)}button:disabled{opacity:.55;cursor:default}button:focus-visible,input:focus-visible,select:focus-visible{outline:2px solid var(--accent);outline-offset:2px}input:not([type="checkbox"]):not([type="color"]),select{width:100%;min-width:0;padding:.5rem;border:1px solid var(--border);border-radius:5px;background:var(--surface-1);color:var(--text)}input[type="checkbox"]{accent-color:#397c67;flex-shrink:0;width:15px;height:15px;margin:0}.selection-box{padding:.75rem;border:1px solid #4a998260;border-radius:8px;background:#4a998210;margin-bottom:.75rem}.selection-heading{display:flex;align-items:start;gap:.3rem}.selection-heading strong{flex:1;overflow-wrap:anywhere;line-height:1.4}.scope{display:grid;grid-template-columns:auto minmax(0,1fr);align-items:center;gap:.5rem;margin-top:.65rem;font-size:.72rem}.scope select{padding:.4rem;font-size:.74rem}.selection-box p{font-size:.68rem}.assign-hint{font-size:.73rem;font-weight:600;color:#33755e;margin-top:.65rem}.group-list{display:flex;flex-direction:column;gap:.25rem}.group-row{display:flex;align-items:center;gap:.1rem;min-height:40px}.group-row label{display:flex;align-items:center;gap:.45rem;flex:1;min-width:0;cursor:pointer;padding:.35rem 0}.group-row label>span{flex:1;overflow-wrap:anywhere;line-height:1.3}.group-row b{font-size:.7rem;color:var(--text-muted);font-weight:500;min-width:14px;text-align:right}i{width:7px;height:7px;border-radius:50%;flex-shrink:0}.icon{border-color:transparent;background:transparent;width:28px;min-width:28px;padding:0;font-size:1.1rem;line-height:1}.icon.add{background:#397c6710;color:#33755e;border-color:#397c6733;margin-left:.25rem}.group.editing{background:var(--surface-2);border-radius:6px}.group-edit{padding:.65rem;border-top:1px solid var(--border)}.group-edit label{display:flex;flex-direction:column;gap:.3rem;font-size:.73rem}.edit-actions{display:flex;align-items:end;justify-content:space-between;margin:.65rem 0}.color-label input{width:40px;height:29px;padding:2px;border:1px solid var(--border);border-radius:4px}h3{font-size:.73rem;margin:1rem 0 .5rem}ul{padding:0;margin:0;list-style:none}li{display:flex;align-items:start;gap:.3rem;padding:.5rem 0;border-bottom:1px solid var(--border)}li>span{flex:1;min-width:0;font-size:.74rem;overflow-wrap:anywhere}small{display:block;color:var(--text-muted);font-size:.66rem;margin-top:.2rem}.rule-scope{color:var(--accent)}.delete{font-size:.72rem;color:var(--danger);margin-top:.8rem;background:transparent}.create{border-top:1px solid var(--border);margin-top:.85rem;padding-top:.85rem;display:grid;gap:.5rem}.create label{font-weight:600;font-size:.74rem}button.primary{background:#397c67;border-color:#397c67;color:#fff;font-size:.75rem}.no-groups{text-align:center;padding:1.2rem .3rem}.no-groups>span{display:block;font-size:1.6rem;color:var(--border-strong);margin-bottom:.4rem}.no-groups strong{font-size:.78rem}.no-groups p{font-size:.71rem}.feedback{border-top:1px solid var(--border);margin-top:.8rem;padding-top:.3rem}.feedback button{font-size:.7rem;padding:.3rem .45rem}.error{color:var(--danger);overflow-wrap:anywhere}.instruction{margin-bottom:.8rem}
</style>
