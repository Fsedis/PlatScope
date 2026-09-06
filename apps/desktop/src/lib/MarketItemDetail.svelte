<script lang="ts">
  import MarketOffers from "./MarketOffers.svelte";
  import MarketCreateOrder from "./MarketCreateOrder.svelte";
  import HistoryChart from "./HistoryChart.svelte";
  import { formatChange, type MarketHistoryView } from "./history";
  import { freshnessLabel, priceReasonMessage, variantLabel, type MarketSearchRow, type PriceRecommendation, type LivePricingResult } from "./market";
  export let row: MarketSearchRow;
  export let recommendation: PriceRecommendation;
  export let live: LivePricingResult | null;
  export let liveLoading = false;
  export let liveError = "";
  export let history: MarketHistoryView | null;
  export let historyLoading = false;
  export let historyError = "";
  export let historyRange: 7 | 30 | 90 = 7;
  export let marketMessage = "";
  export let onRefresh: () => void;
  export let onHistory: (days: 7 | 30 | 90) => void;
  export let onMarket: () => void;
  export let onInventory: () => void;
  export let onBack: () => void;
  let historyOpen = false;
  let orderSide: "sell" | "buy" = "sell";
  const money = (value: number | null | undefined) => value == null ? "Нет оценки" : value.toLocaleString("ru-RU", { maximumFractionDigits: 1 }) + " пл.";
  $: orders = live?.orders.filter(order => order.side === orderSide) ?? [];
  $: trend = history?.trend;
  $: median = historyRange === 7 ? trend?.median7d : historyRange === 30 ? trend?.median30d : trend?.median90d;
  $: volume = historyRange === 7 ? trend?.volumeAvg7d : historyRange === 30 ? trend?.volumeAvg30d : trend?.volumeAvg90d;
</script>

