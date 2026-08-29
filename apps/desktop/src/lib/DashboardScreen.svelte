<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  import {
    bestSellRows,
    dashboardSummary,
    liquidityBand,
    liquidityLabel,
    rowsWorthChecking,
  } from "./dashboard";
  import { formatDiagnosticDate, type DiagnosticsStatus } from "./diagnostics";
  import { providerLabel } from "./foundation";
  import { timingLabel } from "./history";
  import { localeCode, useLocale } from "./i18n";
  import type { InventoryView } from "./inventory";
  import { confidenceLabel, formatPlatinum, formatVolume } from "./market";
  import { priorityLabel, sellNowRowIdentity, type SellNowView } from "./sellNow";

  export let onOpenSellNow: () => void;
  export let onOpenInventory: () => void;
  export let onOpenDiagnostics: () => void;

  const locale = useLocale();
  const copy = {
    ru: {
      kicker: "Сводка", heading: "Что проверить сегодня",
      intro: "Начните с предметов, где цена надёжнее, а спрос выше.",
      openSellNow: "Перейти к продаже", refreshing: "Пересчитываем…", refresh: "Пересчитать",
      reading: "Загружаем данные…", ready: (date: string) => `Инвентарь обновлён ${date}.`,
      loadError: (_reason: string) => "Не удалось собрать сводку. Перезапустите PlatScope или повторите попытку.",
      unavailable: "Сводка недоступна", retry: "Повторить", firstStep: "Первый шаг",
      importHeading: "Добавьте инвентарь", importBody: "После сканирования PlatScope покажет количество и предметы, которые можно продать.", openInventory: "Открыть инвентарь",
      portfolio: "Инвентарь", summary: "По текущим оценкам", coverage: (value: number) => `Цена есть у ${value}% предметов в очереди продажи.`,
      inventoryNominal: "Оценка всего инвентаря",
      sellableNominal: "Оценка предметов к продаже",
      sellableCopies: "Предметов к продаже",
      attention: "Нужно проверить",
      bestKicker: "Сначала выставить", bestHeading: "С чего начать", fullQueue: "Вся очередь",
      sellableDeals: (quantity: number, volume: string) => `${quantity} шт. к продаже · ${volume} сделок`,
      noCandidates: "Нет кандидатов с рассчитанной ценой", noCandidatesBody: "Проверьте инвентарь и дату рыночных данных.",
      liquidityKicker: "Низкий спрос", liquidityHeading: "Что может продаваться медленно",
      confidenceTiming: (confidence: string, timing: string) => `${confidence} · ${timing}`, noTiming: "Нет сигнала момента продажи",
      noWeak: "Явно слабых кандидатов нет", noWeakBody: "Это не гарантирует быструю продажу всего объёма.",
      freshnessKicker: "Данные", freshnessHeading: "Что нужно обновить", marketSnapshot: "Цены рынка",
      noData: "Нет данных", updateMarket: "Обновите рыночные данные", history: "История", days: "дней",
      offlineCache: "Сохранённые данные", cacheReady: "Готовы", cacheAttention: "Нужно проверить", openDiagnostics: "Проверить данные",
    },
    en: {
      kicker: "Offline overview", heading: "What to review today",
      intro: "This summary uses the latest valid inventory, bulk prices, and history. Opening the dashboard does not make network requests.",
      openSellNow: "Open Sell now", refreshing: "Refreshing summary…", refresh: "Refresh summary",
      reading: "Reading local snapshots…", ready: (date: string) => `Summary ready · inventory observed ${date}.`,
      loadError: (_reason: string) => "Unable to build the summary. Restart PlatScope or try again.",
      unavailable: "Dashboard unavailable", retry: "Try again", firstStep: "First step",
      importHeading: "Import inventory", importBody: "Without a local snapshot, PlatScope will not guess owned or sellable copies.", openInventory: "Open inventory",
      portfolio: "Portfolio snapshot", summary: "Local summary", coverage: (value: number) => `${value}% of sellable candidates have a reliable bulk price.`,
      inventoryNominal: "Inventory value",
      sellableNominal: "Sellable value",
      sellableCopies: "Items to sell",
      attention: "Needs review",
      bestKicker: "Best now", bestHeading: "Top candidates", fullQueue: "Open full queue",
      sellableDeals: (quantity: number, volume: string) => `${quantity} sellable · ${volume} trades`,
      noCandidates: "No priced sellable candidates", noCandidatesBody: "Check inventory and market snapshot freshness.",
      liquidityKicker: "Liquidity watch", liquidityHeading: "What may sell slowly",
      confidenceTiming: (confidence: string, timing: string) => `${confidence} confidence · ${timing}`, noTiming: "No timing signal",
      noWeak: "No clearly weak candidates", noWeakBody: "This does not guarantee that the full volume will sell quickly.",
      freshnessKicker: "Data freshness", freshnessHeading: "Freshness and coverage", marketSnapshot: "Market snapshot",
      noData: "No data", updateMarket: "Refresh market data", history: "History", days: "days",
      offlineCache: "Offline cache", cacheReady: "Ready", cacheAttention: "Needs attention", openDiagnostics: "Open diagnostics",
    },
  } as const;
  $: c = copy[$locale];

  let inventory: InventoryView | null = null;
  let sellNow: SellNowView | null = null;
  let diagnostics: DiagnosticsStatus | null = null;
  let loading = true;
  let errorMessage = "";

  $: summary = inventory ? dashboardSummary(inventory, sellNow) : null;
  $: bestRows = bestSellRows(sellNow?.rows ?? []);
  $: attentionRows = rowsWorthChecking(sellNow?.rows ?? []);

  async function loadDashboard(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      [sellNow, inventory, diagnostics] = await Promise.all([
        invoke<SellNowView | null>("sell_now"),
        invoke<InventoryView | null>("load_inventory"),
        invoke<DiagnosticsStatus>("diagnostics_status"),
      ]);
    } catch (error) {
      sellNow = null;
      inventory = null;
      diagnostics = null;
      errorMessage = c.loadError(String(error));
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadDashboard();
  });
