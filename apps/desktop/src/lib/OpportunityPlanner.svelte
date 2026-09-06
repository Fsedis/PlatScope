<script lang="ts">
  import { onMount, tick } from "svelte";
  import PlanSetDetail from "./PlanSetDetail.svelte";
  import { formatPlatinum, type LivePricingResult } from "./market";
  import { setOpportunity, setPriceComparison, type InsightsView, type SetInsightRow } from "./insights";
  import { planCompletionBudget, saleEstimate, type OpportunityGoal } from "./opportunityPlan";

  export let view: InsightsView;
  export let sets: SetInsightRow[];
  export let quotes: Map<string, LivePricingResult>;
  export let busySlug = "";
  export let errors: Map<string, string>;
  export let onCheck: (row: SetInsightRow) => void;
  export let onOpenSet: (row: SetInsightRow) => void;
  export let onOpenParts: (row: SetInsightRow) => void;
  export let onOpenRelic: (slug: string) => void;
  export let onOpenSettings: () => void;
  export let actionStatus = "";
  export let actionStatusSlug = "";
  export let partsBusySlug = "";
  export let initialSetSlug = "";

  let goal: OpportunityGoal = "profit";
  let budget: number | undefined = 100;
  let selectedSlug = initialSetSlug;
  let selectionMode: "complete" | "ready" = "complete";
  let excluded = new Set<string>();
  let query = "";
  let showAllReady = false;
  let workspace: HTMLDivElement;
  let detail: HTMLElement;
  let listHeading: HTMLHeadingElement;
  let searchInput: HTMLInputElement;
  let announcement = "";
  const money = (value: number | null) => formatPlatinum(value, "ru");
  const name = (row: SetInsightRow) => row.displayName.replace(/:\s*комплект\s*$/i, "");
  const normalize = (text: string) => text.toLocaleLowerCase("ru").replaceAll("ё", "е").trim();
  const copies = (n: number) => n % 100 >= 11 && n % 100 <= 14 ? "комплектов" : n % 10 === 1 ? "комплект" : n % 10 >= 2 && n % 10 <= 4 ? "комплекта" : "комплектов";
  $: validBudget = budget !== undefined && Number.isInteger(budget) && budget >= 0 && budget <= 1_000_000;
  $: eligible = view.inventoryAvailable ? sets.filter(row => !excluded.has(row.definition.setSlug)) : [];
  $: plan = planCompletionBudget(eligible, validBudget ? budget! : 0, goal, quotes);
  $: choiceBySlug = new Map(plan.choices.map(choice => [choice.row.definition.setSlug, choice]));
  $: ready = (view.inventoryAvailable ? sets : []).filter(row => {
    const estimate = saleEstimate(row, goal, quotes.get(row.definition.setSlug));
    return setOpportunity(row).sellableCompleteSets > 0 && estimate.price !== null
      && ["set", "equivalent"].includes(setPriceComparison(row, estimate.price));
  }).sort((a, b) => {
    const left = saleEstimate(a, goal, quotes.get(a.definition.setSlug));
    const right = saleEstimate(b, goal, quotes.get(b.definition.setSlug));
    return goal === "speed" ? Number(right.buyer) - Number(left.buyer) || (right.volume ?? -1) - (left.volume ?? -1) : (right.price ?? 0) - (left.price ?? 0);
  });
  $: searching = Boolean(query.trim());
  $: results = searching ? sets.filter(row => normalize(row.displayName + " " + row.definition.displayNameEn).includes(normalize(query))) : plan.choices.map(choice => choice.row);
  $: selected = sets.find(row => row.definition.setSlug === selectedSlug) ?? plan.choices[0]?.row ?? ready[0];
  $: selectedMode = selectedSlug && selected?.definition.setSlug === selectedSlug ? selectionMode : plan.choices.length ? "complete" : "ready";
  $: selectedChoice = selected ? choiceBySlug.get(selected.definition.setSlug) : undefined;

  function savePreferences() {
    const amount = budget !== undefined && Number.isInteger(budget) && budget >= 0 && budget <= 1_000_000 ? budget : 100;
    try { localStorage.setItem("platscope.opportunity-plan.v1", JSON.stringify({goal,budget:amount,excluded:[...excluded]})); } catch { /* Настройки необязательны для расчёта. */ }
  }
  function setBudget(value: number) { budget = value; savePreferences(); }
  async function select(row: SetInsightRow, mode: "complete" | "ready" = "complete") {
    selectedSlug = row.definition.setSlug; selectionMode = mode;
    await tick();
    if (workspace.clientWidth <= 1024) { detail.scrollIntoView({block:"start"}); detail.focus({preventScroll:true}); }
  }
  function returnToList() { listHeading.scrollIntoView({block:"start"}); listHeading.focus({preventScroll:true}); }
  function exclude(row: SetInsightRow) {
    selectedSlug = row.definition.setSlug; selectionMode = "complete";
    excluded = new Set(excluded).add(row.definition.setSlug);
    savePreferences();
    announcement = name(row) + " исключён. План пересчитан без этого комплекта.";
  }
  function restore() { excluded = new Set(); savePreferences(); announcement = "Все комплекты снова участвуют в подборе."; }
  function restoreSelected() { if (selected) excluded = new Set([...excluded].filter(slug => slug !== selected.definition.setSlug)); savePreferences(); }
  onMount(() => {
    try {
      const saved = JSON.parse(localStorage.getItem("platscope.opportunity-plan.v1") ?? "null");
      if (saved?.goal === "profit" || saved?.goal === "speed") goal = saved.goal;
      if (Number.isInteger(saved?.budget) && saved.budget >= 0 && saved.budget <= 1_000_000) budget = saved.budget;
      if (Array.isArray(saved?.excluded)) excluded = new Set(saved.excluded.filter((slug: unknown): slug is string => typeof slug === "string" && sets.some(row => row.definition.setSlug === slug)));
    } catch { /* Используем значения по умолчанию. */ }
  });
