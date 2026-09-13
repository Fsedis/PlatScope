<script lang="ts">
  import type { HiddenMissionName } from "./missionFilters";
  export let names: HiddenMissionName[] = [];
  export let count = 0;
  export let disabled = false;
  export let onrestore: (name: HiddenMissionName) => void;
  export let onrestoreall: () => void;
</script>

<details class="hidden-names">
  <summary>Скрытые <span>{names.length}</span></summary>
  {#if names.length}
    <p>Скрыто на этой карте: {count}. Все экземпляры с этими названиями убраны с карты и из списка.</p>
    <ul>
      {#each names as name}
        <li><div><strong>{name.label || name.nameEn}</strong>{#if name.nameEn && name.nameEn !== name.label}<small>{name.nameEn}</small>{/if}</div><button {disabled} aria-label={`Вернуть на карту: ${name.label || name.nameEn}`} onclick={() => onrestore(name)}>Вернуть</button></li>
      {/each}
    </ul>
    {#if names.length > 1}<button class="restore-all" {disabled} onclick={onrestoreall}>Вернуть все</button>{/if}
    <p>После возврата действуют включённые фильтры карты.</p>
  {:else}<p>Нажмите «Скрыть» рядом с ненужным предметом. Здесь можно будет вернуть его и все экземпляры с таким названием.</p>{/if}
</details>

<style>
  .hidden-names{border-top:1px solid var(--border);padding-top:.85rem;margin-top:.9rem;font-size:.78rem}
  summary{cursor:pointer;font-weight:600;line-height:1.5}summary>span{float:right;font-size:.72rem;color:var(--text-muted);font-weight:400}
  p{font-size:.72rem;line-height:1.5;color:var(--text-muted);margin:.6rem 0}
  ul{list-style:none;padding:0;margin:.6rem 0;max-height:260px;overflow:auto;scrollbar-width:thin}
  li{display:flex;align-items:center;gap:.4rem;padding:.6rem 0;border-bottom:1px solid var(--border)}li>div{flex:1;min-width:0}
  strong,small{display:block;overflow-wrap:anywhere}strong{font-size:.76rem;font-weight:500}small{font-size:.7rem;color:var(--text-muted);margin-top:.2rem}
  button{flex-shrink:0;font-size:.72rem;padding:.35rem .45rem}.restore-all{width:100%}
</style>
