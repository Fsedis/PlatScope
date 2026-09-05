<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { rankRelicsToOpen, refinementLabel, type InsightsView, type SetInsightRow, type RelicOverviewScenario } from "./insights";
  import { formatPlatinum } from "./market";
  import { filterRelicChoices, relicNet, relicProgressSets, rewardChoiceChance, selectedRelicRewards, type RelicSort } from "./relicBrowser";
  import { type UiLocale } from "./i18n";
  import MasteryBadge from "./MasteryBadge.svelte";
  import { masteryTargetsForReward } from "./mastery";

  export let view: InsightsView;
  export let sets: SetInsightRow[];
  export let locale: UiLocale = "ru";
  export let onPlanSet: (slug: string) => void;
  let scenario: RelicOverviewScenario = "solo";
  let sort: RelicSort = "value";
  let query = "";
  let upgrades = true;
  let selectedSlug = "";
  let limit = 20;
  $: t = locale === "ru" ? (ru: string, _en: string) => ru : (_ru: string, en: string) => en;
  const money = (value: number | null) => formatPlatinum(value, locale);
  const percent = (value: number) => `${value.toLocaleString(locale === "ru" ? "ru-RU" : "en-US", {maximumFractionDigits:1})}%`;
  $: ranked = rankRelicsToOpen(view.relics, relicProgressSets(sets), {availableTraces:upgrades ? view.voidTraces ?? 0 : 0, squadSize:scenario === "solo" ? 1 : 4});
  $: choices = filterRelicChoices(ranked, view.relics, query, sort, scenario);
  $: visible = choices.slice(0,limit);
  $: selected = choices.find(row => row.relicSlug === selectedSlug) ?? choices[0];
  $: rewards = selected ? selectedRelicRewards(selected,view.relics,sets) : [];
  $: owned = selected ? view.relics.filter(row => row.definition.relicSlug === selected.relicSlug && row.ownedQuantity > 0) : [];
  $: net = selected ? relicNet(selected,scenario) : null;
  $: gross = selected ? (scenario === "solo" ? selected.grossExpectedPlatinum : selected.squadGrossExpectedPlatinum) : null;
  $: setBonus = selected && net !== null && gross !== null && selected.relicOpportunityCost !== null
    ? Math.max(0,net-gross+selected.relicOpportunityCost+selected.traceOpportunityCost) : null;
  onMount(() => {
    try {
      const saved = JSON.parse(sessionStorage.getItem("platscope.relic-browser.v1") ?? "null");
      if (saved?.scenario === "solo" || saved?.scenario === "matching_squad") scenario = saved.scenario;
      if (["value", "progress", "owned"].includes(saved?.sort)) sort = saved.sort;
      if (typeof saved?.query === "string") query = saved.query.slice(0,200);
      if (typeof saved?.selectedSlug === "string") selectedSlug = saved.selectedSlug;
      if (typeof saved?.upgrades === "boolean") upgrades = saved.upgrades;
    } catch { /* Недоступное хранилище не мешает выбору реликвий. */ }
  });
  onDestroy(() => {
    try { sessionStorage.setItem("platscope.relic-browser.v1",JSON.stringify({scenario,sort,query,selectedSlug:selected?.relicSlug ?? "",upgrades})); }
    catch { /* Предпочтения необязательны. */ }
  });
</script>