</script>

<section class="planner" aria-label="Мой план продажи комплектов">
  <header class="plan-controls">
    <div class="intro"><h2>Что продать и дособрать</h2><p>Подберём выгодную докупку и покажем комплекты, которые уже можно продать.</p></div>
    <div class="controls">
      <div class="budget-control"><label for="plan-budget">На покупку деталей</label><div class="budget-input"><input id="plan-budget" type="number" min="0" max="1000000" step="1" inputmode="numeric" bind:value={budget} onchange={savePreferences} aria-invalid={!validBudget} aria-describedby="plan-budget-note"/><span>платины</span></div>
        <div class="budget-presets" aria-label="Быстрый выбор бюджета">{#each [0, 50, 100, 250] as amount}<button type="button" aria-pressed={budget === amount} onclick={() => setBudget(amount)}>{amount === 0 ? "Без докупки" : amount + " p"}</button>{/each}</div></div>
      <fieldset><legend>Что важнее</legend><div class="goal-switch"><button type="button" aria-pressed={goal === "profit"} onclick={() => { goal = "profit"; savePreferences(); }}>Больше выгоды</button><button type="button" aria-pressed={goal === "speed"} onclick={() => { goal = "speed"; savePreferences(); }}>Выше спрос</button></div><p>{goal === "profit" ? "Максимум дополнительной платины в пределах бюджета." : "Приоритет заявкам покупателей и частоте сделок. Срок продажи неизвестен."}</p></fieldset>
    </div>
    <p id="plan-budget-note" class="input-note" class:invalid={!validBudget}>{!validBudget ? "Укажите целый бюджет от 0 до 1 000 000 платины." : "Бюджет нужен только для расчёта. Перед покупкой проверьте цены у продавцов."}</p>
  </header>

  {#if !view.inventoryAvailable}
    <div class="empty-state"><h3>Сначала загрузите инвентарь</h3><p>План учитывает ваши детали, готовые комплекты и реликвии.</p><button type="button" onclick={onOpenSettings}>Открыть настройки данных</button></div>
  {:else if !sets.length}
    <div class="empty-state"><h3>Нет комплектов для расчёта</h3><p>Обновите данные предметов в настройках, чтобы сопоставить ваши детали с прайм-комплектами.</p><button type="button" onclick={onOpenSettings}>Открыть настройки данных</button></div>
  {:else}
    {#if plan.choices.length}
      <section class="plan-summary" aria-label="Итог подобранного плана">
        <div class="gain"><span>Выгода от сборки, оценка</span><strong>+{money(plan.profit)}</strong><p>сверх продажи своих деталей по отдельности</p></div>
        <div class="summary-body"><div class="summary-heading"><h3>{plan.choices.length} {copies(plan.choices.length)} в плане</h3><span>Остаток бюджета ≈ {money(Math.max(0,budget! - plan.cost))}</span></div>
          <dl class="totals"><div><dt>Докупить детали</dt><dd>≈ {money(plan.cost)}</dd></div><div><dt>Продать комплекты</dt><dd>≈ {money(plan.revenue)}</dd></div></dl>
          <details class="calculation"><summary>Как получилась выгода</summary><p>Продажа комплектов {money(plan.revenue)} − докупка {money(plan.cost)} − стоимость своих деталей {money(plan.ownedValue)} = {money(plan.profit)}. Это оценка, а не гарантированный доход.</p><p>Подбираем по одному дополнительному комплекту каждого типа. Готовые комплекты и одни и те же копии деталей не расходуются дважды.{plan.limited ? " Число сочетаний ограничено: найденный план может быть не самым выгодным из всех возможных." : ""}</p></details>
        </div>
      </section>
    {/if}
    <div class="workspace" bind:this={workspace}>
      <div class="plan-list">
        <section class="panel list-panel" aria-labelledby="plan-list-heading">
          <header class="list-header"><div><h3 id="plan-list-heading" bind:this={listHeading} tabindex="-1">{searching ? "Найти комплект" : "Докупить и продать"}</h3><p>{searching ? "Найдено: " + results.length : "Выберите комплект, чтобы увидеть детали и следующие шаги."}</p></div></header>
          <div class="search"><input type="search" aria-label="Найти комплект" placeholder="Найти другой комплект…" bind:value={query} bind:this={searchInput}/>{#if searching}<button type="button" class="text-button" onclick={() => { query = ""; searchInput.focus(); }}>Сбросить поиск</button>{/if}</div>
          {#if excluded.size}<div class="exclusions"><span>Исключено из подбора: {excluded.size}</span><button type="button" class="text-button" onclick={restore}>Вернуть все</button></div>{/if}
          {#if results.length}
            <div class="row-head" aria-hidden="true"><span>Комплект</span><span>Докупка ≈</span><span>Выгода ≈</span></div>
            <div class="choices">{#each results as row (row.definition.setSlug)}
              {@const choice = choiceBySlug.get(row.definition.setSlug)}
              {@const opportunity = setOpportunity(row)}
              {@const sale = saleEstimate(row,goal,quotes.get(row.definition.setSlug))}
              {@const cost = choice?.cost ?? opportunity.completionCost}
              {@const profit = choice?.profit ?? (sale.price !== null && cost !== null && opportunity.ownedPartsOpportunityValue !== null ? sale.price - cost - opportunity.ownedPartsOpportunityValue : null)}
              <button type="button" class="set-choice" class:selected={selected?.definition.setSlug === row.definition.setSlug && selectedMode === "complete"} aria-pressed={selected?.definition.setSlug === row.definition.setSlug && selectedMode === "complete"} aria-controls="plan-selected" onclick={() => select(row)}>
                <span class="choice-copy"><strong>{name(row)}</strong><small>{searching && choice ? "В плане · " : ""}{excluded.has(row.definition.setSlug) ? "Исключён · " : ""}Не хватает: {opportunity.missingQuantity} шт.{opportunity.availableCompleteSets ? " до следующего комплекта" : ""}</small></span>
                <span class="row-money">{money(cost)}</span><span class="row-money profit" class:negative={profit !== null && profit <= 0}>{profit !== null && profit > 0 ? "+" : ""}{money(profit)}</span>
              </button>
            {/each}</div>
          {:else}<div class="list-empty"><h4>{searching ? "Комплект не найден" : !validBudget ? "Нужен корректный бюджет" : budget === 0 ? "План без покупок" : "Выгодная докупка не найдена"}</h4><p>{searching ? "Попробуйте часть названия на русском или английском." : !validBudget ? "Исправьте сумму выше, чтобы посчитать план." : budget === 0 ? "Ниже показаны готовые комплекты. Через поиск можно выбрать комплект и проверить свои реликвии." : excluded.size ? "Можно вернуть исключённые комплекты или изменить бюджет." : "Увеличьте бюджет или найдите комплект через поиск. Для рекомендации нужны актуальные цены и часть деталей в инвентаре."}</p></div>{/if}
          {#if plan.shopping.length && !searching}<details class="shopping"><summary><span>Общий список покупок</span><span>{plan.shopping.reduce((sum, part) => sum + part.quantity,0)} шт. · ≈ {money(plan.cost)}</span></summary><ul>{#each plan.shopping as part (part.slug)}<li><span>{part.name}<small>Количество: {part.quantity}</small></span><strong>{money(part.cost)}</strong></li>{/each}</ul></details>{/if}
        </section>
        {#if ready.length}<section class="panel ready-panel" aria-labelledby="ready-plan-title"><header class="list-header"><div><h3 id="ready-plan-title">Продать без докупки</h3><p>Эти комплекты уже есть и доступны для продажи.</p></div><span class="count">{ready.length}</span></header>
          {#each (showAllReady ? ready : ready.slice(0,3)) as row (row.definition.setSlug)}{@const estimate = saleEstimate(row,goal,quotes.get(row.definition.setSlug))}
            <button type="button" class="ready-choice" class:selected={selected?.definition.setSlug === row.definition.setSlug && selectedMode === "ready"} aria-pressed={selected?.definition.setSlug === row.definition.setSlug && selectedMode === "ready"} aria-controls="plan-selected" onclick={() => select(row,"ready")}><span><strong>{name(row)}</strong><small>Доступно: {setOpportunity(row).sellableCompleteSets} {copies(setOpportunity(row).sellableCompleteSets)}</small></span><span class="ready-price">≈ {money(estimate.price)}<small>за комплект</small></span></button>
          {/each}{#if ready.length > 3}<button type="button" class="text-button show-ready" onclick={() => showAllReady = !showAllReady}>{showAllReady ? "Свернуть список" : "Показать все " + ready.length}</button>{/if}
        </section>{/if}
      </div>
      <aside id="plan-selected" class="panel selected-plan" tabindex="-1" bind:this={detail} aria-label="Действия с выбранным комплектом">
        <button type="button" class="text-button back-to-list" onclick={returnToList}>← Вернуться к списку</button>
        {#if selected}<PlanSetDetail row={selected} mode={selectedMode} choice={selectedChoice} {view} {goal} quote={quotes.get(selected.definition.setSlug)} busy={busySlug === selected.definition.setSlug} checkingOther={!!busySlug && busySlug !== selected.definition.setSlug} error={errors.get(selected.definition.setSlug) ?? ""} partsBusy={partsBusySlug === selected.definition.setSlug} actionStatus={actionStatusSlug === selected.definition.setSlug ? actionStatus : ""} excluded={excluded.has(selected.definition.setSlug)}
          onCheck={() => { selectedSlug = selected.definition.setSlug; selectionMode = selectedMode; onCheck(selected); }} onOpenSet={() => onOpenSet(selected)} onOpenParts={() => onOpenParts(selected)} {onOpenRelic} onExclude={() => exclude(selected)} onRestore={restoreSelected}/>
        {:else}<div class="detail-empty"><span class="empty-marker" aria-hidden="true">→</span><h3>Выберите комплект</h3><p>Здесь появятся расходы, выгода и недостающие детали. Найти любой комплект можно через поиск в списке.</p></div>{/if}
      </aside>
    </div>
    <p class="announcement" role="status">{announcement}</p>
  {/if}
</section>

<style>
  .planner { container-type:inline-size; display:grid; gap:1.15rem; min-width:0; }
  h2,h3,h4,p { margin:0; } h2 { font-size:1.35rem; } h3 { font-size:1rem; } h4 { font-size:.9375rem; } p { line-height:1.5; }
  .plan-controls,.panel,.empty-state { min-width:0; border:1px solid var(--border); border-radius:.85rem; background:var(--surface-1); box-shadow:var(--shadow-sm); }
  .plan-controls { padding:1.2rem 1.35rem; } .intro p { color:var(--text-muted); font-size:.875rem; margin-top:.3rem; }
  .controls { display:flex; flex-wrap:wrap; align-items:start; gap:1.1rem 2rem; margin-top:1.2rem; } .budget-control { flex:0 1 19rem; } label,legend { font-size:.8125rem; font-weight:600; margin-bottom:.45rem; }
  .budget-control label { display:block; } .budget-input { display:flex; align-items:center; gap:.5rem; padding:.15rem .75rem; border:1px solid var(--border-strong); border-radius:.5rem; background:var(--surface-1); }
  .budget-input:focus-within { outline:2px solid var(--gold); outline-offset:2px; } input { color:var(--text); font:inherit; min-width:0; }
  .budget-input input { width:100%; background:none; border:0; font-size:1.25rem; font-weight:650; padding:.35rem 0; outline:none; }
  .budget-input span { font-size:.8125rem; color:var(--text-muted); } .budget-presets { display:flex; gap:.35rem; flex-wrap:wrap; margin-top:.5rem; }
  .budget-presets button { min-height:1.8rem; padding:.15rem .5rem; border-color:transparent; background:var(--surface-2); color:var(--text-muted); font-size:.75rem; font-weight:500; }
  .budget-presets button[aria-pressed="true"] { border-color:var(--border-strong); color:var(--accent-strong); background:var(--accent-soft); }
  fieldset { border:0; padding:0; margin:0; min-width:0; flex:1 1 20rem; } legend { padding:0; } .goal-switch { display:inline-flex; gap:.2rem; padding:.2rem; border-radius:.55rem; background:var(--surface-2); }
  .goal-switch button { background:none; color:var(--text-muted); border-color:transparent; padding:.45rem .85rem; } .goal-switch button[aria-pressed="true"] { background:var(--surface-1); color:var(--accent-strong); border-color:var(--border); box-shadow:0 1px 3px #00000012; }
  fieldset p,.input-note { font-size:.75rem; color:var(--text-muted); margin-top:.55rem; } .input-note { margin-top:1rem; } .invalid { color:var(--danger); }
  .plan-summary { display:grid; grid-template-columns:minmax(16rem,.7fr) minmax(0,1.3fr); gap:1.5rem; border:1px solid var(--border); border-top:3px solid var(--success); border-radius:.85rem; background:linear-gradient(110deg,var(--success-soft),var(--surface-1) 50%); padding:1.2rem 1.35rem; }
  .gain { display:flex; flex-direction:column; justify-content:center; gap:.3rem; } .gain>span { font-size:.8125rem; font-weight:600; } .gain>strong { color:var(--success); font-size:2.15rem; font-weight:650; letter-spacing:-.035em; font-variant-numeric:tabular-nums; } .gain p { font-size:.75rem; color:var(--text-muted); }
  .summary-body { min-width:0; border-left:1px solid var(--border); padding-left:1.5rem; } .summary-heading { display:flex; justify-content:space-between; align-items:baseline; flex-wrap:wrap; gap:.5rem; } .summary-heading>span { font-size:.75rem; color:var(--text-muted); }
  .totals { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); margin:.9rem 0; gap:1rem; } dt { font-size:.8125rem; color:var(--text-muted); } dd { margin:.2rem 0 0; font-weight:650; font-size:1.3rem; font-variant-numeric:tabular-nums; }
  summary { cursor:pointer; font-size:.8125rem; font-weight:600; } .calculation summary { color:var(--text-muted); } .calculation p { font-size:.8125rem; margin-top:.7rem; color:var(--text-muted); }
  .workspace { display:grid; grid-template-columns:minmax(0,1.06fr) minmax(24rem,1fr); align-items:start; gap:1.1rem; min-width:0; }
  .plan-list { display:grid; gap:1rem; min-width:0; } .list-header { display:flex; align-items:center; gap:1rem; justify-content:space-between; padding:1.1rem 1.2rem .9rem; } .list-header p { font-size:.8125rem; color:var(--text-muted); margin-top:.3rem; }
  .search { margin:0 1.2rem 1rem; display:flex; gap:.5rem; flex-wrap:wrap; } .search input { width:100%; padding:.6rem .7rem; font-size:.8125rem; border:1px solid var(--border); border-radius:.5rem; background:var(--surface-1); }
  .exclusions { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:.5rem; margin:.6rem 1.2rem; font-size:.75rem; color:var(--text-muted); }
  .row-head,.set-choice { display:grid; grid-template-columns:minmax(0,1fr) 5.2rem 5.2rem; gap:.6rem; padding:.8rem 1.2rem; align-items:center; }
  .row-head { background:var(--surface-2); font-size:.75rem; color:var(--text-muted); border-block:1px solid var(--border); padding-block:.5rem; } .row-head>span:not(:first-child) { text-align:right; }
  .set-choice,.ready-choice { width:100%; text-align:left; border:0; border-bottom:1px solid var(--border); border-radius:0; background:transparent; color:var(--text); min-height:4.5rem; box-shadow:inset 3px 0 transparent; }
  .set-choice:last-child,.ready-choice:last-child { border-bottom:0; } .set-choice:hover,.ready-choice:hover { background:var(--surface-2); } .set-choice.selected,.ready-choice.selected { background:var(--accent-soft); box-shadow:inset 3px 0 var(--accent); }
  .choice-copy { min-width:0; } .choice-copy strong,.ready-choice strong { font-size:.875rem; font-weight:600; line-height:1.35; } small { display:block; margin-top:.3rem; color:var(--text-muted); font-size:.75rem; font-weight:400; line-height:1.35; }
  .row-money { font-size:.9375rem; text-align:right; font-weight:650; white-space:nowrap; font-variant-numeric:tabular-nums; } .profit { color:var(--success); } .negative { color:var(--text-muted); }
  .list-empty { padding:1.2rem; } .list-empty p { font-size:.8125rem; color:var(--text-muted); margin-top:.5rem; }
  .shopping { border-top:1px solid var(--border); padding:1rem 1.2rem; } .shopping summary { display:flex; justify-content:space-between; flex-wrap:wrap; gap:.5rem; } .shopping summary::before { content:"＋"; color:var(--accent-strong); } .shopping[open] summary::before { content:"−"; } .shopping summary>span:first-child { flex:1; } .shopping summary>span:last-child { font-size:.75rem; color:var(--text-muted); font-weight:400; }
  .shopping ul { list-style:none; padding:0; margin:1rem 0 0; } .shopping li { display:flex; justify-content:space-between; gap:1rem; padding:.65rem 0; font-size:.8125rem; border-top:1px solid var(--border); } .shopping li>strong { white-space:nowrap; }
  .ready-choice { display:flex; align-items:center; justify-content:space-between; gap:1rem; padding:.85rem 1.2rem; } .ready-price { white-space:nowrap; text-align:right; font-size:1rem; font-weight:650; }
  .count { display:inline-flex; padding:.2rem .5rem; border-radius:.35rem; background:var(--surface-2); font-size:.75rem; color:var(--text-muted); } .show-ready { margin:.5rem 1.2rem; }
  .selected-plan { position:sticky; top:4.5rem; padding:1.2rem; scroll-margin-top:6.5rem; } #plan-list-heading { scroll-margin-top:6.5rem; } .back-to-list { display:none; margin-bottom:1rem; } .detail-empty { padding:2rem .5rem; text-align:center; } .detail-empty p { font-size:.875rem; color:var(--text-muted); margin-top:.6rem; } .empty-marker { display:block; margin-bottom:1rem; color:var(--border-strong); font-size:2rem; }
  .text-button { border:0; color:var(--accent-strong); font-weight:600; font-size:.75rem; padding:.2rem 0; min-height:1.8rem; } .text-button:hover { text-decoration:underline; background:none; }
  .announcement:empty { display:none; } .announcement { font-size:.8125rem; color:var(--text-muted); }
  .empty-state { padding:1.5rem; } .empty-state p { font-size:.875rem; margin:.5rem 0 1rem; color:var(--text-muted); }
  @container (min-width:70rem) { .plan-controls { display:grid; grid-template-columns:minmax(17rem,.7fr) minmax(0,1.3fr); gap:.5rem 2rem; align-items:start; } .controls { margin-top:0; } .intro p { max-width:29rem; } .input-note { grid-column:1 / -1; margin-top:.25rem; } }
  @container (max-width:64rem) { .workspace { grid-template-columns:minmax(0,1fr); } .selected-plan { position:static; } .back-to-list { display:inline-flex; } }
  @container (max-width:42rem) { .plan-summary { grid-template-columns:minmax(0,1fr); gap:1rem; } .summary-body { border-left:0; border-top:1px solid var(--border); padding:1rem 0 0; } .gain>strong { font-size:1.85rem; } .controls { gap:1rem; } .budget-control { flex-basis:100%; } .row-head,.set-choice { grid-template-columns:minmax(0,1fr) 4.5rem 4.5rem; padding-inline:.9rem; gap:.4rem; } }
</style>
