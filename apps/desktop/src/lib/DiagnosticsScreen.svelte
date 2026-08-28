<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { localeCode, useLocale } from "./i18n";

  import { describeFoundationStatus, providerLabel } from "./foundation";
  import {
    formatDiagnosticDate,
    formatLatency,
    providerConditionLabel,
    providerDiagnosticRows,
    type DiagnosticsExportResult,
    type DiagnosticsStatus,
  } from "./diagnostics";

  const locale = useLocale();
  const copy = {
    ru: {
      loadError: (reason: string) => `Не удалось прочитать локальную диагностику. Перезапустите PlatScope или повторите попытку. Техническая причина: ${reason}`,
      exportError: (reason: string) => `Не удалось сохранить отчёт. Проверьте доступ к локальной папке данных и повторите попытку. Техническая причина: ${reason}`,
      localState: "Локальное состояние", heading: "Диагностика без terminal logs", intro: "Показывает последний сохранённый результат источников и покрытие локальных данных. Открытие экрана не выполняет сетевых запросов.", updating: "Обновляем сведения…", update: "Обновить сведения", saving: "Сохраняем отчёт…", export: "Экспортировать безопасный отчёт",
      reading: "Читаем локальное состояние…", updated: (date: string) => `Сведения обновлены ${date}.`, saved: (path: string, bytes: string) => `Отчёт сохранён: ${path} (${bytes} байт).`, unavailable: "Диагностика недоступна", retry: "Повторить",
      foundation: "Offline foundation", storageVersion: "Хранилище и версия", offlineReady: "Готово offline", attention: "Требует внимания", application: "Приложение", storage: "Хранилище", schema: "Версия схемы", database: "База данных",
      providerHealth: "Provider health", sources: "Внешние источники", savedAttempt: "Статус основан только на последней сохранённой попытке.", lastAttempt: "Последняя попытка", lastSuccess: "Последний успех", latency: "Время ответа", failures: "Ошибок подряд", unchecked: "PlatScope ещё не обращался к этому источнику в текущем локальном профиле.",
      localCoverage: "Local coverage", coverage: "Покрытие данных", market: "Рынок", records: "записей", items: "предметов", sourceDate: "Дата источника:", promoted: "Принят:", noMarket: "Корректного рыночного снимка ещё нет.", catalog: "Каталог", catalogBody: "локализованных и canonical записей", history: "История", days: "дней", inventory: "Инвентарь", inventoryBody: "строк последнего корректного локального снимка",
      privacy: "Что здесь не показывается", privacyBody: "Токены, пароли, account ID, nonce, raw inventory и HTTP authorization headers не входят в диагностический контракт. Сообщения provider-ошибок ограничиваются и сохраняются только в безопасном виде.",
    },
    en: {
      loadError: (reason: string) => `Unable to read local diagnostics. Restart PlatScope or try again. Technical reason: ${reason}`,
      exportError: (reason: string) => `Unable to save the report. Check access to the local data folder and try again. Technical reason: ${reason}`,
      localState: "Local state", heading: "Diagnostics without terminal logs", intro: "Shows the last saved provider result and local data coverage. Opening this screen does not make network requests.", updating: "Refreshing details…", update: "Refresh details", saving: "Saving report…", export: "Export safe report",
      reading: "Reading local state…", updated: (date: string) => `Details refreshed ${date}.`, saved: (path: string, bytes: string) => `Report saved: ${path} (${bytes} bytes).`, unavailable: "Diagnostics unavailable", retry: "Try again",
      foundation: "Offline foundation", storageVersion: "Storage and version", offlineReady: "Offline ready", attention: "Needs attention", application: "Application", storage: "Storage", schema: "Schema version", database: "Database",
      providerHealth: "Provider health", sources: "External sources", savedAttempt: "Status is based only on the last saved attempt.", lastAttempt: "Last attempt", lastSuccess: "Last success", latency: "Response time", failures: "Consecutive failures", unchecked: "PlatScope has not contacted this source in the current local profile.",
      localCoverage: "Local coverage", coverage: "Data coverage", market: "Market", records: "records", items: "items", sourceDate: "Source date:", promoted: "Promoted:", noMarket: "No valid market snapshot yet.", catalog: "Catalog", catalogBody: "localized and canonical records", history: "History", days: "days", inventory: "Inventory", inventoryBody: "rows in the latest valid local snapshot",
      privacy: "What is not shown", privacyBody: "Tokens, passwords, account IDs, nonces, raw inventory, and HTTP authorization headers are excluded from the diagnostics contract. Provider error messages are bounded and stored only in a safe form.",
    },
  } as const;
  $: c = copy[$locale];

  let status: DiagnosticsStatus | null = null;
  let loading = true;
  let errorMessage = "";
  let exporting = false;
  let exportError = "";
  let exportResult: DiagnosticsExportResult | null = null;

  $: providers = status ? providerDiagnosticRows(status, $locale) : [];

  async function loadDiagnostics(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      status = await invoke<DiagnosticsStatus>("diagnostics_status");
    } catch (error) {
      status = null;
      errorMessage = c.loadError(String(error));
    } finally {
      loading = false;
    }
  }

  async function exportDiagnostics(): Promise<void> {
    exporting = true;
    exportError = "";
    exportResult = null;
    try {
      exportResult = await invoke<DiagnosticsExportResult>("export_diagnostics_report");
    } catch (error) {
      exportError = c.exportError(String(error));
    } finally {
      exporting = false;
    }
  }

  onMount(() => {
    void loadDiagnostics();
  });
