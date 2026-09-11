<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { useLocale } from "./i18n";
  import type { InventoryView } from "./inventory";

  export let value: number;
  export let disabled = false;
  export let updating = false;
  export let onSaved: (inventory: InventoryView | null) => void | Promise<void>;

  const locale = useLocale();
  let menu: HTMLDetailsElement;
  let popover: HTMLDivElement;
  let right = 0;
  let error = "";
  $: choices = [...new Set([0, 1, 2, value])].sort((a, b) => a - b);

  function positionMenu(): void {
    if (menu?.open && popover) right = Math.min(0, menu.getBoundingClientRect().right - popover.offsetWidth - 16);
  }

  async function save(event: Event): Promise<void> {
    const control = event.currentTarget as HTMLSelectElement;
    updating = true;
    error = "";
    try {
      const inventory = await invoke<InventoryView | null>("set_inventory_keep_copies", { keepCopies: Number(control.value) });
      await onSaved(inventory);
    } catch {
      error = $locale === "ru" ? "Не удалось изменить запас копий. Повторите попытку." : "Unable to change the copy reserve. Try again.";
    } finally {
      updating = false;
      control.value = String(value);
    }
  }

  function dismiss(event: MouseEvent | KeyboardEvent): void {
    if (!menu?.open) return;
    if (event instanceof KeyboardEvent) {
      if (event.key === "Escape") { menu.open = false; menu.querySelector("summary")?.focus(); }
    } else if (event.target instanceof Node && !menu.contains(event.target)) menu.open = false;
  }
</script>

<svelte:window onclick={dismiss} onkeydown={dismiss} onresize={positionMenu} />

<details class="keep-copies" bind:this={menu} ontoggle={positionMenu}>
  <summary>{$locale === "ru" ? "Оставлять себе:" : "Keep:"} {value}</summary>
  <div class="reserve-popover" bind:this={popover} style:right={`${right}px`} aria-busy={updating}>
    <label for="keep-copies">{$locale === "ru" ? "Оставлять копий" : "Keep copies"}</label>
      <select id="keep-copies" value={String(value)} disabled={disabled || updating} onchange={save}>
        {#each choices as count}<option value={String(count)}>{count}</option>{/each}
      </select>
    <p>{$locale === "ru" ? "Общая настройка для инвентаря и рынка. Если продажа затронет оставленные себе копии, появится предупреждение — вы сможете продолжить." : "Shared by inventory and market. Selling reserved copies shows a warning and lets you continue."}</p>
    {#if updating}<p role="status">{$locale === "ru" ? "Сохраняем…" : "Saving…"}</p>{/if}
    {#if error}<p class="reserve-error" role="alert">{error}</p>{/if}
  </div>
</details>

<style>
  .keep-copies { position:relative; font-size:.78rem; }
  summary { cursor:pointer; list-style:none; border-radius:.45rem; color:var(--text-muted); padding:.5rem .3rem; white-space:nowrap; }
  summary::after { content:"⌄"; padding-left:.45rem; }
  summary:hover { color:var(--text); }
  .reserve-popover { position:absolute; right:0; top:calc(100% + .45rem); z-index:6; width:20rem; max-width:calc(100vw - 2rem); box-sizing:border-box; border:1px solid var(--border); border-radius:.7rem; background:var(--surface-1); padding:1rem; box-shadow:0 .65rem 2rem #44271420; }
  label { display:block; font-weight:650; margin-bottom:.4rem; }
  select { width:100%; padding:.45rem; border:1px solid var(--border); border-radius:.4rem; font:inherit; background:var(--surface-1); color:var(--text); }
  p { color:var(--text-muted); line-height:1.55; margin:.65rem 0 0; }
  .reserve-error { color:var(--danger); }
</style>
