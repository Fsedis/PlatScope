<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { localeCode, useLocale } from "./i18n";

  import {
    accountActionErrorMessage,
    type AccountOrder,
    type AccountView,
    type CreateListingInput,
  } from "./account";
  import { formatPlatinum } from "./market";
  import {
    coverageLabel,
    filterAndSortSets,
    formatPercent,
    formatRatio,
    refinementLabel,
    relicReasonMessages,
    rivenCategoryLabel,
    setModeLabel,
    setOpportunity,
    vaultLabel,
    type InsightsView,
    type SetInsightRow,
    type SetViewMode,
  } from "./insights";

  export let onOpenSettings: () => void;
  export let onOpenAccount: () => void;

  type InsightTab = "sets" | "relics" | "ducats" | "rivens";

  const locale = useLocale();
  const copy = {
    ru: {
      loadError: (_r: string) => "Не удалось загрузить аналитику. Сохранённые цены и инвентарь не изменились.", refreshError: (_r: string) => "Не удалось обновить данные предметов. Предыдущая версия сохранена. Сначала обновите цены рынка.",
      refreshing: "Обновляем данные…", refresh: "Обновить данные", reading: "Загружаем сохранённые данные…", validating: "Загружаем и проверяем обновление…", lkg: "Обновление не прошло проверку. Показываем предыдущую версию.", updated: (d: string) => `Данные обновлены: ${d}.`, ready: (d: string) => `Данные от ${d} готовы.`, retry: "Повторить",
      noSnapshot: "Данные для расчётов ещё не загружены", loadOnce: "Загрузите данные предметов", loadBody: "Ручное обновление каталога находится в настройках.", load: "Открыть настройки обновления", relics: "Реликвии", importInventory: "Обновите инвентарь, чтобы увидеть расчёты для своих предметов.", tabs: "Раздел аналитики", sets: "Сеты", ducats: "Дукаты", rivens: "Моды разлома",
      buildable: "Можно собрать", partsSum: "Сумма деталей", part: "Деталь", needed: "Нужно", sellable: "Для продажи", why: "Почему такой вывод?", noParts: "Нет деталей для сравнения", noPartsBody: "Раздел появится, когда в инвентаре найдутся детали одного комплекта Прайм.", owned: "В инвентаре", chanceCovered: "Учтённый шанс", reward: "Награда", chance: "Шанс", ev: "Как рассчитана ценность?", noRelics: "Реликвии не найдены", noRelicsBody: "Для расчёта нужен точный уровень улучшения реликвии: Нетронутая, Исключительная, Безупречная или Сияющая.",
      ducatNote: "Платина за дукат — только показатель для сравнения. Решение об обмене остаётся за вами.", status: "Статус", insufficient: "Недостаточно данных", noDucats: "В инвентаре нет распознанных деталей Прайм с ценой в дукатах.", vaultDisclaimer: "Статус хранилища даётся только для справки и не гарантирует рост цены.",
      fairSet: "Цена комплекта", setPremium: "Премия комплекта", fair: "Цена", relicFair: "Цена реликвии", pricedEv: "Оценённая ценность", credibleFair: "Надёжная цена", primeSetCount: "Комплекты Прайм", primePartCount: "Детали Прайм", itemDefinitionCount: "Предметы с рангом мастерства", primePart: "Деталь Прайм", platinumPerDucat: "Платина / дукат", credible: "Надёжно", rivenWeaponCount: "Оружие с модами разлома", rivenSearch: "Найти оружие", rivenSearchPlaceholder: "Например, Soma", weapon: "Оружие", category: "Категория", disposition: "Коэффициент", multiplier: "Множитель", averageMultiplier: "Средний множитель", multiplierRange: "Диапазон", rivenNote: "Коэффициент показывает общую силу модов разлома для оружия. Он не учитывает характеристики конкретного мода и не является ценой.", noRivens: "Оружие не найдено.", limitedRivens: (shown: number, total: number) => `Показаны первые ${shown} из ${total}. Уточните поиск, чтобы сократить список.`,
      setsTitle: "Сеты",
      setsBody: "Здесь только решения по комплектам: что выгодно дособрать и что уже можно выставить.",
      finishMode: "Выгодно дособрать",
      readyMode: "Готовы к продаже",
      readyBadge: "Готов к продаже",
      allMode: "Все сеты",
      setSearch: "Найти сет",
      setSearchPlaceholder: "Например, Nyx Prime",
      missing: "Не хватает",
      completionCost: "Докупить ≈",
      setValue: "Цена сета",
      setProfit: "Выгода сета",
      readySets: "Готово сетов",
      partsValue: "Детали отдельно",
      openMissing: (count: number) => `Открыть недостающие детали (${count})`,
      openMissingHint: (count: number) => `В браузере откроется ${count} ${count === 1 ? "страница" : "страницы"} Warframe Market.`,
      marketOpened: (count: number) => `Открыто страниц Warframe Market: ${count}.`,
      marketOpenError: "Не удалось открыть Warframe Market. Повторите действие или откройте недостающие детали вручную.",
      sellSet: "Выставить сет",
      existingOrder: "Сет уже выставлен",
      openOrders: "Открыть мои ордера",
      connectAccount: "Подключить Warframe Market",
      verifyAccount: "Подтвердить аккаунт Warframe Market",
      noSetResults: "Подходящих сетов нет",
      noFinishResults: "Сейчас нет сетов, которые можно быстро и выгодно дособрать по надёжным ценам.",
      noReadyResults: "Собранных сетов для продажи сейчас нет.",
      noSearchResults: "По этому запросу сеты не найдены.",
      clearSearch: "Очистить поиск",
      composition: "Состав сета",
      ownedForSet: "Есть",
      missingForSet: "Не хватает",
      oneSetNeeds: "На один сет",
      orderTitle: "Новый ордер на сет",
      orderPrice: "Цена, платина",
      orderQuantity: "Количество сетов",
      publishOrder: "Сразу показать ордер на рынке",
      reviewOrder: "Проверить ордер",
      cancel: "Отменить",
      confirmTitle: "Подтвердите ордер",
      confirmSummary: (name: string, quantity: number, price: number) => `${name}: выставить ${quantity} шт. по ${price}p.`,
      confirmCheck: "Я проверил сет, цену и количество",
      createOrder: "Создать ордер",
      creatingOrder: "Создаём ордер…",
      orderCreated: "Сет выставлен на Warframe Market.",
      invalidOrder: "Укажите цену и количество не меньше 1.",
      setUnavailable: "Этот сет не найден в каталоге Warframe Market. Обновите рыночные данные.",
      priceUnavailable: "Нет надёжной цены сета. Перед публикацией укажите цену вручную.",
    },
    en: {
      loadError: (_r: string) => "Unable to load insights. Saved prices and inventory were not changed.", refreshError: (_r: string) => "Unable to refresh item data. The previous version was preserved. Refresh market prices first.",
      refreshing: "Refreshing data…", refresh: "Refresh data", reading: "Loading saved data…", validating: "Downloading and checking the update…", lkg: "The update did not pass validation. Showing the previous version.", updated: (d: string) => `Data refreshed: ${d}.`, ready: (d: string) => `Data from ${d} is ready.`, retry: "Try again",
      noSnapshot: "Calculation data has not been loaded", loadOnce: "Load item data", loadBody: "Manual catalog updates are available in Settings.", load: "Open update settings", relics: "Relics", importInventory: "Update inventory to see calculations for your items.", tabs: "Insights section", sets: "Sets", ducats: "Ducats", rivens: "Riven mods",
      buildable: "Buildable", partsSum: "Parts total", part: "Part", needed: "Required", sellable: "Sellable", why: "Why this result?", noParts: "No parts to compare", noPartsBody: "This section appears when the inventory contains recognized Prime parts from one set.", owned: "Owned", chanceCovered: "Chance covered", reward: "Reward", chance: "Chance", ev: "How is EV calculated?", noRelics: "No relics found", noRelicsBody: "A recognized exact subtype is required: Intact, Exceptional, Flawless, or Radiant.",
      ducatNote: "Plat/ducat is a comparison metric, not an automatic instruction to trade an item to Baro.", status: "Status", insufficient: "Insufficient data", noDucats: "Inventory has no recognized Prime parts with ducat metadata.", vaultDisclaimer: "Vault status is additional context only. PlatScope does not assume that a vaulted item must rise in price.",
      fairSet: "Fair set", setPremium: "Set premium", fair: "Fair", relicFair: "Relic fair", pricedEv: "Priced EV", credibleFair: "Credible fair", primeSetCount: "Prime sets", primePartCount: "Prime parts", itemDefinitionCount: "Items with MR", primePart: "Prime part", platinumPerDucat: "Plat / ducat", credible: "Credible", rivenWeaponCount: "Weapons with disposition", rivenSearch: "Find a weapon", rivenSearchPlaceholder: "For example, Soma", weapon: "Weapon", category: "Category", disposition: "Disposition", multiplier: "Multiplier", averageMultiplier: "Average multiplier", multiplierRange: "Range", rivenNote: "Disposition is a weapon-wide coefficient from WFCD. It does not account for the stats, rank, polarity, or positive and negative property mix of a specific Riven and is not a price.", noRivens: "No weapons found.", limitedRivens: (shown: number, total: number) => `Showing the first ${shown} of ${total}. Refine the search to narrow the list.`,
      setsTitle: "Sets",
      setsBody: "Only set decisions live here: what is worth finishing and what is ready to list.",
      finishMode: "Worth finishing",
      readyMode: "Ready to sell",
      readyBadge: "Ready to sell",
      allMode: "All sets",
      setSearch: "Find a set",
      setSearchPlaceholder: "For example, Nyx Prime",
      missing: "Missing",
      completionCost: "Buy missing ≈",
      setValue: "Set price",
      setProfit: "Set advantage",
      readySets: "Ready sets",
      partsValue: "Parts separately",
      openMissing: (count: number) => `Open missing parts (${count})`,
      openMissingHint: (count: number) => `${count} Warframe Market ${count === 1 ? "page" : "pages"} will open in your browser.`,
      marketOpened: (count: number) => `Opened ${count} Warframe Market pages.`,
      marketOpenError: "Unable to open Warframe Market. Try again or open the missing parts manually.",
      sellSet: "List set",
      existingOrder: "Set is already listed",
      openOrders: "Open my orders",
      connectAccount: "Connect Warframe Market",
      verifyAccount: "Verify Warframe Market account",
      noSetResults: "No matching sets",
      noFinishResults: "No sets can currently be finished quickly and profitably with reliable prices.",
      noReadyResults: "No complete sets are ready to sell.",
      noSearchResults: "No sets match this search.",
      clearSearch: "Clear search",
      composition: "Set components",
      ownedForSet: "Owned",
      missingForSet: "Missing",
      oneSetNeeds: "One set needs",
      orderTitle: "New set order",
      orderPrice: "Price, platinum",
      orderQuantity: "Set quantity",
      publishOrder: "Show the order on the market immediately",
      reviewOrder: "Review order",
      cancel: "Cancel",
      confirmTitle: "Confirm order",
      confirmSummary: (name: string, quantity: number, price: number) => `${name}: list ${quantity} at ${price}p each.`,
      confirmCheck: "I reviewed the set, price, and quantity",
      createOrder: "Create order",
      creatingOrder: "Creating order…",
      orderCreated: "Set listed on Warframe Market.",
      invalidOrder: "Use a price and quantity of at least 1.",
      setUnavailable: "This set is missing from the Warframe Market catalog. Refresh market data.",
      priceUnavailable: "No reliable set price is available. Enter a price before publishing.",
    },
  } as const;
  $: c = copy[$locale];

  let view: InsightsView | null = null;
  let activeTab: InsightTab = "sets";
  let loading = true;
  let errorMessage = "";
  let accountView: AccountView | null = null;
  let setMode: SetViewMode = "finish";
  let setQuery = "";
  let marketStatus = "";
  let marketBusySlug = "";
  let listingSlug = "";
  let listingPrice = 1;
  let listingQuantity = 1;
  let listingVisible = true;
  let listingStage: "edit" | "confirm" = "edit";
  let listingConfirmed = false;
  let listingBusy = false;
  let listingError = "";
  let rivenQuery = "";
  $: setRows = filterAndSortSets(view?.sets ?? [], setMode, setQuery, $locale);
  $: finishCount = filterAndSortSets(view?.sets ?? [], "finish", "", $locale).length;
  $: readyCount = filterAndSortSets(view?.sets ?? [], "ready", "", $locale).length;
  $: listedSetItemIds = new Set(
    (accountView?.orders ?? [])
      .filter((order) => order.type === "sell")
      .map((order) => order.itemId)
  );
  $: listingRow = (view?.sets ?? []).find((row) => row.definition.setSlug === listingSlug) ?? null;
  $: normalizedRivenQuery = rivenQuery.trim().toLocaleLowerCase(localeCode($locale));
  $: filteredRivens = (view?.rivenDispositions ?? []).filter((definition) =>
    definition.weaponNameEn.toLocaleLowerCase(localeCode($locale)).includes(normalizedRivenQuery)
  );
  $: visibleRivens = filteredRivens.slice(0, 200);
  $: rivenAverage = view?.rivenDispositions.length
    ? view.rivenDispositions.reduce((sum, definition) => sum + definition.multiplier, 0) / view.rivenDispositions.length
    : null;
  $: rivenMinimum = view?.rivenDispositions.length
    ? Math.min(...view.rivenDispositions.map((definition) => definition.multiplier))
    : null;
  $: rivenMaximum = view?.rivenDispositions.length
    ? Math.max(...view.rivenDispositions.map((definition) => definition.multiplier))
    : null;

  async function loadInsights(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      view = await invoke<InsightsView | null>("insights");
    } catch (error) {
      view = null;
      errorMessage = c.loadError(String(error));
    } finally {
      loading = false;
    }
  }

  async function loadAccount(): Promise<void> {
    try {
      accountView = await invoke<AccountView>("account_status");
    } catch {
      accountView = null;
    }
  }

  function selectSetMode(mode: SetViewMode): void {
    setMode = mode;
    marketStatus = "";
  }

  async function openMissingParts(row: SetInsightRow): Promise<void> {
    const slugs = setOpportunity(row).missingParts.map((part) => part.slug);
    if (slugs.length === 0) return;
    marketBusySlug = row.definition.setSlug;
    marketStatus = "";
    try {
      const opened = await invoke<number>("open_market_items", { slugs });
      marketStatus = c.marketOpened(opened);
    } catch {
      marketStatus = c.marketOpenError;
    } finally {
      marketBusySlug = "";
    }
  }

  function startListing(row: SetInsightRow): void {
    listingError = "";
    if (!accountView?.connected) {
      onOpenAccount();
      return;
    }
    if (!accountView.profile?.verification) {
      onOpenAccount();
      return;
    }
    if (row.itemId && listedSetItemIds.has(row.itemId)) {
      onOpenAccount();
      return;
    }
    if (!row.itemId) {
      listingError = c.setUnavailable;
      listingSlug = row.definition.setSlug;
      return;
    }
    listingSlug = row.definition.setSlug;
    listingPrice = Math.max(1, Math.round(row.setRecommendation?.listPrice ?? row.comparison.setFairValue ?? 1));
    listingQuantity = Math.max(1, row.comparison.completeSets);
    listingVisible = true;
    listingStage = "edit";
    listingConfirmed = false;
  }

  function closeListing(): void {
    listingSlug = "";
    listingError = "";
    listingConfirmed = false;
    listingStage = "edit";
  }

  function reviewListing(event: SubmitEvent): void {
    event.preventDefault();
    listingError = "";
    if (!Number.isInteger(listingPrice) || listingPrice < 1 || !Number.isInteger(listingQuantity) || listingQuantity < 1) {
      listingError = c.invalidOrder;
      return;
    }
    listingStage = "confirm";
    listingConfirmed = false;
  }

  async function createSetListing(): Promise<void> {
    if (!listingRow?.itemId || !listingConfirmed) return;
    listingBusy = true;
    listingError = "";
    const input: CreateListingInput = {
      itemId: listingRow.itemId,
      type: "sell",
      platinum: listingPrice,
      quantity: listingQuantity,
      visible: listingVisible,
      perTrade: null,
      rank: null,
      charges: null,
      subtype: null,
      amberStars: null,
      cyanStars: null,
    };
    try {
      const order = await invoke<AccountOrder>("account_create_listing", { input, confirmed: true });
      if (accountView) accountView = { ...accountView, orders: [...accountView.orders, order] };
      marketStatus = c.orderCreated;
      await loadAccount();
      closeListing();
    } catch (error) {
      listingError = accountActionErrorMessage(String(error), $locale);
    } finally {
      listingBusy = false;
    }
  }

  onMount(() => {
    let disposed = false;
    let unlistenMetadata: UnlistenFn | undefined;
    let unlistenMarket: UnlistenFn | undefined;
    let unlistenInventory: UnlistenFn | undefined;
    void loadInsights();
    void loadAccount();
    void listen("game-metadata-updated", () => void loadInsights()).then((cleanup) => {
      if (disposed) cleanup();
      else unlistenMetadata = cleanup;
    });
    void listen("market-data-updated", () => void loadInsights()).then((cleanup) => {
      if (disposed) cleanup();
      else unlistenMarket = cleanup;
    });
    void listen("inventory-updated", () => void loadInsights()).then((cleanup) => {
      if (disposed) cleanup();
      else unlistenInventory = cleanup;
    });
    return () => {
      disposed = true;
      unlistenMetadata?.();
      unlistenMarket?.();
      unlistenInventory?.();
    };
  });
