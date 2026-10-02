<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { refinementLabel } from "./insights";
  import { overlayContentScale } from "./relicRewards";
  import { relicRemainingLabel, relicRewardNeedLabels, relicSelectionEraLabel, type RelicSelectionRow, type RelicSelectionView } from "./relicSelection";

  let view: RelicSelectionView | null = null;
  let loading = true;
  let unavailable = false;
  let receivedUpdate = false;
  let availableHeight = 0;
  let panelHeight = 0;
  $: recommendations = view?.recommendations.slice(0, 3) ?? [];
  $: details = view?.selected ?? recommendations[0] ?? null;
  $: currentRewards = view?.selectedRelicName && view.selectedRewards?.length ? view.selectedRewards : null;
  $: detailRewards = currentRewards ?? details?.rewards ?? [];
  $: detailsCurrent = currentRewards !== null || !!view?.selected;
  $: uncertainChances = detailsCurrent && detailRewards.some(reward => reward.chancePercent === null);
  $: contentScale = overlayContentScale(view?.overlayScale ?? 1, window.devicePixelRatio);
  $: fitScale = availableHeight > 0 && panelHeight > 0 ? Math.min(1, availableHeight / (panelHeight * contentScale)) : 1;
  const amount = (value: number | null | undefined) => value == null ? "—" : value.toLocaleString("ru-RU", {maximumFractionDigits:1});
  const percent = (value: number) => value.toLocaleString("ru-RU", {maximumFractionDigits:2}) + "%";
  const price = (value: number | null | undefined) => value == null ? "—" : amount(value) + " пл.";

  function needSummary(relic: RelicSelectionRow): string {
    const labels: string[] = [];
    if (relic.goalChancePercent > 0) labels.push("Личная цель");
    if (relic.craftingChancePercent > 0) labels.push("Для сборки");
    if (relic.masteryChancePercent > 0) labels.push("Для ранга");
    return labels.join(" · ");
  }

  onMount(() => {
    document.documentElement.classList.add("overlay-mode");
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void listen<RelicSelectionView>("relic-selection-updated", event => {
      if (disposed) return;
      receivedUpdate = true;
      view = event.payload;
      loading = false;
      unavailable = false;
    }).then(cleanup => { if (disposed) cleanup(); else unlisten = cleanup; }).catch(() => { if (!disposed && !view) unavailable = true; });
    void invoke<RelicSelectionView | null>("latest_relic_selection")
      .then(result => { if (!disposed && !receivedUpdate) view = result; })
      .catch(() => { if (!disposed && !receivedUpdate) unavailable = true; })
      .finally(() => { if (!disposed) loading = false; });
    return () => { disposed = true; unlisten?.(); document.documentElement.classList.remove("overlay-mode"); };
  });
</script>

