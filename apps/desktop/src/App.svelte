<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";

  import AppSidebar, { type AppScreen } from "./lib/AppSidebar.svelte";
  import AppUpdatePanel from "./lib/AppUpdatePanel.svelte";
  import BountyHunterScreen from "./lib/BountyHunterScreen.svelte";
  import WorldActivityScreen from "./lib/WorldActivityScreen.svelte";
  import { startWorldActivityAlerts, worldPreferences } from "./lib/worldActivityStore";
  import EquippedModsScreen from "./lib/EquippedModsScreen.svelte";
  import MarketItemDetail from "./lib/MarketItemDetail.svelte";
  import InsightsScreen from "./lib/InsightsScreen.svelte";
  import MarketTradingShift from "./lib/MarketTradingShift.svelte";
  import SellNowScreen from "./lib/SellNowScreen.svelte";
  import SettingsScreen from "./lib/SettingsScreen.svelte";
  import { revealCompactDetail, revealElement } from "./lib/detailNavigation";
  import { startAutomaticUpdateChecks } from "./lib/appUpdate";
  import { startBountyRewardAlerts } from "./lib/bountyAlerts";

  import {
    providerLabel,
    type FoundationStatus,
    type MarketRefreshOutcome,
  } from "./lib/foundation";
  import type { MarketHistoryView } from "./lib/history";
  import {
    filterAndSortRows,
    formatPlatinum,
    formatVolume,
    freshnessLabel,
    rowIdentity,
    variantLabel,
    type MarketSearchResult,
    type MarketSearchRow,
    type LivePricingResult,
    type MarketSortKey,
    type PriceFilter,
    type SortDirection,
  } from "./lib/market";
  import {
    installLocale,
    localeCode,
    localeFromLanguage,
    type AppSettings,
  } from "./lib/i18n";
  import {
    loadMarketViewPreferences,
    saveInsightsViewPreferences,
    saveMarketViewPreferences,
  } from "./lib/viewPreferences";

  const locale = installLocale("ru");
  const shellCopy = {
    ru: {
      skip: "Перейти к содержимому",
      navLabel: "Разделы приложения",
      market: "Рынок",
      worldActivity: "Сейчас в игре",
      worldActivityLede: "Что доступно сейчас, что скоро сменится и куда отправиться.",
      inventory: "Мои предметы",
      equippedMods: "Надетые моды",
      insights: "Возможности",
      bountyHunter: "Охотник за наградами",
      settings: "Настройки",
      marketLede: "Управляйте объявлениями и находите цены на предметы.",
      inventoryLede: "Торговый инвентарь, момент продажи и ордера Warframe Market в одном месте.",
      equippedModsLede: "На каком предмете и в какой конфигурации стоит каждый мод.",
      insightsLede: "Лучшие способы превратить инвентарь и ресурсы в платину.",
      bountyHunterLede: "Найдите нужную награду и выберите активный заказ по шансу выпадения, сложности и стоимости добычи.",
      settingsLede: "Язык, платформа и обновление данных.",
      searching: "Ищем в сохранённых данных…", shown: (visible: number, total: number) => `${visible} из ${total} вариантов показано`,
      storageError: (_reason: string) => "Не удалось открыть сохранённые данные. Перезапустите PlatScope.",
      refreshError: (_reason: string) => "Не удалось обновить рынок. Старые данные сохранены. Проверьте подключение и повторите попытку.",
      searchError: (_reason: string) => "Не удалось выполнить поиск. Сократите запрос или повторите попытку.",
      noBulk: "Для этого варианта пока нет сохранённой оценки.", liveError: (_reason: string) => "Не удалось получить текущие цены. Сохранённая оценка не изменилась.",
      historyError: (_reason: string) => "Не удалось загрузить историю. Текущая цена по-прежнему доступна.",
      refreshing: "Обновляем данные…", refresh: "Обновить данные", openingStorage: "Открываем сохранённые данные…", validatingSnapshot: "Загружаем и проверяем новые цены…", providersUnavailable: "Источники временно недоступны. Показываем последние сохранённые данные.", checkStorage: "Проверить данные",
      noSnapshot: "Данные рынка ещё не загружены", loadMarket: "Загрузите цены рынка", loadMarketBody: "Обновление цен и 90-дневной истории находится в настройках.", loadingMarket: "Загружаем данные…", loadData: "Открыть настройки обновления",
      marketFilters: "Поиск и фильтры рынка", searchItem: "Поиск предмета", searchExample: "Например, Никс Прайм или nyx prime", clear: "Очистить", shortcut: "Быстрый доступ:", priceAvailability: "Наличие оценки", allVariants: "Все варианты", priced: "Есть оценка", unpriced: "Оценки пока нет",
      results: "Результаты", snapshot: "Данные от", marketCaption: "Предметы, цены, продажи и актуальность данных", item: "Предмет", trades: "Сделки", freshness: "Актуальность",
      first60: "Поиск ограничен первыми 60 вариантами. Уточните название предмета, чтобы найти нужный.", noQuery: (query: string) => `По запросу «${query}» ничего не найдено`, noFilter: "Для этого фильтра ничего не найдено", checkSpelling: "Проверьте название предмета.", choosePriceFilter: "Выберите другой фильтр цены.", clearSearch: "Очистить поиск",
      relic: "Реликвия", riven: "Мод разлома", marketItem: "Предмет рынка", gettingLive: "Получаем текущие цены…", updateLive: "Обновить текущие цены", getLive: "Проверить текущие цены", liveHint: "Покажет ордера игроков, которые сейчас в игре.", dataDate: "Цена рассчитана по данным от", masteryRequirement: "Ранг мастерства", whyPrice: "Как рассчитана цена?",
      marketData: "Данные рынка", dataReady: "Загружены", dataMissing: "Не загружены",
      fair: "Цена", fairPrice: "Оценка рынка", listPrice: "Ориентир размещения", closedVolume: "Закрытые сделки", lowestAsk: "Минимальная цена продажи", depthThree: "Средняя цена до 3 шт.", depthPrice: "Средняя цена до 5 шт.", quickSell: "Лучшая заявка на покупку", sell: "продажа", buy: "покупка", currentOrders: "Ордера игроков в игре", side: "Тип", price: "Цена", quantityLot: "Количество · лот", playerStatus: "Статус", sellOrder: "Продажа", buyOrder: "Покупка", noActiveOrders: "Сейчас в игре нет ордеров для этого варианта.",
      priceHistory: "История цены", historyRange: "Период", dayShort: "д", loadingHistory: "Загружаем историю…", historyCoverage: (points: number, coverage: number) => `${points} дней · доступно ${coverage} дней истории`, selectForHistory: "Выберите строку, чтобы посмотреть историю цены.", median: "Медиана", change: "Изменение", averageVolume: "Средний объём", insufficientChart: "Пока недостаточно данных для графика. История накопится после обновлений рынка.", itemDetails: "Подробности предмета", selectItem: "Выберите предмет в таблице, чтобы увидеть цену и расчёт.",
      marketModeLabel: "Режим рынка", mySales: "Мои продажи", marketSearch: "Найти предмет",
    },
    en: {
      skip: "Skip to content",
      navLabel: "Application sections",
      market: "Market",
      worldActivity: "Now in game",
      worldActivityLede: "Current activities, world cycles and upcoming rotations.",
      inventory: "My items",
      equippedMods: "Equipped mods",
      insights: "Opportunities",
      bountyHunter: "Bounty hunter",
      settings: "Settings",
      marketLede: "Your sales, order health, and price research in one workspace.",
      inventoryLede: "Market inventory, sell timing, and Warframe Market orders in one place.",
      equippedModsLede: "See the item and configuration using each mod.",
      insightsLede: "The best ways to turn inventory and resources into platinum.",
      bountyHunterLede: "Find rewards and compare active bounties by drop chance, difficulty and estimated reward value.",
      settingsLede: "Language, market platform, and data refresh controls.",
      searching: "Searching saved data…", shown: (visible: number, total: number) => `${visible} of ${total} variants shown`,
      storageError: (_reason: string) => "Unable to open saved data. Restart PlatScope.",
      refreshError: (_reason: string) => "Unable to refresh the market. Saved data was preserved. Check the connection and try again.",
      searchError: (_reason: string) => "Unable to search. Shorten the query or try again.",
      noBulk: "No saved estimate exists for this variant.", liveError: (_reason: string) => "Unable to retrieve current prices. The saved estimate was preserved.",
      historyError: (_reason: string) => "Unable to load history. The current price remains available.",
      refreshing: "Refreshing data…", refresh: "Refresh data", openingStorage: "Opening saved data…", validatingSnapshot: "Downloading and checking new prices…", providersUnavailable: "Sources are temporarily unavailable. Showing the latest saved data.", checkStorage: "Check data",
      noSnapshot: "Market data has not been loaded", loadMarket: "Load market prices", loadMarketBody: "Price and 90-day history updates are available in Settings.", loadingMarket: "Loading data…", loadData: "Open update settings",
      marketFilters: "Market search and filters", searchItem: "Search items", searchExample: "For example, Nyx Prime or nyx prime", clear: "Clear", shortcut: "Shortcut:", priceAvailability: "Price availability", allVariants: "All variants", priced: "Has an estimate", unpriced: "No estimate yet",
      results: "Results", snapshot: "Data from", marketCaption: "Market items, prices, sales, and data freshness", item: "Item", trades: "Trades", freshness: "Freshness",
      first60: "Showing the first 60 variants. Refine the query to narrow the list.", noQuery: (query: string) => `No results for “${query}”`, noFilter: "No variants match this filter", checkSpelling: "Check the spelling or use a canonical slug.", choosePriceFilter: "Choose a different price filter.", clearSearch: "Clear search",
      relic: "Relic", riven: "Riven mod", marketItem: "Market item", gettingLive: "Getting current prices…", updateLive: "Refresh current prices", getLive: "Check current prices", liveHint: "Shows orders from players who are currently in game.", dataDate: "Price data from", masteryRequirement: "Mastery rank", whyPrice: "How is this price calculated?",
      marketData: "Market data", dataReady: "Loaded", dataMissing: "Not loaded",
      fair: "Fair", fairPrice: "Fair price", listPrice: "List price", closedVolume: "Closed volume", lowestAsk: "Lowest ask", depthThree: "Up to 3 units average", depthPrice: "Up to 5 units average", quickSell: "Quick Sell", sell: "sell", buy: "buy", currentOrders: "Orders from players in game", side: "Side", price: "Price", quantityLot: "Quantity · lot", playerStatus: "Player status", sellOrder: "Sell", buyOrder: "Buy", noActiveOrders: "No players in game have orders for this exact variant.",
      priceHistory: "Price history", historyRange: "History range", dayShort: "d", loadingHistory: "Loading local aggregates…", historyCoverage: (points: number, coverage: number) => `${points} days for this variant · ${coverage} local days covered`, selectForHistory: "Select a row to open compact history for the exact variant.", median: "Median", change: "Change", averageVolume: "Average volume", insufficientChart: "Not enough points for a chart. Background bootstrap adds up to seven days per launch.", itemDetails: "Item details", selectItem: "Select an item in the table to see its calculation and explanation.",
      marketModeLabel: "Market mode", mySales: "My sales", marketSearch: "Find an item",
    },
  } as const;

  $: shell = shellCopy[$locale];
  $: navigationLabels = { world_activity: shell.worldActivity, market: shell.market, inventory: shell.inventory,
    equipped_mods: shell.equippedMods, insights: shell.insights, bounty_hunter: shell.bountyHunter, settings: shell.settings };

  let status: FoundationStatus | null = null;
  let refreshOutcome: MarketRefreshOutcome | null = null;
  let searchResult: MarketSearchResult | null = null;
  let liveResult: LivePricingResult | null = null;
  let liveIdentity = "";
  let liveRequestIdentity = "";
  let historyRequestIdentity = "";
  let historyView: MarketHistoryView | null = null;
  let historyIdentity = "";
  let historyRange: 7 | 30 | 90 = 7;
  type MarketWorkspace = "sales" | "browse";

  let sidebarCompact = false;
  let activeScreen: AppScreen = $worldPreferences.startHere ? "world_activity" : "inventory";
  let bountyRegion = "all";
  let marketWorkspace: MarketWorkspace = "sales";
  let inventoryInitialQuery = "";
  let marketActionMessage = "";
  let pageHeading: HTMLHeadingElement;
  let selectedIdentity = "";
  let query = "";
  let priceFilter: PriceFilter = "all";
  let sortKey: MarketSortKey = "volume";
  let sortDirection: SortDirection = "desc";
  let viewPreferencesReady = false;
  let errorMessage = "";
  let loading = true;
  let searching = false;
  let liveLoading = false;
  let liveError = "";
  let historyLoading = false;
  let historyError = "";
  let searchInput: HTMLInputElement;
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let searchSequence = 0;
  let liveSequence = 0;
  let historySequence = 0;
  let keyboardNavigation = false;
  let detailTrigger: HTMLElement | null = null;
  let settingsScreen: SettingsScreen | undefined;

  function navigateTo(screen: AppScreen): void {
    if (screen === activeScreen) return;
    if (activeScreen === "settings" && settingsScreen) {
      settingsScreen.requestLeave(() => completeNavigation(screen));
      return;
    }
    completeNavigation(screen);
  }

  function completeNavigation(screen: AppScreen): void {
    activeScreen = screen;
    void tick().then(() => {
      window.scrollTo({ top: 0, left: 0 });
      if (keyboardNavigation) pageHeading?.focus();
    });
  }

  function openMarketSales(): void {
    marketWorkspace = "sales";
    navigateTo("market");
  }

  $: visibleRows = filterAndSortRows(
    searchResult?.rows ?? [],
    priceFilter,
    sortKey,
    sortDirection,
  );
  $: if (!visibleRows.some(row => rowIdentity(row) === selectedIdentity)) {
    selectedIdentity = visibleRows[0] ? rowIdentity(visibleRows[0]) : "";
  }
  $: if (viewPreferencesReady) {
    saveMarketViewPreferences({ priceFilter, sortKey, sortDirection });
  }
  $: selectedRow =
    visibleRows.find((row) => rowIdentity(row) === selectedIdentity) ?? null;
  $: activeRecommendation =
    liveResult && liveIdentity === selectedIdentity
      ? liveResult.recommendation
      : selectedRow?.recommendation ?? null;
  $: resultStatus = searching
    ? shell.searching
    : searchResult
      ? shell.shown(visibleRows.length, searchResult.rows.length)
      : "";

  async function readMarket<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    let timer: ReturnType<typeof setTimeout>;
    try {
      return await Promise.race([invoke<T>(command, args), new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error("market_read_timeout")), 20_000);
      })]);
    } finally { clearTimeout(timer!); }
  }

  async function openSelectedMarketItem(): Promise<void> {
    if (!selectedRow) return;
    const identity = selectedIdentity;
    marketActionMessage = "";
    try {
      await invoke("open_market_items", { slugs: [selectedRow.recommendation.key.slug] });
      if (identity === selectedIdentity) marketActionMessage = "Warframe Market открыт в браузере.";
    } catch {
      if (identity === selectedIdentity) marketActionMessage = "Не удалось открыть Warframe Market. Повторите попытку.";
    }
  }

  async function loadStatus(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      status = await invoke<FoundationStatus>("foundation_status");
    } catch (error) {
      status = null;
      errorMessage = shell.storageError(String(error));
    } finally {
      loading = false;
    }
  }

  function handleMarketRefreshed(outcome: MarketRefreshOutcome): void {
    refreshOutcome = outcome;
    void loadStatus().then(searchMarket);
  }

  async function searchMarket(): Promise<void> {
    if (!status?.marketSnapshot) return;
    const requestId = ++searchSequence;
    const requestedQuery = query;
    searching = true;
    errorMessage = "";
    try {
      const result = await readMarket<MarketSearchResult>("search_market", {
        query: requestedQuery,
        limit: 60,
      });
      if (requestId !== searchSequence) return;
      searchResult = result;
      const filtered = filterAndSortRows(result.rows, priceFilter, sortKey, sortDirection);
      const selectedStillExists = filtered.some(
        (row) => rowIdentity(row) === selectedIdentity,
      );
      if (!selectedStillExists) {
        selectedIdentity = filtered[0] ? rowIdentity(filtered[0]) : "";
      }
      if (liveIdentity !== selectedIdentity) {
        liveResult = null;
        liveIdentity = "";
        liveError = "";
      }
      if (historyIdentity !== selectedIdentity) {
        historyView = null;
        historyIdentity = "";
        historyError = "";
      }
    } catch (error) {
      if (requestId !== searchSequence) return;
      searchResult = null;
      selectedIdentity = "";
      errorMessage = shell.searchError(String(error));
    } finally {
      if (requestId === searchSequence) searching = false;
    }
  }

  function scheduleSearch(event: Event): void {
    query = (event.currentTarget as HTMLInputElement).value;
    ++searchSequence;
    searching = true;
    marketActionMessage = "";
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => void searchMarket(), 160);
  }

  function clearSearch(): void {
    query = "";
    if (searchTimer) clearTimeout(searchTimer);
    void searchMarket();
    searchInput.focus();
  }

  function selectRow(row: MarketSearchRow): void {
    detailTrigger = document.activeElement as HTMLElement | null;
    selectedIdentity = rowIdentity(row);
    marketActionMessage = "";
    liveError = "";
    historyError = "";
    void loadLivePrice(row);
    void revealCompactDetail("detail-heading", "(max-width: 1180px)");
  }

  async function loadLivePrice(row: MarketSearchRow): Promise<void> {
    const identity = rowIdentity(row);
    const requestId = ++liveSequence;
    liveRequestIdentity = identity;
    liveLoading = true;
    liveError = "";
    try {
      const result = await readMarket<LivePricingResult | null>("live_price_current_variant", {
        key: row.recommendation.key,
        itemKind: row.itemKind,
      });
      if (requestId !== liveSequence || identity !== selectedIdentity) return;
      liveResult = result;
      liveIdentity = result ? identity : "";
      if (!result) liveError = "Для этого варианта пока нет текущих предложений. Сохранённая оценка остаётся ориентиром.";
      if (result && historyIdentity === identity) void loadHistory(row);
    } catch (error) {
      if (requestId !== liveSequence || identity !== selectedIdentity) return;
      liveResult = null;
      liveIdentity = "";
      liveError = shell.liveError(String(error));
    } finally {
      if (requestId === liveSequence) liveLoading = false;
    }
  }

  async function loadHistory(
    row: MarketSearchRow,
    requestedRange: 7 | 30 | 90 = historyRange,
  ): Promise<void> {
    const identity = rowIdentity(row);
    const requestId = ++historySequence;
    const recommendation =
      liveResult && liveIdentity === identity ? liveResult.recommendation : row.recommendation;
    historyRequestIdentity = identity;
    historyLoading = true;
    historyError = "";
    try {
      const result = await readMarket<MarketHistoryView>("market_history", {
        key: row.recommendation.key,
        days: requestedRange,
        currentPrice: recommendation.fairPrice,
        liveLowestAsk: recommendation.lowestAsk,
      });
      if (requestId !== historySequence || identity !== selectedIdentity) return;
      historyView = result;
      historyIdentity = identity;
    } catch (error) {
      if (requestId !== historySequence || identity !== selectedIdentity) return;
      historyView = null;
      historyIdentity = "";
      historyError = shell.historyError(String(error));
    } finally {
      if (requestId === historySequence) historyLoading = false;
    }
  }

  function changeHistoryRange(days: 7 | 30 | 90): void {
    historyRange = days;
    if (selectedRow) void loadHistory(selectedRow, days);
  }

  function changeSort(nextKey: MarketSortKey): void {
    if (sortKey === nextKey) {
      sortDirection = sortDirection === "asc" ? "desc" : "asc";
    } else {
      sortKey = nextKey;
      sortDirection = nextKey === "name" ? "asc" : "desc";
    }
  }

  function sortAria(
    key: MarketSortKey,
    activeKey: MarketSortKey,
    direction: SortDirection,
  ): "none" | "ascending" | "descending" {
    if (activeKey !== key) return "none";
    return direction === "asc" ? "ascending" : "descending";
  }

  function sortMarker(
    key: MarketSortKey,
    activeKey: MarketSortKey,
    direction: SortDirection,
  ): string {
    if (activeKey !== key) return "";
    return direction === "asc" ? "↑" : "↓";
  }

  function screenTitle(screen: AppScreen, selectedCopy: typeof shell): string {
    return {
      world_activity: selectedCopy.worldActivity,
      market: selectedCopy.market,
      inventory: selectedCopy.inventory,
      equipped_mods: selectedCopy.equippedMods,
      insights: selectedCopy.insights,
      bounty_hunter: selectedCopy.bountyHunter,
      settings: selectedCopy.settings,
    }[screen];
  }

  function screenLede(screen: AppScreen, selectedCopy: typeof shell): string {
    return {
      world_activity: selectedCopy.worldActivityLede,
      market: selectedCopy.marketLede,
      inventory: selectedCopy.inventoryLede,
      equipped_mods: selectedCopy.equippedModsLede,
      insights: selectedCopy.insightsLede,
      bounty_hunter: selectedCopy.bountyHunterLede,
      settings: selectedCopy.settingsLede,
    }[screen];
  }

  async function loadUiSettings(): Promise<void> {
    try {
      const settings = await invoke<AppSettings>("load_settings");
      locale.set(localeFromLanguage(settings.language));
    } catch {
      locale.set("ru");
    }
  }

  function applySettings(settings: AppSettings): void {
    locale.set(localeFromLanguage(settings.language));
    searchResult = null;
    selectedIdentity = "";
    void loadStatus().then(() => searchMarket());
  }

  onMount(() => {
    let disposed = false;
    let unlistenMarketUpdate: UnlistenFn | undefined;
    let unlistenRewardScreen: UnlistenFn | undefined;
    const savedView = loadMarketViewPreferences();
    priceFilter = savedView.priceFilter;
    sortKey = savedView.sortKey;
    sortDirection = savedView.sortDirection;
    viewPreferencesReady = true;
    const handleShortcut = (event: KeyboardEvent): void => {
      keyboardNavigation = true;
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        activeScreen = "market";
        marketWorkspace = "browse";
        void tick().then(() => {
          searchInput?.focus();
          searchInput?.select();
        });
      }
    };
    const handlePointer = (): void => {
      keyboardNavigation = false;
    };
    window.addEventListener("keydown", handleShortcut);
    window.addEventListener("pointerdown", handlePointer);
    const stopUpdateChecks = startAutomaticUpdateChecks();
    const stopBountyRewardAlerts = startBountyRewardAlerts();
    const stopWorldActivityAlerts = startWorldActivityAlerts();
    void loadUiSettings().then(() => loadStatus()).then(() => searchMarket());
    void listen<MarketRefreshOutcome>("market-data-updated", (event) => {
      refreshOutcome = event.payload;
      void loadStatus().then(() => searchMarket());
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlistenMarketUpdate = cleanup;
    });
    void listen("relic-reward-screen", () => {
      void invoke("scan_relic_rewards", { imagePath: null }).catch(() => undefined);
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlistenRewardScreen = cleanup;
    });
    return () => {
      disposed = true;
      unlistenMarketUpdate?.();
      unlistenRewardScreen?.();
      window.removeEventListener("keydown", handleShortcut);
      window.removeEventListener("pointerdown", handlePointer);
      stopUpdateChecks();
      stopBountyRewardAlerts();
      stopWorldActivityAlerts();
      if (searchTimer) clearTimeout(searchTimer);
    };
  });
