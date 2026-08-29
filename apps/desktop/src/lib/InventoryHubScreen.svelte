<script lang="ts">
  import InventoryScreen from "./InventoryScreen.svelte";
  import SellNowScreen from "./SellNowScreen.svelte";
  import { useLocale } from "./i18n";

  type InventoryMode = "all" | "sell";

  export let mode: InventoryMode = "all";
  export let onModeChange: (mode: InventoryMode) => void;
  export let onInventoryChange: (() => void) | undefined = undefined;
  export let onOpenAccount: () => void;

  const locale = useLocale();
  const copy = {
    ru: {
      label: "Показывать",
      all: "Весь инвентарь",
      sell: "К продаже",
      allHint: "Все найденные предметы, количество и данные инвентаря.",
      sellHint: "Только продаваемые предметы с ценой, спросом и приоритетом.",
    },
    en: {
      label: "Show",
      all: "All inventory",
      sell: "Items to sell",
      allHint: "All matched items, quantities, and inventory data.",
      sellHint: "Sellable items with price, demand, and priority.",
    },
  } as const;
  $: c = copy[$locale];

  function selectMode(nextMode: InventoryMode): void {
    if (mode === nextMode) return;
    onModeChange(nextMode);
  }
</script>

<section class="inventory-view-switcher" aria-labelledby="inventory-view-label">
  <p id="inventory-view-label" class="sr-only">{c.label}</p>
  <div class="inventory-view-switcher__controls" aria-label={c.label}>
    <button
      type="button"
      class:active={mode === "all"}
      aria-pressed={mode === "all"}
      onclick={() => selectMode("all")}
    >{c.all}</button>
    <button
      type="button"
      class:active={mode === "sell"}
      aria-pressed={mode === "sell"}
      onclick={() => selectMode("sell")}
    >{c.sell}</button>
  </div>
</section>

{#if mode === "all"}
  <InventoryScreen {onInventoryChange} />
{:else}
  <SellNowScreen onOpenInventory={() => onModeChange("all")} {onOpenAccount} />
{/if}

<style>
  .inventory-view-switcher {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: .65rem;
    margin-block-end: .7rem;
  }

  .inventory-view-switcher p {
    margin: 0;
  }

  .inventory-view-switcher__controls {
    display: grid;
    flex: 0 0 auto;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: .25rem;
    border: 1px solid var(--border);
    border-radius: .55rem;
    padding: .18rem;
    background: var(--surface-2);
  }

  .inventory-view-switcher__controls button {
    min-height: 2.125rem;
    border-color: transparent;
    background: transparent;
    color: var(--text-muted);
  }

  .inventory-view-switcher__controls button.active {
    border-color: var(--border);
    background: var(--surface-1);
    color: var(--accent-strong);
    box-shadow: var(--shadow-sm);
  }

  @media (max-width: 46rem) {
    .inventory-view-switcher {
      align-items: stretch;
    }

    .inventory-view-switcher__controls { width: 100%; }
    .inventory-view-switcher__controls button { min-height: 2.5rem; }
  }

  @media (max-width: 30rem) {
    .inventory-view-switcher__controls {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  @media (forced-colors: active) {
    .inventory-view-switcher,
    .inventory-view-switcher__controls button.active {
      border: 1px solid CanvasText;
    }
  }
</style>
