<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { localeCode, useLocale } from "./i18n";
  import { formatPlatinum } from "./market";

  import {
    filterInventory,
    inventorySourceLabel,
    inventoryVariantLabel,
    resolutionLabel,
    vaultStatusLabel,
    type InventoryCategoryFilter,
    type InventoryDuplicateFilter,
    type InventoryPriceFilter,
    type InventoryVaultFilter,
    type InventoryView,
  } from "./inventory";
  import {
    loadInventoryViewPreferences,
    saveInventoryViewPreferences,
  } from "./viewPreferences";

  export let onInventoryChange: (() => void) | undefined = undefined;

  const locale = useLocale();

  const categoryCopy = {
    ru: { mod: "Моды", relic: "Реликвии", weapon: "Оружие", warframe: "Варфреймы", component: "Компоненты", arcane_enhancement: "Мистификаторы", misc: "Прочее" },
    en: { mod: "Mods", relic: "Relics", weapon: "Weapons", warframe: "Warframes", component: "Components", arcane_enhancement: "Arcanes", misc: "Other" },
  } as const;
  const copy = {
    ru: {
      opening: "Открываем инвентарь…", scanning: "Получаем инвентарь из Warframe…",
      loaded: (count: number) => `Загружено предметов: ${count}.`, noLocal: "Инвентарь ещё не обновлялся.",
      loadError: "Инвентарь не загрузился. Перезапустите PlatScope и повторите попытку.",
      reserveUpdated: (count: number) => `Резерв обновлён: сохраняем ${count} ${count === 1 ? "копию" : "копии"} каждого варианта.`,
      reserveError: "Резерв не обновлён. Повторите попытку.",
      scanHeading: "Обновить инвентарь из Warframe", scanBody: "Запустите Warframe, войдите в аккаунт и оставьте игру открытой. PlatScope получит актуальный список предметов.",
      scanButton: "Обновить инвентарь", scanningButton: "Обновляем…", scanDetails: "Как работает сканирование", scanMethod: "PlatScope только читает два служебных значения из памяти запущенной игры, чтобы запросить ваш инвентарь у Digital Extremes. Приложение не изменяет память игры и не внедряет в неё код.", scanRisk: "Это сторонний способ чтения данных. Digital Extremes не давала отдельной гарантии его безопасности, поэтому запуск остаётся вашим решением.",
      scanComplete: (resolved: number, attention: number) => `Сканирование завершено: ${resolved} строк сопоставлено, ${attention} требуют внимания.`,
      scanError: "Не удалось обновить инвентарь. Запустите Warframe, войдите в аккаунт и повторите попытку. Предыдущие данные сохранены.", technical: "Технические подробности",
      summaryLabel: "Сводка инвентаря", totalCopies: "Доступных копий", sellable: "Можно продать", matched: "Распознано", attention: "Требуют внимания", snapshot: "данные от",
      helperWarning: "Внешний файл подтверждает количество, но не возможность продажи. Такие копии не попадут в очередь, пока статус не определён.",
      filtersHeading: "Поиск и фильтры инвентаря", searchLabel: "Поиск в инвентаре", searchExample: "Например, Поток Прайм",
      category: "Категория", allCategories: "Все категории", duplicates: "Дубликаты", allQuantities: "Все количества", duplicatesOnly: "Только дубликаты",
      vaultStatus: "Доступность в игре", anyVault: "Любая", vaulted: "В хранилище", available: "Доступен", unknown: "Неизвестно",
      salesMedian48h: "Сделки за 48 ч", cardMedian48h: "Средняя цена сделок · 48 ч", anyPrice: "Все предметы", priced: "Были сделки", unpriced: "Сделок не было", keepCopies: "Сохранять копий",
      inventoryHeading: "Инвентарь", resultHint: "Цена и количество к продаже рассчитываются только для предметов с точным рангом.", rankUnknown: "Ранг не определён",
      tableCaption: "Продаваемые предметы, количество и рыночные данные", item: "Предмет", owned: "Доступно", context: "Рынок", matching: "Сопоставление",
      noResults: "Нет предметов для выбранных фильтров", loosenFilters: "Очистите поиск или ослабьте один из фильтров.",
      localOnly: "Инвентарь ещё не получен", firstImport: "Сканируйте запущенный Warframe",
      firstImportBody: "Запустите Warframe, войдите в аккаунт и нажмите «Сканировать Warframe» выше.",
    },
    en: {
      opening: "Opening local inventory…", scanning: "Reading inventory from the running Warframe process…",
      loaded: (count: number) => `Loaded ${count} rows from the local snapshot.`, noLocal: "No local inventory has been imported yet.",
      loadError: "Unable to load inventory. Restart PlatScope and try again.",
      reserveUpdated: (count: number) => `Reserve updated: keeping ${count} ${count === 1 ? "copy" : "copies"} of each variant.`,
      reserveError: "Unable to update the reserve. Try again.",
      scanHeading: "Update inventory from Warframe", scanBody: "Start Warframe, sign in, and leave the game open. PlatScope will retrieve the current item list.",
      scanButton: "Update inventory", scanningButton: "Updating…", scanDetails: "How scanning works", scanMethod: "PlatScope reads only two service values from the running game to request your inventory from Digital Extremes. It does not change game memory or inject code.", scanRisk: "This is a third-party method of reading data. Digital Extremes has not provided a separate safety guarantee, so using it remains your choice.",
      scanComplete: (resolved: number, attention: number) => `Scan complete: ${resolved} rows matched, ${attention} need review.`,
      scanError: "Unable to update inventory. Start Warframe, sign in, and try again. Previous data was preserved.", technical: "Technical details",
      summaryLabel: "Inventory summary", totalCopies: "Tradeable copies", sellable: "Sellable", matched: "Matched", attention: "Needs review", snapshot: "snapshot",
      helperWarning: "An external export confirms quantity but not tradeability. These copies stay out of Sell now while tradeability is unknown.",
      filtersHeading: "Inventory search and filters", searchLabel: "Search inventory", searchExample: "For example, Primed Flow",
      category: "Category", allCategories: "All categories", duplicates: "Duplicates", allQuantities: "Any quantity", duplicatesOnly: "Duplicates only",
      vaultStatus: "Vault status", anyVault: "Any vault status", vaulted: "Vaulted", available: "Available", unknown: "Unknown",
      salesMedian48h: "Sales median · 48 h", cardMedian48h: "Median · 48 h", anyPrice: "All items", priced: "Sold in the last 48 h", unpriced: "No sales in the last 48 h", keepCopies: "Keep copies",
      inventoryHeading: "Inventory", resultHint: "Recognized tradeable items are shown. Price and sellable quantity require an exact rank.", rankUnknown: "Rank not detected",
      tableCaption: "Tradeable items, quantities, and market context", item: "Item", owned: "Tradeable", context: "Context", matching: "Matching",
      noResults: "No items match these filters", loosenFilters: "Clear the search or loosen a filter.",
      localOnly: "Inventory not loaded", firstImport: "Scan the running Warframe client",
      firstImportBody: "Start Warframe, sign in, and select “Scan Warframe” above.",
    },
  } as const;
  $: c = copy[$locale];
  $: categoryLabels = categoryCopy[$locale] as Record<string, string>;

  let inventory: InventoryView | null = null;
  let loading = true;
  let scanning = false;
  let reserveUpdating = false;
  let query = "";
  let duplicates: InventoryDuplicateFilter = "all";
  let vault: InventoryVaultFilter = "all";
  let price: InventoryPriceFilter = "all";
  let category: InventoryCategoryFilter = "all";
  let viewPreferencesReady = false;
  let message = "";
  let errorMessage = "";
  let errorDetail = "";

  $: visibleItems = filterInventory(inventory?.items ?? [], query, {
    category,
    duplicates,
    vault,
    price,
  });
  $: categories = Object.keys(categoryLabels).filter((tag) =>
    inventory?.items.some((item) => item.tags.includes(tag)),
  );
  $: if (viewPreferencesReady) {
    saveInventoryViewPreferences({ category, duplicates, vault, price });
  }

  async function loadInventory(): Promise<void> {
    loading = true;
    errorMessage = "";
    errorDetail = "";
    try {
      inventory = await invoke<InventoryView | null>("load_inventory");
      message = inventory
        ? c.loaded(inventory.items.length)
        : c.noLocal;
    } catch (error) {
      inventory = null;
      errorMessage = c.loadError;
      errorDetail = String(error);
    } finally {
      loading = false;
    }
  }

  async function scanWarframe(): Promise<void> {
    scanning = true;
    errorMessage = "";
    errorDetail = "";
    message = "";
    try {
      inventory = await invoke<InventoryView>("scan_read_only_inventory");
      message = c.scanComplete(inventory.summary.resolvedRows, inventory.summary.attentionRows);
      onInventoryChange?.();
    } catch (error) {
      errorMessage = c.scanError;
      errorDetail = String(error);
    } finally {
      scanning = false;
    }
  }

  async function updateReserve(event: Event): Promise<void> {
    const keepCopies = Number((event.currentTarget as HTMLSelectElement).value);
    reserveUpdating = true;
    errorMessage = "";
    errorDetail = "";
    try {
      inventory = await invoke<InventoryView | null>("set_inventory_keep_copies", {
        keepCopies,
      });
      message = c.reserveUpdated(keepCopies);
    } catch (error) {
      errorMessage = c.reserveError;
      errorDetail = String(error);
    } finally {
      reserveUpdating = false;
    }
  }

  function itemInitials(name: string): string {
    return name
      .split(/\s+/u)
      .filter(Boolean)
      .slice(0, 2)
      .map((part) => part[0])
      .join("")
      .toLocaleUpperCase($locale);
  }

  onMount(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    let unlistenMarket: UnlistenFn | undefined;
    const savedView = loadInventoryViewPreferences();
    category = savedView.category;
    duplicates = savedView.duplicates;
    vault = savedView.vault;
    price = savedView.price;
    viewPreferencesReady = true;
    void loadInventory();
    void listen("inventory-updated", () => {
      void loadInventory().then(() => onInventoryChange?.());
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });
    void listen("market-data-updated", () => {
      void loadInventory();
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlistenMarket = cleanup;
    });
    return () => {
      disposed = true;
      unlisten?.();
      unlistenMarket?.();
    };
  });
