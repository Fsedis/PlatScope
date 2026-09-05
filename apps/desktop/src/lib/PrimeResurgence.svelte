<script lang="ts">
  import MasteryBadge from "./MasteryBadge.svelte";
  import { offerCost, type ActivityOffer } from "./worldActivity";
  import { rotationEquipment, rotationRelics, rotationRewards } from "./primeResurgence";

  export let offers: ActivityOffer[];
  export let catalogAvailable: boolean;
  export let incomplete: boolean;
  export let onOpenSettings: () => void;
  let selectedRef = "";
  $: equipment = rotationEquipment(offers);
  $: if (selectedRef && !equipment.some(offer => offer.gameRef === selectedRef)) selectedRef = "";
  $: warframes = equipment.filter(offer => offer.equipmentCategory === "warframe");
  $: otherEquipment = equipment.filter(offer => offer.equipmentCategory !== "warframe");
  $: selected = equipment.find(offer => offer.gameRef === selectedRef);
  $: relics = rotationRelics(offers, selected?.gameRef);
  $: paidOffers = offers.filter(offer => offer.kind !== "relic");
  function select(offer: ActivityOffer) { selectedRef = selectedRef === offer.gameRef ? "" : offer.gameRef; }
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
            <button type="button" class="warframe-select" aria-pressed={selected?.gameRef === offer.gameRef}
              aria-label={`Показать реликвии: ${offer.displayName}`} onclick={() => select(offer)}>
              {#if offer.imageUrl}<img src={offer.imageUrl} alt="" loading="lazy" onerror={event => (event.currentTarget as HTMLImageElement).hidden = true} />{/if}
              <span><strong>{offer.displayName}</strong><span class="relic-link">Реликвии: {rotationRelics(offers, offer.gameRef).length} <span aria-hidden="true">↓</span></span></span>
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
          <button type="button" class="secondary" aria-pressed={selected?.gameRef === offer.gameRef}
            aria-label={`Показать реликвии: ${offer.displayName}`} onclick={() => select(offer)}>{offer.displayName}</button>
        {/each}
      </div>
    </section>
  {/if}

  <section class="relic-section" aria-label="Реликвии текущей ротации">
    <div class="relic-heading"><div><h3>{selected ? `Реликвии: ${selected.displayName}` : "Реликвии за Ая"} <span class="count">{relics.length}</span></h3>
      <p>{selected ? "Из этих реликвий выпадают детали выбранного предмета." : equipment.length ? "Ниже — детали варфреймов. Выберите предмет выше, чтобы найти его награды." : "Реликвии из текущего ассортимента Варзии."}</p>
      {#if selected?.masteryRef && selected.equipmentCategory !== "warframe"}<div class="selected-mastery"><MasteryBadge gameRef={selected.masteryRef} /></div>{/if}</div>
      {#if selected}<button type="button" class="secondary reset" onclick={() => selectedRef = ""}>Вся ротация</button>{/if}</div>
    <div class="relic-grid" aria-live="polite">
      {#each relics as relic (relic.gameRef)}
        {@const rewards = rotationRewards(relic, warframes.length ? warframes : equipment, selected?.gameRef)}
        <article class="relic-card">
          <header><h4>{shortRelicName(relic.displayName)}</h4><span class="cost">{offerCost(relic, true)}</span></header>
          {#if rewards.length}<ul class="featured-rewards">{#each rewards as reward}<li>{reward.displayName}</li>{/each}</ul>
          {:else if catalogAvailable}<p class="relic-hint">{relic.rewards.length ? "Другие награды — в составе реликвии." : "Состав реликвии ещё не загружен."}</p>{/if}
          {#if relic.rewards.length}<details class="reward-details"><summary>Все награды и шансы</summary>
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
  .resurgence-content { display:grid; gap:1.15rem; margin-top:1rem; min-width:0; container-type:inline-size; }
  h3,h4,p { margin:0; } h3 { font-size:.88rem; } h4 { font-size:.9rem; }
  p { color:var(--text-muted); font-size:.75rem; line-height:1.5; }
  .warframes { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:.65rem; margin-top:.6rem; }
  .warframe { border:1px solid var(--border); border-radius:.65rem; background:var(--surface-2); overflow:hidden; }
  .warframe.selected { border-color:var(--accent); background:var(--accent-soft); }
  .warframe-select { display:flex; align-items:center; gap:.7rem; text-align:left; width:100%; padding:.75rem; border:0; background:none; color:var(--text); box-shadow:none; }
  .warframe-select:hover { background:var(--accent-soft); }
  .warframe-select img { width:3.5rem; height:4rem; object-fit:contain; flex-shrink:0; }
  .warframe-select strong { font-size:1rem; line-height:1.3; display:block; }
  .relic-link { display:block; color:var(--accent-strong); font-size:.75rem; margin-top:.4rem; }
  .mastery { padding:0 .75rem .65rem; }
  .equipment { display:flex; flex-wrap:wrap; gap:.4rem; margin-top:.6rem; }
  button.secondary { font-size:.75rem; padding:.4rem .65rem; }
  .equipment button[aria-pressed="true"] { background:var(--accent-soft); border-color:var(--accent); }
  .relic-heading { display:flex; align-items:start; justify-content:space-between; gap:.5rem; margin-bottom:.65rem; }
  .relic-heading p { margin-top:.35rem; }
  .selected-mastery { margin-top:.4rem; }
  .count { color:var(--text-muted); font-size:.75rem; margin-left:.25rem; }
  .reset { flex-shrink:0; }
  .relic-grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); align-items:start; gap:.6rem; }
  .relic-grid > .notice { grid-column:1 / -1; }
  .relic-card { border:1px solid var(--border); border-radius:.55rem; padding:.75rem; min-width:0; }
  .relic-card header { display:flex; align-items:center; justify-content:space-between; gap:.5rem; }
  .cost { flex-shrink:0; font-size:.75rem; color:var(--text-muted); }
  .featured-rewards { padding:0; margin:.65rem 0 0; list-style:none; font-size:.76rem; line-height:1.5; }
  .featured-rewards li + li { margin-top:.25rem; }
  .relic-hint { margin-top:.6rem; }
  .reward-details { margin-top:.65rem; font-size:.73rem; border-top:1px solid var(--border); padding-top:.55rem; }
  summary { cursor:pointer; color:var(--accent-strong); font-weight:650; }
  .reward-details p,.paid-offers p { margin-top:.7rem; }
  .reward-details ul,.paid-offers ul { list-style:none; padding:0; margin:.6rem 0 0; }
  .reward-details li,.paid-offers li { display:flex; justify-content:space-between; gap:.7rem; padding:.45rem 0; border-bottom:1px solid var(--border); }
  li b { flex-shrink:0; font-weight:500; font-variant-numeric:tabular-nums; }
  .paid-offers { border-top:1px solid var(--border); padding-top:.85rem; font-size:.76rem; }
  .notice { padding:.65rem .75rem; background:var(--surface-2); border-radius:.5rem; }
  .text-button { background:none; color:var(--accent-strong); border:0; padding:0; text-decoration:underline; box-shadow:none; }
  @container (max-width:30rem) { .warframes,.relic-grid { grid-template-columns:minmax(0,1fr); } .relic-heading { flex-wrap:wrap; } }
</style>
