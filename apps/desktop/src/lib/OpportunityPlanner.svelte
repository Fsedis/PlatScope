<script lang="ts">
  import { onMount } from "svelte";
  import { formatPlatinum, type LivePricingResult } from "./market";
  import { refinementLabel, setOpportunity, setPriceComparison, type InsightsView, type SetInsightRow } from "./insights";
  import { planCompletionBudget, planSetAcquisition, saleEstimate, type OpportunityGoal } from "./opportunityPlan";

  export let view: InsightsView;
  export let sets: SetInsightRow[];
  export let quotes: Map<string, LivePricingResult>;
  export let busySlug = "";
  export let errors: Map<string, string>;
  export let onCheck: (row: SetInsightRow) => void;
  export let onOpenSet: (row: SetInsightRow) => void;
  export let onOpenParts: (row: SetInsightRow) => void;
  export let actionStatus = "";
  export let initialSetSlug = "";
  let goal: OpportunityGoal = "profit";
  let budget: number | undefined = 100;
  let openings: number | undefined = 10;
  let selectedSlug = initialSetSlug;
  let query = "";
  const money = (value: number | null) => formatPlatinum(value, "ru");
  $: validBudget = budget !== undefined && Number.isInteger(budget) && budget >= 0 && budget <= 1_000_000;
  $: validOpenings = openings !== undefined && Number.isInteger(openings) && openings >= 1 && openings <= 20;
  $: plan = planCompletionBudget(sets, validBudget ? budget! : 0, goal, quotes);
  $: ready = sets.filter((row) => {
    const estimate = saleEstimate(row, goal, quotes.get(row.definition.setSlug));
    return setOpportunity(row).sellableCompleteSets > 0 && estimate.price !== null
      && ["set", "equivalent"].includes(setPriceComparison(row, estimate.price));
  })
    .sort((a, b) => {
      const left = saleEstimate(a, goal, quotes.get(a.definition.setSlug));
      const right = saleEstimate(b, goal, quotes.get(b.definition.setSlug));
      return goal === "speed" ? Number(right.buyer) - Number(left.buyer) || (right.volume ?? -1) - (left.volume ?? -1) : (right.price ?? 0) - (left.price ?? 0);
    }).slice(0, 3);
  $: selectable = sets.filter((row) => `${row.displayName} ${row.definition.displayNameEn}`.toLocaleLowerCase("ru").includes(query.trim().toLocaleLowerCase("ru")));
  $: selected = sets.find((row) => row.definition.setSlug === selectedSlug) ?? plan.choices[0]?.row ?? ready[0] ?? sets[0];
  $: opportunity = selected ? setOpportunity(selected) : null;
  $: acquisition = selected ? planSetAcquisition(selected, view.relics, view.voidTraces, openings ?? 10) : null;
  $: sale = selected ? saleEstimate(selected, goal, quotes.get(selected.definition.setSlug)) : null;
  function savePreferences() {
    try { localStorage.setItem("platscope.opportunity-plan.v1", JSON.stringify({ goal, budget: validBudget ? budget : 100 })); } catch { /* Недоступное хранилище не блокирует расчёт. */ }
  }
  onMount(() => {
    try {
      const saved = JSON.parse(localStorage.getItem("platscope.opportunity-plan.v1") ?? "null");
      if (saved?.goal === "profit" || saved?.goal === "speed") goal = saved.goal;
      if (typeof saved?.budget === "number" && Number.isFinite(saved.budget) && saved.budget >= 0 && saved.budget <= 1_000_000) budget = saved.budget;
    } catch { /* Используем значения по умолчанию. */ }
  });
</script>

