<script lang="ts">
  import MasteryBadge from "./MasteryBadge.svelte";
  import WorldActivityArtwork from "./WorldActivityArtwork.svelte";
  import { relicArtwork } from "./worldActivityArtwork";
  import { formatPlatinum, type LivePricingResult } from "./market";
  import { componentAvailableQuantity, refinementLabel, setOpportunity, type InsightsView, type SetInsightRow } from "./insights";
  import { planSetAcquisition, saleEstimate, type BudgetChoice, type OpportunityGoal } from "./opportunityPlan";

  export let row: SetInsightRow;
  export let mode: "complete" | "ready";
  export let choice: BudgetChoice | undefined;
  export let view: InsightsView;
  export let goal: OpportunityGoal;
  export let quote: LivePricingResult | undefined;
  export let busy = false;
  export let checkingOther = false;
  export let error = "";
  export let partsBusy = false;
  export let actionStatus = "";
  export let excluded = false;
  export let onCheck: () => void;
  export let onOpenSet: () => void;
  export let onOpenParts: () => void;
  export let onOpenRelic: (slug: string) => void;
  export let onExclude: () => void;
  export let onRestore: () => void;
  let openings: number | undefined = 10;
  let showRelics = false;
  let failedImage = false;
  let previousSlug = "";
  $: if (row.definition.setSlug !== previousSlug) {
    previousSlug = row.definition.setSlug; showRelics = false; failedImage = false;
  }
  const money = (value: number | null) => formatPlatinum(value,"ru");
  const openingWord = (n: number) => n === 1 ? "открытие" : n >= 2 && n <= 4 ? "открытия" : "открытий";
  $: opportunity = setOpportunity(row);
  $: sale = saleEstimate(row,goal,quote);
  $: cost = choice?.cost ?? opportunity.completionCost;
  $: ownValue = choice?.ownedValue ?? opportunity.ownedPartsOpportunityValue;
  $: profit = sale.price !== null && cost !== null && ownValue !== null ? sale.price - cost - ownValue : null;
  $: purchases = choice?.purchases ?? opportunity.missingParts.map(part => ({slug:part.slug,name:part.displayName,quantity:part.quantity,cost:part.estimatedCost}));
  $: missingSlugs = new Set(opportunity.missingParts.map(part => part.slug));
  $: relicCopies = view.relics.filter(relic => relic.rewards.some(reward => reward.definition.rewardSlug && missingSlugs.has(reward.definition.rewardSlug))).reduce((sum,relic) => sum + relic.ownedQuantity,0);
  $: validOpenings = openings !== undefined && Number.isInteger(openings) && openings >= 1 && openings <= 20;
  $: acquisition = mode === "complete" && showRelics && validOpenings ? planSetAcquisition(row,view.relics,view.voidTraces,openings) : null;
  $: needed = row.components.reduce((sum, part) => sum + part.definition.requiredQuantity,0);
  $: ownedForNext = Math.max(0,needed - opportunity.missingQuantity);
  $: checkedAt = quote && quote.quoteState !== "stale_cache" ? new Date(quote.fetchedAt) : null;
</script>

