<script lang="ts">
  import { bountyEstimate, bountyFeaturedReward, type RankedBountyJob, type BountyRewardView } from "./bountyHunter";
  import WorldActivityArtwork from "./WorldActivityArtwork.svelte";
  import type { WorldArtworkKind } from "./worldActivityArtwork";

  export let row: RankedBountyJob;
  export let query = "";
  export let targetKey = "";
  export let watchedKeys: Set<string>;
  export let busy = false;
  export let anyBusy = false;
  export let message = "";
  export let remaining: string;
  export let priceDate: string | null = null;
  export let onCheck: () => void;
  export let onMarket: (reward: BountyRewardView) => void;
  export let onWatch: (reward: BountyRewardView) => void;
  export let onFind: (reward: BountyRewardView) => void;
  export let onBack: () => void;
  const art: Record<string, WorldArtworkKind> = { cetus: "cetus", fortuna: "vallis", necralisk: "cambion" };
  const places: Record<string, string> = { cetus: "в Цетусе", fortuna: "в Фортуне", necralisk: "в Некралиске" };
  const num = (value: number) => value.toLocaleString("ru-RU", { maximumFractionDigits: 1 });
  const platinum = (value: number | null | undefined) => value == null ? "Нет оценки" : "≈ " + num(value) + " пл.";
  $: job = row.job;
  $: estimate = bountyEstimate(job);
  $: featured = bountyFeaturedReward(job, query, targetKey);
  $: seeking = !!featured && (targetKey ? featured.trackingKey === targetKey : !!query.trim() && featured.displayName.toLocaleLowerCase("ru").replaceAll("ё", "е").includes(query.trim().toLocaleLowerCase("ru").replaceAll("ё", "е")));
  $: cycle = job.title.toLocaleLowerCase("ru").includes("бесконечн");
  $: period = cycle ? "за цикл из " + job.stageCount + " этапов" : "за все этапы заказа";
  $: rewards = [...job.rewards].sort((a, b) => Number(b === featured) - Number(a === featured) || b.chancePercent - a.chancePercent);
</script>

<div class="detail-topline">
  <span class="eyebrow">Выбранный заказ</span>
  <button class="secondary back" type="button" onclick={onBack}>К списку заказов</button>
