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
  <div>
    <p id="inventory-view-label">{c.label}</p>
    <span>{mode === "all" ? c.allHint : c.sellHint}</span>
  </div>
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
    justify-content: space-between;
    gap: 1rem;
    margin-block-end: 1rem;
    border: 1px solid #283752;
    border-radius: .8rem;
    padding: .75rem;
    background: #111b2f;
  }

  .inventory-view-switcher p,
  .inventory-view-switcher span {
    margin: 0;
  }

  .inventory-view-switcher p {
    color: #edf4f7;
    font-weight: 750;
  }

  .inventory-view-switcher span {
    display: block;
    margin-block-start: .15rem;
    color: #9ba9bd;
    font-size: .78rem;
  }

  .inventory-view-switcher__controls {
    display: grid;
    flex: 0 0 auto;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: .4rem;
    border-radius: .65rem;
    padding: .25rem;
    background: #0b1424;
  }

  .inventory-view-switcher__controls button {
    min-height: 2.5rem;
    border-color: transparent;
    background: transparent;
    color: #9ba9bd;
  }

  .inventory-view-switcher__controls button.active {
    border-color: #365a73;
    background: #1a3147;
    color: #edf4f7;
  }

  @media (max-width: 46rem) {
    .inventory-view-switcher {
      align-items: stretch;
      flex-direction: column;
    }
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