</script>

<div class="inventory-status" role="status" aria-live="polite">
  {#if loading}
    {c.opening}
  {:else if scanning}
    {c.scanning}
  {:else}
    {message}
  {/if}
</div>

{#if errorMessage}
  <div class="error-block" role="alert">
    <p>{errorMessage}</p>
    {#if errorDetail}<details><summary>{c.technical}</summary><code>{errorDetail}</code></details>{/if}
  </div>
{/if}

<section class="inventory-import scan-panel" aria-labelledby="inventory-scan-heading">
  <div>
    <p class="eyebrow">Warframe</p>
    <h2 id="inventory-scan-heading">{c.scanHeading}</h2>
    <p>{c.scanBody}</p>
    <details class="scan-details">
      <summary>{c.scanDetails}</summary>
      <p>{c.scanMethod}</p>
      <p>{c.scanRisk}</p>
    </details>
  </div>
  <div class="inventory-actions">
    <button type="button" onclick={scanWarframe} disabled={loading || scanning}>
      {scanning ? c.scanningButton : c.scanButton}
    </button>
  </div>
</section>

{#if inventory}
  <section class="inventory-summary" aria-label={c.summaryLabel}>
    <dl>
      <div><dt>{c.totalCopies}</dt><dd>{inventory.summary.ownedQuantity.toLocaleString(localeCode($locale))}</dd></div>
      <div><dt>{c.sellable}</dt><dd>{inventory.summary.sellableQuantity.toLocaleString(localeCode($locale))}</dd></div>
      <div><dt>{c.matched}</dt><dd>{inventory.summary.resolvedRows.toLocaleString(localeCode($locale))}</dd></div>
    </dl>
    <p>
      {inventorySourceLabel(inventory.metadata.source, $locale)} · {c.snapshot}
      <time datetime={inventory.metadata.observedAt}>{new Date(inventory.metadata.observedAt).toLocaleString(localeCode($locale))}</time>
    </p>
    {#if inventory.metadata.source === "helper_import" || inventory.metadata.source === "overwolf_companion"}
      <p class="helper-warning" role="note">
        {c.helperWarning}
      </p>
    {/if}
  </section>

  <section class="inventory-toolbar" aria-labelledby="inventory-filters-heading">
    <h2 id="inventory-filters-heading" class="sr-only">{c.filtersHeading}</h2>
    <div class="search-field">
      <label for="inventory-search">{c.searchLabel}</label>
      <input
        id="inventory-search"
        type="search"
        bind:value={query}
        maxlength="80"
        autocomplete="off"
        placeholder={c.searchExample}
      />
    </div>
    <div class="filter-field">
      <label for="inventory-category">{c.category}</label>
      <select id="inventory-category" bind:value={category}>
        <option value="all">{c.allCategories}</option>
        {#each categories as tag}
          <option value={tag}>{categoryLabels[tag]}</option>
        {/each}
      </select>
    </div>
    <div class="filter-field">
      <label for="inventory-duplicates">{c.duplicates}</label>
      <select id="inventory-duplicates" bind:value={duplicates}>
        <option value="all">{c.allQuantities}</option>
        <option value="duplicates">{c.duplicatesOnly}</option>
      </select>
    </div>
    <div class="filter-field">
      <label for="inventory-vault">{c.vaultStatus}</label>
      <select id="inventory-vault" bind:value={vault}>
        <option value="all">{c.anyVault}</option>
        <option value="vaulted">{c.vaulted}</option>
        <option value="available">{c.available}</option>
        <option value="unknown">{c.unknown}</option>
      </select>
    </div>
    <div class="filter-field">
      <label for="inventory-price">{c.salesMedian48h}</label>
      <select id="inventory-price" bind:value={price}>
        <option value="all">{c.anyPrice}</option>
        <option value="priced">{c.priced}</option>
        <option value="unpriced">{c.unpriced}</option>
      </select>
    </div>
    <div class="filter-field">
      <label for="keep-copies">{c.keepCopies}</label>
      <select
        id="keep-copies"
        value={String(inventory.keepCopies)}
        disabled={reserveUpdating}
        onchange={updateReserve}
      >
        <option value="0">0</option>
        <option value="1">1</option>
        <option value="2">2</option>
      </select>
    </div>
  </section>

  <section class="inventory-results" aria-labelledby="inventory-results-heading">
    <div class="panel-heading">
      <div>
        <h2 id="inventory-results-heading">{c.inventoryHeading}</h2>
        <p>{c.resultHint}</p>
      </div>
      <span class="result-count">{visibleItems.length}</span>
    </div>
    {#if visibleItems.length}
      <p class="sr-only">{c.tableCaption}</p>
      <div class="inventory-card-grid">
        {#each visibleItems as item (`${item.canonicalGameId}:${item.rank}:${item.subtype}`)}
          <article class="inventory-card">
            <div class="inventory-card__heading">
              <div>
                <h3>{item.displayName}</h3>
                <p>{item.resolution === "exact_variant_unavailable" && item.rank === null && !item.subtype ? c.rankUnknown : inventoryVariantLabel(item, $locale)}</p>
              </div>
              <span class="inventory-card__owned" aria-label={`${c.owned}: ${item.ownedQuantity}`}>
                <span aria-hidden="true">✓</span> {item.ownedQuantity}
              </span>
            </div>

            <div class="inventory-card__visual" aria-hidden="true">
              {#if item.imageUrl}
                <img src={item.imageUrl} alt="" loading="lazy" decoding="async" />
              {:else}
                <span>{itemInitials(item.displayName)}</span>
              {/if}
            </div>

            <div class="inventory-card__facts">
              <div>
                <span>{c.sellable}</span>
                <strong class:muted={item.resolution === "exact_variant_unavailable"}>{item.resolution === "exact_variant_unavailable" ? "—" : item.sellableQuantity}</strong>
              </div>
              <div>
                <span>{c.cardMedian48h}</span>
                <strong class:muted={item.closedMedian48h === null}>{formatPlatinum(item.closedMedian48h, $locale)}</strong>
              </div>
            </div>

            <footer>
              <span>{item.resolution === "exact_variant_unavailable" ? resolutionLabel(item.resolution, $locale) : vaultStatusLabel(item.vaultStatus, $locale)}</span>
            </footer>
          </article>
        {/each}
      </div>
    {:else}
      <div class="no-results">
        <h3>{c.noResults}</h3>
        <p>{c.loosenFilters}</p>
      </div>
    {/if}
  </section>
{:else if !loading}
  <section class="empty-panel" aria-labelledby="inventory-empty-heading">
    <p class="empty-panel__label">{c.localOnly}</p>
    <h2 id="inventory-empty-heading">{c.firstImport}</h2>
    <p>{c.firstImportBody}</p>
  </section>
{/if}
