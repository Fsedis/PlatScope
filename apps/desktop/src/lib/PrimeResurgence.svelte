<script lang="ts">
  import { tick } from "svelte";
  import MasteryBadge from "./MasteryBadge.svelte";
  import WorldActivityArtwork from "./WorldActivityArtwork.svelte";
  import { relicArtwork } from "./worldActivityArtwork";
  import { offerCost, type ActivityOffer } from "./worldActivity";
  import { rotationEquipment, rotationRelics, rotationRewards } from "./primeResurgence";

  export let offers: ActivityOffer[];
  export let catalogAvailable: boolean;
  export let incomplete: boolean;
  export let onOpenSettings: () => void;
  let selectedRef = "";
  let relicHeading: HTMLHeadingElement;
  let failedImages = new Set<string>();
  $: equipment = rotationEquipment(offers);
  $: if (selectedRef && !equipment.some(offer => offer.gameRef === selectedRef)) selectedRef = "";
  $: warframes = equipment.filter(offer => offer.equipmentCategory === "warframe");
  $: otherEquipment = equipment.filter(offer => offer.equipmentCategory !== "warframe");
  $: selected = equipment.find(offer => offer.gameRef === selectedRef);
  $: relics = rotationRelics(offers, selected?.gameRef);
  $: paidOffers = offers.filter(offer => offer.kind !== "relic");
  function select(offer: ActivityOffer) { selectedRef = selectedRef === offer.gameRef ? "" : offer.gameRef; }
  async function resetSelection(): Promise<void> {
    selectedRef = "";
    await tick();
    relicHeading?.focus({ preventScroll: true });
  }
  const shortRelicName = (name: string) => name.replace(/^Реликвия\s+/i, "");
  const chance = (value: number) => `${value.toLocaleString("ru-RU", { maximumFractionDigits: 2 })}%`;
</script>

