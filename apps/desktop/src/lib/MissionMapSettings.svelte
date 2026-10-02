<script lang="ts">
  import { tick } from "svelte";
  import MissionObjectIcon from "./MissionObjectIcon.svelte";
  import { objectRule, type CustomMissionFilter, type MissionFilterIndex } from "./missionFilters";
  import { MISSION_FILTERS, type MissionFilter, type MissionObject, type MissionObjectKind } from "./missionResearch";
  import { DEFAULT_MISSION_MARKER_SIZE, MIN_MISSION_MARKER_SIZE, MAX_MISSION_MARKER_SIZE, missionMarkerSize, type MissionMapPreferences } from "./missionMapPreferences";

  export let preferences: MissionMapPreferences;
  export let filters: CustomMissionFilter[] = [];
  export let customIndex: MissionFilterIndex;
  export let selected: MissionObject | null = null;
  export let heightKnown = false;
  export let onchange: (preferences: MissionMapPreferences) => void;
  let target = "base";
  let expanded = false;
  let disclosure: HTMLDetailsElement | undefined;
  const icons: Record<MissionFilter, MissionObjectKind> = {
    feather: "feather", pickup: "pickup", decree_fragments: "decree_fragment", players: "avatar",
    npc: "npc", other: "decoration", goals: "extraction", lootspots: "lootspot", caches: "cache", dragon_doors: "dragon_door",
  };

  $: if (target === "type" && !selected || target.startsWith("group:") && !filters.some(filter => filter.id === target.slice(6))) target = "base";
  $: typeKey = selected ? objectRule(selected).key : "";
  $: size = target === "type" && selected ? missionMarkerSize(selected, customIndex, preferences)
    : target.startsWith("group:") ? preferences.groupSizes[target.slice(6)] ?? preferences.markerSize
    : target.startsWith("category:") ? preferences.categorySizes[target.slice(9) as MissionFilter] ?? preferences.markerSize
    : preferences.markerSize;
  $: previewKind = target === "type" && selected ? selected.kind
    : target.startsWith("category:") ? icons[target.slice(9) as MissionFilter] : "pickup";
  $: separateSize = target === "type" ? preferences.typeSizes[typeKey] !== undefined
    : target.startsWith("group:") ? preferences.groupSizes[target.slice(6)] !== undefined
    : target.startsWith("category:") ? preferences.categorySizes[target.slice(9) as MissionFilter] !== undefined
    : preferences.markerSize !== DEFAULT_MISSION_MARKER_SIZE;

  function changeSize(value: number | null) {
    const next = { ...preferences };
    if (target === "type" && selected) {
      next.typeSizes = { ...preferences.typeSizes };
      if (value === null) delete next.typeSizes[typeKey]; else next.typeSizes[typeKey] = value;
    } else if (target.startsWith("group:")) {
      next.groupSizes = { ...preferences.groupSizes };
      if (value === null) delete next.groupSizes[target.slice(6)]; else next.groupSizes[target.slice(6)] = value;
    } else if (target.startsWith("category:")) {
      next.categorySizes = { ...preferences.categorySizes };
      if (value === null) delete next.categorySizes[target.slice(9) as MissionFilter]; else next.categorySizes[target.slice(9) as MissionFilter] = value;
    } else next.markerSize = value ?? DEFAULT_MISSION_MARKER_SIZE;
    onchange(next);
  }

  export async function configureSelected() {
    if (!selected) return;
    target = "type"; expanded = true;
    await tick();
    disclosure?.scrollIntoView({ block: "nearest" });
  }
</script>

<details class="marker-settings" bind:this={disclosure} bind:open={expanded}>
  <summary>Значки карты</summary>
  <div class="settings-body">
    <label>Изменить размер
      <select bind:value={target}>
        <option value="base">Общий размер</option>
        {#if selected}<option value="type">Выбранный тип</option>{/if}
        <optgroup label="Категории">
          {#each MISSION_FILTERS as filter}<option value={`category:${filter.key}`}>{filter.key === "npc" ? "Персонажи и враги" : filter.key === "lootspots" ? "Места находок" : filter.label}</option>{/each}
        </optgroup>
        {#if filters.length}<optgroup label="Свои группы">{#each filters as filter}<option value={`group:${filter.id}`}>{filter.name}</option>{/each}</optgroup>{/if}
      </select>
    </label>
    {#if target === "type" && selected}
      <p class="selected-type"><strong>{selected.label}</strong>{#if selected.nameEn && selected.nameEn !== selected.label}<span>{selected.nameEn}</span>{/if}<span>Все экземпляры этого типа</span></p>
    {:else if target === "base"}<p>Для значков без отдельной настройки.</p>
    {:else}<p>Отдельный размер типа имеет приоритет.</p>{/if}
    <div class="size-preview" aria-hidden="true"><MissionObjectIcon kind={previewKind} size={size} color="#abe9ce" /></div>
    <label class="size-control"><span>Размер на карте <output>{size} пикс.</output></span><input type="range" min={MIN_MISSION_MARKER_SIZE} max={MAX_MISSION_MARKER_SIZE} step="1" value={size} oninput={event => changeSize(Number(event.currentTarget.value))} /></label>
    <button class="reset-size" disabled={!separateSize} onclick={() => changeSize(null)}>Сбросить размер</button>
    <label class="height-toggle"><input type="checkbox" checked={preferences.showHeightIndicators} onchange={event => onchange({ ...preferences, showHeightIndicators: event.currentTarget.checked })} />Показывать выше / ниже</label>
    <p>↑ выше вас · ↓ ниже вас. Только у объектов, кроме персонажей; разница от 1 м.</p>
    {#if preferences.showHeightIndicators && !heightKnown}<p>Отметки появятся, когда будет известно ваше положение.</p>{/if}
  </div>
</details>

<style>
  .marker-settings { border-top: 1px solid var(--border); padding-top: .85rem; margin-top: .9rem; font-size: .78rem; }
  summary { cursor: pointer; line-height: 1.5; }
  .settings-body { display: grid; gap: .65rem; margin-top: .75rem; }
  label { display: grid; gap: .35rem; }
  select, button { font: inherit; color: var(--text); border: 1px solid var(--border); border-radius: 6px; background: var(--surface-1); min-width: 0; }
  select { width: 100%; padding: .4rem; }
  p { margin: 0; color: var(--text-muted); font-size: .68rem; line-height: 1.5; overflow-wrap: anywhere; }
  .selected-type span { display: block; }
  .selected-type strong { color: var(--text); font-weight: 600; }
  .size-preview { display: grid; place-items: center; height: 64px; border: 1px solid #344b53; border-radius: 7px; background: #111a23; }
  .size-control > span { display: flex; justify-content: space-between; flex-wrap: wrap; gap: .3rem; }
  output { color: var(--text-muted); font-variant-numeric: tabular-nums; }
  input[type="range"] { width: 100%; min-width: 0; margin: 0; accent-color: var(--accent); }
  button { cursor: pointer; padding: .4rem .5rem; min-height: 30px; }
  button:disabled { opacity: .5; cursor: default; }
  .reset-size { justify-self: start; }
  .height-toggle { display: flex; align-items: center; gap: .4rem; margin-top: .4rem; line-height: 1.4; }
  input[type="checkbox"] { width: 15px; height: 15px; margin: 0; flex: none; accent-color: var(--accent); }
  select:focus-visible, button:focus-visible, input:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
</style>