<main class="selection-shell" aria-label="Подсказка выбора реликвии" bind:clientHeight={availableHeight}>
  <div class="scaled-content" style={`width:${100 / contentScale}%;transform:scale(${contentScale * fitScale});`}>
    <section class="selection-panel" aria-live="polite" bind:offsetHeight={panelHeight}>
      <header class="panel-header">
        <div><span class="eyebrow">Выбор реликвии{#if view?.preview}<span class="preview-marker">Предпросмотр</span>{/if}</span><h1>{view?.missionName ?? "Перед миссией"}</h1></div>
        <span class="era-chip">{relicSelectionEraLabel(view?.era ?? null)}</span>
      </header>

      {#if loading && !view}
        <p class="empty-state">Загружаем ваши реликвии…</p>
      {:else if unavailable && !view}
        <p class="empty-state">Не удалось получить подсказку выбора реликвии.</p>
      {:else if !view}
        <p class="empty-state">Откройте выбор реликвии в Warframe.</p>
      {:else}
        <section class="selected-relic" aria-label={view.selectedBrowsing ? "Просматриваемая реликвия" : "Выбранная реликвия"}>
          <div class="section-heading"><h2>{view.selectedBrowsing ? "Просматривается в игре" : "Выбрана в игре"}</h2>{#if view.openedThisSession !== null}<span>Открыто: {view.openedThisSession}</span>{/if}</div>
          {#if view.selected}
            <strong class="selected-name">{view.selected.displayName}</strong>
            {#if view.selected.displayNameEn && view.selected.displayNameEn !== view.selected.displayName}<span class="english-name" lang="en" translate="no">{view.selected.displayNameEn}</span>{/if}
            <p class="selected-info"><span>{refinementLabel(view.selected.refinement)}</span><strong>{relicRemainingLabel(view.selected)}</strong></p>
          {:else if view.selectedRelicName}
            <strong class="selected-name">{view.selectedRelicName}</strong>
            <p class="selected-info"><span>{view.selectedRefinementKnown ? "Вариант не найден" : "Улучшение не определено"}</span><strong>{view.selectedRemainingQuantity === null ? "Остаток не определён" : `Всего этого названия: ${view.selectedRemainingQuantity}`}</strong></p>
          {:else}
            <p class="selection-prompt">Выберите реликвию в списке игры.</p>
          {/if}
          {#if !view.selected && !view.selectedRelicName && view.lastOpenedRelicName}<p class="last-opened">Последняя: {view.lastOpenedRelicName}</p>{/if}
        </section>

        {#if view.message && recommendations.length}<p class="view-message">{view.message}</p>{/if}
        {#if !view.inventoryAvailable}
          <p class="empty-state">Обновите инвентарь в PlatScope, чтобы увидеть свои реликвии.</p>
        {:else}
          <section class="recommendations" aria-label="Подходящие реликвии">
            <div class="section-heading"><h2>Подходящие реликвии</h2><span>Из вашего инвентаря</span></div>
            {#if recommendations.length}
              <ol>
                {#each recommendations as relic, index (JSON.stringify(relic.key))}
                  <li class:best={index === 0}>
                    <span class="rank">{index + 1}</span>
                    <div class="recommendation-main">
                      <div class="relic-title"><strong>{relic.displayName}</strong><span>{refinementLabel(relic.refinement)}</span></div>
                      {#if relic.displayNameEn && relic.displayNameEn !== relic.displayName}<span class="english-name" lang="en" translate="no">{relic.displayNameEn}</span>{/if}
                      <p class="relic-count">{relicRemainingLabel(relic)}{#if needSummary(relic)}<span> · {needSummary(relic)}</span>{/if}</p>
                      <div class="expected-value"><span>Средняя награда: <strong>{relic.expectedPlatinum === null ? "—" : "≈ " + price(relic.expectedPlatinum)}</strong></span><span>{relic.expectedDucats === null ? "—" : "≈ " + amount(relic.expectedDucats)} дук.</span></div>
                      {#if relic.pricingCoverage === "partial" || (relic.expectedDucats !== null && relic.ducatCoveragePercent < 99.9)}<p class="partial-price">Часть наград без оценки</p>{/if}
                    </div>
                  </li>
                {/each}
              </ol>
            {:else}
              <p class="empty-state">{view.message ?? (view.era === null ? "Эра миссии ещё не определена." : "В инвентаре нет подходящих реликвий.")}</p>
            {/if}
          </section>

          {#if details || currentRewards}
            <section class="reward-details" aria-label="Награды реликвии">
              <div class="section-heading"><h2>{detailsCurrent ? view.selectedBrowsing ? "Награды просматриваемой реликвии" : "Награды выбранной реликвии" : "Лучший вариант"}</h2>{#if !detailsCurrent && details}<span>{details.displayName}</span>{/if}</div>
              {#if uncertainChances}<p class="estimate-note">Шансы зависят от улучшения.</p>{/if}
              <ul class="rewards">
                {#each detailRewards.slice(0, 6) as reward, index (reward.gameRef + "|" + index)}
                  {@const needs = relicRewardNeedLabels(reward)}
                  <li class:needed={needs.length > 0}>
                    <div class="reward-copy"><strong>{reward.displayName}</strong>{#if reward.displayNameEn && reward.displayNameEn !== reward.displayName}<span class="english-name" lang="en" translate="no">{reward.displayNameEn}</span>{/if}<div class="reward-facts">{#if reward.chancePercent !== null}<span>{percent(reward.chancePercent)}</span>{/if}<span>{reward.ownedQuantity === null ? "Количество неизвестно" : `Есть: ${reward.ownedQuantity}`}</span>{#if reward.neededQuantity !== null && reward.neededQuantity > 0}<span>Нужно: {reward.neededQuantity}</span>{/if}</div>{#if needs.length}<p class="need-labels">{needs.join(" · ")}</p>{/if}</div>
                    <div class="reward-values"><strong>{price(reward.price)}</strong><span>{reward.ducats === null ? "—" : reward.ducats} дук.</span></div>
                  </li>
                {/each}
              </ul>
              {#if !detailRewards.length}<p class="empty-state">Состав реликвии пока неизвестен.</p>{/if}
            </section>
          {/if}
          <p class="estimate-note">Средняя награда — оценка за открытие, результат может отличаться.</p>
        {/if}
      {/if}
    </section>
  </div>
</main>

<style>
  .selection-shell { width:100vw; height:100vh; background:transparent; color:var(--text); overflow:hidden; user-select:none; }
  .scaled-content { transform-origin:left top; }
  .selection-panel { display:flex; flex-direction:column; gap:.5rem; width:100%; padding:.7rem; overflow:hidden; border:1px solid oklch(0.64 0.045 66 / .72); border-radius:.78rem; background:oklch(0.965 0.022 80 / .97); box-shadow:0 10px 24px oklch(0.25 0.025 55 / .16),inset 0 1px 0 oklch(1 0 0 / .55); font-size:.75rem; }
  .panel-header,.section-heading,.selected-info,.expected-value { display:flex; align-items:center; justify-content:space-between; gap:.4rem; }
  .panel-header { align-items:start; padding-bottom:.6rem; border-bottom:1px solid var(--border); }
  .panel-header > div { min-width:0; }
  .eyebrow { display:block; font-size:.62rem; color:var(--accent); letter-spacing:.07em; text-transform:uppercase; font-weight:750; }
  .preview-marker { display:inline-block; margin-left:.5rem; padding:.1rem .3rem; border:1px solid var(--border); border-radius:.3rem; color:var(--text-muted); font-size:.56rem; letter-spacing:normal; text-transform:none; }
  h1 { font-size:1rem; line-height:1.25; margin:.2rem 0 0; overflow-wrap:anywhere; }
  .era-chip { flex-shrink:0; border:1px solid var(--border); border-radius:999px; padding:.2rem .5rem; color:var(--text-muted); font-size:.68rem; font-weight:750; }
  h2 { font-size:.75rem; line-height:1.3; margin:0; }
  .section-heading > span { min-width:0; text-align:right; color:var(--text-muted); font-size:.63rem; }
  .selected-relic { padding:.65rem; border:1px solid var(--border); border-left:3px solid var(--gold); border-radius:.5rem; background:var(--accent-soft); }
  .selected-name { display:block; margin-top:.4rem; font-size:.9rem; }
  .selected-info { flex-wrap:wrap; margin:.25rem 0 0; font-size:.68rem; color:var(--text-muted); }
  .selected-info strong { color:var(--accent-strong); }
  .selection-prompt { margin:.45rem 0 0; color:var(--text-muted); }
  .last-opened { margin:.3rem 0 0; font-size:.63rem; color:var(--text-muted); }
  ol,ul { list-style:none; padding:0; margin:.4rem 0 0; }
  .recommendations li { display:flex; gap:.5rem; padding:.4rem .45rem; border:1px solid var(--border); border-radius:.45rem; background:var(--surface-1); }
  .recommendations li + li { margin-top:.3rem; }
  .recommendations li.best { border-color:var(--gold); background:oklch(0.965 0.035 78 / .98); }
  .rank { flex-shrink:0; display:grid; place-items:center; width:1.2rem; height:1.2rem; border-radius:50%; color:var(--accent); background:var(--accent-soft); font-size:.65rem; font-weight:800; }
  .recommendation-main { min-width:0; flex:1; }
  .relic-title { display:flex; align-items:baseline; justify-content:space-between; gap:.3rem; }
  .relic-title strong { font-size:.75rem; line-height:1.25; }
  .relic-title > span { font-size:.61rem; color:var(--text-muted); flex-shrink:0; }
  .relic-count { margin:.2rem 0; color:var(--text-muted); font-size:.62rem; line-height:1.3; }
  .relic-count > span { color:var(--success); }
  .expected-value { font-size:.67rem; font-variant-numeric:tabular-nums; }
  .expected-value > span:last-child { color:var(--text-muted); white-space:nowrap; }
  .expected-value strong { color:var(--accent-strong); }
  .partial-price { margin:.15rem 0 0; color:var(--text-muted); font-size:.6rem; }
  .rewards { border-top:1px solid var(--border); }
  .rewards li { display:flex; align-items:center; gap:.6rem; padding:.3rem; border-bottom:1px solid var(--border); }
  .rewards li.needed { border-left:3px solid var(--gold); padding-left:.4rem; background:oklch(0.96 0.035 82 / .8); }
  .reward-copy { min-width:0; flex:1; }
  .reward-copy > strong { display:-webkit-box; -webkit-line-clamp:2; line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; font-size:.71rem; line-height:1.2; }
  .english-name { display:block; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:.59rem; line-height:1.2; color:var(--text-muted); }
  .reward-facts { display:flex; flex-wrap:wrap; gap:.2rem .5rem; margin-top:.2rem; font-size:.61rem; color:var(--text-muted); line-height:1.2; }
  .need-labels { margin:.15rem 0 0; color:var(--success); font-size:.61rem; font-weight:700; line-height:1.2; }
  .reward-values { flex-shrink:0; display:flex; flex-direction:column; gap:.2rem; text-align:right; font-size:.68rem; font-variant-numeric:tabular-nums; }
  .reward-values strong { color:var(--accent-strong); }
  .reward-values span { color:var(--text-muted); font-size:.62rem; }
  .empty-state { margin:.45rem 0; color:var(--text-muted); line-height:1.5; }
  .view-message { margin:0; color:var(--text-muted); line-height:1.35; font-size:.68rem; }
  .estimate-note { margin:0; font-size:.6rem; color:var(--text-muted); line-height:1.3; }
  @media (max-height:700px) { .selection-panel { padding:.6rem; gap:.45rem; } .recommendations li { padding:.35rem .4rem; } .rewards li { padding-block:.3rem; } }
</style>