<div class="resurgence-content">
  {#if !offers.length}
    <p class="notice">Ассортимент этой ротации ещё не получен. Список появится после обновления источника.</p>
  {:else}
  {#if incomplete}<p class="notice">Варзия передала неполный ассортимент. Ниже — подтверждённые товары.</p>{/if}
  {#if warframes.length}
    <section aria-label="Варфреймы текущей ротации">
      <h3>Варфреймы этой ротации</h3>
      <div class="warframes">
        {#each warframes as offer (offer.gameRef)}
          <div class="warframe" class:selected={selected?.gameRef === offer.gameRef}>
            <button type="button" class="warframe-select" aria-pressed={selected?.gameRef === offer.gameRef} aria-controls="rotation-relics"
              aria-label={`Показать реликвии: ${offer.displayName}`} onclick={() => select(offer)}>
              <span class="portrait" aria-hidden="true">{#if offer.imageUrl && !failedImages.has(offer.gameRef)}<img src={offer.imageUrl} alt="" loading="lazy" onerror={() => failedImages = new Set(failedImages).add(offer.gameRef)} />
                {:else}<span class="portrait-fallback"><WorldActivityArtwork kind="warframe" /></span>{/if}</span>
              <span class="warframe-copy"><strong>{offer.displayName}</strong><span class="relic-link">{selected?.gameRef === offer.gameRef ? "Реликвии выбраны" : `Реликвии: ${rotationRelics(offers, offer.gameRef).length}`} <span aria-hidden="true">{selected?.gameRef === offer.gameRef ? "✓" : "↓"}</span></span></span>
            </button>
            {#if offer.masteryRef}<div class="mastery"><MasteryBadge gameRef={offer.masteryRef} /></div>{/if}
          </div>
        {/each}
      </div>
    </section>
  {:else if !catalogAvailable}
    <p class="notice">Справочник предметов ещё не загружен. Пока доступны названия из источника; состав реликвий появится после загрузки.
      <button type="button" class="text-button" onclick={onOpenSettings}>Открыть настройки данных</button></p>
  {:else}<p class="notice">Пока не удалось определить варфреймов по полученному ассортименту.</p>{/if}

  {#if otherEquipment.length}
    <section aria-label="Оружие и спутники текущей ротации">
      <h3>Оружие и спутники</h3>
      <div class="equipment">
        {#each otherEquipment as offer (offer.gameRef)}
          <button type="button" class="secondary" aria-pressed={selected?.gameRef === offer.gameRef} aria-controls="rotation-relics"
            aria-label={`Показать реликвии: ${offer.displayName}`} onclick={() => select(offer)}>{offer.displayName}</button>
        {/each}
      </div>
    </section>
  {/if}

  <section id="rotation-relics" class="relic-section" aria-label="Реликвии текущей ротации">
    <div class="relic-heading"><div><h3 bind:this={relicHeading} tabindex="-1">{selected ? `Реликвии: ${selected.displayName}` : "Реликвии за Ая"} <span class="count">{relics.length}</span></h3>
      <p>{selected ? "Показаны детали выбранного предмета." : equipment.length ? "Выберите предмет выше, чтобы найти его детали." : "Реликвии из текущего ассортимента Варзии."}</p>
      {#if selected?.masteryRef && selected.equipmentCategory !== "warframe"}<div class="selected-mastery"><MasteryBadge gameRef={selected.masteryRef} /></div>{/if}</div>
      {#if selected}<button type="button" class="secondary reset" onclick={resetSelection}>Вся ротация</button>{/if}</div>
    <div class="relic-grid" aria-live="polite">
      {#each relics as relic (relic.gameRef)}
        {@const rewards = rotationRewards(relic, warframes.length ? warframes : equipment, selected?.gameRef)}
        <article class="relic-card">
          <header><div class="relic-name"><span class="relic-icon"><WorldActivityArtwork kind={relicArtwork(relic.relicSlug, relic.displayNameEn)} /></span><h4>{shortRelicName(relic.displayName)}</h4></div><span class="cost">{offerCost(relic, true)}</span></header>
          {#if rewards.length}<ul class="featured-rewards">{#each rewards as reward}<li>{reward.displayName}</li>{/each}</ul>
          {:else if catalogAvailable}<p class="relic-hint">{relic.rewards.length ? "Другие награды — в составе реликвии." : "Состав реликвии ещё не загружен."}</p>{/if}
          {#if relic.rewards.length}<details class="reward-details" name="rotation-rewards"><summary>Все награды и шансы</summary>
            <p>Одно открытие, без улучшения. Выпадет одна награда из списка.</p>
            <ul>{#each relic.rewards as reward}<li><span>{reward.displayName}</span><b>{chance(reward.chancePercent)}</b></li>{/each}</ul>
          </details>{/if}
        </article>
      {:else}<p class="notice">{selected ? "В полученном ассортименте пока нет реликвий с деталями этого предмета." : "Источник пока не передал реликвии этой ротации."}</p>{/each}
    </div>
  </section>

  {#if paidOffers.length}
    <details class="paid-offers"><summary>Готовые предметы, наборы и украшения · {paidOffers.length}</summary>
      <p>Покупка за Королевскую Ая. Для добычи деталей используйте реликвии выше.</p>
      <ul>{#each paidOffers as offer}<li><span>{offer.displayName}</span><b>{offerCost(offer, true)}</b></li>{/each}</ul>
    </details>
  {/if}
  {/if}
</div>

<style>
  .resurgence-content { display:grid; gap:1.25rem; margin-top:1.1rem; min-width:0; container-type:inline-size; }
  h3,h4,p { margin:0; }
  h3 { font-size:.875rem; font-weight:650; line-height:1.4; }
  h4 { font-size:.95rem; font-weight:650; }
  p { color:var(--text-muted); font-size:.8125rem; line-height:1.5; }
  .warframes { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:.8rem; margin-top:.65rem; }
  .warframe { position:relative; min-width:0; border:1px solid var(--border); border-radius:.7rem; background:linear-gradient(120deg,oklch(.92 .033 85),var(--surface-2) 75%); }
  .warframe:nth-child(even) { background:linear-gradient(120deg,oklch(.93 .022 240),var(--surface-2) 75%); }
  .warframe.selected { border-color:var(--accent); box-shadow:0 0 0 1px var(--accent); background:var(--accent-soft); }
  .warframe-select { display:flex; align-items:center; gap:.75rem; text-align:left; width:100%; min-height:7.5rem; padding:.75rem .9rem .4rem; border:0; border-radius:.7rem; background:none; color:var(--text); box-shadow:none; }
  .warframe-select:hover { background:oklch(.99 .01 80 / .35); }
  .warframe-select:active { scale:1; }
  .portrait { display:flex; align-items:center; justify-content:center; width:5.25rem; height:6.3rem; flex-shrink:0; }
  .portrait img { width:100%; height:100%; object-fit:contain; filter:drop-shadow(0 .35rem .2rem oklch(.25 .02 60 / .1)); }
  .portrait-fallback { display:inline-flex; width:3.2rem; height:3.2rem; color:var(--gold); }
  .warframe-copy { min-width:0; }
  .warframe-select strong { font-size:1.12rem; font-weight:650; line-height:1.3; display:block; text-wrap:balance; }
  .relic-link { display:flex; align-items:center; gap:.6rem; color:var(--accent-strong); font-size:.8125rem; line-height:1.4; margin-top:.55rem; }
  .mastery { padding:.25rem .9rem .75rem; }
  .equipment { display:flex; flex-wrap:wrap; gap:.45rem; margin-top:.6rem; }
  button.secondary { font-size:.8125rem; min-height:2.25rem; padding:.4rem .65rem; border-color:var(--border); border-radius:.45rem; font-weight:600; }
  .equipment button:hover { border-color:var(--accent); }
  .equipment button[aria-pressed="true"] { background:var(--accent-soft); border-color:var(--accent); color:var(--accent-strong); }
  .relic-section { padding-top:1.1rem; border-top:1px solid var(--border); }
  .relic-heading { display:flex; align-items:start; justify-content:space-between; flex-wrap:wrap; gap:.65rem; margin-bottom:.85rem; }
  .relic-heading h3 { font-size:1rem; }
  .relic-heading p { margin-top:.3rem; }
  .selected-mastery { margin-top:.45rem; }
  .count { display:inline-flex; align-items:center; justify-content:center; min-width:1.4rem; min-height:1.4rem; border-radius:.35rem; background:var(--surface-3); color:var(--text-muted); font-size:.75rem; margin-left:.3rem; vertical-align:middle; }
  .reset { flex-shrink:0; }
  .relic-grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); align-items:start; gap:.75rem; }
  .relic-grid > .notice { grid-column:1 / -1; }
  .relic-card { border:1px solid var(--border); border-radius:.6rem; padding:.85rem; min-width:0; background:var(--surface-1); }
  .relic-card:has(.reward-details[open]) { grid-column:1 / -1; border-color:var(--border-strong); background:var(--surface-2); }
  .relic-card header { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:.5rem; }
  .relic-name { display:flex; align-items:center; gap:.4rem; min-width:0; }
  .relic-icon { display:inline-flex; flex:none; width:2.2rem; height:2.2rem; color:var(--gold); }
  .cost { flex-shrink:0; font-size:.75rem; color:var(--text-muted); padding:.2rem .4rem; border-radius:.3rem; background:var(--surface-2); }
  .featured-rewards { padding:0; margin:.75rem 0 0; list-style:none; font-size:.8125rem; line-height:1.5; }
  .featured-rewards li { overflow-wrap:anywhere; }
  .featured-rewards li + li { margin-top:.4rem; }
  .relic-hint { margin-top:.65rem; }
  .reward-details { margin-top:.75rem; font-size:.8125rem; border-top:1px solid var(--border); }
  summary { cursor:pointer; color:var(--accent-strong); font-weight:600; line-height:1.5; padding:.65rem 0 .15rem; font-size:.75rem; }
  summary:hover { color:var(--accent); }
  .reward-details p,.paid-offers p { margin-top:.6rem; font-size:.75rem; }
  .reward-details ul,.paid-offers ul { list-style:none; padding:0; margin:.65rem 0 0; }
  .reward-details li,.paid-offers li { display:flex; justify-content:space-between; gap:.75rem; padding:.55rem 0; border-bottom:1px solid var(--border); line-height:1.5; }
  .reward-details li:last-child,.paid-offers li:last-child { border-bottom:0; }
  .reward-details li span,.paid-offers li span { min-width:0; overflow-wrap:anywhere; }
  li b { flex-shrink:0; font-weight:500; font-variant-numeric:tabular-nums; color:var(--text-muted); }
  .paid-offers { border-top:1px solid var(--border); font-size:.8125rem; }
  .paid-offers summary { padding:.85rem 0 .15rem; font-size:.8125rem; }
  .paid-offers ul { max-height:25rem; overflow:auto; scrollbar-width:thin; scrollbar-gutter:stable; overscroll-behavior:contain; }
  .notice { padding:.85rem; background:var(--surface-2); border:1px solid var(--border); border-radius:.5rem; }
  .text-button { display:block; background:none; color:var(--accent-strong); border:0; padding:.25rem 0 0; margin-top:.5rem; text-decoration:underline; text-underline-offset:3px; box-shadow:none; }
  @container (min-width:48rem) { .relic-grid { grid-template-columns:repeat(3,minmax(0,1fr)); } }
  @container (min-width:32rem) {
    .reward-details ul { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); column-gap:1.5rem; }
    .reward-details li:nth-last-child(-n+2) { border-bottom:0; }
  }
  @container (max-width:36rem) {
    .warframe-select { gap:.5rem; padding:.7rem .7rem .25rem; min-height:6.5rem; }
    .portrait { width:3.5rem; height:5.2rem; }
    .warframe-select strong { font-size:1rem; }
    .relic-link { font-size:.75rem; }
    .mastery { padding:.2rem .7rem .65rem; }
  }
  @container (max-width:26rem) { .warframes,.relic-grid { grid-template-columns:minmax(0,1fr); } }
</style>
