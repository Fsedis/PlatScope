<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";

  import AccountScreen from "./lib/AccountScreen.svelte";
  import AppNavIcon from "./lib/AppNavIcon.svelte";
  import AppUpdatePanel from "./lib/AppUpdatePanel.svelte";
  import DiagnosticsScreen from "./lib/DiagnosticsScreen.svelte";
  import HistoryChart from "./lib/HistoryChart.svelte";
  import InsightsScreen from "./lib/InsightsScreen.svelte";
  import MarketTradingShift from "./lib/MarketTradingShift.svelte";
  import SellNowScreen from "./lib/SellNowScreen.svelte";
  import SettingsScreen from "./lib/SettingsScreen.svelte";
  import { startAutomaticUpdateChecks } from "./lib/appUpdate";

  import {
    providerLabel,
    type FoundationStatus,
    type MarketRefreshOutcome,
  } from "./lib/foundation";
  import {
    formatChange,
    timingLabel,
    type MarketHistoryView,
    type TrendSummary,
  } from "./lib/history";
  import {
    filterAndSortRows,
    formatPlatinum,
    formatVolume,
    freshnessLabel,
    liveQuoteLabel,
    liveUserStatusLabel,
    masteryRequirementLabel,
    priceReasonMessage,
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
    saveMarketViewPreferences,
  } from "./lib/viewPreferences";

  const locale = installLocale("ru");
  const shellCopy = {
    ru: {
      skip: "Перейти к содержимому",
      navLabel: "Разделы приложения",
      market: "Рынок",
      inventory: "Мои предметы",
      insights: "Возможности",
      account: "Warframe Market",
      diagnostics: "Состояние данных",
      settings: "Настройки",
      marketLede: "Мои продажи, актуальность ордеров и поиск цены в одном рабочем месте.",
      inventoryLede: "Торговый инвентарь, момент продажи и ордера Warframe Market в одном месте.",
      insightsLede: "Как выгоднее распорядиться предметами: открыть реликвии, дособрать или продать сет.",
      accountLede: "Подключение аккаунта Warframe Market и состояние доступа.",
      diagnosticsLede: "Что загружено, что устарело и где возникла ошибка.",
      settingsLede: "Язык, платформа и обновление данных.",
      searching: "Ищем в сохранённых данных…", shown: (visible: number, total: number) => `${visible} из ${total} вариантов показано`,
      storageError: (_reason: string) => "Не удалось открыть сохранённые данные. Перезапустите PlatScope.",
      refreshError: (_reason: string) => "Не удалось обновить рынок. Старые данные сохранены. Проверьте подключение и повторите попытку.",
      searchError: (_reason: string) => "Не удалось выполнить поиск. Сократите запрос или повторите попытку.",
      noBulk: "Для этого варианта пока нет сохранённой оценки.", liveError: (_reason: string) => "Не удалось получить текущие цены. Сохранённая оценка не изменилась.",
      historyError: (_reason: string) => "Не удалось загрузить историю. Текущая цена по-прежнему доступна.",
      refreshing: "Обновляем данные…", refresh: "Обновить данные", openingStorage: "Открываем сохранённые данные…", validatingSnapshot: "Загружаем и проверяем новые цены…", providersUnavailable: "Источники временно недоступны. Показываем последние сохранённые данные.", checkStorage: "Проверить данные",
      noSnapshot: "Данные рынка ещё не загружены", loadMarket: "Загрузите цены рынка", loadMarketBody: "Обновление цен и 90-дневной истории находится в настройках.", loadingMarket: "Загружаем данные…", loadData: "Открыть настройки обновления",
      marketFilters: "Поиск и фильтры рынка", searchItem: "Поиск предмета", searchExample: "Например, Никс Прайм или nyx prime", clear: "Очистить", shortcut: "Быстрый доступ:", priceAvailability: "Наличие цены", allVariants: "Все варианты", priced: "С надёжной ценой", unpriced: "Без надёжной цены",
      results: "Результаты", snapshot: "Данные от", marketCaption: "Предметы, цены, продажи и актуальность данных", item: "Предмет", trades: "Сделки", freshness: "Актуальность",
      first60: "Показаны первые 60 вариантов. Уточните запрос, чтобы сократить список.", noQuery: (query: string) => `По запросу «${query}» ничего не найдено`, noFilter: "Для этого фильтра ничего не найдено", checkSpelling: "Проверьте название предмета.", choosePriceFilter: "Выберите другой фильтр цены.", clearSearch: "Очистить поиск",
      relic: "Реликвия", riven: "Мод разлома", marketItem: "Предмет рынка", gettingLive: "Получаем текущие цены…", updateLive: "Обновить текущие цены", getLive: "Проверить текущие цены", liveHint: "Покажет активные ордера для выбранного варианта.", dataDate: "Цена рассчитана по данным от", masteryRequirement: "Ранг мастерства", whyPrice: "Как рассчитана цена?",
      marketData: "Данные рынка", dataReady: "Загружены", dataMissing: "Не загружены",
      fair: "Цена", fairPrice: "Оценка рынка", listPrice: "Ориентир размещения", closedVolume: "Закрытые сделки", lowestAsk: "Минимальная цена продажи", depthThree: "Средняя цена до 3 шт.", depthPrice: "Средняя цена до 5 шт.", quickSell: "Лучшая заявка на покупку", sell: "продажа", buy: "покупка", currentOrders: "Активные ордера", side: "Тип", price: "Цена", quantityLot: "Количество · лот", playerStatus: "Статус", sellOrder: "Продажа", buyOrder: "Покупка", noActiveOrders: "Для этого варианта нет активных ордеров.",
      priceHistory: "История цены", historyRange: "Период", dayShort: "д", loadingHistory: "Загружаем историю…", historyCoverage: (points: number, coverage: number) => `${points} дней · доступно ${coverage} дней истории`, selectForHistory: "Выберите строку, чтобы посмотреть историю цены.", median: "Медиана", change: "Изменение", averageVolume: "Средний объём", insufficientChart: "Пока недостаточно данных для графика. История накопится после обновлений рынка.", itemDetails: "Подробности предмета", selectItem: "Выберите предмет в таблице, чтобы увидеть цену и расчёт.",
      marketModeLabel: "Режим рынка", mySales: "Мои продажи", marketSearch: "Поиск рынка",
    },
    en: {
      skip: "Skip to content",
      navLabel: "Application sections",
      market: "Market",
      inventory: "My items",
      insights: "Opportunities",
      account: "Warframe Market",
      diagnostics: "Data status",
      settings: "Settings",
      marketLede: "Your sales, order health, and price research in one workspace.",
      inventoryLede: "Market inventory, sell timing, and Warframe Market orders in one place.",
      insightsLede: "Use owned relics, finish profitable sets, or list complete ones.",
      accountLede: "Warframe Market account connection and access status.",
      diagnosticsLede: "Provider, local cache, and data coverage status without reading terminal logs.",
      settingsLede: "Language, market platform, and data refresh controls.",
      searching: "Searching saved data…", shown: (visible: number, total: number) => `${visible} of ${total} variants shown`,
      storageError: (_reason: string) => "Unable to open saved data. Restart PlatScope.",
      refreshError: (_reason: string) => "Unable to refresh the market. Saved data was preserved. Check the connection and try again.",
      searchError: (_reason: string) => "Unable to search. Shorten the query or try again.",
      noBulk: "No saved estimate exists for this variant.", liveError: (_reason: string) => "Unable to retrieve current prices. The saved estimate was preserved.",
      historyError: (_reason: string) => "Unable to load history. The current price remains available.",
      refreshing: "Refreshing data…", refresh: "Refresh data", openingStorage: "Opening saved data…", validatingSnapshot: "Downloading and checking new prices…", providersUnavailable: "Sources are temporarily unavailable. Showing the latest saved data.", checkStorage: "Check data",
      noSnapshot: "Market data has not been loaded", loadMarket: "Load market prices", loadMarketBody: "Price and 90-day history updates are available in Settings.", loadingMarket: "Loading data…", loadData: "Open update settings",
      marketFilters: "Market search and filters", searchItem: "Search items", searchExample: "For example, Nyx Prime or nyx prime", clear: "Clear", shortcut: "Shortcut:", priceAvailability: "Price availability", allVariants: "All variants", priced: "Reliable price", unpriced: "No reliable price",
      results: "Results", snapshot: "Data from", marketCaption: "Market items, prices, sales, and data freshness", item: "Item", trades: "Trades", freshness: "Freshness",
      first60: "Showing the first 60 variants. Refine the query to narrow the list.", noQuery: (query: string) => `No results for “${query}”`, noFilter: "No variants match this filter", checkSpelling: "Check the spelling or use a canonical slug.", choosePriceFilter: "Choose a different price filter.", clearSearch: "Clear search",
      relic: "Relic", riven: "Riven mod", marketItem: "Market item", gettingLive: "Getting current prices…", updateLive: "Refresh current prices", getLive: "Check current prices", liveHint: "Shows active orders for the selected variant.", dataDate: "Price data from", masteryRequirement: "Mastery rank", whyPrice: "How is this price calculated?",
      marketData: "Market data", dataReady: "Loaded", dataMissing: "Not loaded",
      fair: "Fair", fairPrice: "Fair price", listPrice: "List price", closedVolume: "Closed volume", lowestAsk: "Lowest ask", depthThree: "Up to 3 units average", depthPrice: "Up to 5 units average", quickSell: "Quick Sell", sell: "sell", buy: "buy", currentOrders: "Current active orders", side: "Side", price: "Price", quantityLot: "Quantity · lot", playerStatus: "Player status", sellOrder: "Sell", buyOrder: "Buy", noActiveOrders: "No active orders are available for the exact variant.",
      priceHistory: "Price history", historyRange: "History range", dayShort: "d", loadingHistory: "Loading local aggregates…", historyCoverage: (points: number, coverage: number) => `${points} days for this variant · ${coverage} local days covered`, selectForHistory: "Select a row to open compact history for the exact variant.", median: "Median", change: "Change", averageVolume: "Average volume", insufficientChart: "Not enough points for a chart. Background bootstrap adds up to seven days per launch.", itemDetails: "Item details", selectItem: "Select an item in the table to see its calculation and explanation.",
      marketModeLabel: "Market mode", mySales: "My sales", marketSearch: "Market search",
    },
  } as const;

  $: shell = shellCopy[$locale];

  let status: FoundationStatus | null = null;
  let refreshOutcome: MarketRefreshOutcome | null = null;
  let searchResult: MarketSearchResult | null = null;
  let liveResult: LivePricingResult | null = null;
  let liveIdentity = "";
  let historyView: MarketHistoryView | null = null;
  let historyIdentity = "";
  let historyRange: 7 | 30 | 90 = 7;
  type AppScreen =
    | "market"
    | "inventory"
    | "insights"
    | "account"
    | "diagnostics"
    | "settings";
  type MarketWorkspace = "sales" | "browse";

  let activeScreen: AppScreen = "inventory";
  let marketWorkspace: MarketWorkspace = "sales";
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

  function navigateTo(screen: AppScreen): void {
    activeScreen = screen;
    void tick().then(() => {
      window.scrollTo({ top: 0, left: 0 });
      if (keyboardNavigation) pageHeading?.focus();
    });
  }

  $: visibleRows = filterAndSortRows(
    searchResult?.rows ?? [],
    priceFilter,
    sortKey,
    sortDirection,
  );
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
      const result = await invoke<MarketSearchResult>("search_market", {
        query: requestedQuery,
        limit: 60,
      });
      if (requestId !== searchSequence) return;
      searchResult = result;
      const selectedStillExists = result.rows.some(
        (row) => rowIdentity(row) === selectedIdentity,
      );
      if (!selectedStillExists) {
        selectedIdentity = result.rows[0] ? rowIdentity(result.rows[0]) : "";
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
    selectedIdentity = rowIdentity(row);
    void loadLivePrice(row);
    void loadHistory(row);
  }

  async function loadLivePrice(row: MarketSearchRow): Promise<void> {
    const identity = rowIdentity(row);
    const requestId = ++liveSequence;
    liveLoading = true;
    liveError = "";
    try {
      const result = await invoke<LivePricingResult | null>("live_price_current_variant", {
        key: row.recommendation.key,
        itemKind: row.itemKind,
      });
      if (requestId !== liveSequence || identity !== selectedIdentity) return;
      liveResult = result;
      liveIdentity = result ? identity : "";
      if (!result) liveError = shell.noBulk;
      if (result) void loadHistory(row);
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
    historyLoading = true;
    historyError = "";
    try {
      const result = await invoke<MarketHistoryView>("market_history", {
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

  function trendMedian(trend: TrendSummary, days: 7 | 30 | 90): number | null {
    return days === 7 ? trend.median7d : days === 30 ? trend.median30d : trend.median90d;
  }

  function trendChange(trend: TrendSummary, days: 7 | 30 | 90): number | null {
    return days === 7 ? trend.change7d : days === 30 ? trend.change30d : trend.change90d;
  }

  function trendVolume(trend: TrendSummary, days: 7 | 30 | 90): number | null {
    return days === 7 ? trend.volumeAvg7d : days === 30 ? trend.volumeAvg30d : trend.volumeAvg90d;
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
      market: selectedCopy.market,
      inventory: selectedCopy.inventory,
      insights: selectedCopy.insights,
      account: selectedCopy.account,
      diagnostics: selectedCopy.diagnostics,
      settings: selectedCopy.settings,
    }[screen];
  }

  function screenLede(screen: AppScreen, selectedCopy: typeof shell): string {
    return {
      market: selectedCopy.marketLede,
      inventory: selectedCopy.inventoryLede,
      insights: selectedCopy.insightsLede,
      account: selectedCopy.accountLede,
      diagnostics: selectedCopy.diagnosticsLede,
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
      if (searchTimer) clearTimeout(searchTimer);
    };
  });
</script>

<svelte:head>
  <title>PlatScope — {screenTitle(activeScreen, shell).toLocaleLowerCase(localeCode($locale))}</title>
</svelte:head>

<a class="skip-link" href="#app-content">{shell.skip}</a>

<div class="app-shell">
  <aside class="app-sidebar">
    <div class="app-brand">
      <svg class="app-brand__mark" viewBox="0 0 32 32" aria-hidden="true">
        <path d="M16 2 28 9v14l-12 7L4 23V9z" />
        <path d="m10 21 6-14 6 14-6-4z" />
      </svg>
      <span class="app-brand__copy"><strong>PlatScope</strong><small>Warframe Market</small></span>
    </div>

    <nav class="section-tabs" aria-label={shell.navLabel}>
        <button
          type="button"
          class:active={activeScreen === "market"}
          aria-current={activeScreen === "market" ? "page" : undefined}
          onclick={() => navigateTo("market")}
        ><AppNavIcon screen="market" /><span>{shell.market}</span></button>
        <button
          type="button"
          class:active={activeScreen === "inventory"}
          aria-current={activeScreen === "inventory" ? "page" : undefined}
          onclick={() => navigateTo("inventory")}
        ><AppNavIcon screen="inventory" /><span>{shell.inventory}</span></button>
        <button
          type="button"
          class:active={activeScreen === "insights"}
          aria-current={activeScreen === "insights" ? "page" : undefined}
          onclick={() => navigateTo("insights")}
        ><AppNavIcon screen="insights" /><span>{shell.insights}</span></button>
        <button
          type="button"
          class:active={activeScreen === "account"}
          aria-current={activeScreen === "account" ? "page" : undefined}
          onclick={() => navigateTo("account")}
        ><AppNavIcon screen="account" /><span>{shell.account}</span></button>
        <button
          type="button"
          class:active={activeScreen === "diagnostics"}
          aria-current={activeScreen === "diagnostics" ? "page" : undefined}
          onclick={() => navigateTo("diagnostics")}
        ><AppNavIcon screen="diagnostics" /><span>{shell.diagnostics}</span></button>
        <button
          type="button"
          class:active={activeScreen === "settings"}
          aria-current={activeScreen === "settings" ? "page" : undefined}
          onclick={() => navigateTo("settings")}
        ><AppNavIcon screen="settings" /><span>{shell.settings}</span></button>
    </nav>

    <div class="sidebar-status">
      <span class:ready={Boolean(status?.marketSnapshot)} aria-hidden="true"></span>
      <div><strong>{shell.marketData}</strong><small>{status?.marketSnapshot ? `${shell.dataReady} · ${status.marketSnapshot.sourceDate}` : shell.dataMissing}</small></div>
    </div>
  </aside>

  <main id="app-content" class="app-main">
  <header class="app-header">
    <div class="page-heading">
      <h1 bind:this={pageHeading} tabindex="-1">{screenTitle(activeScreen, shell)}</h1>
      <p class="lede">{screenLede(activeScreen, shell)}</p>
    </div>
  </header>

  <AppUpdatePanel mode="banner" />

  <div class="screen-body">

  {#key $locale}
  {#if activeScreen === "market"}
  <div class="live-region" role="status" aria-live="polite">
    {#if loading}
      {shell.openingStorage}
    {:else if refreshOutcome?.stale}
      {shell.providersUnavailable}
    {:else}
      {resultStatus}
    {/if}
  </div>

  {#if errorMessage}
    <div class="error-block" role="alert">
      <p>{errorMessage}</p>
      <button type="button" onclick={loadStatus}>{shell.checkStorage}</button>
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
      onOpenAccount={() => navigateTo("account")}
      onOpenInventory={() => navigateTo("inventory")}
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
            <table>
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
                      {shell.fair} <span aria-hidden="true">{sortMarker("fair", sortKey, sortDirection)}</span>
                    </button>
                  </th>
                  <th scope="col" aria-sort={sortAria("volume", sortKey, sortDirection)}>
                    <button type="button" onclick={() => changeSort("volume")}>
                      {shell.trades} <span aria-hidden="true">{sortMarker("volume", sortKey, sortDirection)}</span>
                    </button>
                  </th>
                  <th scope="col">{shell.freshness}</th>
                </tr>
              </thead>
              <tbody>
                {#each visibleRows as row (rowIdentity(row))}
                  <tr class:selected={rowIdentity(row) === selectedIdentity}>
                    <td data-label={shell.item}>
                      <button class="item-button" type="button" onclick={() => selectRow(row)}>
                        {#if row.imageUrl}
                          <img class="item-thumb" src={row.imageUrl} alt="" loading="lazy" decoding="async" />
                        {/if}
                        <span class="item-button__copy">
                        <span>{row.displayName}</span>
                        <small>{variantLabel(row.recommendation.key, $locale)}</small>
                        </span>
                      </button>
                    </td>
                    <td class="numeric price-cell" data-label={shell.fair}>
                      {formatPlatinum(row.recommendation.fairPrice, $locale)}
                    </td>
                    <td class="numeric" data-label={shell.trades}>
                      {formatVolume(row.recommendation.closedVolume, $locale)}
                    </td>
                    <td data-label={shell.freshness}>
                      <span class={`freshness freshness--${row.recommendation.freshness}`}>
                        {freshnessLabel(row.recommendation.freshness, $locale)}
                      </span>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
          {#if searchResult?.truncated}
            <p class="result-note">{shell.first60}</p>
          {/if}
        {:else if !searching}
          <div class="no-results">
            <h3>{query ? shell.noQuery(query) : shell.noFilter}</h3>
            <p>{query ? shell.checkSpelling : shell.choosePriceFilter}</p>
            {#if query}
              <button type="button" onclick={clearSearch}>{shell.clearSearch}</button>
            {/if}
          </div>
        {/if}
      </section>

      <aside class="detail-panel" aria-labelledby="detail-heading">
        {#if selectedRow && activeRecommendation}
          <div class="detail-heading">
            {#if selectedRow.imageUrl}<img class="detail-art" src={selectedRow.imageUrl} alt="" decoding="async" />{/if}
            <p>{selectedRow.itemKind === "relic" ? shell.relic : selectedRow.itemKind === "riven" ? shell.riven : shell.marketItem}</p>
            <h2 id="detail-heading">{selectedRow.displayName}</h2>
            <span>{variantLabel(selectedRow.recommendation.key, $locale)}</span>
          </div>

          <div class="live-actions">
            <button
              type="button"
              disabled={liveLoading}
              onclick={() => loadLivePrice(selectedRow)}
            >
              {liveLoading ? shell.gettingLive : liveResult && liveIdentity === selectedIdentity ? shell.updateLive : shell.getLive}
            </button>
            <div class="live-status" aria-live="polite">
              {#if liveResult && liveIdentity === selectedIdentity}
                <span>{liveQuoteLabel(liveResult.quoteState, $locale)} · {liveResult.sellOrderCount} {shell.sell} / {liveResult.buyOrderCount} {shell.buy}</span>
                {#if liveResult.warning}
                  <strong>{$locale === "ru" ? "Часть текущих ордеров недоступна. Проверьте список перед продажей." : "Some current orders are unavailable. Review the list before selling."}</strong>
                {/if}
              {:else if liveError}
                <strong>{liveError}</strong>
              {:else}
                <span>{shell.liveHint}</span>
              {/if}
            </div>
          </div>

          <dl class="price-grid">
            <div class="price-grid__primary">
              <dt>{shell.fairPrice}</dt>
              <dd>{formatPlatinum(activeRecommendation.fairPrice, $locale)}</dd>
            </div>
            <div>
              <dt>{shell.listPrice}</dt>
              <dd>{formatPlatinum(activeRecommendation.listPrice, $locale)}</dd>
            </div>
            <div>
              <dt>{shell.closedVolume}</dt>
              <dd>{formatVolume(activeRecommendation.closedVolume, $locale)}</dd>
            </div>
          </dl>

          {#if liveResult && liveIdentity === selectedIdentity}
            <dl class="live-price-grid">
              <div><dt>{shell.lowestAsk}</dt><dd>{formatPlatinum(activeRecommendation.lowestAsk, $locale)}</dd></div>
              <div><dt>{shell.depthThree}</dt><dd>{formatPlatinum(activeRecommendation.depthThree, $locale)}</dd></div>
              <div><dt>{shell.depthPrice}</dt><dd>{formatPlatinum(activeRecommendation.depthPrice, $locale)}</dd></div>
              <div><dt>{shell.quickSell}</dt><dd>{formatPlatinum(activeRecommendation.quickSell, $locale)}</dd></div>
            </dl>
            <section class="live-orders" aria-labelledby="live-orders-heading">
              <h3 id="live-orders-heading">{shell.currentOrders}</h3>
              <div class="live-orders__scroll">
                <table>
                  <thead><tr><th>{shell.side}</th><th>{shell.price}</th><th>{shell.quantityLot}</th><th>{shell.playerStatus}</th></tr></thead>
                  <tbody>
                    {#each liveResult.orders as order, index (`${order.side}:${order.platinum}:${order.quantity}:${index}`)}
                      <tr>
                        <th scope="row"><span class={`order-side order-side--${order.side}`}>{order.side === "sell" ? shell.sellOrder : shell.buyOrder}</span></th>
                        <td>{formatPlatinum(order.platinum, $locale)}</td>
                        <td>{order.quantity.toLocaleString(localeCode($locale))} · {order.perTrade.toLocaleString(localeCode($locale))}</td>
                        <td>{liveUserStatusLabel(order.userStatus, $locale)}</td>
                      </tr>
                    {:else}
                      <tr><td colspan="4">{shell.noActiveOrders}</td></tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            </section>
          {/if}

          <div class="detail-meta">
            <div>
              <span>{shell.freshness}</span>
              <strong>{freshnessLabel(activeRecommendation.freshness, $locale)}</strong>
            </div>
            <div>
              <span>{shell.dataDate}</span>
              <strong>{activeRecommendation.sourceDate}</strong>
            </div>
            <div>
              <span>{shell.masteryRequirement}</span>
              <strong>{masteryRequirementLabel(selectedRow.masteryRequirement, $locale)}</strong>
            </div>
          </div>

          <details class="explanation" open>
            <summary>{shell.whyPrice}</summary>
            <ul>
              {#each activeRecommendation.reasons as reason}
                <li>{priceReasonMessage(reason, $locale)}</li>
              {/each}
            </ul>
          </details>

          <section class="history-panel" aria-labelledby="history-heading">
            <div class="history-heading">
              <h3 id="history-heading">{shell.priceHistory}</h3>
              <div class="history-ranges" role="group" aria-label={shell.historyRange}>
                {#each [7, 30, 90] as days}
                  <button
                    type="button"
                    aria-pressed={historyRange === days}
                    onclick={() => changeHistoryRange(days as 7 | 30 | 90)}
                  >{days}{shell.dayShort}</button>
                {/each}
              </div>
            </div>

            <div class="history-status" aria-live="polite">
              {#if historyLoading}
                {shell.loadingHistory}
              {:else if historyError}
                <strong>{historyError}</strong>
              {:else if historyView && historyIdentity === selectedIdentity}
                {shell.historyCoverage(historyView.points.length, historyView.coverage.dayCount)}
              {:else}
                {shell.selectForHistory}
              {/if}
            </div>

            {#if historyView && historyIdentity === selectedIdentity}
              <dl class="trend-grid">
                <div><dt>{shell.median} {historyRange}{shell.dayShort}</dt><dd>{formatPlatinum(trendMedian(historyView.trend, historyRange), $locale)}</dd></div>
                <div><dt>{shell.change}</dt><dd>{formatChange(trendChange(historyView.trend, historyRange), $locale)}</dd></div>
                <div><dt>{shell.averageVolume}</dt><dd>{formatVolume(trendVolume(historyView.trend, historyRange), $locale)}</dd></div>
              </dl>
              {#if historyView.trend.timing}
                <p class={`timing timing--${historyView.trend.timing}`}>
                  {timingLabel(historyView.trend.timing, $locale)}
                </p>
              {/if}
              {#if historyView.points.length >= 2}
                <HistoryChart points={historyView.points} />
              {:else}
                <p class="history-empty">{shell.insufficientChart}</p>
              {/if}
            {/if}
          </section>

        {:else}
          <div class="detail-placeholder">
            <h2 id="detail-heading">{shell.itemDetails}</h2>
            <p>{shell.selectItem}</p>
          </div>
        {/if}
      </aside>
    </div>
  {/if}
  {:else if activeScreen === "inventory"}
    <SellNowScreen
      onInventoryChange={() => void loadStatus()}
      onOpenAccount={() => navigateTo("account")}
    />
  {:else if activeScreen === "insights"}
    <InsightsScreen
      onOpenSettings={() => navigateTo("settings")}
      onOpenAccount={() => navigateTo("account")}
    />
  {:else if activeScreen === "account"}
    <AccountScreen onOpenMarketSales={() => { marketWorkspace = "sales"; navigateTo("market"); }} />
  {:else if activeScreen === "diagnostics"}
    <DiagnosticsScreen />
  {:else if activeScreen === "settings"}
    <SettingsScreen onSettingsSaved={applySettings} onMarketRefreshed={handleMarketRefreshed} />
  {/if}
  {/key}
  </div>
  </main>
</div>
