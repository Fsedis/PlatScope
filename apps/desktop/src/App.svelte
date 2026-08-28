<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import AccountScreen from "./lib/AccountScreen.svelte";
  import AppNavIcon from "./lib/AppNavIcon.svelte";
  import DashboardScreen from "./lib/DashboardScreen.svelte";
  import DiagnosticsScreen from "./lib/DiagnosticsScreen.svelte";
  import HistoryChart from "./lib/HistoryChart.svelte";
  import InsightsScreen from "./lib/InsightsScreen.svelte";
  import InventoryHubScreen from "./lib/InventoryHubScreen.svelte";
  import SettingsScreen from "./lib/SettingsScreen.svelte";

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
    confidenceLabel,
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
      dashboard: "Обзор",
      market: "Рынок",
      inventory: "Инвентарь",
      insights: "Аналитика",
      account: "Аккаунт WFM",
      diagnostics: "Диагностика",
      settings: "Настройки",
      dashboardLede: "Номинальная стоимость, лучшие кандидаты, слабая ликвидность и свежесть локальных данных.",
      marketLede: "Локальный поиск по проверенному снимку — без сетевого запроса на каждую строку.",
      inventoryLede: "Предметы, количество, цены и готовность к продаже в одном разделе.",
      insightsLede: "Prime sets, relic EV и ducat efficiency по локальным данным с явным покрытием цен.",
      accountLede: "Подключение по желанию, защищённый токен и только явно подтверждённые операции с ордерами.",
      diagnosticsLede: "Состояние источников, локального кэша и покрытия данных без чтения terminal logs.",
      settingsLede: "Язык, рыночная платформа и параметры обновления данных.",
      searching: "Ищем в локальном снимке…", shown: (visible: number, total: number) => `${visible} из ${total} вариантов показано`,
      storageError: (reason: string) => `Не удалось открыть локальное хранилище. Перезапустите PlatScope. Техническая причина: ${reason}`,
      refreshError: (reason: string) => `Обновление не завершено. Сохранённые данные не изменены. Проверьте подключение и повторите попытку. Техническая причина: ${reason}`,
      searchError: (reason: string) => `Поиск не выполнен. Сократите запрос или повторите попытку. Техническая причина: ${reason}`,
      noBulk: "Для этого варианта нет текущего bulk snapshot.", liveError: (reason: string) => `Live-цены недоступны; bulk-оценка сохранена. ${reason}`,
      historyError: (reason: string) => `История не загрузилась; текущая цена доступна. ${reason}`,
      refreshing: "Обновляем данные…", refresh: "Обновить данные", openingStorage: "Открываем локальное хранилище…", validatingSnapshot: "Загружаем и проверяем новый снимок…", providersUnavailable: "Провайдеры недоступны. Используется последний корректный снимок.", checkStorage: "Проверить хранилище",
      noSnapshot: "Локальный снимок не найден", loadMarket: "Загрузите рыночные данные", loadMarketBody: "PlatScope проверит каталог и цены, а затем сохранит их для работы без сети.", loadingMarket: "Загружаем данные…", loadData: "Загрузить данные",
      marketFilters: "Поиск и фильтры рынка", searchItem: "Поиск предмета", searchExample: "Например, Никс Прайм или nyx prime", clear: "Очистить", shortcut: "Быстрый доступ:", priceAvailability: "Наличие цены", allVariants: "Все варианты", priced: "С надёжной ценой", unpriced: "Без надёжной цены",
      results: "Результаты", snapshot: "Снимок", marketCaption: "Рыночные варианты и рассчитанные bulk-цены", item: "Предмет", trades: "Сделки", confidence: "Уверенность", freshness: "Свежесть",
      first60: "Показаны первые 60 вариантов. Уточните запрос, чтобы сузить список.", noQuery: (query: string) => `Ничего не найдено по запросу «${query}»`, noFilter: "Нет вариантов для этого фильтра", checkSpelling: "Проверьте написание или используйте canonical slug.", choosePriceFilter: "Выберите другой фильтр цены.", clearSearch: "Очистить поиск",
      relic: "Реликвия", riven: "Riven — отдельная модель", marketItem: "Рыночный предмет", gettingLive: "Получаем live-цены…", updateLive: "Обновить live-цены", getLive: "Получить live-цены", liveHint: "Один запрос к top orders для точного варианта; результат кэшируется на 90 секунд.", dataDate: "Дата данных", masteryRequirement: "Требование мастерства", whyPrice: "Почему такая цена?",
      fair: "Справедливая", fairPrice: "Справедливая цена", listPrice: "Цена размещения", closedVolume: "Объём закрытых сделок", lowestAsk: "Минимальная продажа", depthThree: "Средняя до 3 ед.", depthPrice: "Средняя до 5 ед.", quickSell: "Быстрая продажа", sell: "продажа", buy: "покупка", currentOrders: "Текущие активные ордера", side: "Сторона", price: "Цена", quantityLot: "Количество · лот", playerStatus: "Статус игрока", sellOrder: "Продажа", buyOrder: "Покупка", noActiveOrders: "Активных ордеров для точного варианта нет.",
      priceHistory: "История цены", historyRange: "Диапазон истории", dayShort: "д", loadingHistory: "Загружаем локальные агрегаты…", historyCoverage: (points: number, coverage: number) => `${points} дней для варианта · локальное покрытие ${coverage} дней`, selectForHistory: "Выберите строку, чтобы открыть компактную историю точного варианта.", median: "Медиана", change: "Изменение", averageVolume: "Средний объём", insufficientChart: "Недостаточно точек для графика. Фоновый bootstrap добавляет до семи дней за запуск.", itemDetails: "Подробности предмета", selectItem: "Выберите предмет в таблице, чтобы увидеть расчёт и объяснение.",
    },
    en: {
      skip: "Skip to content",
      navLabel: "Application sections",
      dashboard: "Overview",
      market: "Market",
      inventory: "Inventory",
      insights: "Insights",
      account: "WFM account",
      diagnostics: "Diagnostics",
      settings: "Settings",
      dashboardLede: "Nominal value, top candidates, weak liquidity, and local data freshness.",
      marketLede: "Search a validated local snapshot without a network request for every row.",
      inventoryLede: "Items, quantities, prices, and sell readiness in one section.",
      insightsLede: "Prime sets, relic EV, and ducat efficiency from local data with explicit price coverage.",
      accountLede: "Optional connection, protected credentials, and explicitly confirmed order changes only.",
      diagnosticsLede: "Provider, local cache, and data coverage status without reading terminal logs.",
      settingsLede: "Language, market platform, and data refresh controls.",
      searching: "Searching the local snapshot…", shown: (visible: number, total: number) => `${visible} of ${total} variants shown`,
      storageError: (reason: string) => `Unable to open local storage. Restart PlatScope. Technical reason: ${reason}`,
      refreshError: (reason: string) => `Refresh did not complete. Saved data was not changed. Check the connection and try again. Technical reason: ${reason}`,
      searchError: (reason: string) => `Search failed. Shorten the query or try again. Technical reason: ${reason}`,
      noBulk: "No current bulk snapshot exists for this variant.", liveError: (reason: string) => `Live prices unavailable; the bulk estimate was preserved. ${reason}`,
      historyError: (reason: string) => `History failed to load; the current price remains available. ${reason}`,
      refreshing: "Refreshing data…", refresh: "Refresh data", openingStorage: "Opening local storage…", validatingSnapshot: "Downloading and validating a new snapshot…", providersUnavailable: "Providers are unavailable. Using the latest valid snapshot.", checkStorage: "Check storage",
      noSnapshot: "No local snapshot", loadMarket: "Load market data", loadMarketBody: "PlatScope validates the catalog and prices, then saves them for offline use.", loadingMarket: "Loading data…", loadData: "Load data",
      marketFilters: "Market search and filters", searchItem: "Search items", searchExample: "For example, Nyx Prime or nyx prime", clear: "Clear", shortcut: "Shortcut:", priceAvailability: "Price availability", allVariants: "All variants", priced: "Reliable price", unpriced: "No reliable price",
      results: "Results", snapshot: "Snapshot", marketCaption: "Market variants and calculated bulk prices", item: "Item", trades: "Trades", confidence: "Confidence", freshness: "Freshness",
      first60: "Showing the first 60 variants. Refine the query to narrow the list.", noQuery: (query: string) => `No results for “${query}”`, noFilter: "No variants match this filter", checkSpelling: "Check the spelling or use a canonical slug.", choosePriceFilter: "Choose a different price filter.", clearSearch: "Clear search",
      relic: "Relic", riven: "Riven — separate model", marketItem: "Market item", gettingLive: "Getting live prices…", updateLive: "Refresh live prices", getLive: "Get live prices", liveHint: "One top-orders request for the exact variant; the result is cached for 90 seconds.", dataDate: "Data date", masteryRequirement: "Mastery requirement", whyPrice: "Why this price?",
      fair: "Fair", fairPrice: "Fair price", listPrice: "List price", closedVolume: "Closed volume", lowestAsk: "Lowest ask", depthThree: "Up to 3 units average", depthPrice: "Up to 5 units average", quickSell: "Quick Sell", sell: "sell", buy: "buy", currentOrders: "Current active orders", side: "Side", price: "Price", quantityLot: "Quantity · lot", playerStatus: "Player status", sellOrder: "Sell", buyOrder: "Buy", noActiveOrders: "No active orders are available for the exact variant.",
      priceHistory: "Price history", historyRange: "History range", dayShort: "d", loadingHistory: "Loading local aggregates…", historyCoverage: (points: number, coverage: number) => `${points} days for this variant · ${coverage} local days covered`, selectForHistory: "Select a row to open compact history for the exact variant.", median: "Median", change: "Change", averageVolume: "Average volume", insufficientChart: "Not enough points for a chart. Background bootstrap adds up to seven days per launch.", itemDetails: "Item details", selectItem: "Select an item in the table to see its calculation and explanation.",
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
    | "dashboard"
    | "market"
    | "inventory"
    | "insights"
    | "account"
    | "diagnostics"
    | "settings";

  type InventoryMode = "all" | "sell";

  let activeScreen: AppScreen = "inventory";
  let inventoryMode: InventoryMode = "sell";
  let selectedIdentity = "";
  let query = "";
  let priceFilter: PriceFilter = "all";
  let sortKey: MarketSortKey = "volume";
  let sortDirection: SortDirection = "desc";
  let viewPreferencesReady = false;
  let errorMessage = "";
  let loading = true;
  let refreshing = false;
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

  async function refreshMarketData(): Promise<void> {
    refreshing = true;
    errorMessage = "";
    refreshOutcome = null;
    try {
      refreshOutcome = await invoke<MarketRefreshOutcome>("refresh_market_data");
      await loadStatus();
      await searchMarket();
    } catch (error) {
      errorMessage = shell.refreshError(String(error));
    } finally {
      refreshing = false;
    }
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
    return days === 7 ? trend.change7d : days === 30 ? trend.change30d : null;
  }

  function trendVolume(trend: TrendSummary, days: 7 | 30 | 90): number | null {
    return days === 7 ? trend.volumeAvg7d : days === 30 ? trend.volumeAvg30d : null;
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
      dashboard: selectedCopy.dashboard,
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
      dashboard: selectedCopy.dashboardLede,
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
    const savedView = loadMarketViewPreferences();
    priceFilter = savedView.priceFilter;
    sortKey = savedView.sortKey;
    sortDirection = savedView.sortDirection;
    viewPreferencesReady = true;
    const handleShortcut = (event: KeyboardEvent): void => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        searchInput?.focus();
        searchInput?.select();
      }
    };
    window.addEventListener("keydown", handleShortcut);
    void loadUiSettings().then(() => loadStatus()).then(() => searchMarket());
    void listen<MarketRefreshOutcome>("market-data-updated", (event) => {
      refreshOutcome = event.payload;
      void loadStatus().then(() => searchMarket());
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlistenMarketUpdate = cleanup;
    });
    return () => {
      disposed = true;
      unlistenMarketUpdate?.();
      window.removeEventListener("keydown", handleShortcut);
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
      <span class="app-brand__copy"><strong>PlatScope</strong><small>Warframe Intel</small></span>
    </div>

    <nav class="section-tabs" aria-label={shell.navLabel}>
        <button
          type="button"
          class:active={activeScreen === "dashboard"}
          aria-current={activeScreen === "dashboard" ? "page" : undefined}
          onclick={() => (activeScreen = "dashboard")}
        ><AppNavIcon screen="dashboard" /><span>{shell.dashboard}</span></button>
        <button
          type="button"
          class:active={activeScreen === "market"}
          aria-current={activeScreen === "market" ? "page" : undefined}
          onclick={() => (activeScreen = "market")}
        ><AppNavIcon screen="market" /><span>{shell.market}</span></button>
        <button
          type="button"
          class:active={activeScreen === "inventory"}
          aria-current={activeScreen === "inventory" ? "page" : undefined}
          onclick={() => (activeScreen = "inventory")}
        ><AppNavIcon screen="inventory" /><span>{shell.inventory}</span></button>
        <button
          type="button"
          class:active={activeScreen === "insights"}
          aria-current={activeScreen === "insights" ? "page" : undefined}
          onclick={() => (activeScreen = "insights")}
        ><AppNavIcon screen="insights" /><span>{shell.insights}</span></button>
        <button
          type="button"
          class:active={activeScreen === "account"}
          aria-current={activeScreen === "account" ? "page" : undefined}
          onclick={() => (activeScreen = "account")}
        ><AppNavIcon screen="account" /><span>{shell.account}</span></button>
        <button
          type="button"
          class:active={activeScreen === "diagnostics"}
          aria-current={activeScreen === "diagnostics" ? "page" : undefined}
          onclick={() => (activeScreen = "diagnostics")}
        ><AppNavIcon screen="diagnostics" /><span>{shell.diagnostics}</span></button>
        <button
          type="button"
          class:active={activeScreen === "settings"}
          aria-current={activeScreen === "settings" ? "page" : undefined}
          onclick={() => (activeScreen = "settings")}
        ><AppNavIcon screen="settings" /><span>{shell.settings}</span></button>
    </nav>

    <div class="sidebar-status">
      <span class:ready={Boolean(status?.marketSnapshot)} aria-hidden="true"></span>
      <div><strong>WF DATA</strong><small>{status?.marketSnapshot?.sourceDate ?? "offline"}</small></div>
    </div>
  </aside>

  <main id="app-content" class="app-main">
  <header class="app-header">
    <div class="page-heading">
      <p class="eyebrow">Warframe Market Intelligence</p>
      <h1>{screenTitle(activeScreen, shell)}</h1>
      <p class="lede">{screenLede(activeScreen, shell)}</p>
    </div>
    <div class="app-header__actions">
      <span class:ready={Boolean(status?.marketSnapshot)} class="data-health" role="status">
        <span aria-hidden="true"></span>
        WF DATA {status?.marketSnapshot ? "OK" : "OFFLINE"}
      </span>
      {#if activeScreen === "market"}
      <button
        class="refresh-button"
        type="button"
        onclick={refreshMarketData}
        disabled={refreshing || loading}
      >
        {refreshing ? shell.refreshing : shell.refresh}
      </button>
      {/if}
    </div>
  </header>

  <div class="screen-body">

  {#key $locale}
  {#if activeScreen === "dashboard"}
    <DashboardScreen
      onOpenSellNow={() => { inventoryMode = "sell"; activeScreen = "inventory"; }}
      onOpenInventory={() => { inventoryMode = "all"; activeScreen = "inventory"; }}
      onOpenDiagnostics={() => (activeScreen = "diagnostics")}
    />
  {:else if activeScreen === "market"}
  <div class="live-region" role="status" aria-live="polite">
    {#if loading}
      {shell.openingStorage}
    {:else if refreshing}
      {shell.validatingSnapshot}
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

  {#if !loading && !status?.marketSnapshot}
    <section class="empty-panel" aria-labelledby="empty-heading">
      <p class="empty-panel__label">{shell.noSnapshot}</p>
      <h2 id="empty-heading">{shell.loadMarket}</h2>
      <p>{shell.loadMarketBody}</p>
      <button type="button" onclick={refreshMarketData} disabled={refreshing}>
        {refreshing ? shell.loadingMarket : shell.loadData}
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
                  <th scope="col" aria-sort={sortAria("confidence", sortKey, sortDirection)}>
                    <button type="button" onclick={() => changeSort("confidence")}>
                      {shell.confidence} <span aria-hidden="true">{sortMarker("confidence", sortKey, sortDirection)}</span>
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
                    <td data-label={shell.confidence}>
                      <span class={`confidence confidence--${row.recommendation.confidence}`}>
                        {confidenceLabel(row.recommendation.confidence, $locale)}
                      </span>
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
                {#if liveResult.warning}<strong>{liveResult.warning}</strong>{/if}
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
              <span>{shell.confidence}</span>
              <strong>{confidenceLabel(activeRecommendation.confidence, $locale)}</strong>
            </div>
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
    <InventoryHubScreen
      mode={inventoryMode}
      onModeChange={(mode) => (inventoryMode = mode)}
      onInventoryChange={() => void loadStatus()}
      onOpenAccount={() => (activeScreen = "account")}
    />
  {:else if activeScreen === "insights"}
    <InsightsScreen />
  {:else if activeScreen === "account"}
    <AccountScreen />
  {:else if activeScreen === "diagnostics"}
    <DiagnosticsScreen />
  {:else if activeScreen === "settings"}
    <SettingsScreen onSettingsSaved={applySettings} />
  {/if}
  {/key}
  </div>
  </main>
</div>