<section class="relic-browser" aria-label={t("Выбор реликвии","Choose a relic")}>
  <header class="controls">
    <div><h2>{t("Что открыть?","What should I open?")}</h2><p>{t("Выбери реликвию — посмотри награды, шансы и подготовку.","Choose a relic to see rewards, chances and preparation.")}</p></div>
    <div class="filters">
      <label class="search">{t("Реликвия или награда","Relic or reward")}<input type="search" bind:value={query} oninput={() => limit=20} placeholder={t("Например, Лит N12 или Никс","For example, Lith N12 or Nyx")} /></label>
      <label>{t("Сначала","Sort by")}<select bind:value={sort} onchange={() => limit=20}><option value="value">{t("Выгоднее открывать","Higher estimated value")}</option><option value="progress">{t("Шанс нужной детали","Needed part chance")}</option><option value="owned">{t("Больше копий","More copies owned")}</option></select></label>
      <div class="scenario" role="group" aria-label={t("Как открывать","Opening scenario")}><button type="button" aria-pressed={scenario === "solo"} onclick={() => scenario="solo"}>{t("Соло","Solo")}</button><button type="button" aria-pressed={scenario === "matching_squad"} onclick={() => scenario="matching_squad"}>{t("Отряд с одинаковыми реликвиями","Matching squad")}</button></div>
    </div>
    <div class="options"><label><input type="checkbox" bind:checked={upgrades} />{t("Предлагать улучшения за мои следы","Suggest upgrades within my Trace balance")}</label><span>{view.voidTraces == null ? t("Баланс следов неизвестен — только имеющиеся улучшения.","Trace balance unknown — only refinements already owned.") : t(`Следов: ${view.voidTraces}`,`Traces: ${view.voidTraces}`)}</span></div>
    {#if scenario === "matching_squad"}<p class="scenario-note">{t("Расчёт для четырёх игроков с одной и той же реликвией и улучшением. Ты выбираешь одну из четырёх наград. Для случайного отряда эта оценка не подходит.","Four players use the same relic and refinement; you choose one reward. This estimate does not apply to a random squad.")}</p>{/if}
  </header>
  <div class="relic-workspace">
    <section class="catalog" aria-label={t("Мои реликвии","My relics")}>
      <header><h3>{t("Мои реликвии","My relics")}</h3><span>{choices.length} / {ranked.length}</span></header>
      <div class="choice-list">
        {#each visible as row (row.relicSlug)}
          {@const value = relicNet(row,scenario)}
          <button type="button" class="choice" aria-pressed={selected?.relicSlug === row.relicSlug} onclick={() => selectedSlug=row.relicSlug}>
            {#if row.imageUrl}<img src={row.imageUrl} alt="" loading="lazy" />{/if}
            <span class="identity"><strong>{row.displayName}</strong><small>{t(`Всего: ${row.totalOwnedQuantity}`,`Total: ${row.totalOwnedQuantity}`)} · {row.traceCost ? t(`Улучшить: ${refinementLabel(row.recommendedRefinement,locale)} · ${row.traceCost} следов`,`Refine: ${refinementLabel(row.recommendedRefinement,locale)} · ${row.traceCost} Traces`) : t(`Готово: ${refinementLabel(row.recommendedRefinement,locale)} ×${row.sourceQuantity}`,`Ready: ${refinementLabel(row.recommendedRefinement,locale)} ×${row.sourceQuantity}`)}</small></span>
            <span class="value">{sort === "progress" ? percent(rewardChoiceChance(row.progressChancePercent,scenario)) : value === null ? "—" : `≈ ${money(value)}`}<small>{sort === "progress" ? t("нужная деталь","needed part") : t("оценка выгоды","estimated value")}</small></span>
          </button>
        {:else}<div class="empty"><h3>{query ? t("Ничего не найдено","No matches") : t("Нет реликвий в инвентаре","No owned relics")}</h3><p>{query ? t("Попробуй название реликвии или её награды.","Try a relic or reward name.") : t("Обнови инвентарь в разделе «Мои предметы».","Refresh your inventory in My items.")}</p>{#if query}<button type="button" class="secondary" onclick={() => query=""}>{t("Сбросить поиск","Clear search")}</button>{/if}</div>{/each}
      </div>
      {#if choices.length > limit}<button type="button" class="more secondary" onclick={() => limit+=20}>{t("Показать ещё 20","Show 20 more")}</button>{/if}
    </section>
    <aside class="relic-detail" aria-label={t("Выбранная реликвия","Selected relic") }>
      {#if selected}
        <header class="detail-title">{#if selected.imageUrl}<img src={selected.imageUrl} alt="" />{/if}<div><h3>{selected.displayName}</h3><p>{owned.map(row => `${refinementLabel(row.definition.refinement,locale)} ×${row.ownedQuantity}`).join(" · ")}</p></div></header>
        <div class="preparation"><strong>{selected.traceCost > 0 ? t(`Подготовка: ${refinementLabel(selected.sourceRefinement,locale)} → ${refinementLabel(selected.recommendedRefinement,locale)}`,`Preparation: ${refinementLabel(selected.sourceRefinement,locale)} → ${refinementLabel(selected.recommendedRefinement,locale)}`) : t(`Можно открывать: ${refinementLabel(selected.recommendedRefinement,locale)}`,`Ready to open: ${refinementLabel(selected.recommendedRefinement,locale)}`)}</strong><p>{selected.traceCost > 0 ? t(`Улучши одну копию в игре за ${selected.traceCost} следов. Подходящих копий: ${selected.sourceQuantity}.`,`Refine one copy in game for ${selected.traceCost} Traces. Matching copies: ${selected.sourceQuantity}.`) : t("Дополнительные следы не нужны. Открытие выполняется в игре.","No additional Traces needed. Open the relic in game.")}</p></div>
        <div class="summary"><div><span>{t("Средняя ценность награды","Average reward value")}</span><strong>{gross === null ? t("Не хватает цен","Missing prices") : `≈ ${money(gross)}`}</strong></div><div><span>{t("Оценочная выгода открытия","Estimated opening value")}</span><strong>{net === null ? t("Не хватает цен","Missing prices") : `≈ ${money(net)}`}</strong></div></div>
        <p class="estimate-note">{net === null ? t("Неизвестные цены не заменяем нулём — рейтинг этой реликвии пока неполный.","Unknown prices are not treated as zero; this estimate is incomplete.") : net <= 0 ? t("По текущей оценке открытие не даёт преимущества перед сохранением или продажей реликвии. Нужная тебе награда всё равно может быть причиной открыть её.","Opening has no estimated advantage over keeping or selling this relic. A reward you need may still make it worthwhile.") : t("Это средняя оценка многих открытий, не обещанная прибыль за один забег.","An average across many openings, not guaranteed profit per run.")}</p>
        <section class="rewards"><h4>{t("Что может выпасть","Possible rewards")}</h4><div class="reward-labels"><span>{t("Награда","Reward")}</span><span>{scenario === "solo" ? t("Шанс","Chance") : t("Увидеть в выборе","In reward choices")}</span><span>{t("Цена ≈","Price ≈")}</span></div>
          {#each rewards as reward (reward.definition.rewardGameRef)}<div class="reward"><div><strong>{reward.displayName}</strong>{#each masteryTargetsForReward(reward.definition, sets) as gameRef (gameRef)}<MasteryBadge {gameRef} showName />{/each}{#each reward.targets as target (target.slug)}<button type="button" class="set-link" onclick={() => onPlanSet(target.slug)}>{target.finishes ? t("Завершит сет","Completes set") : t("Нужна для сета","Needed for set")}: {target.name} →</button>{/each}</div><span>{percent(rewardChoiceChance(reward.chance,scenario))}</span><span>{money(reward.price)}</span></div>{/each}
          {#if scenario !== "solo"}<p class="estimate-note">{t("Шанс увидеть хотя бы одну такую награду среди четырёх. Шансы строк не складываются; забрать можно только одну награду.","Chance to see at least one such reward among four. Row chances do not add up; you can take only one reward.")}</p>{/if}
        </section>
        <details><summary>{t("Как рассчитана выгода","How value is calculated")}</summary><dl><div><dt>{t("Средняя ценность награды","Average reward value")}</dt><dd>{money(gross)}</dd></div><div><dt>{t("Минус стоимость своей реликвии","Minus the owned relic's value")}</dt><dd>{money(selected.relicOpportunityCost)}</dd></div><div><dt>{t("Минус условная стоимость следов","Minus modeled Trace cost")}</dt><dd>{money(selected.traceOpportunityCost)}</dd></div>{#if setBonus !== null && setBonus > 0.01}<div><dt>{t("Вклад возможного завершения сета","Potential set-completion contribution")}</dt><dd>{money(setBonus)}</dd></div>{/if}</dl><p>{t("Следы не продаются. Для сравнения используется условная оценка: 100 следов = 2 платины. Стоимость реликвии вычитается как альтернатива её продаже. Премия за завершение сета — оценочная, не дополнительная награда.","Traces cannot be sold. The model values 100 Traces at 2 platinum and deducts the relic's value as an alternative to selling it. Set-completion premium is an estimate, not an extra reward.")}</p><p>{t("Расчёт относится к одной твоей реликвии, а не ко всему запасу.","The estimate applies to one of your relics, not your entire stock.")}</p></details>
      {:else}<div class="empty"><p>{t("Здесь появятся награды выбранной реликвии.","Rewards for the selected relic appear here.")}</p></div>{/if}
    </aside>
  </div>
</section>

<style>
  .relic-browser {display:grid;gap:1rem;min-width:0} .controls,.catalog,.relic-detail {background:var(--surface-1);border:1px solid var(--border);border-radius:.8rem;min-width:0}.controls,.relic-detail {padding:1rem} h2,h3,h4,p {margin:0}h2{font-size:1.15rem}h3{font-size:1.05rem}h4{font-size:.95rem}p{line-height:1.5}.controls p,.detail-title p,.preparation p,.estimate-note{color:var(--text-muted);font-size:.83rem;margin-top:.4rem}.filters{display:flex;align-items:end;gap:.8rem;flex-wrap:wrap;margin-top:1rem}.search{flex:1;min-width:15rem}label{display:grid;gap:.35rem;font-size:.82rem}input,select{padding:.55rem;border:1px solid var(--border);border-radius:.45rem;background:var(--surface-1);color:var(--text);font:inherit;min-width:0}.scenario{display:flex;gap:.25rem;padding:.2rem;border:1px solid var(--border);border-radius:.5rem}.scenario button{background:transparent;color:var(--text);border-color:transparent}.scenario button[aria-pressed="true"]{background:var(--accent-soft);border-color:var(--accent)}.options{display:flex;flex-wrap:wrap;gap:.6rem 1rem;margin-top:.8rem;align-items:center}.options label{display:flex;align-items:center}.options span{font-size:.78rem;color:var(--text-muted)}.scenario-note{max-width:70rem}.relic-workspace{display:grid;grid-template-columns:minmax(340px,.9fr) minmax(450px,1.1fr);gap:1rem;align-items:start}.catalog header{display:flex;justify-content:space-between;padding:1rem}.catalog header span{color:var(--text-muted);font-size:.85rem}.choice-list{max-height:65vh;overflow:auto;overscroll-behavior:contain}.choice{display:flex;align-items:center;gap:.6rem;width:100%;padding:.85rem;text-align:left;border:0;border-top:1px solid var(--border);border-radius:0;background:transparent;color:var(--text)}.choice[aria-pressed="true"]{background:var(--accent-soft);box-shadow:inset 3px 0 var(--accent)}.choice img{width:38px;height:38px;object-fit:contain;flex-shrink:0}.identity{min-width:0;flex:1}.identity strong{font-size:.86rem}small{display:block;font-weight:400;font-size:.75rem;color:var(--text-muted);line-height:1.45;margin-top:.2rem}.value{font-size:.9rem;text-align:right;white-space:nowrap}.more{display:block;margin:.8rem auto}.detail-title{display:flex;align-items:center;gap:.7rem}.detail-title img{width:48px;height:48px;object-fit:contain}.preparation{padding:.8rem;background:var(--surface-2);border-radius:.5rem;margin-top:1rem;font-size:.9rem}.summary{display:grid;grid-template-columns:1fr 1fr;gap:1rem;margin-top:1rem}.summary span{font-size:.8rem;color:var(--text-muted)}.summary strong{display:block;font-size:1.15rem;margin-top:.3rem}.rewards{margin-top:1.2rem}.reward-labels,.reward{display:grid;grid-template-columns:minmax(0,1fr) 6rem 4rem;gap:.6rem;align-items:start}.reward-labels{color:var(--text-muted);font-size:.72rem;margin-top:.8rem}.reward{border-bottom:1px solid var(--border);padding:.7rem 0;font-size:.82rem}.reward>span,.reward-labels>span:not(:first-child){text-align:right}.reward strong{font-weight:600}.set-link{display:block;text-align:left;background:transparent;color:var(--accent-strong);border:0;padding:.25rem 0;font-size:.76rem;line-height:1.4;text-decoration:underline}.set-link:hover{background:var(--surface-2)}details{margin-top:1rem;border-top:1px solid var(--border);padding-top:.8rem}summary{cursor:pointer;font-size:.83rem;font-weight:600}details p,dl{font-size:.8rem;color:var(--text-muted);margin-top:.6rem}dl>div{display:flex;justify-content:space-between;gap:1rem;padding:.25rem 0}dd{margin:0;white-space:nowrap}.empty{padding:1rem;color:var(--text-muted)}.empty button{margin-top:.8rem}
  @media(max-width:1050px){.relic-workspace{grid-template-columns:minmax(0,1fr)}.choice-list{max-height:22rem}} @media(max-width:600px){.reward-labels,.reward{grid-template-columns:minmax(0,1fr) 4rem 3rem}.summary{grid-template-columns:1fr}.scenario{flex-wrap:wrap}.search{min-width:0;width:100%}}
</style>