</script>

<section class="insights-shell" aria-label={c.tabs}>
  <div class="insights-status" role="status" aria-live="polite">
    {#if loading}
      {c.reading}
    {:else if view}
      {c.ready(view.metadata.fetchedAt.slice(0, 10))}
    {/if}
  </div>

  {#if errorMessage}
    <div class="insights-error" role="alert">
      <p>{errorMessage}</p>
      <button type="button" onclick={loadInsights}>{c.retry}</button>
    </div>
  {/if}

  {#if !loading && !view}
    <div class="insights-empty">
      <p class="section-kicker">{c.noSnapshot}</p><h3>{c.loadOnce}</h3><p>{c.loadBody}</p><button type="button" onclick={onOpenSettings}>{c.load}</button>
    </div>
  {:else if view}
    {#if !view.inventoryAvailable}
      <div class="insights-note" role="note">
        {c.importInventory}
      </div>
    {/if}

    <div class="insight-tabs" role="group" aria-label={c.tabs}>
      <button type="button" aria-pressed={activeTab === "sets"} onclick={() => (activeTab = "sets")}>
        {c.sets} <span>{view.sets.length}</span>
      </button>
      <button type="button" aria-pressed={activeTab === "relics"} onclick={() => (activeTab = "relics")}>
        {c.relics} <span>{view.relics.length}</span>
      </button>
      <button type="button" aria-pressed={activeTab === "ducats"} onclick={() => (activeTab = "ducats")}>
        {c.ducats} <span>{view.ducats.length}</span>
      </button>
      <button type="button" aria-pressed={activeTab === "rivens"} onclick={() => (activeTab = "rivens")}>
        {c.rivens} <span>{view.rivenDispositions.length}</span>
      </button>
    </div>

    {#if activeTab === "sets"}
      <section class="sets-workspace" aria-labelledby="sets-heading">
        <header class="sets-heading">
          <div>
            <h2 id="sets-heading">{c.setsTitle}</h2>
            <p>{c.setsBody}</p>
          </div>
          <label class="set-search">
            <span>{c.setSearch}</span>
            <input bind:value={setQuery} type="search" placeholder={c.setSearchPlaceholder} />
          </label>
        </header>

        <div class="set-mode-tabs" role="group" aria-label={c.setsTitle}>
          <button type="button" aria-pressed={setMode === "finish"} onclick={() => selectSetMode("finish")}>
            {c.finishMode} <span>{finishCount}</span>
          </button>
          <button type="button" aria-pressed={setMode === "ready"} onclick={() => selectSetMode("ready")}>
            {c.readyMode} <span>{readyCount}</span>
          </button>
          <button type="button" aria-pressed={setMode === "all"} onclick={() => selectSetMode("all")}>
            {c.allMode} <span>{view.sets.length}</span>
          </button>
        </div>

        <div class="set-action-status" role="status" aria-live="polite">{marketStatus}</div>

        <div class="set-list">
          {#each setRows as row (row.definition.setSlug)}
            {@const opportunity = setOpportunity(row)}
            {@const showCompletion = setMode === "finish" || opportunity.completeSets === 0}
            <article class="set-card">
              <header class="set-card__header">
                <div class="set-identity">
                  {#if row.imageUrl}<img class="insight-thumb" src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                  <div>
                    <p class={`vault vault--${row.definition.vaultStatus}`}>{vaultLabel(row.definition.vaultStatus, $locale)}</p>
                    <h3>{row.displayName}</h3>
                  </div>
                </div>
                <strong class:decision-positive={opportunity.profitableToComplete || opportunity.completeSets > 0} class="set-decision">
                  {showCompletion && opportunity.profitableToComplete ? c.finishMode : opportunity.completeSets > 0 ? c.readyBadge : setModeLabel(row.comparison.recommendedMode, $locale)}
                </strong>
              </header>

              <dl class="set-metrics">
                <div>
                  <dt>{showCompletion ? c.missing : c.readySets}</dt>
                  <dd>{showCompletion ? opportunity.missingQuantity : opportunity.completeSets}</dd>
                </div>
                <div><dt>{showCompletion ? c.completionCost : c.partsValue}</dt><dd>{formatPlatinum(showCompletion ? opportunity.completionCost : opportunity.partsFairValue, $locale)}</dd></div>
                <div><dt>{c.setValue}</dt><dd>{formatPlatinum(opportunity.setFairValue, $locale)}</dd></div>
                <div class:positive-value={(opportunity.setPremiumValue ?? 0) > 0}>
                  <dt>{c.setProfit}</dt>
                  <dd>{formatPlatinum(opportunity.setPremiumValue, $locale)} <small>{formatPercent(opportunity.setPremiumPercent, $locale)}</small></dd>
                </div>
              </dl>

              {#if showCompletion && opportunity.missingParts.length > 0}
                <div class="missing-parts" aria-label={c.missing}>
                  {#each opportunity.missingParts as part (part.slug)}
                    <span>{part.displayName} ×{part.quantity} <strong>{formatPlatinum(part.estimatedCost, $locale)}</strong></span>
                  {/each}
                </div>
              {/if}

              <div class="set-card__actions">
                {#if showCompletion && opportunity.missingParts.length > 0}
                  <button type="button" disabled={marketBusySlug === row.definition.setSlug} onclick={() => openMissingParts(row)}>
                    {c.openMissing(opportunity.missingParts.length)}
                  </button>
                {/if}
                {#if opportunity.completeSets > 0}
                  <button type="button" class="secondary" onclick={() => row.itemId && listedSetItemIds.has(row.itemId) ? onOpenAccount() : startListing(row)}>
                    {#if row.itemId && listedSetItemIds.has(row.itemId)}{c.openOrders}{:else if !accountView?.connected}{c.connectAccount}{:else if !accountView.profile?.verification}{c.verifyAccount}{:else}{c.sellSet}{/if}
                  </button>
                {/if}
              </div>
              {#if showCompletion && opportunity.missingParts.length > 0}
                <p class="action-hint">{c.openMissingHint(opportunity.missingParts.length)}</p>
              {/if}

              {#if listingSlug === row.definition.setSlug}
                <section class="set-order-panel" aria-labelledby={`set-order-${row.definition.setSlug}`}>
                  <h4 id={`set-order-${row.definition.setSlug}`}>{listingStage === "edit" ? c.orderTitle : c.confirmTitle}</h4>
                  {#if listingError}<p class="inline-error" role="alert">{listingError}</p>{/if}
                  {#if listingStage === "edit"}
                    <form onsubmit={reviewListing}>
                      <div class="set-order-fields">
                        <label><span>{c.orderPrice}</span><input bind:value={listingPrice} type="number" min="1" step="1" inputmode="numeric" /></label>
                        <label><span>{c.orderQuantity}</span><input bind:value={listingQuantity} type="number" min="1" max={opportunity.completeSets} step="1" inputmode="numeric" /></label>
                      </div>
                      <label class="set-order-visible"><input bind:checked={listingVisible} type="checkbox" /> <span>{c.publishOrder}</span></label>
                      <div class="set-order-actions"><button type="submit">{c.reviewOrder}</button><button type="button" class="secondary" onclick={closeListing}>{c.cancel}</button></div>
                    </form>
                  {:else}
                    <p>{c.confirmSummary(row.displayName, listingQuantity, listingPrice)}</p>
                    <label class="set-order-visible"><input bind:checked={listingConfirmed} type="checkbox" /> <span>{c.confirmCheck}</span></label>
                    <div class="set-order-actions"><button type="button" disabled={listingBusy || !listingConfirmed} onclick={createSetListing}>{listingBusy ? c.creatingOrder : c.createOrder}</button><button type="button" class="secondary" disabled={listingBusy} onclick={closeListing}>{c.cancel}</button></div>
                  {/if}
                </section>
              {/if}

              <details class="set-composition">
                <summary>{c.composition}</summary>
                <div class="table-scroll">
                  <table>
                    <thead><tr><th>{c.part}</th><th>{c.oneSetNeeds}</th><th>{c.ownedForSet}</th><th>{c.missingForSet}</th><th>{c.fair}</th></tr></thead>
                    <tbody>
                      {#each row.components as component (component.definition.slug)}
                        <tr>
                          <th scope="row"><span class="insight-item-name">{#if component.imageUrl}<img src={component.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{component.displayName}</span></th>
                          <td>{component.definition.requiredQuantity}</td>
                          <td>{component.ownedQuantity}</td>
                          <td>{Math.max(0, component.definition.requiredQuantity * (opportunity.completeSets + 1) - component.ownedQuantity)}</td>
                          <td>{formatPlatinum(component.recommendation?.fairPrice ?? null, $locale)}</td>
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              </details>
            </article>
          {:else}
            <div class="insights-empty compact">
              <h3>{c.noSetResults}</h3>
              <p>{setQuery ? c.noSearchResults : setMode === "finish" ? c.noFinishResults : setMode === "ready" ? c.noReadyResults : c.noPartsBody}</p>
              {#if setQuery}<button type="button" onclick={() => (setQuery = "")}>{c.clearSearch}</button>{/if}
            </div>
          {/each}
        </div>
      </section>
    {:else if activeTab === "relics"}
      <div class="insight-list">
        {#each view.relics as row (`${row.definition.relicSlug}:${row.definition.refinement}`)}
          <article class="insight-card">
            <header>
              {#if row.imageUrl}<img class="insight-thumb" src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
              <div>
                <p class={`vault vault--${row.definition.vaultStatus}`}>{vaultLabel(row.definition.vaultStatus, $locale)}</p><h3>{row.definition.displayNameEn} · {refinementLabel(row.definition.refinement, $locale)}</h3>
              </div>
              <strong class={`decision decision--${row.expectedValue.coverage}`}>
                {coverageLabel(row.expectedValue.coverage, $locale)}
              </strong>
            </header>
            <dl class="metric-grid">
              <div><dt>{c.owned}</dt><dd>{row.ownedQuantity}</dd></div><div><dt>{c.relicFair}</dt><dd>{formatPlatinum(row.relicRecommendation?.fairPrice ?? null, $locale)}</dd></div><div><dt>{c.pricedEv}</dt><dd>{formatPlatinum(row.expectedValue.pricedExpectedValue, $locale)}</dd></div><div><dt>{c.chanceCovered}</dt><dd>{row.expectedValue.pricedChancePercent.toLocaleString(localeCode($locale), { maximumFractionDigits: 1 })}%</dd></div>
            </dl>
            <div class="table-scroll">
              <table>
                <thead><tr><th>{c.reward}</th><th>{c.chance}</th><th>{c.credibleFair}</th></tr></thead>
                <tbody>
                  {#each row.rewards as reward (reward.definition.rewardGameRef)}
                    <tr>
                      <th scope="row"><span class="insight-item-name">{#if reward.imageUrl}<img src={reward.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{reward.definition.displayNameEn}</span></th>
                      <td>{reward.definition.chancePercent.toLocaleString(localeCode($locale), { maximumFractionDigits: 2 })}%</td><td>{formatPlatinum(reward.recommendation?.fairPrice ?? null, $locale)}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
            <details>
              <summary>{c.ev}</summary>
              <ul>{#each relicReasonMessages(row, $locale) as reason}<li>{reason}</li>{/each}</ul>
            </details>
          </article>
        {:else}
          <div class="insights-empty compact">
            <h3>{c.noRelics}</h3><p>{c.noRelicsBody}</p>
          </div>
        {/each}
      </div>
    {:else if activeTab === "ducats"}
      <div class="insight-card ducat-card">
        <div class="insights-note" role="note">
          {c.ducatNote}
        </div>
        <div class="table-scroll">
          <table>
            <thead><tr><th>{c.primePart}</th><th>{c.sellable}</th><th>{c.fair}</th><th>{c.ducats}</th><th>{c.platinumPerDucat}</th><th>{c.status}</th></tr></thead>
            <tbody>
              {#each view.ducats as row (row.metadata.slug)}
                <tr>
                  <th scope="row"><span class="insight-item-name">{#if row.imageUrl}<img src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{row.displayName}</span></th>
                  <td>{row.sellableQuantity}</td>
                  <td>{formatPlatinum(row.efficiency.fairPrice, $locale)}</td>
                  <td>{row.efficiency.ducats}</td>
                  <td>{formatRatio(row.efficiency.platinumPerDucat, $locale)}</td><td>{row.efficiency.credible ? c.credible : c.insufficient}</td>
                </tr>
              {:else}
                <tr><td colspan="6">{c.noDucats}</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {:else}
      <div class="insight-card riven-card">
        <div class="insights-note" role="note">{c.rivenNote}</div>
        <dl class="metric-grid riven-metrics">
          <div><dt>{c.rivenWeaponCount}</dt><dd>{view.rivenDispositions.length.toLocaleString(localeCode($locale))}</dd></div>
          <div><dt>{c.averageMultiplier}</dt><dd>{formatRatio(rivenAverage, $locale)}</dd></div>
          <div><dt>{c.multiplierRange}</dt><dd>{formatRatio(rivenMinimum, $locale)}–{formatRatio(rivenMaximum, $locale)}</dd></div>
        </dl>
        <label class="riven-search">
          <span>{c.rivenSearch}</span>
          <input bind:value={rivenQuery} type="search" placeholder={c.rivenSearchPlaceholder} />
        </label>
        <div class="table-scroll">
          <table>
            <thead><tr><th>{c.weapon}</th><th>{c.category}</th><th>{c.disposition}</th><th>{c.multiplier}</th></tr></thead>
            <tbody>
              {#each visibleRivens as definition (definition.weaponGameRef)}
                <tr>
                  <th scope="row">{definition.weaponNameEn}</th>
                  <td>{rivenCategoryLabel(definition.category, $locale)}</td>
                  <td>{definition.disposition} / 5</td>
                  <td>{formatRatio(definition.multiplier, $locale)}×</td>
                </tr>
              {:else}
                <tr><td colspan="4">{c.noRivens}</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#if filteredRivens.length > visibleRivens.length}
          <p class="result-limit" role="status">{c.limitedRivens(visibleRivens.length, filteredRivens.length)}</p>
        {/if}
      </div>
    {/if}

    <p class="vault-disclaimer">
      {c.vaultDisclaimer}
    </p>
  {/if}
</section>

<style>
  .insights-shell { display: grid; gap: .7rem; }
  .insights-status { min-height: 1.5rem; color: var(--text-muted); }
  .insights-error, .insights-empty, .insights-note { border: 1px solid var(--border); border-radius: .6rem; padding: .75rem; background: var(--surface-2); }
  .insights-error { border-color: var(--danger); background: var(--danger-soft); }
  .insights-error p, .insights-empty p { color: var(--text-muted); }
  .insights-error p, .insights-empty p:last-of-type { margin-block-end: .8rem; }
  .insights-empty.compact { text-align: center; }
  .metric-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: .45rem; margin: 0; }
  .metric-grid div { min-width: 0; border: 1px solid var(--border); border-radius: .55rem; padding: .6rem; background: var(--surface-2); }
  dt { color: var(--text-muted); font-size: .8rem; }
  dd { margin: .2rem 0 0; font-variant-numeric: tabular-nums; font-size: 1.2rem; font-weight: 760; }
  .insight-tabs { display: flex; gap: .5rem; overflow-x: auto; margin: -.4rem; padding: .4rem; }
  .insight-tabs button { flex: 0 0 auto; border-color: var(--border); background: var(--surface-1); color: var(--text); }
  .insight-tabs button:hover { border-color: var(--border-strong); background: var(--surface-2); }
  .insight-tabs button[aria-pressed="true"] { border-color: var(--accent); background: var(--accent); color: oklch(0.985 0.009 84); }
  .insight-tabs span { margin-inline-start: .35rem; color: inherit; font-variant-numeric: tabular-nums; }
  .sets-workspace { display: grid; gap: .6rem; }
  .sets-heading { display: flex; align-items: end; justify-content: space-between; gap: 1rem; }
  .sets-heading h2 { margin-block-end: .2rem; font-size: 1.2rem; }
  .sets-heading p { max-width: 58ch; margin: 0; color: var(--text-muted); }
  .set-search { display: grid; flex: 0 1 20rem; gap: .3rem; color: var(--text); font-size: .8rem; font-weight: 650; }
  .set-search input { min-height: 2.25rem; width: 100%; border: 1px solid var(--border); border-radius: .5rem; padding-inline: .6rem; background: oklch(0.995 0.004 84); color: var(--text); }
  .set-mode-tabs { display: flex; flex-wrap: wrap; gap: .4rem; }
  .set-mode-tabs button { border-color: var(--border); background: var(--surface-1); color: var(--text); }
  .set-mode-tabs button:hover { border-color: var(--border-strong); background: var(--surface-2); }
  .set-mode-tabs button[aria-pressed="true"] { border-color: var(--accent); background: var(--accent); color: oklch(0.985 0.009 84); }
  .set-mode-tabs span { margin-inline-start: .3rem; font-variant-numeric: tabular-nums; }
  .set-action-status { min-height: 1.25rem; color: var(--success); font-size: .8rem; font-weight: 650; }
  .set-list { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 31rem), 1fr)); align-items: start; gap: .6rem; }
  .set-card { min-width: 0; border: 1px solid var(--border); border-radius: .75rem; padding: .7rem; background: var(--surface-1); box-shadow: var(--shadow-sm); }
  .set-card__header { display: flex; align-items: start; justify-content: space-between; gap: .75rem; margin-block-end: .6rem; }
  .set-identity { display: flex; align-items: center; min-width: 0; gap: .6rem; }
  .set-identity h3 { margin: 0; font-size: 1.05rem; }
  .set-decision { flex: none; max-width: 14rem; border-radius: 999px; padding: .2rem .45rem; background: var(--surface-3); color: var(--text-muted); font-size: .6875rem; line-height: 1.3; text-align: center; }
  .set-decision.decision-positive { background: var(--success-soft); color: oklch(0.37 0.08 145); }
  .set-metrics { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: .35rem; margin: 0; }
  .set-metrics > div { min-width: 0; border-radius: .5rem; padding: .5rem; background: var(--surface-2); }
  .set-metrics dt { font-size: .72rem; }
  .set-metrics dd { margin-block-start: .15rem; font-size: .98rem; }
  .set-metrics dd small { display: block; color: var(--text-muted); font-size: .68rem; font-weight: 650; }
  .set-metrics .positive-value dd { color: oklch(0.37 0.08 145); }
  .missing-parts { display: flex; flex-wrap: wrap; gap: .35rem; margin-block-start: .55rem; }
  .missing-parts > span { border-radius: 999px; padding: .2rem .45rem; background: var(--accent-soft); color: var(--accent-strong); font-size: .72rem; }
  .missing-parts strong { margin-inline-start: .2rem; font-variant-numeric: tabular-nums; }
  .set-card__actions, .set-order-actions { display: flex; flex-wrap: wrap; gap: .4rem; margin-block-start: .55rem; }
  .set-card__actions button { flex: 0 1 auto; }
  .action-hint { margin: .3rem 0 0; color: var(--text-muted); font-size: .72rem; }
  .set-composition { margin-block-start: .45rem; }
  .set-composition table { min-width: 34rem; }
  .set-order-panel { margin-block-start: .6rem; border: 1px solid var(--border); border-radius: .6rem; padding: .6rem; background: var(--surface-2); }
  .set-order-panel h4 { margin: 0 0 .5rem; font-size: .92rem; }
  .set-order-panel p { margin-block-end: .5rem; color: var(--text-muted); }
  .set-order-panel form { display: grid; gap: .5rem; }
  .set-order-fields { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .45rem; }
  .set-order-fields label { display: grid; gap: .25rem; color: var(--text); font-size: .78rem; font-weight: 650; }
  .set-order-fields input { min-width: 0; min-height: 2.25rem; width: 100%; border: 1px solid var(--border); border-radius: .45rem; padding-inline: .55rem; background: oklch(0.995 0.004 84); color: var(--text); }
  .set-order-visible { display: flex; align-items: center; width: fit-content; min-height: 2.125rem; gap: .45rem; color: var(--text); font-size: .78rem; font-weight: 650; cursor: pointer; }
  .set-order-visible input { width: 1.15rem; height: 1.15rem; accent-color: var(--accent); }
  .set-order-actions button { flex: 0 1 auto; }
  .inline-error { color: var(--danger) !important; font-size: .8rem; }
  .insight-list { display: grid; gap: .6rem; }
  .insight-card { min-width: 0; border: 1px solid var(--border); border-radius: .75rem; padding: .75rem; background: var(--surface-1); box-shadow: var(--shadow-sm); }
  .insight-card > header { display: flex; align-items: start; justify-content: space-between; gap: .7rem; margin-block-end: .6rem; }
  .insight-thumb { flex: none; width: 4rem; height: 4rem; object-fit: contain; outline: 1px solid oklch(0 0 0 / 0.1); outline-offset: -1px; }
  .insight-item-name { display: inline-flex; align-items: center; gap: .55rem; }
  .insight-item-name img { flex: none; width: 2rem; height: 2rem; object-fit: contain; outline: 1px solid oklch(0 0 0 / 0.1); outline-offset: -1px; }
  .insight-card h3 { margin-block-end: 0; font-size: 1.05rem; }
  .vault { margin-block-end: .25rem; font-size: .78rem; font-weight: 720; }
  .vault--available { color: var(--success); }
  .vault--vaulted { color: var(--gold); }
  .vault--unknown { color: var(--text-muted); }
  .decision { max-width: 18rem; border-radius: 999px; padding: .2rem .45rem; background: var(--accent-soft); color: var(--accent-strong); font-size: .6875rem; text-align: center; }
  .decision--set, .decision--complete { background: var(--success-soft); color: oklch(0.37 0.08 145); }
  .decision--parts, .decision--partial { background: oklch(0.92 0.055 78); color: oklch(0.43 0.075 68); }
  .decision--insufficient, .decision--insufficient_pricing { background: var(--danger-soft); color: var(--danger); }
  .metric-grid { grid-template-columns: repeat(4, minmax(0, 1fr)); margin-block-end: .6rem; }
  .metric-grid dd { font-size: 1rem; }
  .table-scroll { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-size: .88rem; }
  th, td { border-block-end: 1px solid var(--border); padding: .45rem .5rem; text-align: start; font-variant-numeric: tabular-nums; }
  thead th { color: var(--text-muted); font-size: .76rem; text-transform: uppercase; letter-spacing: .04em; }
  tbody th { font-weight: 650; }
  details { margin-block-start: .75rem; }
  summary { min-height: 2.125rem; padding-block: .4rem; color: var(--accent-strong); cursor: pointer; font-weight: 650; }
  details ul { margin-block-end: 0; padding-inline-start: 1.25rem; color: var(--text-muted); }
  details li + li { margin-block-start: .35rem; }
  .ducat-card { padding: 0; overflow: hidden; }
  .ducat-card .insights-note { border-width: 0 0 1px; border-radius: 0; }
  .ducat-card table { min-width: 46rem; }
  .riven-card { padding: 0; overflow: hidden; }
  .riven-card .insights-note { border-width: 0 0 1px; border-radius: 0; }
  .riven-metrics { grid-template-columns: repeat(3, minmax(0, 1fr)); padding: .7rem; }
  .riven-search { display: grid; gap: .4rem; padding: 0 .7rem .7rem; color: var(--text); font-weight: 650; }
  .riven-search input { min-height: 2.25rem; width: min(100%, 32rem); }
  .riven-card table { min-width: 38rem; }
  .result-limit { margin: 0; padding: .6rem .7rem; color: var(--text-muted); }
  .vault-disclaimer { margin: 0; color: var(--text-muted); font-size: .86rem; }
  @media (max-width: 48rem) {
    summary, .riven-search input, .set-search input, .set-order-fields input, .set-order-visible { min-height: 2.5rem; }
    .riven-search input, .set-search input, .set-order-fields input { font-size: 1rem; }
    .insight-card > header { align-items: stretch; flex-direction: column; }
    .metric-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .sets-heading, .set-card__header { align-items: stretch; flex-direction: column; }
    .set-search { flex-basis: auto; width: 100%; }
    .set-decision { width: fit-content; }
    .set-metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .set-card__actions button { flex: 1 1 12rem; }
  }
  @media (max-width: 30rem) {
    .metric-grid { grid-template-columns: 1fr 1fr; }
    .insight-card { padding: .8rem; }
    .set-mode-tabs { display: grid; grid-template-columns: minmax(0, 1fr); }
    .set-order-fields { grid-template-columns: minmax(0, 1fr); }
  }
  @media (forced-colors: active) {
    .set-card, .set-order-panel, .set-mode-tabs button[aria-pressed="true"] { border-color: CanvasText; }
  }
</style>
