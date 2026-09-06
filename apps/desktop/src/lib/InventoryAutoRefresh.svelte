<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { useLocale } from "./i18n";
  import { inventoryRefreshHint, type InventoryRefreshStatus } from "./inventoryRefresh";

  let { onBusy = (_busy: boolean) => {}, compact = false } = $props<{ onBusy?: (busy: boolean) => void; compact?: boolean }>();
  const locale = useLocale();
  let status = $state<InventoryRefreshStatus | null>(null);
  let saving = $state(false);
  let failed = $state(false);
  let disposed = false;
  let revision = 0;

  function accept(value: InventoryRefreshStatus) {
    if (disposed) return;
    ++revision;
    status = value;
    onBusy(value.running === "inventory");
  }

  async function load() {
    failed = false;
    const request = revision;
    try {
      const value = await invoke<InventoryRefreshStatus>("inventory_refresh_status");
      if (request === revision) accept(value);
    } catch { if (!disposed) failed = true; }
  }

  async function change(event: Event) {
    const checkbox = event.currentTarget as HTMLInputElement;
    saving = true;
    failed = false;
    try {
      accept(await invoke<InventoryRefreshStatus>("set_inventory_auto_refresh", {
        enabled: checkbox.checked,
      }));
    } catch { if (!disposed) failed = true; }
    finally {
      if (!disposed) {
        saving = false;
        checkbox.checked = status?.enabled ?? false;
      }
    }
  }

  onMount(() => {
    let cleanup: UnlistenFn | undefined;
    void listen<InventoryRefreshStatus>("inventory-refresh-status", event => accept(event.payload))
      .then(unlisten => { if (disposed) unlisten(); else cleanup = unlisten; })
      .catch(() => { if (!disposed) failed = true; });
    void load();
    return () => { disposed = true; cleanup?.(); onBusy(false); };
  });
</script>

<div class="inventory-auto-refresh" class:compact>
  <label title={$locale === "ru" ? "Инвентарь — после событий игры. Магазин Норы — при смене ротации." : "Inventory follows game events. Nora’s shop follows its rotation."}>
    <input type="checkbox" checked={status?.enabled ?? false} disabled={!status || saving} onchange={change} />
    <span>{compact ? ($locale === "ru" ? "Автообновление" : "Auto-update") : ($locale === "ru" ? "Обновлять автоматически" : "Update automatically")}</span>
  </label>
  {#if !compact || failed || status?.enabled && !status.waitingForGame && (status.inventoryError || status.nightwaveError)}
  <small role="status" class:warning={failed || status?.enabled && !status.waitingForGame && (status.inventoryError || status.nightwaveError)}>
    {#if failed}
      {$locale === "ru" ? "Не удалось загрузить или сохранить настройку." : "Could not load or save this setting."}
      <button type="button" class="text-button" onclick={load}>{$locale === "ru" ? "Повторить" : "Retry"}</button>
    {:else if status}
      {inventoryRefreshHint(status, $locale === "ru")}
    {:else}
      {$locale === "ru" ? "Загружаем настройку…" : "Loading setting…"}
    {/if}
  </small>
  {/if}
</div>

<style>
  .inventory-auto-refresh { display: grid; gap: .3rem; min-width: 0; max-width: 21rem; }
  label { display: flex; align-items: center; gap: .4rem; font-size: .8rem; cursor: pointer; }
  input { width: 1rem; height: 1rem; margin: 0; accent-color: var(--accent); flex: 0 0 auto; }
  small { color: var(--text-muted); font-size: .72rem; line-height: 1.4; }
  .warning { color: var(--warning-text, var(--text-muted)); }
  .text-button { border: 0; padding: 0; background: none; color: inherit; text-decoration: underline; font-size: inherit; }
</style>