</script>

<svelte:head>
  <title>PlatScope — {screenTitle(activeScreen, shell).toLocaleLowerCase(localeCode($locale))}</title>
</svelte:head>

<a class="skip-link" href="#app-content">{shell.skip}</a>

<div class="app-shell" class:sidebar-compact={sidebarCompact}>
  <AppSidebar {activeScreen} labels={navigationLabels} locale={$locale} snapshot={status?.marketSnapshot ?? null}
    pending={loading} unavailable={!status && !loading} bind:compact={sidebarCompact}
    onNavigate={screen => {
      if (screen === "inventory") inventoryInitialQuery = "";
      if (screen === "bounty_hunter") bountyRegion = "all";
      navigateTo(screen);
    }} />

  <main id="app-content" class="app-main">
  <header class="app-header">
    <div class="page-heading">
      <h1 bind:this={pageHeading} tabindex="-1">{screenTitle(activeScreen, shell)}</h1>
      <p class="lede" class:sr-only={["world_activity", "market", "inventory", "insights", "bounty_hunter"].includes(activeScreen)}>{screenLede(activeScreen, shell)}</p>
    </div>
  </header>

  <AppUpdatePanel mode="banner" />

  <div class="screen-body">

  {#key $locale}
  {#if activeScreen === "world_activity"}
    <WorldActivityScreen onOpenSettings={() => navigateTo("settings")}
      onOpenBounties={region => { bountyRegion = region; navigateTo("bounty_hunter"); }}
      onOpenInsights={mode => { saveInsightsViewPreferences({ mode }); navigateTo("insights"); }} />
  {:else if activeScreen === "market"}
  <div class="live-region sr-only" role="status" aria-live="polite">
    {#if loading}
      {shell.openingStorage}
    {:else if refreshOutcome?.stale}
      {shell.providersUnavailable}
    {:else if marketWorkspace === "browse"}
      {resultStatus}
    {/if}
  </div>

  {#if errorMessage}
    <div class="error-block" role="alert">
      <p>{errorMessage}</p>
      <button type="button" onclick={() => void loadStatus().then(searchMarket)}>Повторить загрузку</button>
    </div>
  {/if}

  <section class="market-workspace-switch" aria-label={shell.marketModeLabel}>
    <div class="market-workspace-switch__controls" role="group" aria-label={shell.marketModeLabel}>
      <button
        type="button"
        aria-pressed={marketWorkspace === "sales"}
        onclick={() => (marketWorkspace = "sales")}
      >{shell.mySales}</button>
      <button
        type="button"
        aria-pressed={marketWorkspace === "browse"}
        onclick={() => (marketWorkspace = "browse")}
      >{shell.marketSearch}</button>
    </div>
  </section>

  {#if marketWorkspace === "sales"}
    <MarketTradingShift
      onOpenInventory={() => { inventoryInitialQuery = ""; navigateTo("inventory"); }}
      onBrowseMarket={() => (marketWorkspace = "browse")}
    />
  {:else if !loading && !status?.marketSnapshot}
    <section class="empty-panel" aria-labelledby="empty-heading">
      <p class="empty-panel__label">{shell.noSnapshot}</p>
      <h2 id="empty-heading">{shell.loadMarket}</h2>
      <p>{shell.loadMarketBody}</p>
      <button type="button" onclick={() => navigateTo("settings")}>
        {shell.loadData}
      </button>
    </section>
  {:else if status?.marketSnapshot}
    <div class="market-search-intro"><h2>Узнайте цену нужного предмета</h2><p>Найдите предмет, выберите ранг и сравните цены продавцов и покупателей.</p></div>
    <section class="market-toolbar" aria-labelledby="search-heading">
      <h2 id="search-heading" class="sr-only">{shell.marketFilters}</h2>
      <div class="search-field">
        <label for="market-search">{shell.searchItem}</label>
        <div class="search-control">
          <svg aria-hidden="true" viewBox="0 0 24 24" width="20" height="20">
            <circle cx="11" cy="11" r="7"></circle>
            <path d="m16.5 16.5 4 4"></path>
          </svg>
          <input
            id="market-search"
            bind:this={searchInput}
            value={query}
            oninput={scheduleSearch}
            type="search"
            name="market-search"
            maxlength="80"
            autocomplete="off"
            placeholder={shell.searchExample}
            aria-describedby="search-hint"
          />
          {#if query}
            <button class="clear-button" type="button" onclick={clearSearch}>
              {shell.clear}
            </button>
          {/if}
        </div>
        <p id="search-hint" class="field-hint">{shell.shortcut} <kbd>Ctrl</kbd> + <kbd>K</kbd></p>
      </div>

      <div class="filter-field">
        <label for="price-filter">{shell.priceAvailability}</label>
        <select id="price-filter" bind:value={priceFilter}>
          <option value="all">{shell.allVariants}</option><option value="priced">{shell.priced}</option><option value="unpriced">{shell.unpriced}</option>
        </select>
      </div>
    </section>

    <div class="market-layout">
      <section class="results-panel" aria-labelledby="results-heading" aria-busy={searching}>
        <div class="panel-heading">
          <div>
            <h2 id="results-heading">{shell.results}</h2>
            <p>
              {shell.snapshot} {status.marketSnapshot.sourceDate} · {providerLabel(status.marketSnapshot.provider, $locale)}
            </p>
          </div>
          <span class="result-count">{visibleRows.length}</span>
        </div>

        {#if visibleRows.length}
          <div class="table-wrap">
            <table class="market-table">
              <caption class="sr-only">{shell.marketCaption}</caption>
              <thead>
                <tr>
                  <th scope="col" aria-sort={sortAria("name", sortKey, sortDirection)}>
                    <button type="button" onclick={() => changeSort("name")}>
                      {shell.item} <span aria-hidden="true">{sortMarker("name", sortKey, sortDirection)}</span>
                    </button>
                  </th>
                  <th scope="col" aria-sort={sortAria("fair", sortKey, sortDirection)}>
                    <button type="button" onclick={() => changeSort("fair")}>
                      {$locale === "ru" ? "Оценка / шт." : "Estimate / item"} <span aria-hidden="true">{sortMarker("fair", sortKey, sortDirection)}</span>
                    </button>
                  </th>
                  <th scope="col" aria-sort={sortAria("volume", sortKey, sortDirection)}>
                    <button type="button" onclick={() => changeSort("volume")}>
                      {$locale === "ru" ? "Сделок в данных" : "Recorded trades"} <span aria-hidden="true">{sortMarker("volume", sortKey, sortDirection)}</span>
                    </button>
                  </th>
                </tr>
              </thead>
              <tbody>
                {#each visibleRows as row (rowIdentity(row))}
                  <tr class:selected={rowIdentity(row) === selectedIdentity}>
                    <td data-label={shell.item}>
                      <button class="item-button" type="button" aria-pressed={rowIdentity(row) === selectedIdentity} onclick={() => selectRow(row)}>
                        {#if row.imageUrl}
                          <img class="item-thumb" src={row.imageUrl} alt="" loading="lazy" decoding="async" />
                        {/if}
                        <span class="item-button__copy">
                        <span>{row.displayName}</span>
                        {#if variantLabel(row.recommendation.key, $locale) !== ($locale === "ru" ? "базовый вариант" : "base variant")}<small>{variantLabel(row.recommendation.key, $locale)}</small>{/if}
                        </span>
                      </button>
                    </td>
                    <td class="numeric price-cell" data-label={$locale === "ru" ? "Оценка / шт." : "Estimate / item"}>
                      {row.recommendation.fairPrice === null ? ($locale === "ru" ? "Нет оценки" : "No estimate") : formatPlatinum(row.recommendation.fairPrice, $locale).replace(/p$/, $locale === "ru" ? " пл." : "p")}
                      {#if row.recommendation.freshness !== "fresh"}<small class="estimate-age">{freshnessLabel(row.recommendation.freshness, $locale)}</small>{/if}
                    </td>
                    <td class="numeric" data-label={shell.trades}>
                      {formatVolume(row.recommendation.closedVolume, $locale)}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else if searching}
          <p class="no-results" role="status">{shell.searching}</p>
        {:else if !errorMessage}
          <div class="no-results">
            {#if searchResult?.rows.length}
              <h3>Нет вариантов с выбранной оценкой</h3>
              <p>Предметы найдены, но скрыты фильтром наличия оценки.</p>
              <button type="button" onclick={() => priceFilter = "all"}>Показать все варианты</button>
            {:else if query}
              <h3>{shell.noQuery(query)}</h3><p>{shell.checkSpelling}</p>
              <button type="button" onclick={clearSearch}>{shell.clearSearch}</button>
            {:else}
              <h3>Предметы пока не загружены</h3><p>Обновите данные рынка, чтобы начать поиск.</p>
              <button type="button" onclick={() => navigateTo("settings")}>{shell.loadData}</button>
            {/if}
          </div>
        {/if}
        {#if searchResult?.truncated}
          <p class="result-note">{shell.first60}</p>
        {/if}
      </section>

      <aside class="detail-panel market-detail" aria-labelledby="detail-heading">
        {#if selectedRow && activeRecommendation}
          {#key selectedIdentity}
            <MarketItemDetail row={selectedRow} recommendation={activeRecommendation}
              live={liveIdentity === selectedIdentity ? liveResult : null}
              liveLoading={liveRequestIdentity === selectedIdentity && liveLoading} liveError={liveRequestIdentity === selectedIdentity ? liveError : ""}
              history={historyIdentity === selectedIdentity ? historyView : null}
              historyLoading={historyRequestIdentity === selectedIdentity && historyLoading} historyError={historyRequestIdentity === selectedIdentity ? historyError : ""} {historyRange}
              marketMessage={marketActionMessage}
              onRefresh={() => selectedRow && loadLivePrice(selectedRow)}
              onHistory={changeHistoryRange}
              onMarket={openSelectedMarketItem}
              onInventory={() => { inventoryInitialQuery = selectedRow?.displayName ?? ""; navigateTo("inventory"); }}
              onBack={() => revealElement(detailTrigger?.isConnected ? detailTrigger : document.getElementById("results-heading"))} />
          {/key}
        {:else}
          <div class="detail-placeholder"><h2 id="detail-heading">{shell.itemDetails}</h2><p>{shell.selectItem}</p></div>
        {/if}
      </aside>
    </div>
  {/if}
  {:else if activeScreen === "inventory"}
    <SellNowScreen initialQuery={inventoryInitialQuery}
      onInventoryChange={() => void loadStatus()}
      onOpenMarketSales={openMarketSales}
    />
  {:else if activeScreen === "equipped_mods"}
    <EquippedModsScreen onInventoryChange={() => void loadStatus()} />
  {:else if activeScreen === "insights"}
    <InsightsScreen
      onOpenSettings={() => navigateTo("settings")}
      onOpenMarketSales={openMarketSales}
    />
  {:else if activeScreen === "bounty_hunter"}
    <BountyHunterScreen initialRegion={bountyRegion} onOpenSettings={() => navigateTo("settings")} />
  {:else if activeScreen === "settings"}
    <SettingsScreen bind:this={settingsScreen} onSettingsSaved={applySettings} onMarketRefreshed={handleMarketRefreshed} />
  {/if}
  {/key}
  </div>
  </main>
</div>
