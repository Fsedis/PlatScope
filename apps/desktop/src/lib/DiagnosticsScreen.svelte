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
      loadError: (_reason: string) => "Не удалось проверить состояние данных. Перезапустите PlatScope или повторите попытку.",
      exportError: (_reason: string) => "Не удалось сохранить отчёт. Проверьте доступ к папке данных и повторите попытку.",
      localState: "Проверка данных", updating: "Проверяем…", update: "Проверить снова", saving: "Сохраняем отчёт…", export: "Сохранить отчёт",
      reading: "Проверяем сохранённые данные…", updated: (date: string) => `Проверено ${date}.`, saved: (path: string, bytes: string) => `Отчёт сохранён: ${path} (${bytes} байт).`, unavailable: "Не удалось проверить данные", retry: "Повторить",
      foundation: "Приложение", storageVersion: "Версия и сохранённые файлы", offlineReady: "Готово к работе", attention: "Нужно проверить", application: "Версия PlatScope", storage: "Состояние файлов", schema: "Версия данных", database: "Папка базы данных",
      providerHealth: "Обновление", sources: "Источники данных", savedAttempt: "Показан результат последней попытки обновления.", lastAttempt: "Последняя проверка", lastSuccess: "Последнее успешное обновление", latency: "Время ответа", failures: "Неудач подряд", unchecked: "Этот источник ещё не проверялся.", lastError: "Последняя ошибка",
      localCoverage: "Сохранённые данные", coverage: "Что доступно", market: "Цены рынка", records: "ценовых записей", items: "предметов", sourceDate: "Данные от:", promoted: "Загружены:", noMarket: "Цены рынка ещё не загружены.", catalog: "Предметы", catalogBody: "названий и вариантов", history: "История цен", days: "дней", inventory: "Инвентарь", inventoryBody: "распознанных строк",
      privacy: "Конфиденциальные данные", privacyBody: "Пароли и ключи входа не попадают в отчёт. Его можно приложить к сообщению об ошибке.",
    },
    en: {
      loadError: (_reason: string) => "Unable to check data status. Restart PlatScope or try again.",
      exportError: (_reason: string) => "Unable to save the report. Check access to the data folder and try again.",
      localState: "Data check", updating: "Checking…", update: "Check again", saving: "Saving report…", export: "Save report",
      reading: "Reading local state…", updated: (date: string) => `Details refreshed ${date}.`, saved: (path: string, bytes: string) => `Report saved: ${path} (${bytes} bytes).`, unavailable: "Diagnostics unavailable", retry: "Try again",
      foundation: "Offline foundation", storageVersion: "Storage and version", offlineReady: "Offline ready", attention: "Needs attention", application: "Application", storage: "Storage", schema: "Schema version", database: "Database",
      providerHealth: "Refresh", sources: "Data sources", savedAttempt: "The result of the last refresh attempt is shown.", lastAttempt: "Last check", lastSuccess: "Last successful refresh", latency: "Response time", failures: "Failures in a row", unchecked: "This source has not been checked yet.", lastError: "Last error",
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
  <div class="diagnostics-intro" aria-label={c.localState}>
    <div class="intro-actions">
      <button type="button" onclick={loadDiagnostics} disabled={loading || exporting}>
        {loading ? c.updating : c.update}
      </button>
      <button type="button" class="secondary" onclick={exportDiagnostics} disabled={loading || exporting || !status}>
        {exporting ? c.saving : c.export}
      </button>
    </div>
  </div>

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
        <div><dt>{c.application}</dt><dd>{status.foundation.appName} {status.foundation.appVersion}</dd></div><div><dt>{c.storage}</dt><dd>{describeFoundationStatus(status.foundation, $locale)}</dd></div><div><dt>{c.schema}</dt><dd>{status.foundation.schemaVersion}</dd></div>
      </dl>
      <details class="database-path"><summary>{c.database}</summary><code>{status.foundation.databasePath}</code></details>
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
              <details class="provider-error">
                <summary>{c.lastError}</summary>
                <code><strong>{provider.lastErrorCode}</strong>{provider.lastErrorMessage ? ` · ${provider.lastErrorMessage}` : ""}</code>
              </details>
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
  .diagnostics-intro { justify-content: end; }
  .diagnostics-error, .privacy-note, .provider-card, .coverage-grid article, .summary-grid > div { border: 1px solid var(--border); border-radius: .75rem; background: var(--surface-1); box-shadow: var(--shadow-sm); }
  .diagnostics-error, .privacy-note { padding: .75rem; }
  .section-heading h2, .diagnostics-error h2, .privacy-note h2 { margin-block-end: .35rem; font-size: 1.2rem; }
  .section-heading p, .diagnostics-error p, .privacy-note p { max-width: 68ch; margin-block-end: 0; color: var(--text-muted); line-height: 1.5; }
  .section-kicker { margin-block-end: .3rem !important; color: var(--accent-strong) !important; font-size: .76rem; font-weight: 750; letter-spacing: .08em; text-transform: uppercase; }
  .diagnostics-status { min-height: 1.5rem; color: var(--text-muted); }
  .diagnostics-status span { overflow-wrap: anywhere; color: var(--text); }
  .intro-actions { display: flex; flex: 0 0 auto; gap: .65rem; }
  button.secondary { background: transparent; }
  .export-error { margin: 0; border: 1px solid var(--danger); border-radius: .55rem; padding: .6rem; background: var(--danger-soft); color: var(--danger); }
  .diagnostics-error { border-color: var(--danger); background: var(--danger-soft); }
  .diagnostics-error p { margin-block-end: .8rem; }
  section { min-width: 0; }
  .section-heading { align-items: center; margin-block-end: .7rem; }
  .section-heading > p { max-width: 32rem; font-size: .84rem; text-align: end; }
  .state-pill { flex: 0 0 auto; border-radius: 999px; padding: .18rem .42rem; background: oklch(0.92 0.055 78); color: oklch(0.43 0.075 68); font-size: .6875rem; font-weight: 750; }
  .state-pill.healthy, .state-pill--ok { background: var(--success-soft); color: oklch(0.37 0.08 145); }
  .state-pill--degraded { background: oklch(0.92 0.055 78); color: oklch(0.43 0.075 68); }
  .state-pill--error { background: var(--danger-soft); color: var(--danger); }
  .state-pill--unchecked { background: var(--surface-3); color: var(--text-muted); }
  .summary-grid, .provider-grid, .coverage-grid { display: grid; gap: .75rem; }
  .summary-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); margin: 0; }
  .summary-grid > div { min-width: 0; padding: .6rem; }
  .database-path { margin-block-start: .75rem; border-radius: .75rem; padding: .75rem; background: var(--surface-2); color: var(--text-muted); }
  .database-path summary { color: var(--text); cursor: pointer; font-weight: 700; }
  .database-path code { display: block; margin-block-start: .5rem; overflow-wrap: anywhere; white-space: normal; }
  dt { color: var(--text-muted); font-size: .78rem; }
  dd { margin: .2rem 0 0; overflow-wrap: anywhere; color: var(--text); font-variant-numeric: tabular-nums; font-weight: 700; }
  .provider-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .provider-card { min-width: 0; border-block-start-width: .2rem; padding: .75rem; }
  .provider-card--ok { border-block-start-color: #4eac83; }
  .provider-card--degraded { border-block-start-color: #d0a24e; }
  .provider-card--error { border-block-start-color: #d46e74; }
  .provider-card--unchecked { border-block-start-color: #637985; }
  .provider-card header { display: flex; align-items: start; justify-content: space-between; gap: .75rem; }
  .provider-card h3 { margin-block-end: .55rem; overflow-wrap: anywhere; }
  .provider-card dl { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .5rem; margin: 0; }
  .provider-error, .provider-note { margin: .85rem 0 0; border-radius: .6rem; padding: .65rem; overflow-wrap: anywhere; background: var(--surface-2); color: var(--text-muted); font-size: .82rem; line-height: 1.45; }
  .provider-error { color: var(--danger); }
  .provider-error summary { cursor: pointer; font-weight: 700; }
  .provider-error code { display: block; margin-block-start: .5rem; white-space: normal; }
  .coverage-grid { grid-template-columns: repeat(4, minmax(0, 1fr)); }
  .coverage-grid article { min-width: 0; padding: .65rem; }
  .coverage-grid h3 { margin-block-end: .55rem; }
  .coverage-grid p { margin-block-end: .35rem; overflow-wrap: anywhere; color: var(--text-muted); font-size: .84rem; line-height: 1.45; }
  .coverage-grid .metric { color: var(--text); font-size: 1.2rem; font-weight: 760; }
  .coverage-grid .empty-copy { color: var(--gold); }
  .privacy-note { background: var(--surface-2); }

  @media (max-width: 64rem) {
    .provider-grid { grid-template-columns: minmax(0, 1fr); }
    .coverage-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }

  @media (max-width: 44rem) {
    .section-heading { align-items: stretch; flex-direction: column; }
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
    .diagnostics-error, .privacy-note, .provider-card, .coverage-grid article, .summary-grid > div { border: 1px solid CanvasText; }
  }
</style>
