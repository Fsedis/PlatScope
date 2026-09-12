<script lang="ts">
  import { rivenStatName, type SquadPart } from "./squad";
  export let part: SquadPart;
  $: f = part.fingerprint;
</script>

{#if f}
  <div class="riven">
    <p><strong>Мод разлома для {f.weaponName || f.weaponNameEn || "неопределённого оружия"}</strong></p>
    {#if f.weaponNameEn && f.weaponNameEn !== f.weaponName}<small lang="en">{f.weaponNameEn}</small>{/if}
    <p>Ранг: {part.rank ?? "неизвестен"} · Мастерство: {f.masteryRank ?? "неизвестно"} · Преобразований: {f.rerolls ?? "неизвестно"}</p>
    {#each f.buffs as stat}<p>+ {rivenStatName(stat.tag)}</p>{/each}
    {#each f.curses as stat}<p>− {rivenStatName(stat.tag)}</p>{/each}
    <small>Вид свойств прочитан. Проценты бонусов пока не рассчитаны.</small>
    <details><summary>Исходные свойства</summary>
      {#if f.weaponPath}<code>{f.weaponPath}</code>{/if}
      <p>Полярность: {f.polarity ?? "не получена"}</p>
      {#each [...f.buffs, ...f.curses] as stat}<code>{stat.tag}: {stat.value ?? "значение не получено"}</code>{/each}
      <small>Эти числа не являются процентами бонусов.</small>
    </details>
  </div>
{/if}

<style>
  .riven { display:grid; gap:.4rem; min-width:0; overflow-wrap:anywhere; font-size:.82rem; }
  p { margin:0; } small,code { color:var(--text-muted); font-size:.75rem; } code { display:block; overflow-wrap:anywhere; }
  details { margin-top:.4rem; } summary { cursor:pointer; } details p { margin:.4rem 0; }
</style>
