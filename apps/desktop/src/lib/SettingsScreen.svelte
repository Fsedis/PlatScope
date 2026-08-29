<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";

  import {
    languageFromLocale,
    localeFromLanguage,
    useLocale,
    type AppSettings,
    type UiLocale,
  } from "./i18n";
  import type {
    FoundationStatus,
    HistoryBootstrapOutcome,
    MarketRefreshOutcome,
  } from "./foundation";
  import type { GameMetadataRefreshOutcome, InsightsView } from "./insights";
  import type { RelicRewardScanView } from "./relicRewards";
  import AppUpdatePanel from "./AppUpdatePanel.svelte";

  export let onSettingsSaved: (settings: AppSettings) => void;
  export let onMarketRefreshed: (outcome: MarketRefreshOutcome) => void;

  const locale = useLocale();
  const copy = {
    ru: {
      kicker: "Интерфейс",
      heading: "Язык приложения",
      description: "Названия предметов останутся такими, как они указаны на рынке.",
      label: "Язык",
      russian: "Русский",
      english: "English",
      dataKicker: "Рынок",
      dataHeading: "Обновление рыночных данных",
      dataDescription: "Выберите платформу, на которой вы торгуете.",
      platform: "Платформа",
      platformHint: "Полная история цен доступна для PC. На других платформах PlatScope получает текущие ордера по запросу.",
      platforms: {
        pc: "PC",
        playstation: "PlayStation",
        xbox: "Xbox",
        switch: "Nintendo Switch",
        mobile: "Mobile",
      } satisfies Record<AppSettings["platform"], string>,
      crossplay: "Использовать общий рынок Warframe Market",
      crossplayHint: "Включите, если в игре у вас включена кроссплатформенная торговля.",
      overlayKicker: "Оверлей",
      overlayHeading: "Размер и положение",
      overlayDescription: "Настройте карточки наград поверх окна Warframe. Оверлей всегда остаётся внутри игрового окна.",
      overlayScale: "Масштаб",
      overlayOffsetX: "Смещение по горизонтали",
      overlayOffsetY: "Смещение по вертикали",
      overlayHint: "Выберите размер и положение, затем проверьте результат прямо поверх Warframe.",
      overlayReset: "Сбросить положение",
      overlayPreviewTitle: "Проверка в игре",
      overlayPreviewDescription: "Warframe должен быть открыт. Покажем до четырёх тестовых наград из прайм-сетов, части которых уже есть в вашем инвентаре.",
      overlayPreviewAction: "Показать в Warframe",
      overlayPreviewRefresh: "Обновить предпросмотр",
      overlayPreviewing: "Открываем оверлей…",
      overlayPreviewShown: (count: number) => `Показано тестовых наград: ${count}. Оверлей закроется автоматически.`,
      overlayPreviewError: (error: string) => `Не удалось показать оверлей. ${error}`,
      signedPercent: (value: number) => `${value > 0 ? "+" : ""}${value}%`,
      advanced: "Дополнительные настройки обновления",
      bulkInterval: "Проверять историю цен каждые",
      liveTtl: "Не запрашивать текущую цену повторно в течение",
      hours: (value: number) => `${value} ч`,
      seconds: (value: number) => `${value} с`,
      save: "Сохранить настройки",
      saving: "Сохраняем…",
      loading: "Загружаем настройки…",
      ready: "",
      saved: "Настройки сохранены.",
      loadError: "Не удалось загрузить настройки. Повторите попытку.",
      saveError: "Не удалось сохранить настройки. Повторите попытку.",
      retry: "Повторить",
      refreshKicker: "Данные",
      refreshHeading: "Обновление данных",
      refreshDescription: "Все ручные обновления собраны здесь. Во время обновления сохранённые данные остаются доступными.",
      marketData: "Цены и история рынка",
      marketDataBody: "Загружает свежие цены и все недостающие дни 90-дневной истории relics.run. Первое обновление может занять несколько минут.",
      updateMarket: "Обновить цены рынка",
      updatingMarket: "Загружаем 90 дней…",
      itemData: "Данные предметов",
      itemDataBody: "Обновляет каталог предметов, реликвии, дукаты и данные модов разлома.",
      updateItems: "Обновить данные предметов",
      updatingItems: "Обновляем предметы…",
      dataFrom: (date: string) => `Данные от ${date}`,
      historyCoverage: (days: number, target = 90) => `История цены: ${Math.min(days, target)} из ${target} дней`,
      dataMissing: "Ещё не загружены",
      marketUpdated: (date: string, days: number, target: number) => `Цены рынка обновлены: ${date}. ${Math.min(days, target)} из ${target} дней истории загружены.`,
      historyIncomplete: (days: number, target: number) => `Текущие цены обновлены, но история загружена не полностью: ${Math.min(days, target)} из ${target} дней. Повторите обновление.`,
      itemsUpdated: (date: string) => `Данные предметов обновлены: ${date}.`,
      marketRefreshError: "Не удалось обновить цены рынка. Предыдущие данные сохранены.",
      historyRefreshError: "Текущие цены обновлены, но не удалось догрузить историю за 90 дней.",
      itemRefreshError: "Не удалось обновить данные предметов. Предыдущие данные сохранены.",
    },
    en: {
      kicker: "Interface",
      heading: "Application language",
      description: "Item names stay as they appear on the market.",
      label: "Language",
      russian: "Русский",
      english: "English",
      dataKicker: "Market",
      dataHeading: "Market data refresh",
      dataDescription: "Choose the platform where you trade.",
      platform: "Platform",
      platformHint: "Full price history is available for PC. On other platforms PlatScope retrieves current orders on demand.",
      platforms: {
        pc: "PC",
        playstation: "PlayStation",
        xbox: "Xbox",
        switch: "Nintendo Switch",
        mobile: "Mobile",
      } satisfies Record<AppSettings["platform"], string>,
      crossplay: "Use the shared Warframe Market",
      crossplayHint: "Enable this if cross-platform trading is enabled in the game.",
      overlayKicker: "Overlay",
      overlayHeading: "Size and position",
      overlayDescription: "Adjust reward cards over the Warframe window. The overlay always stays inside the game window.",
      overlayScale: "Scale",
      overlayOffsetX: "Horizontal offset",
      overlayOffsetY: "Vertical offset",
      overlayHint: "Choose the size and position, then verify the result directly over Warframe.",
      overlayReset: "Reset position",
      overlayPreviewTitle: "Test in game",
      overlayPreviewDescription: "Warframe must be open. PlatScope will show up to four test rewards from Prime sets already represented in your inventory.",
      overlayPreviewAction: "Show in Warframe",
      overlayPreviewRefresh: "Refresh preview",
      overlayPreviewing: "Opening overlay…",
      overlayPreviewShown: (count: number) => `Showing ${count} test rewards. The overlay will close automatically.`,
      overlayPreviewError: (error: string) => `Unable to show the overlay. ${error}`,
      signedPercent: (value: number) => `${value > 0 ? "+" : ""}${value}%`,
      advanced: "Advanced refresh settings",
      bulkInterval: "Check price history every",
      liveTtl: "Do not request the current price again for",
      hours: (value: number) => `${value} hr`,
      seconds: (value: number) => `${value} sec`,
      save: "Save settings",
      saving: "Saving…",
      loading: "Loading settings…",
      ready: "",
      saved: "Settings saved.",
      loadError: "Unable to load settings. Try again.",
      saveError: "Unable to save settings. Check the path and local storage, then try again.",
      retry: "Try again",
      refreshKicker: "Data",
      refreshHeading: "Data updates",
      refreshDescription: "All manual updates are kept here. Saved data remains available while an update is running.",
      marketData: "Market prices and history",
      marketDataBody: "Downloads current prices and every missing day of the 90-day relics.run history. The first update may take several minutes.",
      updateMarket: "Update market prices",
      updatingMarket: "Loading 90 days…",
      itemData: "Item data",
      itemDataBody: "Updates the item catalog, relics, ducats, and Riven data.",
      updateItems: "Update item data",
      updatingItems: "Updating items…",
      dataFrom: (date: string) => `Data from ${date}`,
      historyCoverage: (days: number, target = 90) => `Price history: ${Math.min(days, target)} of ${target} days`,
      dataMissing: "Not loaded yet",
      marketUpdated: (date: string, days: number, target: number) => `Market prices updated: ${date}. ${Math.min(days, target)} of ${target} history days loaded.`,
      historyIncomplete: (days: number, target: number) => `Current prices were updated, but history is incomplete: ${Math.min(days, target)} of ${target} days. Run the update again.`,
      itemsUpdated: (date: string) => `Item data updated: ${date}.`,
      marketRefreshError: "Unable to update market prices. Previous data was preserved.",
      historyRefreshError: "Current prices were updated, but the 90-day history could not be downloaded.",
      itemRefreshError: "Unable to update item data. Previous data was preserved.",
    },
  } as const;

  let settings: AppSettings | null = null;
  let selectedLocale: UiLocale = "ru";
  let selectedPlatform: AppSettings["platform"] = "pc";
  let crossplay = true;
  let bulkRefreshHours = 4;
  let liveQuoteTtlSeconds = 90;
  let overlayScalePercent = 100;
  let overlayOffsetXPercent = 0;
  let overlayOffsetYPercent = 0;
  let loading = true;
  let saving = false;
  let statusMessage = "";
  let errorMessage = "";
  let marketRefreshing = false;
  let itemDataRefreshing = false;
  let marketDataDate = "";
  let historyCoverageDays = 0;
  let itemDataDate = "";
  let refreshStatusMessage = "";
  let refreshErrorMessage = "";
  let overlayPreviewing = false;
  let overlayPreviewActive = false;
  let overlayPreviewMessage = "";
  let overlayPreviewError = "";
  let overlayPreviewDebounce: ReturnType<typeof setTimeout> | undefined;
  let overlayPreviewLifetime: ReturnType<typeof setTimeout> | undefined;

  $: c = copy[$locale];
  $: changed = settings !== null && (
    selectedLocale !== localeFromLanguage(settings.language) ||
    selectedPlatform !== settings.platform ||
    crossplay !== settings.crossplay ||
    bulkRefreshHours !== settings.bulk_refresh_hours ||
    liveQuoteTtlSeconds !== settings.live_quote_ttl_seconds ||
    overlayScalePercent !== settings.reward_overlay_scale_percent ||
    overlayOffsetXPercent !== settings.reward_overlay_offset_x_percent ||
    overlayOffsetYPercent !== settings.reward_overlay_offset_y_percent
  );

  function draftSettings(): AppSettings | null {
    if (!settings) return null;
    return {
      ...settings,
      language: languageFromLocale(selectedLocale),
      platform: selectedPlatform,
      crossplay,
      bulk_refresh_hours: bulkRefreshHours,
      live_quote_ttl_seconds: liveQuoteTtlSeconds,
      reward_overlay_scale_percent: overlayScalePercent,
      reward_overlay_offset_x_percent: overlayOffsetXPercent,
      reward_overlay_offset_y_percent: overlayOffsetYPercent,
    };
  }

  async function loadSettings(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      settings = await invoke<AppSettings>("load_settings");
      selectedLocale = localeFromLanguage(settings.language);
      selectedPlatform = settings.platform;
      crossplay = settings.crossplay;
      bulkRefreshHours = settings.bulk_refresh_hours;
      liveQuoteTtlSeconds = settings.live_quote_ttl_seconds;
      overlayScalePercent = settings.reward_overlay_scale_percent;
      overlayOffsetXPercent = settings.reward_overlay_offset_x_percent;
      overlayOffsetYPercent = settings.reward_overlay_offset_y_percent;
      statusMessage = c.ready;
    } catch {
      settings = null;
      selectedLocale = $locale;
      statusMessage = "";
      errorMessage = c.loadError;
    } finally {
      loading = false;
    }
  }

  async function saveSettings(): Promise<void> {
    if (!settings || !changed) return;
    saving = true;
    errorMessage = "";
    statusMessage = "";
    const nextSettings = draftSettings();
    if (!nextSettings) return;
    try {
      await invoke("save_settings", { settings: nextSettings });
      settings = nextSettings;
      onSettingsSaved(nextSettings);
      statusMessage = copy[selectedLocale].saved;
    } catch {
      errorMessage = c.saveError;
    } finally {
      saving = false;
    }
  }

  function resetOverlayPlacement(): void {
    overlayScalePercent = 100;
    overlayOffsetXPercent = 0;
    overlayOffsetYPercent = 0;
    scheduleOverlayPreview();
  }

  function scheduleOverlayPreview(): void {
    if (!overlayPreviewActive) return;
    clearTimeout(overlayPreviewDebounce);
    overlayPreviewDebounce = setTimeout(() => void previewOverlay(), 140);
  }

  async function previewOverlay(): Promise<void> {
    const previewSettings = draftSettings();
    if (!previewSettings) return;
    overlayPreviewing = true;
    overlayPreviewMessage = "";
    overlayPreviewError = "";
    try {
      const view = await invoke<RelicRewardScanView>("preview_reward_overlay", {
        settings: previewSettings,
      });
      overlayPreviewMessage = c.overlayPreviewShown(view.rewards.length);
      overlayPreviewActive = true;
      clearTimeout(overlayPreviewLifetime);
      overlayPreviewLifetime = setTimeout(() => {
        overlayPreviewActive = false;
      }, 18_000);
    } catch (error) {
      overlayPreviewActive = false;
      overlayPreviewError = c.overlayPreviewError(String(error));
    } finally {
      overlayPreviewing = false;
    }
  }

  async function loadDataDates(): Promise<void> {
    try {
      const [foundation, insights] = await Promise.all([
        invoke<FoundationStatus>("foundation_status"),
        invoke<InsightsView | null>("insights"),
      ]);
      marketDataDate = foundation.marketSnapshot?.sourceDate ?? "";
      historyCoverageDays = foundation.historyCoverage.dayCount;
      itemDataDate = insights?.metadata.fetchedAt.slice(0, 10) ?? "";
    } catch {
      // Даты справочные: ошибка не должна блокировать настройки и ручное обновление.
    }
  }

  async function refreshMarket(): Promise<void> {
    marketRefreshing = true;
    refreshStatusMessage = "";
    refreshErrorMessage = "";
    try {
      const outcome = await invoke<MarketRefreshOutcome>("refresh_market_data");
      marketDataDate = outcome.snapshot.sourceDate;
      onMarketRefreshed(outcome);
      try {
        const history = await invoke<HistoryBootstrapOutcome>("bootstrap_history");
        historyCoverageDays = history.coverage.dayCount;
        if (history.failures.length > 0 || history.coverage.dayCount < history.targetDays) {
          refreshErrorMessage = c.historyIncomplete(history.coverage.dayCount, history.targetDays);
          return;
        }
        refreshStatusMessage = c.marketUpdated(
          marketDataDate,
          history.coverage.dayCount,
          history.targetDays,
        );
      } catch {
        refreshErrorMessage = c.historyRefreshError;
        return;
      }
    } catch {
      refreshErrorMessage = c.marketRefreshError;
    } finally {
      marketRefreshing = false;
    }
  }

  async function refreshItemData(): Promise<void> {
    itemDataRefreshing = true;
    refreshStatusMessage = "";
    refreshErrorMessage = "";
    try {
      const outcome = await invoke<GameMetadataRefreshOutcome>("refresh_game_metadata");
      itemDataDate = outcome.metadata.fetchedAt.slice(0, 10);
      refreshStatusMessage = c.itemsUpdated(itemDataDate);
    } catch {
      refreshErrorMessage = c.itemRefreshError;
    } finally {
      itemDataRefreshing = false;
    }
  }

  onMount(() => {
    void loadSettings();
    void loadDataDates();
  });

  onDestroy(() => {
    clearTimeout(overlayPreviewDebounce);
    clearTimeout(overlayPreviewLifetime);
  });