<section class="planner" aria-label="План заработка">
  <header class="plan-controls">
    <div><h2>Как хочешь заработать?</h2><p>Выбери цель и сумму на докупку. Ничего не покупаем и не выставляем автоматически.</p></div>
    <div class="control-row">
      <div class="goal-switch" role="group" aria-label="Цель заработка">
        <button type="button" aria-pressed={goal === "profit"} onclick={() => { goal = "profit"; savePreferences(); }}>Больше заработать</button>
        <button type="button" aria-pressed={goal === "speed"} onclick={() => { goal = "speed"; savePreferences(); }}>Быстрее продать</button>
      </div>
      <label>Бюджет на докупку, платина<input type="number" min="0" max="1000000" step="1" bind:value={budget} onchange={savePreferences} /></label>
    </div>
    {#if !validBudget}<p role="alert">Введи бюджет от 0 до 1 000 000 платины.</p>{/if}
    {#if goal === "speed"}<p>Приоритет — заявки покупателей. Пока они не проверены, ориентируемся на закрытые сделки; срок продажи неизвестен.</p>{/if}
  </header>

  <div class="workspace">
    <div class="plan-list">
      <section class="panel" aria-labelledby="budget-plan-title">
        <header><h3 id="budget-plan-title">Дособрать в пределах бюджета</h3><span>Сетов: {plan.choices.length}</span></header>
        {#if plan.choices.length}
          <dl class="totals">
            <div><dt>Потратить ≈</dt><dd>{money(plan.cost)}</dd></div>
            <div><dt>Получить от продажи ≈</dt><dd>{money(plan.revenue)}</dd></div>
            <div><dt>Дополнительная выгода ≈</dt><dd>{money(plan.profit)}</dd></div>
          </dl>
          <p class="muted">Стоимость своих деталей {money(plan.ownedValue)} уже вычтена из выгоды.</p>
          {#each plan.choices as choice (choice.row.definition.setSlug)}
            <button class="set-choice" class:selected={selected?.definition.setSlug === choice.row.definition.setSlug} aria-pressed={selected?.definition.setSlug === choice.row.definition.setSlug} type="button" onclick={() => selectedSlug = choice.row.definition.setSlug}>
              <span>{choice.row.displayName}<small>Дособрать 1 сет · докупить: {setOpportunity(choice.row).missingQuantity} шт.</small></span><span aria-hidden="true">→</span>
            </button>
          {/each}
          <details><summary>Список покупок · видов деталей: {plan.shopping.length}</summary>
            <ul>{#each plan.shopping as part (part.slug)}<li><span>{part.name} ×{part.quantity}</span><strong>≈ {money(part.cost)}</strong></li>{/each}</ul>
          </details>
          <details><summary>Как подобран план</summary><p>Считаем следующий комплект из оставшихся деталей. Одни и те же копии не используются дважды. Общая докупка учитывает количество одинаковых деталей. Цены могут измениться; это оценка, а не заказ.</p><p>{plan.limited ? "Для большого числа сочетаний использован ограниченный поиск — это хороший найденный вариант, не доказанный максимум." : "Сравнены допустимые сочетания рассмотренных вариантов."}</p></details>
        {:else}
          <p class="empty">{!validBudget ? "Укажи корректный бюджет, чтобы подобрать план." : budget === 0 ? "Без докупки можно продавать готовые сеты или открывать имеющиеся реликвии." : "В этот бюджет нет подходящей докупки с положительной оценкой выгоды. Можно увеличить сумму или выбрать сет и посмотреть его реликвии."}</p>
        {/if}
      </section>

      {#if ready.length}<section class="panel" aria-labelledby="ready-plan-title"><header><h3 id="ready-plan-title">Можно продать без докупки</h3></header>
        {#each ready as row (row.definition.setSlug)}{@const estimate = saleEstimate(row, goal, quotes.get(row.definition.setSlug))}
          <button type="button" class="set-choice" class:selected={selected?.definition.setSlug === row.definition.setSlug} aria-pressed={selected?.definition.setSlug === row.definition.setSlug} onclick={() => selectedSlug = row.definition.setSlug}><span>{row.displayName}<small>Доступно сетов: {setOpportunity(row).sellableCompleteSets} · {estimate.buyer ? "есть заявка покупателя" : "оценка за один сет"}</small></span><strong>≈ {money(estimate.price)}</strong></button>
        {/each}
      </section>{/if}

      <details class="panel browse"><summary>Выбрать другой сет</summary><label>Найти сет<input type="search" bind:value={query} placeholder="Название сета" /></label>
        <div class="set-picker">{#each selectable as row (row.definition.setSlug)}<button type="button" class="set-choice" onclick={() => selectedSlug = row.definition.setSlug}>{row.displayName}</button>{:else}<p>Ничего не найдено.</p>{/each}</div>
      </details>
    </div>

    <aside class="panel selected-plan" aria-label="План выбранного сета">
      {#if selected && opportunity && acquisition && sale}
        <p class="eyebrow">Выбранный сет</p><h3>{selected.displayName}</h3>
        <p>{opportunity.completeSets > 0 ? `Уже есть полных сетов: ${opportunity.completeSets}. План ниже — для следующего.` : "Сравни способы получить недостающие детали."}</p>
        <div class="sale-line"><span>{sale.buyer ? "Заявка покупателя за сет" : "Оценка продажи сета"}</span><strong>{money(sale.price)}</strong></div>
        {#if sale.volume !== null}<p class="muted">Закрытых сделок в данных: {sale.volume}. Это показатель спроса, не срок продажи.</p>{/if}
        <div class="actions"><button type="button" disabled={!!busySlug} onclick={() => { selectedSlug = selected.definition.setSlug; onCheck(selected); }}>{busySlug === selected.definition.setSlug ? "Проверяем…" : "Проверить спрос и цену"}</button>{#if opportunity.completeSets > 0}<button class="secondary" type="button" onclick={() => onOpenSet(selected)}>Перейти к продаже</button>{/if}</div>
        {#if sale.buyer && sale.price !== null && opportunity.completionCost !== null && opportunity.ownedPartsOpportunityValue !== null && sale.price <= opportunity.completionCost + opportunity.ownedPartsOpportunityValue}<p class="muted">По текущей заявке покупателя докупка невыгодна: цена не покрывает стоимость всех деталей.</p>{/if}
        {#if errors.get(selected.definition.setSlug)}<p role="alert">{errors.get(selected.definition.setSlug)}</p>{/if}
        <div class="route">
          <h4>Купить недостающие детали</h4><strong class="route-value">{opportunity.completionCost === null ? "Нужны свежие цены" : `≈ ${money(opportunity.completionCost)}`}</strong>
          <ul>{#each opportunity.missingParts as part (part.slug)}<li><span>{part.displayName} ×{part.quantity}</span><span>{money(part.estimatedCost)}</span></li>{/each}</ul>
          {#if opportunity.missingParts.length}<button type="button" class="secondary" onclick={() => onOpenParts(selected)}>Найти продавцов деталей</button>{/if}
          {#if actionStatus}<p class="muted" role="status">{actionStatus}</p>{/if}
        </div>
        <div class="route">
          <h4>Добыть из своих реликвий</h4>
          <label class="opening-limit">Готов открыть реликвий<input type="number" min="1" max="20" step="1" bind:value={openings} /></label>
          {#if !validOpenings}<p role="alert">Укажи целое число открытий от 1 до 20.</p>
          {:else if acquisition.steps.length}
            <strong class="route-value">{acquisition.chance.toLocaleString("ru", {maximumFractionDigits:1})}% — получить {acquisition.buy.length ? "остальные детали после докупки" : "все недостающие детали"}</strong>
            <p>Открытий: {acquisition.openings} · следов Пустоты: {acquisition.traces}</p>
            <ol>{#each acquisition.steps as step}<li>{step.source.displayName} ×{step.quantity}<small>{refinementLabel(step.source.definition.refinement)}{step.source.definition.refinement !== step.target ? ` → ${refinementLabel(step.target)}` : " · уже есть"}{step.traceCost ? ` · ${step.traceCost} следов` : ""}</small></li>{/each}</ol>
            <p class="muted">Стоимость использованных реликвий: {acquisition.relicValue === null ? "неизвестна" : `≈ ${money(acquisition.relicValue)}`}. Это альтернатива их продаже.</p>
            {#if acquisition.buy.length}<p>Отдельно докупить: {acquisition.buy.map(part => `${part.displayName} ×${part.quantity}`).join(", ")} · {money(acquisition.buyCost)}.</p>{/if}
            <p class="muted">Соло, без гарантии выпадения. Подбор стремится к 80% в пределах числа открытий и имеющихся следов. {view.voidTraces == null ? "Баланс следов неизвестен — улучшения не предлагаются." : ""}</p>
          {:else}<p class="empty">В имеющихся реликвиях не найден подходящий путь. Для этих деталей остаётся докупка.</p>{/if}
        </div>
      {:else}<h3>Сначала нужен инвентарь</h3><p>После загрузки предметов здесь появятся персональные планы.</p>{/if}
    </aside>
  </div>
</section>

<style>
  .planner { display:grid; gap:1rem; min-width:0; }
  .plan-controls,.panel { border:1px solid var(--border); border-radius:.8rem; background:var(--surface-1); padding:1rem; min-width:0; }
  h2,h3,h4,p { margin:0; } h2 {font-size:1.25rem;} h3 {font-size:1rem;} h4 {font-size:.95rem;} p {line-height:1.5; margin-top:.45rem;}
  .plan-controls>div>p,.muted,.empty {color:var(--text-muted);font-size:.82rem;}
  .control-row { display:flex; align-items:end; gap:1rem; margin-top:1rem; flex-wrap:wrap; }
  .goal-switch { display:flex; gap:.25rem; padding:.2rem; background:var(--surface-2);border-radius:.6rem; }
  .goal-switch button { background:transparent;color:var(--text);border-color:transparent; }
  .goal-switch button[aria-pressed="true"] { background:var(--accent-soft);border-color:var(--accent);color:var(--accent-strong); }
  label {display:grid;gap:.35rem;font-size:.8rem;} input {padding:.5rem;min-width:0;border:1px solid var(--border);border-radius:.45rem;background:var(--surface-1);color:var(--text);font:inherit;} .control-row input {width:12rem;}
  .workspace {display:grid;grid-template-columns:minmax(0,1fr) minmax(340px,.9fr);gap:1rem;align-items:start;}
  .plan-list {display:grid;gap:.8rem;min-width:0;}.panel header {display:flex;justify-content:space-between;gap:.5rem;margin-bottom:.8rem;}.panel header>span {color:var(--text-muted);white-space:nowrap;}
  .totals {display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:.4rem;margin:0;}.totals>div {padding:.7rem;background:var(--surface-2);border-radius:.5rem;}dt {font-size:.75rem;color:var(--text-muted);}dd {margin:.35rem 0 0;font-weight:700;font-size:1.1rem;}
  .set-choice {display:flex;width:100%;text-align:left;justify-content:space-between;align-items:center;gap:.7rem;background:transparent;color:var(--text);border:1px solid var(--border);border-radius:.5rem;padding:.7rem;margin-top:.5rem;font-size:.85rem;}.set-choice.selected {background:var(--accent-soft);border-color:var(--accent);}small {display:block;color:var(--text-muted);font-weight:400;font-size:.75rem;margin-top:.25rem;}.set-choice strong {white-space:nowrap;}
  details {margin-top:.8rem;} summary {cursor:pointer;font-size:.82rem;font-weight:600;}details p {font-size:.8rem;color:var(--text-muted);}ul {list-style:none;padding:0;}ul li {display:flex;justify-content:space-between;gap:1rem;padding:.4rem 0;border-bottom:1px solid var(--border);font-size:.82rem;}ol {padding-left:1.2rem;font-size:.85rem;}ol li {margin:.6rem 0;}
  .eyebrow {text-transform:uppercase;font-size:.7rem;color:var(--text-muted);margin:0 0 .4rem;}.selected-plan>h3 {font-size:1.15rem;}.selected-plan>p {font-size:.82rem;}.sale-line {display:flex;gap:1rem;justify-content:space-between;margin-top:1rem;font-size:.85rem;}.sale-line strong {font-size:1.2rem;white-space:nowrap;}.actions {display:flex;gap:.5rem;flex-wrap:wrap;margin-top:.7rem;}.route {border-top:1px solid var(--border);padding-top:1rem;margin-top:1rem;}.route-value {display:block;margin-top:.6rem;}.opening-limit {display:flex;align-items:center;justify-content:space-between;margin:.6rem 0;gap:.5rem;}.opening-limit input {width:5rem;}.set-picker {max-height:16rem;overflow:auto;margin-top:.6rem;}.browse label {margin-top:.7rem;}
  @media(max-width:1100px) {.workspace {grid-template-columns:minmax(0,1fr);} .selected-plan {position:static;}}
</style>
