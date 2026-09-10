<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import type { TradeShiftRow } from "./tradeShift";
  import type { MarketAnalyticsSummary } from "./marketAnalytics";
  import type { MarketHistoryView } from "./history";
  import type { LivePricingResult } from "./market";
  import MarketOffers from "./MarketOffers.svelte";
  import MarketTrendChart from "./MarketTrendChart.svelte";
  import { summarizeMarketPeriod, marketChangeLabel } from "./marketPeriod";
  export let row: TradeShiftRow;
  export let onClose: () => void;
  export let showClose = true;
  export let showOffers = true;
  export let loading = false;
  export let onRetry: () => void;
  export let unavailable = "";
  let activeDate: string | null = null;
  export let summary: MarketAnalyticsSummary | null = null;
  let range: 7 | 30 | 90 = 7;
  let history: MarketHistoryView | null = null;
  let historyLoading = false;
  let historyError = "";
  let live: LivePricingResult | null = null;
  let liveLoading = false;
  let liveError = "";
  let revision = 0;
  let disposed = false;
  onDestroy(() => { disposed = true; ++revision; });
  const number = (v: number | null | undefined) => v == null ? "—" : v.toLocaleString("ru-RU", { maximumFractionDigits:1 });
  $: points = range === 7 ? (summary?.days.slice(7).map(day => ({sourceDate:day.date, closedMedian:day.price, closedVolume:day.volume})) ?? []) : extendedPoints(history, range);
  $: stats = summarizeMarketPeriod(points);
  $: activePoint = points.find(point => point.sourceDate === activeDate);
  const shortDate = (value: string | undefined) => value ? value.slice(5).split("-").reverse().join(".") : "";
  function extendedPoints(view: MarketHistoryView | null, days: number) {
    const end = new Date(); end.setUTCHours(0,0,0,0); end.setUTCDate(end.getUTCDate() - 1);
    return Array.from({length:days}, (_,i) => {
      const date = new Date(end.getTime() - (days-i-1)*86400000).toISOString().slice(0,10);
      const p = view?.points.find(p => p.sourceDate === date);
      return {sourceDate:date, closedMedian:p && p.closedVolume > 0 ? p.closedMedian : null, closedVolume:p && (p.closedMedian !== null || p.closedVolume > 0) ? p.closedVolume : null};
    });
  }
  async function changeRange(days: 7 | 30 | 90) {
    range = days; historyError = ""; activeDate = null;
    const request = ++revision;
    if (days === 7 || !row.key) { historyLoading = false; return; }
    historyLoading = true; history = null;
    try { const result = await invoke<MarketHistoryView>("market_history", {key:row.key,days}); if (!disposed && request === revision) history = result; }
    catch { if (!disposed && request === revision) historyError = "Не удалось загрузить историю. Повторите выбор периода."; }
    finally { if (!disposed && request === revision) historyLoading = false; }
  }
  async function checkOffers() {
    if (!row.key || liveLoading) return;
    liveLoading = true; liveError = "";
    try { const result = await invoke<LivePricingResult | null>("live_price_current_variant",{key:row.key,itemKind:row.itemKind}); if (!disposed) {live = result; if (!result) liveError = "Предложения пока недоступны.";} }
    catch { if (!disposed) liveError = "Не удалось получить предложения. Повторите проверку."; }
    finally { if (!disposed) liveLoading = false; }
  }
</script>

