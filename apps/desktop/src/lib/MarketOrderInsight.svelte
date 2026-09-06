<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import type { TradeShiftRow } from "./tradeShift";
  import type { MarketAnalyticsSummary } from "./marketAnalytics";
  import type { MarketHistoryView } from "./history";
  import type { LivePricingResult } from "./market";
  import MarketOffers from "./MarketOffers.svelte";
  import MarketTrendChart from "./MarketTrendChart.svelte";
  export let row: TradeShiftRow;
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
  $: priced = points.flatMap(p => p.closedMedian === null ? [] : [p.closedMedian]).sort((a,b) => a-b);
  $: median = priced.length ? (priced[Math.floor((priced.length-1)/2)] + priced[Math.ceil((priced.length-1)/2)])/2 : null;
  function extendedPoints(view: MarketHistoryView | null, days: number) {
    const end = new Date(); end.setUTCHours(0,0,0,0); end.setUTCDate(end.getUTCDate() - 1);
    return Array.from({length:days}, (_,i) => {
      const date = new Date(end.getTime() - (days-i-1)*86400000).toISOString().slice(0,10);
      const p = view?.points.find(p => p.sourceDate === date);
      return {sourceDate:date, closedMedian:p?.closedMedian ?? null, closedVolume:p && (p.closedMedian !== null || p.closedVolume > 0) ? p.closedVolume : null};
    });
  }
  async function changeRange(days: 7 | 30 | 90) {
    range = days; historyError = "";
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

<div class="insight-toolbar"><div role="group" aria-label="Период статистики">{#each [7,30,90] as days}<button class="secondary" aria-pressed={range === days} onclick={() => changeRange(days as 7 | 30 | 90)}>{days} дней</button>{/each}</div><span>Медиана цены: <strong>{number(median)} пл.</strong> / шт. <small>(данные {priced.length}/{range} дней)</small></span></div>
{#if historyLoading}<p role="status">Загружаем историю…</p>{:else if historyError}<p role="alert">{historyError}</p>{:else}
  <div class="charts"><section><h4>Цена закрытых сделок, пл.</h4><MarketTrendChart {points}/></section><section><h4>Объём закрытых сделок</h4><MarketTrendChart {points} metric="volume"/></section></div>
  <p class="hint">{points[0]?.sourceDate ?? ""} — {points.at(-1)?.sourceDate ?? ""} · Данные по выбранному рангу и варианту. Пробел означает отсутствие данных. Объём отражает закрытые сделки на Warframe Market, а не все обмены в игре.</p>
  <details><summary>Значения по дням и расчёт</summary><p class="hint">Цена за неделю — медиана дневных медиан; изменение сравнивает последние 7 полных дней с предыдущими 7. Средний объём и сравнение показываются только при полном покрытии. Цена объявления за партию указана отдельно в списке.</p><div class="data-scroll"><table><thead><tr><th>Дата</th><th>Цена, пл. / шт.</th><th>Объём</th></tr></thead><tbody>{#each points as point}<tr><td>{point.sourceDate}</td><td>{number(point.closedMedian)}</td><td>{number(point.closedVolume)}</td></tr>{/each}</tbody></table></div></details>
{/if}
<div class="offers-heading"><strong>Предложения игроков в игре</strong><button class="secondary" disabled={liveLoading || !row.key} onclick={checkOffers}>{liveLoading ? "Проверяем…" : live ? "Обновить предложения" : "Загрузить предложения"}</button></div>
{#if liveError}<p role="alert">{liveError}</p>{/if}
{#if live}<p class="hint">{live.quoteState === "stale_cache" ? "Сохранённые предложения могли устареть" : "Проверено"} · {new Date(live.fetchedAt).toLocaleTimeString("ru-RU",{hour:"2-digit",minute:"2-digit"})}.</p><MarketOffers {live} showHeading={false} itemNameEn={row.item?.displayNameEn ?? ""} itemKey={row.key}/>{/if}

<style>
  p { margin:.6rem 0; font-size:.8125rem; color:var(--text-muted); } h4 { font-size:.8125rem; margin:0 0 .7rem; }
  .insight-toolbar,.offers-heading { display:flex; gap:1rem; align-items:center; justify-content:space-between; font-size:.8125rem; }
  .insight-toolbar > div { display:flex; gap:.3rem; } button { min-height:2rem; padding:.35rem .65rem; font-size:.75rem; } button[aria-pressed=true] { background:var(--accent-soft); border-color:var(--accent); }
  .charts { display:grid; grid-template-columns:1fr 1fr; gap:1.5rem; margin:.9rem 0; }
  .charts > section { min-width:0; background:var(--surface-1); border:1px solid var(--border); padding:.8rem; border-radius:.5rem; }
  .hint { font-size:.75rem; line-height:1.5; }
  summary { font-size:.75rem; cursor:pointer; color:var(--text-muted); } .data-scroll { max-height:14rem; overflow:auto; }
  table { border-collapse:collapse; width:100%; font-size:.75rem; } th,td { text-align:left; border-bottom:1px solid var(--border); padding:.3rem; }
  .offers-heading { border-top:1px solid var(--border); padding-top:.8rem; margin-top:.8rem; }

  @container order-list (max-width:45rem) { .charts { grid-template-columns:1fr; gap:.6rem; } .insight-toolbar { flex-wrap:wrap; } }
</style>