<div class="detail-top"><span>Выбранный предмет</span><button class="secondary market-back" onclick={onBack}>К результатам</button></div>
<header class="item-heading">{#if row.imageUrl}<img src={row.imageUrl} alt="" />{/if}<div><h2 id="detail-heading" tabindex="-1">{row.displayName}</h2>{#if row.displayNameEn && row.displayNameEn !== row.displayName}<p lang="en" translate="no">{row.displayNameEn}</p>{/if}{#if variantLabel(row.recommendation.key) !== "базовый вариант"}<p>{variantLabel(row.recommendation.key)}</p>{/if}</div></header>
<section class="price-overview" aria-label="Стоимость предмета">
  <div class="selling-price"><span>Ориентир для продажи</span><strong>{money(recommendation.listPrice)}</strong><small>за одну штуку</small></div>
  <div class="buying-price"><span>Лучшая заявка покупателя</span><strong>{live ? (recommendation.quickSell === null ? "Нет заявок" : money(recommendation.quickSell)) : "Нужно проверить"}</strong><small>{live && recommendation.quickSell === null ? "Подходящих заявок нет" : "за одну штуку"}</small></div>
</section>
<p class="price-explanation">Оценка помогает выбрать цену. Реальная продажа зависит от покупателя и доступных предложений.</p>
<button class="check-price" disabled={liveLoading} onclick={onRefresh}>{liveLoading ? "Проверяем предложения…" : "Проверить текущие цены"}</button>
<p class="quote-status" class:error={!!liveError} role="status">{liveError || (live ? (live.quoteState === "stale_cache" ? "Сохранённые предложения могли устареть" : "Предложения проверены") + " · " + new Date(live.fetchedAt).toLocaleString("ru-RU", {day:"numeric", month:"short", hour:"2-digit", minute:"2-digit"}) : "Сохранённая оценка от " + recommendation.sourceDate + ". Проверка запросит предложения игроков в игре.")}</p>
{#if live?.warning}<p class="quote-status error">Часть предложений недоступна. Перед сделкой проверьте цену на Warframe Market.</p>{/if}
<MarketCreateOrder {row}/>
<div class="item-actions"><button class="secondary" onclick={onMarket}>Открыть Warframe Market ↗</button><button class="text-button" onclick={onInventory}>Найти в моих предметах</button></div>
{#if marketMessage}<p class="quote-status" role="status">{marketMessage}</p>{/if}

{#if live}<MarketOffers {live} itemNameEn={row.displayNameEn ?? ""} itemKey={row.recommendation.key}/>{/if}

<details class="history" ontoggle={event => { historyOpen = event.currentTarget.open; if (historyOpen && !history && !historyLoading) onHistory(historyRange); }}>
  <summary>Как менялась цена</summary>
  <div class="history-content"><div class="range" role="group" aria-label="Период истории">{#each [7,30,90] as days}<button class="secondary" aria-pressed={historyRange === days} onclick={() => onHistory(days as 7 | 30 | 90)}>{days} дней</button>{/each}</div>
    {#if historyLoading}<p role="status">Загружаем историю…</p>{:else if historyError}<p class="error" role="alert">{historyError}</p><button class="secondary" onclick={() => onHistory(historyRange)}>Повторить загрузку истории</button>{:else if history}
      <dl class="history-stats"><div><dt>Типичная цена*</dt><dd>{money(median)}</dd></div><div><dt>Изменение</dt><dd>{formatChange(historyRange === 7 ? trend?.change7d ?? null : historyRange === 30 ? trend?.change30d ?? null : trend?.change90d ?? null)}</dd></div><div><dt>Объём / день</dt><dd>{volume == null ? "Нет данных" : volume.toLocaleString("ru-RU", {maximumFractionDigits:1})}</dd></div></dl>
      {#if history.points.length >= 2}<HistoryChart points={history.points} />{:else}<p>Для графика пока недостаточно данных.</p>{/if}<p class="offer-hint">* Медиана за выбранный период. В истории есть данные за {history.points.length} дней.</p>
    {/if}
  </div>
</details>
<details class="calculation"><summary>Оценка и сведения о предмете</summary><dl><div><dt>Оценка рынка</dt><dd>{money(recommendation.fairPrice)}</dd></div><div><dt>Объём закрытых сделок</dt><dd>{recommendation.closedVolume ?? "Нет данных"}</dd></div><div><dt>Данные от</dt><dd>{recommendation.sourceDate}</dd></div><div><dt>Актуальность</dt><dd>{freshnessLabel(recommendation.freshness)}</dd></div><div><dt>Ранг мастерства</dt><dd>{row.masteryRequirement ?? "Неизвестен"}</dd></div>{#if live}<div><dt>Средняя цена до 3 шт.</dt><dd>{money(recommendation.depthThree)}</dd></div><div><dt>Средняя цена до 5 шт.</dt><dd>{money(recommendation.depthPrice)}</dd></div>{/if}</dl><ul>{#each recommendation.reasons as reason}<li>{priceReasonMessage(reason)}</li>{/each}</ul></details>

<style>
  h2,p,dl,dd { margin:0; } p { font-size:.8125rem; line-height:1.5; color:var(--text-muted); }
  button { min-height:2.4rem; }
  .detail-top { display:flex; align-items:center; justify-content:space-between; gap:1rem; margin-bottom:1rem; }
  .detail-top > span { font-size:.75rem; text-transform:uppercase; letter-spacing:.08em; font-weight:600; color:var(--text-subtle); }
  .market-back { display:none; }
  .item-heading { display:flex; gap:1rem; align-items:center; margin-bottom:1.25rem; }
  .item-heading img { width:4.5rem; height:4.5rem; object-fit:contain; flex:none; }
  .item-heading h2 { font-size:1.4rem; line-height:1.25; overflow-wrap:anywhere; scroll-margin:1rem; }
  .item-heading p { margin-top:.4rem; }
  .price-overview { display:grid; grid-template-columns:1fr 1fr; gap:.75rem; }
  .price-overview > div { padding:1rem; border:1px solid var(--border); border-radius:.65rem; background:var(--surface-2); }
  .price-overview .selling-price { background:var(--accent-soft); border-color:var(--border-strong); }
  .price-overview span { display:block; font-size:.8125rem; }
  .price-overview strong { display:block; font-size:1.4rem; color:var(--accent); line-height:1.3; margin:.5rem 0 .2rem; }
  .price-overview small { display:block; font-size:.75rem; color:var(--text-muted); }
  .price-explanation { margin:.75rem 0 1rem; }
  .check-price { width:100%; }
  .quote-status { margin-top:.65rem; font-size:.75rem; }
  .error { color:var(--danger); }
  .item-actions { display:flex; align-items:center; flex-wrap:wrap; gap:.5rem 1rem; margin:1rem 0 1.25rem; }
  .item-actions button { font-size:.8125rem; }






  .offer-hint { margin:.6rem 0 1rem; font-size:.75rem; }
  .history,.calculation { border-top:1px solid var(--border); padding:1rem 0; }
  summary { font-size:.875rem; font-weight:600; cursor:pointer; }
  .history-content { margin-top:1rem; }
  .range { display:flex; gap:.4rem; margin-bottom:1rem; }
  .range button { min-height:2rem; font-size:.75rem; }
  .range button[aria-pressed=true] { background:var(--accent-soft); border-color:var(--accent); }
  .history-stats { display:grid; grid-template-columns:repeat(3,1fr); gap:.6rem; margin-bottom:1rem; }
  .history-stats dt { font-size:.75rem; color:var(--text-muted); }
  .history-stats dd { margin-top:.3rem; font-weight:600; font-size:.875rem; }
  .calculation dl { margin-top:.75rem; }
  .calculation dl > div { display:flex; justify-content:space-between; gap:1rem; font-size:.8125rem; padding:.4rem 0; }
  .calculation dt { color:var(--text-muted); }.calculation dd { text-align:right; }
  .calculation ul { padding-left:1.1rem; margin-bottom:0; font-size:.8125rem; line-height:1.5; color:var(--text-muted); }
  .calculation li { margin-top:.4rem; }
  @media (max-width:1180px) { .market-back { display:block; } }
  @media (max-width:500px) { .price-overview { grid-template-columns:1fr; } .item-heading img { width:3rem; height:3.5rem; } }
</style>