<div class="set-detail">
  <header class="detail-heading">
    {#if row.imageUrl && !failedImage}<img class="set-image" src={row.imageUrl} alt="" onerror={() => failedImage = true}/>{/if}
    <div><span class="selection-label">{mode === "ready" ? "Готовый комплект" : choice ? "В вашем плане · 1 комплект" : "Отдельный расчёт · 1 комплект"}</span><h3>{row.displayName.replace(/:\s*комплект\s*$/i,"")}</h3><MasteryBadge gameRef={row.definition.setGameRef}/></div>
  </header>

  {#if mode === "ready"}
    <div class="ready-banner"><strong>Доступно для продажи: {opportunity.sellableCompleteSets}</strong><p>Все детали уже есть. Ничего докупать не нужно.</p></div>
  {:else}
    <div class="progress-copy"><span>Есть детали: <strong>{ownedForNext} из {needed}</strong></span><span>Докупить: <strong>{opportunity.missingQuantity} шт.</strong></span></div>
    <div class="parts-progress" aria-hidden="true">{#each row.components as part}<span class:owned={componentAvailableQuantity(part) >= (opportunity.availableCompleteSets + 1) * part.definition.requiredQuantity} title={part.displayName}></span>{/each}</div>
    {#if excluded}<p class="notice">Этот комплект исключён из общего плана. Отдельный расчёт остаётся доступен.</p>
    {:else if !choice}<p class="notice">Этот расчёт не входит в общий итог: комплект не выбран при текущем бюджете и приоритете.</p>{/if}
  {/if}

  <section class="economics" aria-label={mode === "ready" ? "Оценка продажи готового комплекта" : "Расчёт выгоды выбранного комплекта"}>
    <dl>
      <div><dt>{sale.buyer ? "Заявка покупателя за комплект" : "Продажа комплекта, оценка"}</dt><dd>{sale.price !== null ? "≈ " : ""}{money(sale.price)}</dd></div>
      {#if mode === "complete"}
        <div><dt>Докупка деталей</dt><dd>{cost !== null ? "− " : ""}{money(cost)}</dd></div>
        <div><dt>Свои детали по отдельности</dt><dd>{ownValue !== null ? "− " : ""}{money(ownValue)}</dd></div>
        <div class="benefit" class:unprofitable={profit === null || profit <= 0}><dt>Выгода от сборки</dt><dd>{profit !== null && profit > 0 ? "+ " : ""}{money(profit)}</dd></div>
      {/if}
    </dl>
    {#if mode === "complete"}<p class="muted">{profit === null ? "Для оценки выгоды не хватает надёжных цен." : profit > 0 ? "На столько сборка выгоднее продажи имеющихся деталей." : "При этой цене докупка не выгоднее продажи своих деталей."}</p>{/if}
    <div class="price-check"><span>{checkedAt && Number.isFinite(checkedAt.getTime()) ? "Проверено в " + checkedAt.toLocaleTimeString("ru-RU",{hour:"2-digit",minute:"2-digit"}) : sale.price === null ? "Цена пока неизвестна" : "По сохранённым ценам"}</span><button type="button" class="secondary" disabled={busy || checkingOther} onclick={onCheck}>{busy ? "Проверяем цену…" : "Уточнить цену продажи"}</button></div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <details class="demand"><summary>На чём основана оценка</summary><p>{sale.price === null ? "Надёжной цены продажи пока нет. Уточните текущие предложения на рынке." : sale.buyer ? "На момент проверки найдена заявка игрока в игре на покупку одного комплекта." : "Ориентир — надёжная оценка цены продажи. Реальный покупатель может предложить меньше."}</p><p>{sale.volume !== null ? "Закрытых сделок в данных: " + sale.volume + ". Это ориентир спроса, а не срок продажи." : "Статистики завершённых сделок недостаточно для оценки спроса."}</p>{#if choice}<p>Стоимость одинаковых деталей распределена с учётом общего количества покупок в плане.</p>{/if}</details>
  </section>

  {#if mode === "ready"}
    <div class="primary-action"><button type="button" disabled={opportunity.sellableCompleteSets < 1} onclick={onOpenSet}>Перейти к продаже комплекта →</button><p class="muted">Откроется раздел продажи с выбранным комплектом.</p></div>
    <details class="composition"><summary>Проверить состав комплекта</summary><ul>{#each row.components as part}<li><span>{part.displayName}</span><span>{part.definition.requiredQuantity} шт.</span></li>{/each}</ul></details>
  {:else}
    <section class="purchase-route" aria-label="Недостающие детали">
      <div class="route-heading"><span class="step-number" aria-hidden="true">1</span><div><h4>Купите недостающие детали</h4><p>Для одного дополнительного комплекта</p></div></div>
      <ul class="purchase-list">{#each purchases as part (part.slug)}<li><span>{part.name}<small>Количество: {part.quantity}</small></span><strong>{part.cost !== null ? "≈ " : ""}{money(part.cost)}</strong></li>{/each}</ul>
      {#if purchases.length}<button class="buy-button" type="button" disabled={partsBusy} onclick={onOpenParts}>{partsBusy ? "Открываем рынок…" : "Найти продавцов деталей →"}</button><p class="muted market-destination">Откроются страницы деталей на Warframe Market.</p>{/if}
      {#if actionStatus}<p class="action-message" role="status">{actionStatus}</p>{/if}
    </section>
    <div class="sale-step"><span class="step-number" aria-hidden="true">2</span><div><h4>Продайте комплект целиком</h4><p>После покупки обновите инвентарь — комплект появится в разделе «Продать без докупки».</p></div></div>

    <details class="relic-route" bind:open={showRelics}>
      <summary><span>Можно ли обойтись своими реликвиями?</span><small>{relicCopies > 0 ? "Подходящих копий: " + relicCopies : "Подходящих реликвий пока нет"}</small></summary>
      <div class="relic-content">
        <p>Вероятностный путь вместо покупки части деталей. Расчёт для одиночных открытий.</p>
        <label class="opening-limit">Открыть не больше<input type="number" min="1" max="20" step="1" bind:value={openings} aria-invalid={!validOpenings}/></label>
        {#if !validOpenings}<p class="error" role="alert">Укажите целое число от 1 до 20.</p>
        {:else if acquisition?.steps.length}
          <div class="chance"><strong>{acquisition.chance.toLocaleString("ru-RU",{maximumFractionDigits:1})}%</strong><span>{acquisition.buy.length ? "шанс добрать оставшиеся детали после докупки" : "шанс получить все недостающие детали"}</span></div>
          <div class="chance-track" aria-hidden="true"><span style:width={acquisition.chance + "%"}></span></div>
          <p class="muted">{acquisition.openings} {openingWord(acquisition.openings)} · {acquisition.traces} следов Пустоты{view.voidTraces != null ? " из " + view.voidTraces : ""}</p>
          <ol class="relic-steps">{#each acquisition.steps as step}<li><span class="relic-image"><WorldActivityArtwork kind={relicArtwork(step.source.definition.relicSlug,step.source.definition.displayNameEn)}/></span><div><strong>{step.source.displayName.replace(/^Реликвия\s+/i,"")} ×{step.quantity}</strong><small>{refinementLabel(step.source.definition.refinement)}{step.source.definition.refinement !== step.target ? " → " + refinementLabel(step.target) : " · улучшение уже есть"}{step.traceCost ? " · " + step.traceCost + " следов" : ""}</small><button type="button" class="text-button" onclick={() => onOpenRelic(step.source.definition.relicSlug)}>Посмотреть реликвию →</button></div></li>{/each}</ol>
          {#if acquisition.buy.length}<div class="remaining-buy"><strong>Эти детали всё равно нужно докупить</strong><ul>{#each acquisition.buy as part}<li><span>{part.displayName} ×{part.quantity}</span><span>{money(part.estimatedCost)}</span></li>{/each}</ul></div>{/if}
          <p class="muted">Реликвии будут потрачены. Их оценка для продажи: {acquisition.relicValue === null ? "неизвестна" : money(acquisition.relicValue)}. Выпадение нужных деталей не гарантировано.</p>
          <details class="method"><summary>Как рассчитан этот путь</summary><p>Используем имеющиеся копии и следы. Подбор стремится к 80% вероятности и ограничен указанным числом открытий. {view.voidTraces == null ? "Баланс следов неизвестен, поэтому улучшения не предлагаются." : ""} Результаты открытий считаются независимыми.</p></details>
        {:else}<p class="muted">В ваших реликвиях нет пути к недостающим деталям. Можно найти их продавцов выше.</p>{/if}
      </div>
    </details>
    <footer class="detail-footer">{#if excluded}<button type="button" class="text-button" onclick={onRestore}>Вернуть в подбор</button>{:else if choice}<button type="button" class="text-button" onclick={onExclude}>Не учитывать этот комплект</button>{/if}</footer>
  {/if}
</div>

<style>
  .set-detail { min-width:0; } h3,h4,p { margin:0; } h3 { font-size:1.25rem; line-height:1.3; } h4 { font-size:.9375rem; line-height:1.4; } p { line-height:1.5; }
  .detail-heading { display:flex; align-items:center; gap:.85rem; } .detail-heading>div { min-width:0; } .set-image { width:4rem; height:4rem; object-fit:contain; flex:none; }
  .selection-label { display:block; color:var(--text-muted); font-size:.75rem; margin-bottom:.35rem; }
  .progress-copy { display:flex; flex-wrap:wrap; justify-content:space-between; gap:.4rem; margin-top:1.1rem; color:var(--text-muted); font-size:.8125rem; } .progress-copy strong { color:var(--text); font-weight:600; }
  .parts-progress { display:flex; gap:.25rem; margin:.5rem 0 1rem; } .parts-progress span { height:.25rem; background:var(--surface-3); flex:1; border-radius:.2rem; } .parts-progress .owned { background:var(--success); }
  .notice { background:var(--surface-2); border-radius:.4rem; padding:.65rem; color:var(--text-muted); font-size:.8125rem; margin:.75rem 0; }
  .economics { margin-top:1.1rem; } dl { margin:0; } dl>div { display:flex; justify-content:space-between; gap:.75rem; align-items:baseline; padding:.35rem 0; } dt { font-size:.8125rem; color:var(--text-muted); } dd { margin:0; font-size:1rem; font-weight:600; white-space:nowrap; font-variant-numeric:tabular-nums; }
  dl>div:first-child dd { font-size:1.25rem; } dl>.benefit { margin-top:.5rem; border-top:1px solid var(--border); padding-top:.75rem; } .benefit dt { font-weight:650; color:var(--text); } .benefit dd { font-size:1.55rem; color:var(--success); font-weight:650; } .benefit.unprofitable dd { color:var(--text-muted); }
  .muted { color:var(--text-muted); font-size:.75rem; margin-top:.45rem; }
  .price-check { display:flex; align-items:center; justify-content:space-between; flex-wrap:wrap; gap:.5rem; margin-top:1rem; } .price-check>span { color:var(--text-muted); font-size:.75rem; } .price-check button { font-size:.75rem; }
  .error { color:var(--danger); font-size:.8125rem; margin-top:.7rem; } summary { cursor:pointer; font-size:.8125rem; font-weight:600; }
  .demand { margin-top:.8rem; } .demand summary { font-size:.75rem; color:var(--text-muted); font-weight:400; } .demand p,.method p { font-size:.8125rem; color:var(--text-muted); margin-top:.5rem; }
  .purchase-route { border-top:1px solid var(--border); margin-top:1.2rem; padding-top:1.15rem; } .route-heading,.sale-step { display:flex; gap:.7rem; align-items:start; }
  .step-number { display:inline-flex; flex:none; justify-content:center; align-items:center; width:1.65rem; height:1.65rem; font-size:.8125rem; font-weight:650; border-radius:50%; background:var(--accent-soft); color:var(--accent-strong); }
  .route-heading p,.sale-step p { font-size:.75rem; color:var(--text-muted); margin-top:.2rem; }
  ul { list-style:none; padding:0; margin:.8rem 0; } ul li { display:flex; justify-content:space-between; align-items:start; gap:.7rem; border-bottom:1px solid var(--border); padding:.65rem 0; font-size:.8125rem; line-height:1.4; } ul li:last-child { border:0; } ul li>strong,ul li>span:last-child { white-space:nowrap; }
  small { display:block; font-size:.75rem; color:var(--text-muted); font-weight:400; margin-top:.2rem; line-height:1.4; }
  .buy-button,.primary-action>button { width:100%; min-height:2.55rem; } .market-destination { text-align:center; } .action-message { font-size:.8125rem; margin-top:.65rem; color:var(--text-muted); }
  .sale-step { margin-top:1.2rem; } .sale-step .step-number { background:var(--surface-2); color:var(--text-muted); }
  .relic-route { margin-top:1.2rem; border:1px solid var(--border); border-radius:.6rem; overflow:hidden; } .relic-route>summary { padding:.9rem; background:var(--surface-2); } .relic-route>summary small { margin-left:1rem; } .relic-content { padding:.9rem; }
  .relic-content>p { font-size:.8125rem; color:var(--text-muted); } .opening-limit { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:.5rem; font-size:.8125rem; margin:1rem 0; } .opening-limit input { width:5rem; padding:.45rem .6rem; border:1px solid var(--border); border-radius:.4rem; background:var(--surface-1); color:var(--text); font:inherit; }
  .chance { display:flex; gap:.65rem; align-items:center; } .chance strong { font-size:1.8rem; font-weight:650; letter-spacing:-.03em; white-space:nowrap; color:var(--accent-strong); } .chance>span { font-size:.8125rem; line-height:1.35; }
  .chance-track { height:.35rem; border-radius:1rem; background:var(--surface-3); margin:.65rem 0; overflow:hidden; } .chance-track>span { display:block; height:100%; background:var(--accent); border-radius:inherit; }
  .relic-steps { list-style:none; padding:0; margin:1rem 0; } .relic-steps li { display:flex; align-items:start; gap:.6rem; padding:.65rem 0; border-top:1px solid var(--border); } .relic-steps strong { font-size:.875rem; } .relic-image { display:inline-flex; flex:none; width:2rem; height:2rem; }
  .remaining-buy { background:var(--surface-2); border-radius:.4rem; padding:.65rem; margin:.7rem 0; font-size:.8125rem; } .remaining-buy ul { margin:.2rem 0 0; }
  .method { margin-top:.8rem; } .method summary { font-size:.75rem; color:var(--text-muted); }
  .text-button { padding:.2rem 0; min-height:1.7rem; border:0; color:var(--accent-strong); font-size:.75rem; font-weight:500; } .text-button:hover { background:none; text-decoration:underline; }
  .detail-footer { text-align:right; margin-top:.75rem; } .detail-footer:empty { display:none; } .ready-banner { padding:.85rem; background:var(--success-soft); border-radius:.5rem; margin-top:1rem; font-size:.875rem; } .ready-banner p { margin-top:.3rem; font-size:.8125rem; color:var(--text-muted); }
  .primary-action { margin-top:1.1rem; } .composition { margin-top:1rem; }
</style>
