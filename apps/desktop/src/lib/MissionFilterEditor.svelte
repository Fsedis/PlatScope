<script lang="ts">
  import { addFilterObjects, type CustomMissionFilter } from "./missionFilters";
  import type { MissionObject } from "./missionResearch";
  export let filters: CustomMissionFilter[] = [];
  export let selected: MissionObject[] = [];
  export let onchange: (filters: CustomMissionFilter[]) => void;
  let mode: "add" | "manage" | null = null;
  let target = "";
  let name = "";
  let color = "#e79bf4";
  let error = "";
  let notice = "";
  let editingId = "";
  let editName = "";
  let editColor = "#e79bf4";
  let undo: CustomMissionFilter[] | null = null;
  function commit(next: CustomMissionFilter[]) { undo = filters; onchange(next); error = ""; }
  function add() {
    if (!selected.length) return;
    const existing = filters.find(filter => filter.id === target);
    if (target && !existing) { error = "Выберите существующий фильтр."; return; }
    if (!existing && (!name.trim() || filters.some(filter => filter.name.toLocaleLowerCase("ru") === name.trim().toLocaleLowerCase("ru")))) { error = "Введите новое, непустое название фильтра."; return; }
    if (!existing && filters.length >= 40) { error = "Можно сохранить до 40 фильтров."; return; }
    try {
      const next = addFilterObjects(existing ?? { id: crypto.randomUUID(), name: name.trim(), color, enabled: true, rules: [] }, selected);
      commit(existing ? filters.map(filter => filter.id === next.id ? next : filter) : [...filters, next]);
      notice = `Типы выбранных объектов добавлены в «${next.name}».`; mode = null; name = "";
    } catch (reason) { error = reason instanceof Error ? reason.message : "Не удалось добавить объекты."; }
  }
  function edit(filter: CustomMissionFilter) { editingId = filter.id; editName = filter.name; editColor = filter.color; }
  function saveEdit() {
    if (!editName.trim() || filters.some(filter => filter.id !== editingId && filter.name.toLocaleLowerCase("ru") === editName.trim().toLocaleLowerCase("ru"))) { error = "Введите непустое название, отличающееся от других фильтров."; return; }
    commit(filters.map(filter => filter.id === editingId ? { ...filter, name: editName.trim(), color: editColor } : filter)); editingId = "";
  }
</script>

<section class="custom-editor" aria-label="Свои фильтры">
  <div class="actions"><button disabled={!selected.length} onclick={() => { mode = mode === "add" ? null : "add"; error = ""; notice = ""; }}>Добавить в фильтр{selected.length ? ` · ${selected.length}` : ""}</button><button onclick={() => { mode = mode === "manage" ? null : "manage"; error = ""; notice = ""; }}>Настроить фильтры{filters.length ? ` · ${filters.length}` : ""}</button>{#if !selected.length}<span>Выберите объект на карте или отметьте несколько объектов в списке.</span>{/if}</div>
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if mode === "add"}<div class="panel"><p>В фильтр попадут все объекты выбранных типов, в том числе на следующих миссиях.</p><div class="fields"><label>Куда добавить<select bind:value={target}><option value="">Создать новый фильтр</option>{#each filters as filter}<option value={filter.id}>{filter.name}</option>{/each}</select></label>{#if !target}<label>Название фильтра<input maxlength="60" bind:value={name} placeholder="Например, шкафчики" /></label><label>Цвет<input type="color" bind:value={color} /></label>{/if}<button class="primary" disabled={!selected.length || (!target && !name.trim())} onclick={add}>{target ? "Добавить выбранные типы" : "Создать фильтр с объектами"}</button><button onclick={() => mode = null}>Отмена</button></div></div>{/if}
  {#if mode === "manage"}<div class="panel"><p>Типы, добавленные в свои фильтры, показываются по их переключателям. Остальные объекты — по стандартным группам. Все изменения сохраняются автоматически.</p>{#if !filters.length}<p>Своих фильтров пока нет. Выберите объекты и нажмите «Добавить в фильтр».</p>{/if}{#each filters as filter (filter.id)}<details><summary><i style:background={filter.color}></i>{filter.name} · типов: {filter.rules.length}</summary><div class="fields">{#if editingId === filter.id}<label>Новое название<input maxlength="60" bind:value={editName} /></label><label>Новый цвет<input type="color" bind:value={editColor} /></label><button onclick={saveEdit}>Сохранить изменения</button>{:else}<button onclick={() => edit(filter)}>Изменить название и цвет</button>{/if}<button onclick={() => { commit(filters.filter(item => item.id !== filter.id)); notice = `Фильтр «${filter.name}» удалён.`; }}>Удалить фильтр</button></div><ul>{#each filter.rules as rule}<li><span>{rule.label || "Тип объекта"}{#if rule.nameEn && rule.nameEn !== rule.label}<small>{rule.nameEn}</small>{/if}</span><button aria-label={`Убрать тип ${rule.label || rule.nameEn} из ${filter.name}`} onclick={() => commit(filters.map(item => item.id === filter.id ? { ...item, rules: item.rules.filter(entry => entry.key !== rule.key) } : item))}>Убрать из фильтра</button></li>{/each}</ul>{#if !filter.rules.length}<p>Фильтр пуст. Добавьте в него объекты.</p>{/if}</details>{/each}</div>{/if}
  {#if undo}<button class="undo" onclick={() => { const previous = undo!; undo = null; onchange(previous); notice = "Последнее изменение отменено."; }}>Отменить последнее изменение фильтров</button>{/if}
</section>

<style>
  .custom-editor{margin:.4rem 0 .8rem;color:var(--text);font-size:.8rem}.actions,.fields{display:flex;align-items:center;gap:.6rem;flex-wrap:wrap}.actions>span,p{color:var(--text-muted);line-height:1.5}.panel{background:var(--surface);border:1px solid var(--border);border-radius:.6rem;padding:.8rem;margin-top:.6rem}.fields{align-items:end}label{display:flex;flex-direction:column;gap:.3rem}button,input,select{font:inherit;color:var(--text);background:var(--surface);border:1px solid var(--border);border-radius:.4rem;padding:.5rem .65rem}input:not([type="color"]){width:240px;max-width:100%}input[type="color"]{height:34px;width:50px;padding:.15rem}button{cursor:pointer}button:disabled{opacity:.5;cursor:default}button.primary{background:#397c67;color:#fff;border-color:#529b84}button:focus-visible,input:focus-visible,select:focus-visible,summary:focus-visible{outline:2px solid #e4bb75;outline-offset:2px}details{border-top:1px solid var(--border);padding:.6rem 0}summary{cursor:pointer;display:list-item}summary i{display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:.5rem}ul{list-style:none;padding:0;max-height:280px;overflow:auto}li{display:flex;align-items:center;justify-content:space-between;gap:1rem;margin:.6rem 0}li span{overflow-wrap:anywhere}small{display:block;color:var(--text-muted);margin-top:.2rem}.error{color:#c04c4c}.undo{margin-top:.5rem}
</style>
