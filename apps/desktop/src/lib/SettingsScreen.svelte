<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  import {
    languageFromLocale,
    localeFromLanguage,
    useLocale,
    type AppSettings,
    type UiLocale,
  } from "./i18n";
  import type { CompanionImportState } from "./inventory";

  export let onSettingsSaved: (settings: AppSettings) => void;

  const locale = useLocale();
  const copy = {
    ru: {
      kicker: "Интерфейс",
      heading: "Язык приложения",
      description: "Язык влияет на интерфейс и локализованные названия. Рыночная идентичность всегда остаётся canonical slug/ID.",
      label: "Язык",
      russian: "Русский",
      english: "English",
      dataKicker: "Рынок",
      dataHeading: "Обновление рыночных данных",
      dataDescription: "Выберите рыночную платформу, частоту daily bulk-проверки и срок локального live-кэша. Изменение не удаляет текущий LKG.",
      platform: "Рыночная платформа",
      platformHint: "Daily bulk-снимок содержит только PC-цены. На других платформах bulk-оценка остаётся недоступной, а выбор применяется к явным live-запросам WFM.",
      platforms: {
        pc: "PC",
        playstation: "PlayStation",
        xbox: "Xbox",
        switch: "Nintendo Switch",
        mobile: "Mobile",
      } satisfies Record<AppSettings["platform"], string>,
      crossplay: "Использовать crossplay-рынок WFM",
      crossplayHint: "Применяется только к явным live-запросам. Bulk snapshot остаётся отдельным проверенным источником.",
      bulkInterval: "Проверять bulk snapshot",
      liveTtl: "Хранить live quote",
      hours: (value: number) => `${value} ч`,
      seconds: (value: number) => `${value} с`,
      companionKicker: "Локальная автоматизация",
      companionHeading: "Импортировать companion-файл автоматически",
      companionDescription: "PlatScope проверяет выбранный локальный JSON каждые несколько секунд. Файл никуда не отправляется, а ошибочный снимок не заменяет предыдущий.",
      enableCompanion: "Автоматически импортировать companion-файл",
      pathLabel: "Полный путь к inventory JSON",
      pathExample: String.raw`C:\Users\name\AppData\Local\PlatScope Companion\inventory.json`,
      pathHint: "Укажите абсолютный путь к versioned Overwolf companion JSON. Файл может появиться позже.",
      pathRequired: "Укажите полный путь к companion-файлу.",
      pathAbsolute: "Укажите абсолютный путь, например C:\\…\\inventory.json.",
      save: "Сохранить настройки",
      saving: "Сохраняем…",
      check: "Проверить файл",
      checking: "Проверяем…",
      loading: "Загружаем настройки…",
      ready: "Настройки загружены.",
      saved: "Настройки сохранены.",
      loadError: "Не удалось загрузить настройки. Повторите попытку.",
      saveError: "Не удалось сохранить настройки. Проверьте путь и локальное хранилище, затем повторите попытку.",
      checkError: "Не удалось проверить companion-файл. Повторите попытку.",
      retry: "Повторить",
      statusHeading: "Состояние автоматического импорта",
      status: {
        disabled: "Автоматический импорт выключен.",
        needs_path: "Укажите путь и сохраните настройки.",
        missing: "Файл пока не найден. PlatScope продолжит проверять выбранный путь.",
        stabilizing: "Файл найден. Ждём завершения записи перед импортом.",
        up_to_date: "Последняя стабильная версия уже импортирована.",
        imported: "Новый стабильный снимок импортирован.",
        error: "Файл не импортирован. Предыдущий снимок сохранён.",
      } satisfies Record<CompanionImportState, string>,
      lastImport: "Последний импорт",
      technicalDetail: "Техническая причина",
      scopeHeading: "Что не меняется",
      scopeBody: "Резерв копий настраивается отдельно в «Инвентаре». Read-only сканирование запускается только явной кнопкой на экране инвентаря.",
    },
    en: {
      kicker: "Interface",
      heading: "Application language",
      description: "Language affects the interface and localized names. Market identity always remains the canonical slug or ID.",
      label: "Language",
      russian: "Русский",
      english: "English",
      dataKicker: "Market",
      dataHeading: "Market data refresh",
      dataDescription: "Choose the market platform, daily bulk check interval, and local live-cache lifetime. Changing these values does not remove the current LKG.",
      platform: "Market platform",
      platformHint: "The daily bulk snapshot contains PC prices only. On other platforms the bulk estimate remains unavailable, and the selection applies to explicit WFM live requests.",
      platforms: {
        pc: "PC",
        playstation: "PlayStation",
        xbox: "Xbox",
        switch: "Nintendo Switch",
        mobile: "Mobile",
      } satisfies Record<AppSettings["platform"], string>,
      crossplay: "Use the WFM crossplay market",
      crossplayHint: "This applies only to explicit live requests. The bulk snapshot remains a separate validated source.",
      bulkInterval: "Check the bulk snapshot",
      liveTtl: "Keep a live quote",
      hours: (value: number) => `${value} hr`,
      seconds: (value: number) => `${value} sec`,
      companionKicker: "Local automation",
      companionHeading: "Import a companion file automatically",
      companionDescription: "PlatScope checks the selected local JSON every few seconds. The file is never uploaded, and an invalid snapshot never replaces the previous one.",
      enableCompanion: "Automatically import the companion file",
      pathLabel: "Full path to inventory JSON",
      pathExample: String.raw`C:\Users\name\AppData\Local\PlatScope Companion\inventory.json`,
      pathHint: "Enter an absolute path to a versioned Overwolf companion JSON file. The file may be created later.",
      pathRequired: "Enter the full path to the companion file.",
      pathAbsolute: "Enter an absolute path, such as C:\\…\\inventory.json.",
      save: "Save settings",
      saving: "Saving…",
      check: "Check file",
      checking: "Checking…",
      loading: "Loading settings…",
      ready: "Settings loaded.",
      saved: "Settings saved.",
      loadError: "Unable to load settings. Try again.",
      saveError: "Unable to save settings. Check the path and local storage, then try again.",
      checkError: "Unable to check the companion file. Try again.",
      retry: "Try again",
      statusHeading: "Automatic import status",
      status: {
        disabled: "Automatic import is off.",
        needs_path: "Enter a path and save the settings.",
        missing: "The file has not been created yet. PlatScope will keep checking the selected path.",
        stabilizing: "File found. Waiting for the write to finish before importing.",
        up_to_date: "The latest stable version has already been imported.",
        imported: "A new stable snapshot was imported.",
        error: "The file was not imported. The previous snapshot was preserved.",
      } satisfies Record<CompanionImportState, string>,
      lastImport: "Last import",
      technicalDetail: "Technical reason",
      scopeHeading: "What stays unchanged",
      scopeBody: "Configure the copy reserve separately in Inventory. Read-only scanning starts only from the explicit button on the Inventory screen.",
    },
  } as const;

  let settings: AppSettings | null = null;
  let selectedLocale: UiLocale = "ru";
  let selectedPlatform: AppSettings["platform"] = "pc";
  let crossplay = true;
  let bulkRefreshHours = 4;
  let liveQuoteTtlSeconds = 90;
  let loading = true;
  let saving = false;
  let statusMessage = "";
  let errorMessage = "";

  $: c = copy[$locale];
  $: changed = settings !== null && (
    selectedLocale !== localeFromLanguage(settings.language) ||
    selectedPlatform !== settings.platform ||
    crossplay !== settings.crossplay ||
    bulkRefreshHours !== settings.bulk_refresh_hours ||
    liveQuoteTtlSeconds !== settings.live_quote_ttl_seconds
  );

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
    const nextSettings: AppSettings = {
      ...settings,
      language: languageFromLocale(selectedLocale),
      platform: selectedPlatform,
      crossplay,
      bulk_refresh_hours: bulkRefreshHours,
      live_quote_ttl_seconds: liveQuoteTtlSeconds,
    };
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

  onMount(() => {
    void loadSettings();
  });
