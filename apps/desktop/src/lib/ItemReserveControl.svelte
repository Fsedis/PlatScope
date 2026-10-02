<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import { useLocale } from "./i18n";
  import type { InventoryView } from "./inventory";
  import type { MarketVariantKey } from "./market";

  export let key: MarketVariantKey;
  export let value: number | null | undefined;
  export let generalReserve: number;
  export let expectedInventoryChecksum: string;
  export let expectedReserveAccountKey: string | null;
  export let context = "";
  export let disabled = false;
  export let updating = false;
  export let onSaving: (() => void) | undefined = undefined;
  export let onSaved: (inventory: InventoryView | null) => void | Promise<void>;

  const locale = useLocale();
  let menu: HTMLDetailsElement;
  let draft: number | undefined;
  let draftSource = "";
  let error = "";
  let destroyed = false;
  $: source = JSON.stringify([context, expectedReserveAccountKey, expectedInventoryChecksum, key, value ?? null, generalReserve]);
  $: if (source !== draftSource) { draftSource = source; draft = value ?? generalReserve; error = ""; }
  $: effectiveReserve = value ?? generalReserve;

  async function save(keepCopies: number | null): Promise<void> {
    if (disabled || updating || !expectedReserveAccountKey) return;
    if (keepCopies !== null && (!Number.isInteger(keepCopies) || keepCopies < 0 || keepCopies > 9999)) {
      error = $locale === "ru" ? "Укажите целое количество от 0 до 9999." : "Enter a whole number from 0 to 9999.";
      return;
    }
    const requestedSource = source;
    const requestedKey = { ...key };
    const requestedChecksum = expectedInventoryChecksum;
    const requestedAccountKey = expectedReserveAccountKey;
    const requestedContext = context;
    updating = true;
    error = "";
    onSaving?.();
    try {
      const updated = await invoke<InventoryView | null>("set_inventory_item_reserve", {
        key: requestedKey, keepCopies, expectedInventoryChecksum: requestedChecksum, expectedReserveAccountKey: requestedAccountKey,
      });
      if (destroyed || requestedSource !== source) return;
      await onSaved(updated);
      if (!destroyed && requestedContext === context && requestedAccountKey === expectedReserveAccountKey
        && requestedChecksum === expectedInventoryChecksum && JSON.stringify(requestedKey) === JSON.stringify(key)) menu.open = false;
    } catch {
      if (!destroyed && requestedSource === source) error = $locale === "ru"
        ? "Не удалось сохранить резерв. Обновите инвентарь и повторите попытку."
        : "Unable to save this reserve. Refresh inventory and try again.";
    } finally { if (!destroyed) updating = false; }
  }

  onDestroy(() => { destroyed = true; updating = false; });
</script>

<details class="item-reserve" bind:this={menu}>
  <summary>{$locale === "ru" ? "Оставлять себе:" : "Keep:"} <strong>{effectiveReserve}</strong> <span>{value == null ? ($locale === "ru" ? "общий резерв" : "general reserve") : ($locale === "ru" ? "для этого предмета" : "for this item")}</span></summary>
  <div class="reserve-editor" aria-busy={updating}>
    <label><span>{$locale === "ru" ? "Копий этого варианта" : "Copies of this variant"}</span><input type="number" inputmode="numeric" min="0" max="9999" step="1" bind:value={draft} disabled={disabled || updating || !expectedReserveAccountKey} onkeydown={event => { if (event.key === "Enter") { event.preventDefault(); void save(draft ?? NaN); } }} /></label>
    <button class="secondary" type="button" disabled={disabled || updating || !expectedReserveAccountKey || draft === value} onclick={() => save(draft ?? NaN)}>{updating ? ($locale === "ru" ? "Сохраняем…" : "Saving…") : ($locale === "ru" ? "Сохранить резерв" : "Save reserve")}</button>
    {#if value != null}<button class="reset-reserve" type="button" disabled={disabled || updating || !expectedReserveAccountKey} onclick={() => save(null)}>{$locale === "ru" ? "Использовать общий резерв" : "Use general reserve"} · {generalReserve}</button>{/if}
    {#if !expectedReserveAccountKey}<p class="reserve-unbound">{$locale === "ru" ? "Обновите инвентарь из Warframe, чтобы сохранить резерв для своего аккаунта." : "Refresh inventory from Warframe to save a reserve for your account."}</p>{/if}
    {#if error}<p role="alert">{error}</p>{/if}
  </div>
</details>

<style>
  .item-reserve { font-size:.78rem; min-width:0; }
  summary { cursor:pointer; color:var(--text-muted); line-height:1.55; }
  summary strong { color:var(--text); }
  summary span { font-size:.7rem; margin-left:.3rem; }
  .reserve-editor { display:flex; flex-wrap:wrap; align-items:end; gap:.5rem; padding:.6rem 0 .15rem; }
  label { display:flex; flex-direction:column; gap:.3rem; flex:1 1 9rem; min-width:0; }
  input { min-width:0; width:100%; box-sizing:border-box; background:var(--surface-1); border:1px solid var(--border); border-radius:.4rem; color:var(--text); font:inherit; padding:.45rem .6rem; }
  button { font:inherit; white-space:normal; line-height:1.4; }
  .reset-reserve { flex-basis:100%; border:0; background:transparent; color:var(--accent); text-align:left; padding:.2rem 0; cursor:pointer; }
  .reset-reserve:disabled { cursor:default; opacity:.55; }
  p { color:var(--danger); margin:0; flex-basis:100%; line-height:1.5; }
  .reserve-unbound { color:var(--text-muted); }
</style>