</script>

<div class="diagnostics-shell" aria-busy={loading}>
  <section class="diagnostics-intro" aria-labelledby="diagnostics-overview-heading">
    <div>
      <p class="section-kicker">{c.localState}</p><h2 id="diagnostics-overview-heading">{c.heading}</h2><p>{c.intro}</p>
    </div>
    <div class="intro-actions">
      <button type="button" onclick={loadDiagnostics} disabled={loading || exporting}>
        {loading ? c.updating : c.update}
      </button>
      <button type="button" class="secondary" onclick={exportDiagnostics} disabled={loading || exporting || !status}>
        {exporting ? c.saving : c.export}
      </button>
    </div>
  </section>

  <div class="diagnostics-status" role="status" aria-live="polite">
    {#if loading}
      {c.reading}
    {:else if status}
      {c.updated(formatDiagnosticDate(status.generatedAt, $locale))}
    {/if}
    {#if exportResult}
      {c.saved(exportResult.path, exportResult.bytes.toLocaleString(localeCode($locale)))}
    {/if}
  </div>

  {#if exportError}
    <p class="export-error" role="alert">{exportError}</p>
  {/if}

  {#if errorMessage}
    <section class="diagnostics-error" role="alert">
      <h2>{c.unavailable}</h2>
      <p>{errorMessage}</p>
      <button type="button" onclick={loadDiagnostics}>{c.retry}</button>
    </section>
  {:else if status}
    <section aria-labelledby="storage-heading">
      <div class="section-heading">
        <div>
          <p class="section-kicker">{c.foundation}</p><h2 id="storage-heading">{c.storageVersion}</h2>
        </div>
        <span class:healthy={status.foundation.offlineReady} class="state-pill">
          {status.foundation.offlineReady ? c.offlineReady : c.attention}
        </span>
      </div>
      <dl class="summary-grid">
        <div><dt>{c.application}</dt><dd>{status.foundation.appName} {status.foundation.appVersion}</dd></div><div><dt>{c.storage}</dt><dd>{describeFoundationStatus(status.foundation, $locale)}</dd></div><div><dt>{c.schema}</dt><dd>{status.foundation.schemaVersion}</dd></div><div class="path-cell"><dt>{c.database}</dt><dd>{status.foundation.databasePath}</dd></div>
      </dl>
    </section>

    <section aria-labelledby="providers-heading">
      <div class="section-heading">
        <div>
          <p class="section-kicker">{c.providerHealth}</p><h2 id="providers-heading">{c.sources}</h2>
        </div>
        <p>{c.savedAttempt}</p>
      </div>
      <div class="provider-grid">
        {#each providers as provider (provider.provider)}
          <article class="provider-card provider-card--{provider.condition}">
            <header>
              <h3>{provider.label}</h3>
              <span class="state-pill state-pill--{provider.condition}">
                {providerConditionLabel(provider.condition, $locale)}
              </span>
            </header>
            <dl>
              <div><dt>{c.lastAttempt}</dt><dd>{formatDiagnosticDate(provider.lastAttempt, $locale)}</dd></div><div><dt>{c.lastSuccess}</dt><dd>{formatDiagnosticDate(provider.lastSuccess, $locale)}</dd></div><div><dt>{c.latency}</dt><dd>{formatLatency(provider.latencyMs, $locale)}</dd></div><div><dt>{c.failures}</dt><dd>{provider.consecutiveFailures.toLocaleString(localeCode($locale))}</dd></div>
            </dl>
            {#if provider.lastErrorCode}
              <p class="provider-error"><strong>{provider.lastErrorCode}</strong>{provider.lastErrorMessage ? ` · ${provider.lastErrorMessage}` : ""}</p>
            {:else if provider.condition === "unchecked"}
              <p class="provider-note">{c.unchecked}</p>
            {/if}
          </article>
        {/each}
      </div>
    </section>

    <section aria-labelledby="coverage-heading">
      <div class="section-heading">
        <div>
          <p class="section-kicker">{c.localCoverage}</p><h2 id="coverage-heading">{c.coverage}</h2>
        </div>
      </div>
      <div class="coverage-grid">
        <article>
          <h3>{c.market}</h3>
          {#if status.foundation.marketSnapshot}
            <p class="metric">{status.foundation.marketSnapshot.recordCount.toLocaleString(localeCode($locale))} {c.records}</p><p>{status.foundation.marketSnapshot.itemCount.toLocaleString(localeCode($locale))} {c.items} · {providerLabel(status.foundation.marketSnapshot.provider, $locale)}</p><p>{c.sourceDate} <time datetime={status.foundation.marketSnapshot.sourceDate}>{status.foundation.marketSnapshot.sourceDate}</time></p><p>{c.promoted} <time datetime={status.foundation.marketSnapshot.promotedAt}>{formatDiagnosticDate(status.foundation.marketSnapshot.promotedAt, $locale)}</time></p>
          {:else}
            <p class="empty-copy">{c.noMarket}</p>
          {/if}
        </article>
        <article>
          <h3>{c.catalog}</h3><p class="metric">{status.foundation.catalogItemCount?.toLocaleString(localeCode($locale)) ?? "—"}</p><p>{c.catalogBody}</p>
        </article>
        <article>
          <h3>{c.history}</h3><p class="metric">{status.foundation.historyCoverage.dayCount.toLocaleString(localeCode($locale))} {c.days}</p>
          <p>{status.foundation.historyCoverage.oldestDate ?? "—"} — {status.foundation.historyCoverage.newestDate ?? "—"}</p>
        </article>
        <article>
          <h3>{c.inventory}</h3><p class="metric">{status.foundation.inventoryItemCount?.toLocaleString(localeCode($locale)) ?? "—"}</p><p>{c.inventoryBody}</p>
        </article>
      </div>
    </section>

    <aside class="privacy-note" aria-labelledby="privacy-heading">
      <h2 id="privacy-heading">{c.privacy}</h2><p>{c.privacyBody}</p>
    </aside>
  {/if}
</div>

<style>
  .diagnostics-shell { display: grid; gap: 1rem; }
  .diagnostics-intro, .section-heading { display: flex; align-items: start; justify-content: space-between; gap: 1.25rem; }
  .diagnostics-intro, .diagnostics-error, .privacy-note, .provider-card, .coverage-grid article, .summary-grid > div { border: 1px solid #283752; border-radius: .8rem; background: #111b2f; box-shadow: 0 .75rem 2rem rgb(0 0 0 / 14%); }
  .diagnostics-intro, .diagnostics-error, .privacy-note { padding: 1rem; }
  .diagnostics-intro h2, .section-heading h2, .diagnostics-error h2, .privacy-note h2 { margin-block-end: .35rem; font-size: 1.2rem; }
  .diagnostics-intro p, .section-heading p, .diagnostics-error p, .privacy-note p { max-width: 68ch; margin-block-end: 0; color: #9ba9bd; line-height: 1.5; }
  .section-kicker { margin-block-end: .3rem !important; color: #72a7ff !important; font-size: .78rem; font-weight: 750; letter-spacing: .08em; text-transform: uppercase; }
  .diagnostics-status { min-height: 1.5rem; color: #9ba9bd; }
  .diagnostics-status span { overflow-wrap: anywhere; color: #d8e5e9; }
  .intro-actions { display: flex; flex: 0 0 auto; gap: .65rem; }
  button.secondary { background: transparent; }
  .export-error { margin: 0; border: 1px solid #9c5555; border-radius: .7rem; padding: .8rem; background: #2b1719; color: #ffd0cc; }
  .diagnostics-error { border-color: #9c5555; background: #2b1719; }
  .diagnostics-error p { margin-block-end: .8rem; }
  section { min-width: 0; }
  .section-heading { align-items: center; margin-block-end: .7rem; }
  .section-heading > p { max-width: 32rem; font-size: .84rem; text-align: end; }
  .state-pill { flex: 0 0 auto; border-radius: 999px; padding: .32rem .62rem; background: #3f3020; color: #efd29b; font-size: .76rem; font-weight: 750; }
  .state-pill.healthy, .state-pill--ok { background: #173c30; color: #a8e7ca; }
  .state-pill--degraded { background: #43351d; color: #efd49a; }
  .state-pill--error { background: #4a272a; color: #f3b9bd; }
  .state-pill--unchecked { background: #233039; color: #b7c7cd; }
  .summary-grid, .provider-grid, .coverage-grid { display: grid; gap: .75rem; }
  .summary-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); margin: 0; }
  .summary-grid > div { min-width: 0; padding: .85rem; }
  .summary-grid .path-cell { grid-column: 1 / -1; }
  dt { color: #91aab3; font-size: .78rem; }
  dd { margin: .2rem 0 0; overflow-wrap: anywhere; color: #edf4f7; font-variant-numeric: tabular-nums; font-weight: 700; }
  .provider-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .provider-card { min-width: 0; border-block-start-width: .2rem; padding: 1rem; }
  .provider-card--ok { border-block-start-color: #4eac83; }
  .provider-card--degraded { border-block-start-color: #d0a24e; }
  .provider-card--error { border-block-start-color: #d46e74; }
  .provider-card--unchecked { border-block-start-color: #637985; }
  .provider-card header { display: flex; align-items: start; justify-content: space-between; gap: .75rem; }
  .provider-card h3 { margin-block-end: .8rem; overflow-wrap: anywhere; }
  .provider-card dl { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .7rem; margin: 0; }
  .provider-error, .provider-note { margin: .85rem 0 0; border-radius: .5rem; padding: .65rem; overflow-wrap: anywhere; background: #ffffff08; color: #c8d5d9; font-size: .82rem; line-height: 1.45; }
  .provider-error { color: #efc0c3; }
  .coverage-grid { grid-template-columns: repeat(4, minmax(0, 1fr)); }
  .coverage-grid article { min-width: 0; padding: .9rem; }
  .coverage-grid h3 { margin-block-end: .55rem; }
  .coverage-grid p { margin-block-end: .35rem; overflow-wrap: anywhere; color: #a9bac1; font-size: .84rem; line-height: 1.45; }
  .coverage-grid .metric { color: #edf4f7; font-size: 1.2rem; font-weight: 760; }
  .coverage-grid .empty-copy { color: #d5bd8d; }
  .privacy-note { border-color: #34496b; background: #0c1526; }

  @media (max-width: 64rem) {
    .provider-grid { grid-template-columns: minmax(0, 1fr); }
    .coverage-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }

  @media (max-width: 44rem) {
    .diagnostics-intro, .section-heading { align-items: stretch; flex-direction: column; }
    .intro-actions { width: 100%; flex-direction: column; }
    .diagnostics-intro button { width: 100%; }
    .section-heading > p { text-align: start; }
    .summary-grid, .coverage-grid { grid-template-columns: minmax(0, 1fr); }
    .provider-card header { align-items: stretch; flex-direction: column; }
    .state-pill { width: fit-content; }
  }

  @media (max-width: 28rem) {
    .provider-card dl { grid-template-columns: minmax(0, 1fr); }
  }

  @media (forced-colors: active) {
    .diagnostics-intro, .diagnostics-error, .privacy-note, .provider-card, .coverage-grid article, .summary-grid > div { border: 1px solid CanvasText; }
  }
</style>
