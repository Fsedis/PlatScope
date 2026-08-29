<script lang="ts">
  import {
    appUpdateState,
    checkForAppUpdate,
    dismissUpdateBanner,
    installAppUpdate,
  } from "./appUpdate";

  export let mode: "banner" | "settings" = "settings";

  $: busy = ["checking", "downloading", "installing"].includes($appUpdateState.status);
  $: updateKnown = Boolean($appUpdateState.availableVersion);
  $: showBanner = mode === "banner" && updateKnown && !$appUpdateState.bannerDismissed;
  $: statusText = (() => {
    switch ($appUpdateState.status) {
      case "checking": return "Проверяем обновления…";
      case "current": return "Установлена последняя версия.";
      case "available": return `Доступна версия ${$appUpdateState.availableVersion}.`;
      case "downloading": return $appUpdateState.progressPercent === null
        ? "Загружаем обновление…"
        : `Загружено ${$appUpdateState.progressPercent}%.`;
      case "installing": return "Устанавливаем обновление…";
      case "installed": return "Обновление установлено. Перезапустите PlatScope.";
      case "error": return $appUpdateState.errorMessage;
      default: return "PlatScope автоматически проверяет новые версии.";
    }
  })();
</script>

{#if showBanner}
  <aside class="update-banner" aria-labelledby="update-banner-title">
    <div class="update-banner__copy">
      <strong id="update-banner-title">
        {$appUpdateState.status === "error" ? "Обновление не установлено" : `Новая версия ${$appUpdateState.availableVersion}`}
      </strong>
      <span role="status" aria-live="polite">{statusText}</span>
      {#if $appUpdateState.status === "downloading"}
        <progress value={$appUpdateState.downloadedBytes} max={$appUpdateState.totalBytes ?? undefined} aria-label="Загрузка обновления"></progress>
      {/if}
    </div>
    <div class="update-actions">
      <button type="button" onclick={() => void installAppUpdate()} disabled={busy}>
        {$appUpdateState.status === "error" ? "Повторить установку" : "Скачать и установить"}
      </button>
      <button type="button" class="secondary" onclick={dismissUpdateBanner} disabled={busy}>Позже</button>
    </div>
  </aside>
{:else if mode === "settings"}
  <section class="update-settings" aria-labelledby="app-update-heading">
    <div>
      <p class="eyebrow">Приложение</p>
      <h2 id="app-update-heading">Обновления PlatScope</h2>
      <p>Новые версии проверяются автоматически. Установка начинается только после подтверждения.</p>
    </div>
    <div class="update-settings__control">
      <div class="version-row">
        <span>Установленная версия</span>
        <strong>{$appUpdateState.currentVersion || "Определяем…"}</strong>
      </div>
      {#if updateKnown}
        <div class="version-row version-row--available">
          <span>Доступная версия</span>
          <strong>{$appUpdateState.availableVersion}</strong>
        </div>
      {/if}
      <div class:error={$appUpdateState.status === "error"} class="update-status" role="status" aria-live="polite">{statusText}</div>
      {#if $appUpdateState.status === "downloading"}
        <progress value={$appUpdateState.downloadedBytes} max={$appUpdateState.totalBytes ?? undefined} aria-label="Загрузка обновления"></progress>
      {/if}
      <div class="update-actions">
        {#if updateKnown}
          <button type="button" onclick={() => void installAppUpdate()} disabled={busy}>
            {$appUpdateState.status === "error" ? "Повторить установку" : "Скачать и установить"}
          </button>
        {/if}
        <button type="button" class:secondary={updateKnown} onclick={() => void checkForAppUpdate(true)} disabled={busy}>
          {$appUpdateState.status === "checking" ? "Проверяем…" : "Проверить обновления"}
        </button>
      </div>
    </div>
  </section>
{/if}

<style>
  .update-banner, .update-settings { border: 1px solid var(--border); border-radius: .75rem; background: var(--surface-1); box-shadow: var(--shadow-sm); }
  .update-banner { display: flex; align-items: center; justify-content: space-between; gap: 1rem; margin-block-end: .7rem; padding: .65rem .75rem; border-color: var(--accent); background: var(--accent-soft); }
  .update-banner__copy { display: grid; gap: .2rem; min-width: 0; }
  .update-banner__copy strong { color: var(--text); }
  .update-banner__copy span { color: var(--text-muted); font-size: .86rem; }
  .update-settings { display: grid; grid-template-columns: minmax(16rem, .7fr) minmax(22rem, 1.3fr); align-items: start; gap: 1.25rem; margin-block-start: .7rem; padding: .75rem; }
  .update-settings h2 { margin-block-end: .4rem; font-size: 1.2rem; }
  .update-settings p { max-width: 68ch; margin: 0; color: var(--text-muted); line-height: 1.5; }
  .update-settings__control { display: grid; gap: .55rem; min-width: 0; }
  .version-row { display: flex; align-items: baseline; justify-content: space-between; gap: 1rem; padding: .55rem .65rem; border-radius: .55rem; background: var(--surface-2); }
  .version-row span { color: var(--text-muted); font-size: .86rem; }
  .version-row strong { color: var(--text); font-variant-numeric: tabular-nums; }
  .version-row--available strong { color: var(--accent-strong); }
  .update-status { min-height: 1.3rem; color: var(--text-muted); font-size: .86rem; font-weight: 700; }
  .update-status.error { color: var(--danger); }
  .update-actions { display: flex; flex-wrap: wrap; justify-content: end; gap: .45rem; }
  .update-actions button { min-height: 2.125rem; }
  progress { width: 100%; height: .55rem; accent-color: var(--accent); }
  @media (max-width: 46rem) {
    .update-banner { align-items: stretch; flex-direction: column; }
    .update-settings { grid-template-columns: minmax(0, 1fr); }
    .update-actions { justify-content: stretch; }
    .update-actions button { flex: 1 1 auto; min-height: 2.5rem; }
  }
  @media (forced-colors: active) {
    .update-banner, .update-settings { border-color: CanvasText; }
  }
</style>
