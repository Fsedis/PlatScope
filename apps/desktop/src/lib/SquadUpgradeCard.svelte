<script lang="ts">
  import PersonalGoalImage from "./PersonalGoalImage.svelte";
  import SquadRiven from "./SquadRiven.svelte";
  import { partName, type SquadPart } from "./squad";

  export let part: SquadPart | null = null;
  export let position: number | null = null;
  export let status: "empty" | "resolved" | "unresolved" = "resolved";
  export let compact = false;

  $: resolved = status === "resolved" && part !== null;
</script>

<article class="upgrade-card" class:compact class:resolved class:empty={status === "empty"}>
  {#if position !== null}<p class="position">Позиция {position}</p>{/if}
  <div class="card-body">
    {#if resolved && part}
      <div class="artwork"><PersonalGoalImage src={part.imageUrl ?? null} size={compact ? "small" : "large"} /></div>
      <div class="identity">
        <h4>{partName(part)}</h4>
        {#if part.nameEn && part.nameEn !== part.name}<p class="english" lang="en" translate="no">{part.nameEn}</p>{/if}
        <p class="rank">{part.rank === null ? "Ранг неизвестен" : `Ранг ${part.rank}`}</p>
      </div>
    {:else}
      <div class="unknown-artwork" aria-hidden="true">
        <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="m24 6 16 9v18l-16 9-16-9V15Z" />
          {#if status === "empty"}<path d="M18 24h12" />{:else}<path d="M19 18a5 5 0 0 1 10 0c0 4-5 4-5 8m0 5h.01" />{/if}
        </svg>
      </div>
      <div class="identity">
        <h4>{status === "empty" ? "Пусто" : "Не определено"}</h4>
        <p class="state-hint">{status === "empty" ? "В полученной записи нет улучшения." : "Не удалось прочитать улучшение."}</p>
      </div>
    {/if}
  </div>
  {#if resolved && part?.fingerprint}
    <details class="riven-details">
      <summary>Свойства разлома</summary>
      <div class="riven-content"><SquadRiven {part} /></div>
    </details>
  {/if}
</article>

<style>
  .upgrade-card {
    display:flex;
    flex-direction:column;
    gap:.7rem;
    width:100%;
    height:100%;
    min-width:0;
    box-sizing:border-box;
    padding:.85rem;
    border:1px solid var(--border);
    border-top:2px solid var(--border);
    border-radius:.65rem;
    background:var(--surface-1);
    color:var(--text);
    overflow-wrap:anywhere;
  }
  .resolved { border-top-color:var(--gold); }
  .empty { border-style:dashed; background:var(--surface-2); }
  .position { color:var(--text-muted); font-size:.7rem; line-height:1.4; }
  .card-body { display:flex; flex:1; flex-direction:column; gap:.8rem; min-width:0; }
  .artwork,.unknown-artwork { display:flex; align-items:center; justify-content:center; min-width:0; }
  .artwork { padding:.25rem 0; }
  .artwork :global(.artwork.large) { width:min(7rem,100%); max-width:100%; height:auto; aspect-ratio:1; flex-shrink:1; }
  .unknown-artwork { min-height:7.5rem; color:var(--text-subtle); }
  .unknown-artwork svg { width:3rem; height:3rem; flex:none; }
  .identity { display:flex; flex:1; flex-direction:column; align-items:flex-start; gap:.3rem; min-width:0; }
  h4,p { margin:0; }
  h4 { font-size:.88rem; font-weight:650; line-height:1.4; }
  .english,.state-hint { color:var(--text-muted); font-size:.73rem; line-height:1.5; }
  .rank { margin-top:auto; padding-top:.6rem; color:var(--text-muted); font-size:.73rem; line-height:1.4; }
  .riven-details { min-width:0; border-top:1px solid var(--border); padding-top:.65rem; }
  summary { width:fit-content; max-width:100%; color:var(--accent-strong); font-size:.76rem; line-height:1.5; cursor:pointer; }
  summary:focus-visible { outline:2px solid var(--gold); outline-offset:3px; border-radius:.15rem; }
  .riven-content { margin-top:.65rem; min-width:0; }
  .compact { padding:.75rem; }
  .compact .card-body { flex-direction:row; align-items:flex-start; gap:.75rem; }
  .compact .artwork { padding:0; flex:none; }
  .compact .unknown-artwork { flex:none; width:3rem; min-height:3rem; }
  .compact .unknown-artwork svg { width:2.5rem; height:2.5rem; }
  .compact .rank { margin-top:.05rem; padding-top:0; }
</style>