</div>
<div class="destination">
  {#if art[row.regionKey]}<span class="region-art"><WorldActivityArtwork kind={art[row.regionKey]} /></span>{/if}
  <div><strong>{row.regionName}</strong><span>Заказ сменится через {remaining}</span></div>
</div>
<h2>{job.title}</h2>
<p class="instruction">Возьмите этот заказ {places[row.regionKey] ?? ("в регионе «" + row.regionName + "»")}.</p>
<dl class="facts">
  <div><dt>Уровень врагов</dt><dd>{job.minLevel}–{job.maxLevel}</dd></div>
  <div><dt>{cycle ? "Этапов в цикле" : "Этапов"}</dt><dd>{job.stageCount || "Неизвестно"}</dd></div>
  <div><dt>Репутация</dt><dd>{num(job.totalStanding)}</dd></div>
</dl>
{#if job.minMasteryRank > 0 || job.timeBound}<p class="condition">{#if job.minMasteryRank > 0}Нужен ранг мастерства {job.minMasteryRank}.{/if} {job.timeBound ?? ""}</p>{/if}

<div class="outcomes" class:seeking>
<section class="estimate" aria-label="Оценка наград">
  <div class="estimate-line"><span>Средняя стоимость наград<br /><small>{period}</small></span><strong>{estimate.value === null ? (!job.rewards.length ? "Нет данных" : estimate.total ? "Нет оценки" : "Не для продажи") : platinum(estimate.value)}</strong></div>
  <p>Платина от продажи выпавших предметов. Выпадение и продажа не гарантированы.</p>
  {#if estimate.total > 0}
    {#if estimate.priced < estimate.total}<p class="partial">Неполная оценка: учтено цен {estimate.priced} из {estimate.total}.</p>{/if}
    <button type="button" onclick={onCheck} disabled={anyBusy}>{busy ? "Проверяем цены…" : "Проверить цены на рынке"}</button>
  {:else if job.rewards.length}<p>Эти награды пригодятся в игре, но не продаются на Warframe Market.</p>{/if}
  {#if message}<p class="check-message" role="status">{message}</p>{/if}
</section>

<section class="reward-section" aria-label="Награды заказа">
<div class="rewards-heading"><h3>{seeking ? "Искомая награда и другая добыча" : "Что может выпасть"}</h3><span>Наград: {job.rewards.length}</span></div>
<p class="chance-help">Шанс получить награду хотя бы один раз {period}.</p>
<div class="rewards">
  {#each rewards as reward (reward.trackingKey)}
    <article class="reward" class:target={seeking && reward === featured}>
      <div class="reward-heading">
        {#if reward.imageUrl}<img src={reward.imageUrl} alt="" loading="lazy" onerror={event => { (event.currentTarget as HTMLImageElement).style.display = "none"; }} onload={event => { (event.currentTarget as HTMLImageElement).style.display = ""; }} />{/if}
        <div class="reward-name"><h4>{reward.displayName}</h4><span>{reward.ownedQuantity == null ? "Нет данных о количестве" : "У вас: " + num(reward.ownedQuantity)}{#if reward.rarity} · {reward.rarity}{/if}</span></div>
        <strong class="chance">{num(reward.chancePercent)}<small>%</small></strong>
      </div>
      <div class="reward-bottom">
        <span class="unit-price">{reward.marketKey ? (reward.unitPrice == null ? "Цена неизвестна" : platinum(reward.unitPrice) + " за шт.") : "Не продаётся на рынке"}</span>
        <button class="secondary watch" type="button" aria-pressed={watchedKeys.has(reward.trackingKey)} aria-label={(watchedKeys.has(reward.trackingKey) ? "Не отслеживать: " : "Отслеживать: ") + reward.displayName} onclick={() => onWatch(reward)}>{watchedKeys.has(reward.trackingKey) ? "Отслеживается" : "Отслеживать"}</button>
      </div>
      <div class="reward-links">
        <button class="text-button" type="button" onclick={() => onFind(reward)} aria-label={"Найти заказы: " + reward.displayName}>Сравнить заказы с наградой</button>
        {#if reward.slug && reward.marketKey}<button class="text-button" type="button" onclick={() => onMarket(reward)} aria-label={"Открыть на Warframe Market: " + reward.displayName}>Warframe Market ↗</button>{/if}
      </div>
    </article>
  {:else}<p class="empty-rewards">В источнике нет списка наград этого заказа.</p>{/each}
</div>
</section>
</div>

<details class="calculation">
  <summary>Как рассчитана стоимость</summary>
  <p>Для каждой продаваемой награды учитываем цену и среднее количество выпадений за этапы. Складываем известные оценки; награды без надёжной цены в сумму не входят.</p>
  <p>Среднее количество учитывает повторные выпадения и может отличаться от шанса получить предмет хотя бы один раз. При ненадёжных данных оценка может быть снижена.</p>
  <dl>{#each job.rewards.filter(reward => reward.marketKey) as reward}<div><dt>{reward.displayName}<small>В среднем {num(reward.expectedQuantity)} шт.</small></dt><dd>{platinum(reward.expectedPlatinum)}</dd></div>{/each}</dl>
  {#if priceDate}<p>Сохранённые цены от {priceDate}. Кнопка проверки запрашивает текущие предложения игроков.</p>{/if}
</details>

<style>
  h2,h3,h4,p,dl,dd { margin:0; }
  h2 { font-size:1.4rem; line-height:1.25; margin-top:1rem; overflow-wrap:anywhere; }
  h3 { font-size:1.05rem; } h4 { font-size:.9rem; line-height:1.35; overflow-wrap:anywhere; }
  p { line-height:1.5; font-size:.8125rem; color:var(--text-muted); }
  button { min-height:2.25rem; }
  .detail-topline,.destination,.estimate-line,.rewards-heading,.reward-heading,.reward-bottom,.reward-links { display:flex; align-items:center; gap:.75rem; }
  .detail-topline,.rewards-heading,.reward-bottom { justify-content:space-between; }
  .eyebrow { font-size:.75rem; letter-spacing:.1em; text-transform:uppercase; color:var(--text-subtle); font-weight:700; }
  .back { display:none; }
  .destination { margin-top:1.1rem; }
  .region-art { width:2.25rem; height:2.25rem; color:var(--accent); }
  .destination div { display:grid; gap:.15rem; }
  .destination strong { font-size:.875rem; }
  .destination span:not(.region-art) { font-size:.75rem; color:var(--text-muted); font-variant-numeric:tabular-nums; }
  .instruction { margin-top:.5rem; }
  .facts { display:grid; grid-template-columns:repeat(3,1fr); margin:1.2rem 0; padding:.85rem 0; border-block:1px solid var(--border); gap:.75rem; }
  .facts dt { font-size:.75rem; color:var(--text-muted); margin-bottom:.2rem; }
  .facts dd { font-size:1rem; font-weight:700; }
  .condition { margin:-.5rem 0 1rem; }
  .estimate { padding:1rem; border-radius:.65rem; background:var(--surface-2); border:1px solid var(--border); }
  .outcomes { display:flex; flex-direction:column; }
  .outcomes.seeking .estimate { order:2; margin-bottom:1rem; }
  .outcomes.seeking .rewards-heading { margin-top:.25rem; }
  .estimate-line { justify-content:space-between; align-items:flex-start; }
  .estimate-line > span { font-size:.875rem; font-weight:700; }
  .estimate-line small { display:inline-block; margin-top:.2rem; font-size:.75rem; font-weight:400; color:var(--text-muted); }
  .estimate-line strong { font-size:1.35rem; color:var(--accent); white-space:nowrap; }
  .estimate p { margin-top:.65rem; }
  .estimate button { margin-top:.8rem; width:100%; }
  .estimate .partial { font-weight:600; color:var(--accent); }
  .estimate .check-message { color:var(--text); }
  .rewards-heading { margin-top:1.5rem; }
  .rewards-heading span { font-size:.75rem; color:var(--text-muted); white-space:nowrap; }
  .chance-help { margin:.45rem 0 1rem; }
  .reward { border-top:1px solid var(--border); padding:1rem 0; }
  .reward.target { border-radius:.5rem; background:var(--accent-soft); padding:1rem .75rem; }
  .reward-heading img { width:2.2rem; height:2.75rem; object-fit:contain; flex:none; }
  .reward-name { flex:1; min-width:0; }
  .reward-name span { display:block; font-size:.75rem; color:var(--text-muted); margin-top:.25rem; }
  .chance { color:var(--accent); font-size:1.15rem; font-variant-numeric:tabular-nums; white-space:nowrap; }
  .chance small { font-size:.8125rem; margin-left:.12rem; }
  .reward-bottom { margin-top:.7rem; flex-wrap:wrap; }
  .unit-price { color:var(--text-muted); font-size:.8125rem; }
  .watch { font-size:.75rem; }
  .watch[aria-pressed="true"] { background:var(--accent-soft); border-color:var(--accent); color:var(--accent-strong); }
  .reward-links { flex-wrap:wrap; gap:.4rem .8rem; margin-top:.4rem; }
  .reward-links button { padding:.2rem 0; border:0; min-height:1.9rem; font-size:.75rem; color:var(--accent); font-weight:600; text-align:left; }
  .calculation { border-top:1px solid var(--border); padding-top:1rem; }
  summary { cursor:pointer; font-size:.8125rem; font-weight:600; }
  .calculation p { margin-top:.7rem; }
  .calculation dl { margin-top:.8rem; }
  .calculation dl div { display:flex; justify-content:space-between; gap:1rem; padding:.5rem 0; font-size:.8125rem; }
  .calculation dt small { display:block; color:var(--text-muted); font-size:.75rem; margin-top:.2rem; }
  .calculation dd { flex:none; }
  .empty-rewards { padding-bottom:1rem; }
  @container bounty (max-width:68rem) { .back { display:block; } }
</style>