</script>

<div class="settings-status" role="status" aria-live="polite">
  {loading ? c.loading : statusMessage}
</div>

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
      <span>{c.crossplay}</span>
    </label>
    <p class="field-hint">{c.crossplayHint}</p>
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
</section>

<div class="settings-actions">
  <button type="button" onclick={saveSettings} disabled={loading || saving || !settings || !changed}>
    {saving ? c.saving : c.save}
  </button>
</div>

<section class="settings-note" aria-labelledby="settings-scope-heading">
  <h2 id="settings-scope-heading">{c.scopeHeading}</h2>
  <p>{c.scopeBody}</p>
</section>

<style>
  .settings-status { min-height: 1.5rem; color: #9ba9bd; }
  .settings-card, .settings-note, .settings-error { border: 1px solid #283752; border-radius: .8rem; padding: 1rem; background: #111b2f; box-shadow: 0 .75rem 2rem rgb(0 0 0 / 14%); }
  .settings-card { display: grid; grid-template-columns: minmax(0, 1fr) minmax(16rem, .7fr); align-items: end; gap: 1.5rem; }
  .settings-card h2, .settings-note h2 { margin-block-end: .4rem; font-size: 1.2rem; }
  .settings-card p, .settings-note p, .settings-error p { max-width: 68ch; margin: 0; color: #9ba9bd; line-height: 1.5; }
  .settings-control { display: grid; gap: .45rem; min-width: 0; }
  .market-settings-card, .settings-note, .settings-error, .settings-actions { margin-block-start: 1rem; }
  .settings-control-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .8rem; min-width: 0; }
  .platform-control { grid-column: 1 / -1; }
  .settings-control-grid > .check-field, .settings-control-grid > .field-hint { grid-column: 1 / -1; }
  .check-field { display: flex; align-items: center; gap: .65rem; min-height: 44px; cursor: pointer; }
  .check-field input { width: 1.25rem; height: 1.25rem; flex: 0 0 auto; accent-color: #72a7ff; }
  .field-hint { font-size: .82rem; }
  .settings-actions { display: flex; justify-content: end; }
  .settings-actions button, .settings-error button { min-height: 44px; }
  .settings-actions button { min-width: 12rem; }
  .settings-error { border-color: #9c5555; background: #2b1719; }
  .settings-error button { margin-block-start: .75rem; }
  @media (max-width: 46rem) {
    .settings-card { grid-template-columns: minmax(0, 1fr); }
    .settings-control-grid { grid-template-columns: minmax(0, 1fr); }
    .settings-actions button { width: 100%; }
  }
  @media (forced-colors: active) {
    .settings-card, .settings-note, .settings-error { border-color: CanvasText; }
  }
</style>