</script>

{#if loading || statusMessage}
  <div class="settings-status" role="status" aria-live="polite">
    {loading ? c.loading : statusMessage}
  </div>
{/if}

{#if errorMessage}
  <section class="settings-error" role="alert">
    <p>{errorMessage}</p>
    {#if !settings}<button type="button" onclick={loadSettings}>{c.retry}</button>{/if}
  </section>
{/if}

<section class="settings-card" aria-labelledby="language-settings-heading">
  <div>
    <p class="eyebrow">{c.kicker}</p>
    <h2 id="language-settings-heading">{c.heading}</h2>
    <p>{c.description}</p>
  </div>
  <div class="settings-control">
    <label for="interface-language">{c.label}</label>
    <select id="interface-language" bind:value={selectedLocale} disabled={loading || saving || !settings}>
      <option value="ru">{c.russian}</option>
      <option value="en">{c.english}</option>
    </select>
  </div>
</section>

<section class="settings-card market-settings-card" aria-labelledby="market-settings-heading">
  <div>
    <p class="eyebrow">{c.dataKicker}</p>
    <h2 id="market-settings-heading">{c.dataHeading}</h2>
    <p>{c.dataDescription}</p>
  </div>
  <div class="settings-control-grid">
    <div class="settings-control platform-control">
      <label for="market-platform">{c.platform}</label>
      <select
        id="market-platform"
        bind:value={selectedPlatform}
        disabled={loading || saving || !settings}
        aria-describedby="market-platform-hint"
      >
        {#each Object.entries(c.platforms) as [value, label]}
          <option value={value}>{label}</option>
        {/each}
      </select>
      <p id="market-platform-hint" class="field-hint">{c.platformHint}</p>
    </div>
    <label class="check-field">
      <input type="checkbox" bind:checked={crossplay} disabled={loading || saving || !settings} />
      <span><strong>{c.crossplay}</strong><small>{c.crossplayHint}</small></span>
    </label>
    <details class="advanced-settings">
      <summary>{c.advanced}</summary>
      <div class="advanced-settings__grid">
        <div class="settings-control">
          <label for="bulk-refresh-hours">{c.bulkInterval}</label>
          <select id="bulk-refresh-hours" bind:value={bulkRefreshHours} disabled={loading || saving || !settings}>
            {#each [1, 2, 4, 8, 12, 24] as hours}
              <option value={hours}>{c.hours(hours)}</option>
            {/each}
          </select>
        </div>
        <div class="settings-control">
          <label for="live-quote-ttl">{c.liveTtl}</label>
          <select id="live-quote-ttl" bind:value={liveQuoteTtlSeconds} disabled={loading || saving || !settings}>
            {#each [30, 60, 90, 120, 300, 600] as seconds}
              <option value={seconds}>{c.seconds(seconds)}</option>
            {/each}
          </select>
        </div>
      </div>
    </details>
  </div>
</section>

<section class="settings-card overlay-settings-card" aria-labelledby="overlay-settings-heading">
  <div>
    <p class="eyebrow">{c.overlayKicker}</p>
    <h2 id="overlay-settings-heading">{c.overlayHeading}</h2>
    <p>{c.overlayDescription}</p>
  </div>
  <div class="overlay-settings-controls">
    <div class="overlay-slider-grid">
      <div class="overlay-slider">
        <div><label for="overlay-scale">{c.overlayScale}</label><output for="overlay-scale">{overlayScalePercent}%</output></div>
        <input
          id="overlay-scale"
          type="range"
          min="70"
          max="140"
          step="1"
          bind:value={overlayScalePercent}
          oninput={scheduleOverlayPreview}
          disabled={loading || saving || !settings}
          aria-describedby="overlay-settings-hint"
        />
      </div>
      <div class="overlay-slider">
        <div><label for="overlay-offset-x">{c.overlayOffsetX}</label><output for="overlay-offset-x">{c.signedPercent(overlayOffsetXPercent)}</output></div>
        <input
          id="overlay-offset-x"
          type="range"
          min="-40"
          max="40"
          step="1"
          bind:value={overlayOffsetXPercent}
          oninput={scheduleOverlayPreview}
          disabled={loading || saving || !settings}
          aria-describedby="overlay-settings-hint"
        />
      </div>
      <div class="overlay-slider">
        <div><label for="overlay-offset-y">{c.overlayOffsetY}</label><output for="overlay-offset-y">{c.signedPercent(overlayOffsetYPercent)}</output></div>
        <input
          id="overlay-offset-y"
          type="range"
          min="-40"
          max="40"
          step="1"
          bind:value={overlayOffsetYPercent}
          oninput={scheduleOverlayPreview}
          disabled={loading || saving || !settings}
          aria-describedby="overlay-settings-hint"
        />
      </div>
    </div>

    <div class="overlay-settings-footer">
      <p id="overlay-settings-hint" class="field-hint">{c.overlayHint}</p>
      <div class="overlay-settings-actions">
        <button
          type="button"
          class="secondary"
          onclick={resetOverlayPlacement}
          disabled={loading || saving || !settings || (overlayScalePercent === 100 && overlayOffsetXPercent === 0 && overlayOffsetYPercent === 0)}
        >
          {c.overlayReset}
        </button>
      </div>
    </div>
    <div class="overlay-preview-action">
      <div>
        <strong>{c.overlayPreviewTitle}</strong>
        <p>{c.overlayPreviewDescription}</p>
      </div>
      <button type="button" onclick={previewOverlay} disabled={loading || saving || overlayPreviewing || !settings}>
        {overlayPreviewing
          ? c.overlayPreviewing
          : overlayPreviewActive
            ? c.overlayPreviewRefresh
            : c.overlayPreviewAction}
      </button>
    </div>
    <div class="overlay-preview-message" role="status" aria-live="polite">{overlayPreviewMessage}</div>
    {#if overlayPreviewError}
      <div class="overlay-preview-error" role="alert">{overlayPreviewError}</div>
    {/if}
  </div>
</section>

<section class="settings-card refresh-settings-card" aria-labelledby="data-refresh-heading">
  <div>
    <p class="eyebrow">{c.refreshKicker}</p>
    <h2 id="data-refresh-heading">{c.refreshHeading}</h2>
    <p>{c.refreshDescription}</p>
  </div>
  <div class="refresh-controls">
    <article class="refresh-option">
      <div>
        <h3>{c.marketData}</h3>
        <p>{c.marketDataBody}</p>
        <small>{marketDataDate ? c.dataFrom(marketDataDate) : c.dataMissing}</small>
        <small>{c.historyCoverage(historyCoverageDays)}</small>
      </div>
      <button
        type="button"
        onclick={refreshMarket}
        disabled={marketRefreshing || itemDataRefreshing}
      >
        {marketRefreshing ? c.updatingMarket : c.updateMarket}
      </button>
    </article>
    <article class="refresh-option">
      <div>
        <h3>{c.itemData}</h3>
        <p>{c.itemDataBody}</p>
        <small>{itemDataDate ? c.dataFrom(itemDataDate) : c.dataMissing}</small>
      </div>
      <button
        type="button"
        onclick={refreshItemData}
        disabled={marketRefreshing || itemDataRefreshing}
      >
        {itemDataRefreshing ? c.updatingItems : c.updateItems}
      </button>
    </article>
    <div class="refresh-message" role="status" aria-live="polite">{refreshStatusMessage}</div>
    {#if refreshErrorMessage}
      <div class="refresh-error" role="alert">{refreshErrorMessage}</div>
    {/if}
  </div>
</section>

<AppUpdatePanel mode="settings" />

<div class="settings-actions">
  <button type="button" onclick={saveSettings} disabled={loading || saving || !settings || !changed}>
    {saving ? c.saving : c.save}
  </button>
</div>

<style>
  .settings-status { min-height: 1.5rem; color: var(--text-muted); }
  .settings-card, .settings-error { border: 1px solid var(--border); border-radius: .75rem; padding: .75rem; background: var(--surface-1); box-shadow: var(--shadow-sm); }
  .settings-card { display: grid; grid-template-columns: minmax(16rem, .7fr) minmax(22rem, 1.3fr); align-items: start; gap: 1.25rem; }
  .settings-card h2 { margin-block-end: .4rem; font-size: 1.2rem; }
  .settings-card p, .settings-error p { max-width: 68ch; margin: 0; color: var(--text-muted); line-height: 1.5; }
  .settings-control { display: grid; gap: .45rem; min-width: 0; }
  .market-settings-card, .overlay-settings-card, .refresh-settings-card, .settings-error, .settings-actions { margin-block-start: .7rem; }
  .settings-control-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .55rem; min-width: 0; }
  .platform-control { grid-column: 1 / -1; }
  .settings-control-grid > .check-field { grid-column: 1 / -1; }
  .check-field { display: flex; align-items: center; gap: .5rem; min-height: 2.125rem; cursor: pointer; }
  .check-field input { width: 1.25rem; height: 1.25rem; flex: 0 0 auto; accent-color: var(--accent); }
  .check-field > span { display: grid; gap: .2rem; }
  .check-field small { color: var(--text-muted); font-weight: 500; }
  .field-hint { font-size: .82rem; }
  .advanced-settings { grid-column: 1 / -1; border-radius: .55rem; padding: .6rem; background: var(--surface-2); }
  .advanced-settings summary { color: var(--text); font-weight: 700; cursor: pointer; }
  .advanced-settings__grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .55rem; margin-block-start: .6rem; }
  .overlay-settings-controls { display: grid; gap: .7rem; min-width: 0; }
  .overlay-slider-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: .55rem; }
  .overlay-slider { display: grid; gap: .35rem; min-width: 0; padding: .55rem; border-radius: .55rem; background: var(--surface-2); }
  .overlay-slider > div { display: flex; align-items: baseline; justify-content: space-between; gap: .5rem; }
  .overlay-slider label { color: var(--text); font-size: .82rem; font-weight: 700; }
  .overlay-slider output { color: var(--accent-strong); font-size: .82rem; font-weight: 800; font-variant-numeric: tabular-nums; }
  .overlay-slider input { width: 100%; min-height: 1.5rem; margin: 0; accent-color: var(--accent); cursor: pointer; }
  .overlay-slider input:disabled { cursor: default; }
  .overlay-settings-footer { display: flex; align-items: center; justify-content: space-between; gap: .75rem; }
  .overlay-settings-footer .field-hint { max-width: none; }
  .overlay-settings-footer button { flex: 0 0 auto; min-height: 2.125rem; }
  .overlay-settings-actions { display: flex; flex: 0 0 auto; gap: .5rem; }
  .overlay-preview-action { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: 1rem; padding: .65rem; border-radius: .65rem; background: var(--surface-2); box-shadow: var(--shadow-sm); }
  .overlay-preview-action > div { display: grid; gap: .25rem; }
  .overlay-preview-action strong { color: var(--text); font-size: .92rem; }
  .overlay-preview-action p { font-size: .82rem; }
  .overlay-preview-action button { min-height: 2.125rem; white-space: nowrap; }
  .overlay-preview-message { min-height: 1.2rem; color: var(--success); font-size: .82rem; font-weight: 700; }
  .overlay-preview-error { padding: .55rem; border-radius: .55rem; background: var(--danger-soft); color: var(--danger); font-size: .82rem; font-weight: 700; }
  .refresh-controls { display: grid; gap: .55rem; min-width: 0; }
  .refresh-option { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: .7rem; padding: .65rem; border: 1px solid var(--border); border-radius: .55rem; background: var(--surface-2); }
  .refresh-option h3 { margin: 0 0 .25rem; font-size: 1rem; }
  .refresh-option p { font-size: .9rem; }
  .refresh-option small { display: block; margin-block-start: .45rem; color: var(--text-muted); font-weight: 700; }
  .refresh-option button { min-width: 11rem; min-height: 2.125rem; }
  .refresh-message { min-height: 1.35rem; color: var(--success); font-weight: 700; }
  .refresh-error { padding: .6rem; border: 1px solid var(--danger); border-radius: .55rem; background: var(--danger-soft); color: var(--danger); font-weight: 700; }
  .settings-actions { display: flex; justify-content: end; }
  .settings-actions button, .settings-error button { min-height: 2.125rem; }
  .settings-actions button { min-width: 10rem; }
  .settings-error { border-color: var(--danger); background: var(--danger-soft); }
  .settings-error button { margin-block-start: .75rem; }
  @media (max-width: 46rem) {
    .settings-card { grid-template-columns: minmax(0, 1fr); }
    .settings-control-grid, .advanced-settings__grid { grid-template-columns: minmax(0, 1fr); }
    .overlay-slider-grid { grid-template-columns: minmax(0, 1fr); }
    .overlay-settings-footer { align-items: stretch; flex-direction: column; }
    .overlay-preview-action { grid-template-columns: minmax(0, 1fr); }
    .overlay-preview-action button { width: 100%; }
    .refresh-option { grid-template-columns: minmax(0, 1fr); }
    .refresh-option button { width: 100%; }
    .refresh-option button, .settings-actions button, .settings-error button, .check-field { min-height: 2.5rem; }
    .settings-actions button { width: 100%; }
  }
  @media (forced-colors: active) {
    .settings-card, .settings-error, .refresh-option, .refresh-error { border-color: CanvasText; }
  }
</style>