</script>

<div class="dashboard-shell" aria-busy={loading}>
  <section class="dashboard-intro" aria-labelledby="dashboard-overview-heading">
    <div>
      <p class="section-kicker">{c.kicker}</p>
      <h2 id="dashboard-overview-heading">{c.heading}</h2>
      <p>{c.intro}</p>
    </div>
    <div class="dashboard-actions">
      <button type="button" onclick={onOpenSellNow}>{c.openSellNow}</button>
      <button type="button" class="secondary" onclick={loadDashboard} disabled={loading}>
        {loading ? c.refreshing : c.refresh}
      </button>
    </div>
  </section>

  <div class="dashboard-status" role="status" aria-live="polite">
    {#if loading}
      {c.reading}
    {:else if inventory && diagnostics}
      {c.ready(formatDiagnosticDate(inventory.metadata.observedAt, $locale))}
    {/if}
  </div>

  {#if errorMessage}
    <section class="dashboard-error" role="alert">
      <h2>{c.unavailable}</h2>
      <p>{errorMessage}</p>
      <button type="button" onclick={loadDashboard}>{c.retry}</button>
    </section>
  {:else if !loading && !inventory}
    <section class="dashboard-empty" aria-labelledby="dashboard-empty-heading">
      <p class="section-kicker">{c.firstStep}</p>
      <h2 id="dashboard-empty-heading">{c.importHeading}</h2>
      <p>{c.importBody}</p>
      <button type="button" onclick={onOpenInventory}>{c.openInventory}</button>
    </section>
  {:else if summary && inventory && diagnostics}
    <section aria-labelledby="portfolio-heading">
      <div class="section-heading">
        <div>
          <p class="section-kicker">{c.portfolio}</p>
          <h2 id="portfolio-heading">{c.summary}</h2>
        </div>
        <p>{c.coverage(summary.pricedCoveragePercent)}</p>
      </div>
      <dl class="dashboard-summary">
        <div>
          <dt>{c.inventoryNominal}</dt>
          <dd>{formatPlatinum(summary.nominalInventoryValue, $locale)}</dd>
        </div>
        <div>
          <dt>{c.sellableNominal}</dt>
          <dd>{formatPlatinum(summary.nominalSellableValue, $locale)}</dd>
        </div>
        <div>
          <dt>{c.sellableCopies}</dt>
          <dd>{summary.sellableCopies.toLocaleString(localeCode($locale))}</dd>
        </div>
        <div>
          <dt>{c.attention}</dt>
          <dd>{summary.attentionRows.toLocaleString(localeCode($locale))}</dd>
        </div>
      </dl>
    </section>

    <div class="dashboard-grid">
      <section class="dashboard-panel" aria-labelledby="best-heading">
        <div class="panel-heading">
          <div>
            <p class="section-kicker">{c.bestKicker}</p>
            <h2 id="best-heading">{c.bestHeading}</h2>
          </div>
          <button type="button" class="text-button" onclick={onOpenSellNow}>{c.fullQueue}</button>
        </div>
        {#if bestRows.length}
          <ol class="candidate-list">
            {#each bestRows as row (sellNowRowIdentity(row))}
              <li>
                {#if row.inventory.imageUrl}<img class="dashboard-item-art" src={row.inventory.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                <div>
                  <h3>{row.inventory.displayName}</h3>
                  <p>{c.sellableDeals(row.inventory.sellableQuantity, formatVolume(row.recommendation?.closedVolume ?? null, $locale))}</p>
                </div>
                <div class="candidate-value">
                  <strong>{formatPlatinum(row.recommendation?.fairPrice ?? null, $locale)}</strong>
                  <span>{priorityLabel(row.priority.band, $locale)}</span>
                </div>
              </li>
            {/each}
          </ol>
        {:else}
          <div class="inline-empty">
            <h3>{c.noCandidates}</h3>
            <p>{c.noCandidatesBody}</p>
          </div>
        {/if}
      </section>

      <section class="dashboard-panel" aria-labelledby="liquidity-heading">
        <div class="panel-heading">
          <div>
            <p class="section-kicker">{c.liquidityKicker}</p>
            <h2 id="liquidity-heading">{c.liquidityHeading}</h2>
          </div>
        </div>
        {#if attentionRows.length}
          <ul class="liquidity-list">
            {#each attentionRows as row (sellNowRowIdentity(row))}
              <li>
                {#if row.inventory.imageUrl}<img class="dashboard-item-art" src={row.inventory.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                <div>
                  <h3>{row.inventory.displayName}</h3>
                  <p>{c.confidenceTiming(confidenceLabel(row.recommendation?.confidence ?? "unknown", $locale), row.trend?.timing ? timingLabel(row.trend.timing, $locale) : c.noTiming)}</p>
                </div>
                <span class="liquidity-pill liquidity-pill--{liquidityBand(row)}">{liquidityLabel(liquidityBand(row), $locale)}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <div class="inline-empty">
            <h3>{c.noWeak}</h3>
            <p>{c.noWeakBody}</p>
          </div>
        {/if}
      </section>
    </div>

    <section class="freshness-panel" aria-labelledby="freshness-heading">
      <div>
        <p class="section-kicker">{c.freshnessKicker}</p>
        <h2 id="freshness-heading">{c.freshnessHeading}</h2>
      </div>
      <dl>
        <div>
          <dt>{c.marketSnapshot}</dt>
          <dd>{diagnostics.foundation.marketSnapshot?.sourceDate ?? c.noData}</dd>
          <p>{diagnostics.foundation.marketSnapshot ? providerLabel(diagnostics.foundation.marketSnapshot.provider, $locale) : c.updateMarket}</p>
        </div>
        <div>
          <dt>{c.history}</dt>
          <dd>{diagnostics.foundation.historyCoverage.dayCount.toLocaleString(localeCode($locale))} {c.days}</dd>
          <p>{diagnostics.foundation.historyCoverage.oldestDate ?? "—"} — {diagnostics.foundation.historyCoverage.newestDate ?? "—"}</p>
        </div>
        <div>
          <dt>{c.offlineCache}</dt>
          <dd>{diagnostics.foundation.offlineReady ? c.cacheReady : c.cacheAttention}</dd>
          <button type="button" class="text-button" onclick={onOpenDiagnostics}>{c.openDiagnostics}</button>
        </div>
      </dl>
    </section>
  {/if}
</div>

<style>
  .dashboard-shell { display: grid; gap: 1rem; }
  .dashboard-intro, .dashboard-panel, .freshness-panel, .dashboard-empty, .dashboard-error, .dashboard-summary > div { border: 1px solid var(--border); border-radius: .75rem; background: var(--surface-1); box-shadow: var(--shadow-sm); }
  .dashboard-intro, .section-heading, .panel-heading { display: flex; align-items: start; justify-content: space-between; gap: 1.25rem; }
  .dashboard-intro, .dashboard-empty, .dashboard-error, .freshness-panel { padding: .75rem; }
  .dashboard-intro h2, .section-heading h2, .panel-heading h2, .freshness-panel h2, .dashboard-empty h2, .dashboard-error h2 { margin-block-end: .35rem; font-size: 1.2rem; }
  .dashboard-intro p, .dashboard-empty p, .dashboard-error p { max-width: 68ch; margin-block-end: 0; color: var(--text-muted); line-height: 1.5; }
  .section-kicker { margin-block-end: .3rem !important; color: var(--accent-strong) !important; font-size: .76rem; font-weight: 750; letter-spacing: .08em; text-transform: uppercase; }
  .dashboard-actions { display: flex; flex: 0 0 auto; gap: .65rem; }
  button.secondary, button.text-button { background: transparent; }
  button.text-button { min-height: 2.125rem; border-color: transparent; color: var(--accent-strong); }
  .dashboard-status { min-height: 1.5rem; color: var(--text-muted); }
  .dashboard-error { border-color: var(--danger); background: var(--danger-soft); }
  .dashboard-error p, .dashboard-empty p { margin-block-end: .85rem; }
  section { min-width: 0; }
  .dashboard-item-art { flex: none; width: 3.25rem; height: 3.25rem; object-fit: contain; outline: 1px solid oklch(0 0 0 / 0.1); outline-offset: -1px; }
  .section-heading, .panel-heading { align-items: center; margin-block-end: .7rem; }
  .section-heading > p { max-width: 32rem; margin: 0; color: var(--text-muted); font-size: .84rem; text-align: end; }
  .dashboard-summary { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: .75rem; margin: 0; }
  .dashboard-summary > div { display: grid; min-width: 0; align-content: center; min-height: 3.75rem; padding: .65rem; }
  dt { color: var(--text-muted); font-size: .78rem; }
  dd { margin: .2rem 0 0; color: var(--text); font-size: 1.25rem; font-variant-numeric: tabular-nums; font-weight: 760; }
  .freshness-panel p { margin: .35rem 0 0; color: var(--text-muted); font-size: .76rem; line-height: 1.4; }
  .dashboard-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .8rem; }
  .dashboard-panel { min-width: 0; padding: .75rem; }
  .candidate-list, .liquidity-list { display: grid; gap: .55rem; margin: 0; padding: 0; list-style: none; }
  .candidate-list li, .liquidity-list li { display: flex; align-items: center; justify-content: space-between; gap: .8rem; border: 1px solid var(--border); border-radius: .75rem; padding: .75rem; background: var(--surface-2); }
  .candidate-list h3, .liquidity-list h3 { margin-block-end: .2rem; overflow-wrap: anywhere; }
  .candidate-list p, .liquidity-list p { margin: 0; color: var(--text-muted); font-size: .78rem; }
  .candidate-value { display: grid; flex: 0 0 auto; gap: .15rem; text-align: end; }
  .candidate-value strong { color: var(--accent-strong); font-size: 1.05rem; }
  .candidate-value span { color: var(--text-muted); font-size: .72rem; }
  .liquidity-pill { flex: 0 0 auto; border-radius: 999px; padding: .18rem .42rem; font-size: .6875rem; font-weight: 750; }
  .liquidity-pill--unpriced, .liquidity-pill--thin { background: var(--danger-soft); color: var(--danger); }
  .liquidity-pill--limited { background: oklch(0.92 0.055 78); color: oklch(0.43 0.075 68); }
  .liquidity-pill--active { background: var(--success-soft); color: oklch(0.37 0.08 145); }
  .inline-empty { border-radius: .55rem; padding: .75rem; background: var(--surface-2); text-align: center; }
  .inline-empty p { margin: 0; color: var(--text-muted); }
  .freshness-panel { display: flex; align-items: start; justify-content: space-between; gap: 1rem; }
  .freshness-panel dl { display: grid; grid-template-columns: repeat(3, minmax(9rem, 1fr)); gap: .65rem; width: min(60rem, 70%); margin: 0; }
  .freshness-panel dl > div { min-width: 0; border: 1px solid var(--border); border-radius: .75rem; padding: .7rem; background: var(--surface-2); }
  .freshness-panel dd { overflow-wrap: anywhere; font-size: 1rem; }
  .freshness-panel .text-button { padding-inline: 0; }

  @media (max-width: 64rem) {
    .dashboard-summary { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .freshness-panel { flex-direction: column; }
    .freshness-panel dl { width: 100%; }
  }

  @media (max-width: 46rem) {
    .dashboard-intro, .section-heading, .panel-heading { align-items: stretch; flex-direction: column; }
    .dashboard-actions { width: 100%; flex-direction: column; }
    .dashboard-actions button { width: 100%; }
    .section-heading > p { text-align: start; }
    .dashboard-grid, .freshness-panel dl { grid-template-columns: minmax(0, 1fr); }
    .candidate-list li, .liquidity-list li { align-items: stretch; flex-direction: column; }
    .candidate-value { text-align: start; }
    .liquidity-pill { width: fit-content; }
  }

  @media (max-width: 30rem) {
    .dashboard-summary { grid-template-columns: minmax(0, 1fr); }
  }

  @media (forced-colors: active) {
    .dashboard-intro, .dashboard-panel, .freshness-panel, .dashboard-empty, .dashboard-error, .dashboard-summary > div { border: 1px solid CanvasText; }
  }
</style>