<section class="insight" aria-label={"Статистика: " + (row.item?.displayName ?? "предмет")}>
  <div class="insight-toolbar">
    <div class="range-control" role="group" aria-label="Период статистики">{#each [7,30,90] as days}<button aria-pressed={range === days} onclick={() => changeRange(days as 7 | 30 | 90)}>{days} дней</button>{/each}</div>
    <span class="period-dates">{shortDate(points[0]?.sourceDate)} — {shortDate(points.at(-1)?.sourceDate)}</span>
    {#if showClose}<button class="collapse" onclick={onClose}>Свернуть <span aria-hidden="true">×</span></button>{/if}
  </div>
  {#if historyLoading || (range === 7 && loading)}<div class="history-state" role="status">Загружаем историю за {range} дней…</div>
  {:else if historyError || (range === 7 && unavailable)}<div class="history-state" role="alert"><span>{historyError || unavailable}</span>{#if summary?.supported !== false}<button class="secondary" onclick={() => range === 7 ? onRetry() : changeRange(range)}>Повторить загрузку</button>{/if}</div>
  {:else}
    <div class="charts">
      <section class="metric">
        <div class="metric-heading"><h4>Цена сделок <small>за штуку</small></h4><strong>{number(stats.median)}{#if stats.median !== null}<span> пл.</span>{/if}</strong></div>
        <p class="comparison">
          {#if range === 7 && summary?.priceChangePct != null}<b class:up={summary.priceChangePct > 0} class:down={summary.priceChangePct < 0}>{marketChangeLabel(summary.priceChangePct)}</b> к прошлой неделе <span>· было {number(summary.previous.medianPrice)} пл.</span>
          {:else if stats.pricedDays < range}Есть цены за {stats.pricedDays} из {range} дней
          {:else}За период: {number(stats.low)}–{number(stats.high)} пл. <span>· в заголовке медиана</span>{/if}
        </p>
        <MarketTrendChart {points} bind:activeDate/>
      </section>
      <section class="metric">
        <div class="metric-heading"><h4>Объём торгов <small>в среднем</small></h4><strong>{stats.dailyVolume === null ? "—" : (Number.isInteger(stats.dailyVolume) ? "" : "≈ ") + number(stats.dailyVolume)}{#if stats.dailyVolume !== null}<span> в день</span>{/if}</strong></div>
        <p class="comparison">
          {#if range === 7 && summary?.volumeChangePct != null}<b class:up={summary.volumeChangePct > 0} class:down={summary.volumeChangePct < 0}>{marketChangeLabel(summary.volumeChangePct)}</b> к прошлой неделе <span>· было {number(summary.previous.dailyVolume)} в день</span>
          {:else if stats.observedDays < range}Есть объём за {stats.observedDays} из {range} дней
          {:else}Объём за {range} дней: {number(stats.volume)}{/if}
        </p>
        <MarketTrendChart {points} metric="volume" bind:activeDate/>
      </section>
    </div>
    <div class="day-readout" aria-live="polite">
      {#if activePoint}<strong>{shortDate(activePoint.sourceDate)}</strong><span>Цена: <b>{activePoint.closedMedian === null ? "нет данных" : number(activePoint.closedMedian) + " пл./шт."}</b></span><span>Объём: <b>{activePoint.closedVolume === null ? "нет данных" : number(activePoint.closedVolume)}</b></span>
      {:else}<span>Подписи графиков — значения за день. Наведите на график, чтобы сравнить цену и объём.</span>{/if}
    </div>
  {/if}
  <div class="more">
    <details class="calculation"><summary>Данные и расчёт</summary>
      <p>Источник — закрытые сделки Warframe Market, не все обмены в игре. Цена — медиана дневных цен за выбранный период. Объём — исходный показатель торгов WFM; среднее за день доступно только при полной истории. Сравнение недель использует две полные недели. «—» означает отсутствие данных. Учитываются ранг и вариант предмета.</p>
      <div class="data-scroll"><table><thead><tr><th>Дата</th><th>Цена, пл./шт.</th><th>Объём за день</th></tr></thead><tbody>{#each points as point}<tr><td>{shortDate(point.sourceDate)}</td><td>{number(point.closedMedian)}</td><td>{number(point.closedVolume)}</td></tr>{/each}</tbody></table></div>
    </details>
    {#if showOffers}<details class="offers" ontoggle={(event) => { if (event.currentTarget.open && !live && !liveLoading) void checkOffers(); }}><summary>Предложения игроков в игре</summary>
      <div class="offers-content">
        <div class="offers-heading"><span>{liveLoading ? "Загружаем предложения…" : live ? (live.quoteState === "stale_cache" ? "Сохранённые предложения могли устареть" : "Проверено") + " · " + new Date(live.fetchedAt).toLocaleTimeString("ru-RU",{hour:"2-digit",minute:"2-digit"}) : "Текущие предложения Warframe Market"}</span><button class="secondary" disabled={liveLoading || !row.key} onclick={checkOffers}>Обновить предложения</button></div>
        {#if liveError}<p role="alert">{liveError}</p>{/if}
        {#if live}<MarketOffers {live} showHeading={false} itemNameEn={row.item?.displayNameEn ?? ""} itemKey={row.key}/>{/if}
      </div>
    </details>{/if}
  </div>
</section>

<style>
  .insight { font-size:.8125rem; } p { margin:0; } button { min-height:1.9rem; padding:.25rem .65rem; font-size:.75rem; }
  .insight-toolbar { display:flex; gap:.8rem; align-items:center; margin-bottom:.3rem; }
  .range-control { display:flex; padding:2px; border:1px solid var(--border); border-radius:.4rem; background:var(--surface-1); }
  .range-control button { border:0; background:transparent; box-shadow:none; color:var(--text-muted); min-height:1.7rem; padding:.2rem .6rem; }
  .range-control button[aria-pressed=true] { background:var(--accent-soft); color:var(--accent-strong); box-shadow:inset 0 0 0 1px var(--border-strong); }
  .period-dates { color:var(--text-muted); font-size:.75rem; font-variant-numeric:tabular-nums; }
  .collapse { margin-left:auto; background:transparent; color:var(--text-muted); border:0; box-shadow:none; display:flex; align-items:center; gap:.6rem; }
  .collapse span { font-size:1.2rem; line-height:1; } .collapse:hover { background:var(--accent-soft); color:var(--text); }
  .charts { display:grid; grid-template-columns:minmax(0,1fr) minmax(0,1fr); gap:1.6rem; }
  .metric { min-width:0; } .metric + .metric { padding-left:1.6rem; border-left:1px solid var(--border); }
  .metric-heading { display:flex; align-items:baseline; justify-content:space-between; gap:.5rem; }
  h4 { margin:0; font-size:.8125rem; } h4 small { font-size:.75rem; color:var(--text-muted); font-weight:400; margin-left:.35rem; }
  .metric-heading > strong { font-size:1rem; white-space:nowrap; font-variant-numeric:tabular-nums; } .metric-heading strong span { font-size:.75rem; font-weight:400; margin-left:.25rem; }
  .comparison { min-height:1.4rem; font-size:.75rem; color:var(--text-muted); line-height:1.4; padding-top:.2rem; }
  .comparison b { font-weight:600; } .up { color:var(--success); } .down { color:var(--danger); }
  .day-readout { display:flex; align-items:center; gap:1rem; min-height:1.5rem; font-size:.75rem; color:var(--text-muted); font-variant-numeric:tabular-nums; }
  .day-readout b,.day-readout strong { color:var(--text); font-weight:600; }
  .more { display:flex; flex-wrap:wrap; column-gap:1.5rem; border-top:1px solid var(--border); margin-top:.15rem; padding-top:.2rem; align-items:flex-start; }
  details { min-width:0; } details[open] { width:100%; } details[open] + details { margin-top:.6rem; }
  summary { width:fit-content; cursor:pointer; color:var(--text-muted); font-size:.75rem; padding:.15rem 0; }
  details p { margin:.5rem 0; font-size:.75rem; line-height:1.5; color:var(--text-muted); max-width:70rem; }
  .data-scroll { max-height:12rem; overflow:auto; max-width:32rem; }
  table { border-collapse:collapse; width:100%; font-size:.75rem; font-variant-numeric:tabular-nums; } th,td { text-align:left; border-bottom:1px solid var(--border); padding:.3rem .5rem; }
  .offers-heading { display:flex; justify-content:space-between; align-items:center; gap:1rem; margin:.6rem 0; font-size:.75rem; color:var(--text-muted); }
  .history-state { min-height:8rem; display:flex; align-items:center; justify-content:center; gap:1rem; color:var(--text-muted); }
  @container order-list (max-width:56rem) { .charts { gap:1rem; } .metric + .metric { padding-left:1rem; } h4 small { display:block; margin:0; } }
  @container order-list (max-width:34rem) { .charts { grid-template-columns:1fr; gap:.6rem; } .metric + .metric { padding:0; border:0; border-top:1px solid var(--border); padding-top:.5rem; } .period-dates { display:none; } .day-readout { gap:.5rem; flex-wrap:wrap; } }
</style>
