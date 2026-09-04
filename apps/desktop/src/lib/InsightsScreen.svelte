<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import {
    accountActionErrorMessage,
    type AccountOrder,
    type AccountView,
    type CreateListingInput,
  } from "./account";
  import { localeCode, useLocale } from "./i18n";
  import {
    adjustDucatsForMarketReservations,
    filterAndSortOpportunitySets,
    formatPercent,
    formatRatio,
    rankRelicsToOpen,
    reservePublishedSetListings,
    refinementLabel,
    safeOverviewSetPrice,
    selectBestOverviewReadySet,
    selectBestOverviewRelic,
    setLiveMinimumPrice,
    setLiveSellOrders,
    setOpportunity,
    setRelicSupport,
    vaultLabel,
    type InsightsView,
    type RelicOverviewScenario,
    type SetInsightRow,
    type SetSaleMode,
  } from "./insights";
  import {
    formatPlatinum,
    liveQuoteLabel,
    liveUserStatusLabel,
    type LivePricingResult,
  } from "./market";
  import ResourceConverter from "./ResourceConverter.svelte";
  import {
    loadInsightsViewPreferences,
    saveInsightsViewPreferences,
    type InsightsViewMode,
  } from "./viewPreferences";

  export let onOpenSettings: () => void;
  export let onOpenMarketSales: () => void;

  const SET_PAGE_SIZE = 12;
  const DUCAT_PAGE_SIZE = 24;

  function russianPartWord(count: number): string {
    const lastTwo = Math.abs(count) % 100;
    const last = lastTwo % 10;
    if (lastTwo >= 11 && lastTwo <= 14) return "деталей";
    if (last === 1) return "деталь";
    if (last >= 2 && last <= 4) return "детали";
    return "деталей";
  }

  const locale = useLocale();
  const copy = {
    ru: {
      region: "Возможности заработка",
      reading: "Считаем возможности по сохранённым данным…",
      ready: (date: string) => `Данные предметов от ${date}`,
      loadError: "Не удалось рассчитать возможности. Цены и инвентарь не изменились.",
      retry: "Повторить",
      noSnapshot: "Сначала загрузите данные предметов",
      noSnapshotBody: "Обновление рынка и игровых данных находится в настройках.",
      openSettings: "Открыть настройки",
      noInventory: "Инвентарь ещё не загружен",
      noInventoryBody: "Запустите чтение инвентаря в настройках — после этого появятся персональные варианты.",
      overviewMode: "Лучшее сейчас",
      resourcesMode: "Ресурсы",
      relicMode: "Реликвии",
      buyMode: "Дособрать",
      readyMode: "Готовые сеты",
      ducatMode: "Дукаты",
      filters: "Разделы заработка",
      overviewTitle: "Что выгодно сделать сейчас",
      overviewBody: "Быстрые решения для реликвий и прайм-комплектов по вашему инвентарю и сохранённым ценам.",
      overviewSell: "Продать готовый сет",
      overviewComplete: "Дособрать ещё один сет",
      overviewRelic: "Открыть реликвию",
      overviewReadyCount: (count: number) => `Готово к продаже: ${count}`,
      overviewMissingCount: (count: number) => `Докупить: ${count} ${russianPartWord(count)}`,
      overviewRelicCopies: (count: number) => `Подходящих копий: ${count}`,
      overviewEstimated: "Цена за сет",
      overviewProfit: "Выгода",
      overviewNet: "Чистая выгода",
      overviewNoReady: "Нет готовых сетов с надёжной ценой, которые стоит продавать комплектом.",
      overviewNoComplete: "Выгодных вариантов для докупки пока нет.",
      overviewNoRelic: "Нет реликвий с положительной рассчитанной выгодой.",
      showReadySets: "Показать готовые сеты",
      showCompleteSets: "Показать варианты",
      showRelicRanking: "Открыть рейтинг",
      modeResourcesHint: "Репутация, кредиты Норы, дукаты, стальная эссенция и лишние мистификаторы.",
      modeRelicsHint: "Сравнение наград, стоимости реликвии, следов и шанса дособрать комплект.",
      modeCompleteTitle: "Что выгодно дособрать",
      modeCompleteHint: "Быстрые варианты: не больше двух видов и трёх деталей. Выгода учитывает стоимость уже имеющихся частей.",
      modeReadyTitle: "Готовые сеты для рынка",
      modeReadyHint: "Сначала — комплекты, которые выгоднее продавать целиком. Перед ордером можно проверить текущие предложения.",
      saleAdvice: (mode: SetSaleMode) => ({
        set: "Выгоднее комплектом",
        parts: "Выгоднее по частям",
        equivalent: "Цена примерно равна",
        insufficient_inventory: "Не хватает деталей",
        insufficient_pricing: "Проверьте цену",
      })[mode],
      modeDucatsHint: "Детали с наименьшей потерей платины за один дукат.",
      setResultCount: (visible: number, total: number) => `Показано ${visible} из ${total}`,
      showMore: (count: number) => `Показать ещё ${count}`,
      showLess: "Свернуть список",
      search: "Найти сет",
      searchPlaceholder: "Например, Стран Прайм",
      clearSearch: "Очистить поиск",
      missing: "Не хватает",
      missingNextSet: "До следующего сета",
      ownedRelics: "Подходящих копий",
      usefulChance: "Шанс дособрать сет",
      chanceHint: "получить все недостающие детали из имеющихся реликвий",
      buyFor: "Оценка докупки",
      sellSetFor: "Оценка продажи сета",
      completionProfit: "Выгода после сборки",
      profitHint: "с учётом цены всех использованных деталей",
      priceUnavailable: "Не хватает текущих цен",
      setPrice: "Цена сета",
      livePriceHint: "минимальная среди активных ордеров",
      checkLivePrice: "Проверить актуальную цену",
      checkingLivePrice: "Проверяем цену…",
      liveOrdersTitle: "Ордера продавцов в игре",
      liveOrdersOrder: "от дешёвых к дорогим",
      liveOrderPrice: "Цена комплекта",
      liveOrderQuantity: "Доступно",
      liveOrderStatus: "Статус",
      liveOrderQuantityValue: (quantity: number) => `${quantity} шт.`,
      livePriceChecked: (price: string, count: number) => `Минимальная цена ${price}. Показано ордеров: ${count}.`,
      livePriceUnavailable: "Warframe Market не вернул цену для этого комплекта. Повторите проверку позже.",
      liveSellOrdersUnavailable: "Сейчас нет активных ордеров на продажу этого комплекта.",
      livePriceError: "Не удалось проверить цену. Проверьте подключение и повторите.",
      readySets: "Готово сетов",
      partsPrice: "Детали отдельно",
      setPremium: "Премия сета",
      setPremiumHint: "к цене деталей",
      allPartsCovered: "Все недостающие виды деталей могут выпасть из ваших реликвий.",
      somePartsCovered: (covered: number, total: number) => `Из ваших реликвий выпадают ${covered} из ${total} недостающих видов деталей. Остальные можно докупить.`,
      buyUnknown: "Не хватает текущих заявок, чтобы честно посчитать стоимость покупки и выгоду.",
      showRelics: (count: number) => `Показать реликвии (${count})`,
      hideRelics: "Скрыть реликвии",
      buyMissing: (count: number) => `Открыть детали на рынке (${count})`,
      openingMarket: "Открываем рынок…",
      marketOpened: (count: number) => `Открыто страниц Warframe Market: ${count}.`,
      marketOpenError: "Не удалось открыть Warframe Market. Повторите действие.",
      relicPlan: "Подходящие реликвии",
      aggregateChance: "Шанс получить все недостающие детали из указанных реликвий",
      probabilityNote: "Это вероятность, а не гарантия. Расчёт предполагает одиночное открытие каждой копии и независимые результаты.",
      openNowTitle: "Какую реликвию открыть сейчас",
      openNowBodySolo: "Сначала — максимальная чистая выгода одного открытия после стоимости реликвии и следов.",
      openNowBodySquad: "Сначала — лучший результат отряда из четырёх игроков с одинаковой реликвией и улучшением.",
      relicScenario: "Расчёт для",
      soloScenario: "Соло",
      squadScenario: "4 одинаковые реликвии",
      soloNet: "Чистая выгода · соло",
      publicNet: "Чистая выгода · 4 одинаковые реликвии",
      noNetEstimate: "Не хватает цен",
      netAfterCosts: "на одно открытие, после реликвии и условной стоимости следов",
      bestOfFour: "лучший выбор из четырёх наград",
      pricedCoverage: (coverage: string) => `цены покрывают ${coverage} шанса`,
      finishSetSolo: "Закрыть комплект · соло",
      finishSetSquad: "Закрыть комплект · среди 4 наград",
      setProgress: "Нужная деталь",
      noSetProgress: "текущие комплекты не продвигает",
      preparation: "Подготовка",
      alreadyOwned: "улучшение уже есть",
      openRefinement: (refinement: string) => `Открыть: ${refinement}`,
      upgradeRefinement: (source: string, target: string) => `Улучшить: ${source} → ${target}`,
      noRelicPriceAction: "Выгода пока неизвестна",
      negativeRelicAction: "Для платины невыгодно",
      traces: (count: number) => `${count} следов Пустоты`,
      ownedRelicCopies: (count: number) => `Есть всего: ${count}`,
      showAllRanked: (count: number) => `Показать все (${count})`,
      showTopRanked: "Показать лучшие",
      rankingDetails: "Как составлен рейтинг",
      rankingExplanation: "Рейтинг сравнивает чистое матожидание: стоимость наград и выгода полного сета минус цена самой реликвии. Следы не продаются, поэтому для сравнения улучшений 100 следов условно считаются 2p; доступный баланс ограничивает варианты. Награды без подтверждённых цен не заменяются нулём.",
      noRankedRelics: "В инвентаре нет реликвий, которые можно оценить. Обновите инвентарь и данные предметов в настройках.",
      completableSetsTitle: "Какие сеты можно дособрать из ваших реликвий",
      owned: "Есть",
      perOpen: "За одно открытие",
      fromCopies: "Из всех копий",
      usefulDrops: "Нужные награды",
      needShort: "нужно",
      composition: "Состав сета",
      part: "Деталь",
      oneSetNeeds: "На один сет",
      ownedForSet: "Для продажи",
      missingForSet: "До следующего сета",
      price: "Цена",
      sellSet: "Выставить сет",
      openOrders: "Открыть мои ордера",
      connectAccount: "Подключить Warframe Market",
      verifyAccount: "Подтвердить аккаунт",
      orderTitle: "Новый ордер на сет",
      orderPrice: "Цена, платина",
      orderQuantity: "Количество сетов",
      publishOrder: "Сразу опубликовать ордер",
      reviewOrder: "Проверить ордер",
      cancel: "Отменить",
      confirmTitle: "Подтвердите ордер",
      confirmSummary: (name: string, quantity: number, price: number) => `${name}: выставить ${quantity} шт. по ${price}p.`,
      confirmCheck: "Я проверил сет, цену и количество",
      createOrder: "Создать ордер",
      creatingOrder: "Создаём ордер…",
      orderCreated: "Сет выставлен на Warframe Market.",
      invalidOrder: "Укажите целую цену и количество не больше доступных для продажи сетов.",
      setUnavailable: "Сет не найден в каталоге Warframe Market. Обновите данные рынка.",
      noResults: "Подходящих вариантов нет",
      noSearchResults: "Сеты с таким названием не найдены.",
      noRelicResults: (count: number) => count > 0
        ? `В инвентаре распознано реликвий: ${count}, но сейчас из них не выпадают недостающие детали ваших сетов.`
        : "В инвентаре нет распознанных реликвий. Запустите чтение инвентаря в настройках.",
      noBuyResults: "Нет сетов, которые выгодно дособрать покупкой недостающих деталей по надёжным ценам.",
      noReadyResults: "Готовых сетов для продажи сейчас нет.",
      ducatTitle: "Что дешевле обменять на дукаты",
      ducatBody: "Сначала показаны детали с наименьшей потерей платины за один дукат.",
      ducatWarning: "Проверьте количество перед обменом: действие в игре необратимо.",
      primePart: "Деталь Прайм",
      sellable: "Можно обменять",
      ducats: "Дукаты",
      platinumPerDucat: "Платина / дукат",
      noDucats: "Нет распознанных деталей Прайм с надёжной ценой и стоимостью в дукатах.",
    },
    en: {
      region: "Earning opportunities",
      reading: "Calculating opportunities from saved data…",
      ready: (date: string) => `Item data from ${date}`,
      loadError: "Unable to calculate opportunities. Prices and inventory were not changed.",
      retry: "Try again",
      noSnapshot: "Load item data first",
      noSnapshotBody: "Market and game-data updates are available in Settings.",
      openSettings: "Open settings",
      noInventory: "Inventory has not been loaded",
      noInventoryBody: "Run the inventory scan in Settings to see personal opportunities.",
      overviewMode: "Best now",
      resourcesMode: "Resources",
      relicMode: "Relics",
      buyMode: "Complete sets",
      readyMode: "Ready sets",
      ducatMode: "Ducats",
      filters: "Earning sections",
      overviewTitle: "Best actions now",
      overviewBody: "Quick decisions for relics and Prime sets based on your inventory and saved prices.",
      overviewSell: "Sell a complete set",
      overviewComplete: "Complete one more set",
      overviewRelic: "Open a relic",
      overviewReadyCount: (count: number) => `Ready to sell: ${count}`,
      overviewMissingCount: (count: number) => `Parts needed: ${count}`,
      overviewRelicCopies: (count: number) => `Matching copies: ${count}`,
      overviewEstimated: "Price per set",
      overviewProfit: "Profit",
      overviewNet: "Net value",
      overviewNoReady: "No complete set has both reliable pricing and a reason to sell it as a set.",
      overviewNoComplete: "No profitable purchases are available.",
      overviewNoRelic: "No relic has a positive calculated net value.",
      showReadySets: "Show complete sets",
      showCompleteSets: "Show opportunities",
      showRelicRanking: "Open ranking",
      modeResourcesHint: "Standing, Nora Cred, ducats, Steel Essence, and spare Arcanes.",
      modeRelicsHint: "Compare rewards, relic and Trace costs, and set completion chance.",
      modeCompleteTitle: "Sets worth completing",
      modeCompleteHint: "Quick options: at most two part types and three pieces. Profit includes the value of parts you already own.",
      modeReadyTitle: "Ready sets for the market",
      modeReadyHint: "Sets that are worth selling complete come first. Check current offers before listing.",
      saleAdvice: (mode: SetSaleMode) => ({
        set: "Better as a set",
        parts: "Better as parts",
        equivalent: "About the same value",
        insufficient_inventory: "Parts missing",
        insufficient_pricing: "Check the price",
      })[mode],
      modeDucatsHint: "Parts with the lowest platinum loss per ducat.",
      setResultCount: (visible: number, total: number) => `Showing ${visible} of ${total}`,
      showMore: (count: number) => `Show ${count} more`,
      showLess: "Show fewer",
      search: "Find a set",
      searchPlaceholder: "For example, Strun Prime",
      clearSearch: "Clear search",
      missing: "Missing",
      missingNextSet: "For one more set",
      ownedRelics: "Matching copies",
      usefulChance: "Chance to finish the set",
      chanceHint: "get every missing part from the owned relics",
      buyFor: "Estimated purchase cost",
      sellSetFor: "Estimated set sale",
      completionProfit: "Profit after completion",
      profitHint: "including the value of every part used",
      priceUnavailable: "Current prices unavailable",
      setPrice: "Set price",
      livePriceHint: "lowest active sell order",
      checkLivePrice: "Check current price",
      checkingLivePrice: "Checking price…",
      liveOrdersTitle: "Sell orders from players in game",
      liveOrdersOrder: "lowest price first",
      liveOrderPrice: "Set price",
      liveOrderQuantity: "Available",
      liveOrderStatus: "Status",
      liveOrderQuantityValue: (quantity: number) => `${quantity} pcs.`,
      livePriceChecked: (price: string, count: number) => `Lowest price ${price}. Orders shown: ${count}.`,
      livePriceUnavailable: "Warframe Market did not return a price for this set. Try again later.",
      liveSellOrdersUnavailable: "There are no active sell orders for this set.",
      livePriceError: "Unable to check the price. Check your connection and try again.",
      readySets: "Ready sets",
      partsPrice: "Parts separately",
      setPremium: "Set premium",
      setPremiumHint: "over the parts price",
      allPartsCovered: "Every missing part type can drop from your relics.",
      somePartsCovered: (covered: number, total: number) => `${covered} of ${total} missing part types can drop from your relics. Buy the rest if needed.`,
      buyUnknown: "Current orders are insufficient to calculate the purchase cost and profit reliably.",
      showRelics: (count: number) => `Show relics (${count})`,
      hideRelics: "Hide relics",
      buyMissing: (count: number) => `Open parts on market (${count})`,
      openingMarket: "Opening market…",
      marketOpened: (count: number) => `Opened ${count} Warframe Market pages.`,
      marketOpenError: "Unable to open Warframe Market. Try again.",
      relicPlan: "Matching relics",
      aggregateChance: "Chance to get every missing part from the listed relics",
      probabilityNote: "This is a probability, not a guarantee. It assumes a solo opening for each copy and independent outcomes.",
      openNowTitle: "Which relic to open now",
      openNowBodySolo: "Highest single-opening net value comes first after relic and Trace costs.",
      openNowBodySquad: "Highest best-of-four value comes first for four players using the same relic and refinement.",
      relicScenario: "Calculate for",
      soloScenario: "Solo",
      squadScenario: "4 matching relics",
      soloNet: "Net value · solo",
      publicNet: "Net value · 4 matching relics",
      noNetEstimate: "Not enough prices",
      netAfterCosts: "per opening, after the relic and modeled Trace cost",
      bestOfFour: "best of four rewards",
      pricedCoverage: (coverage: string) => `prices cover ${coverage} of outcomes`,
      finishSetSolo: "Finish a set · solo",
      finishSetSquad: "Finish a set · among 4 rewards",
      setProgress: "Useful part",
      noSetProgress: "does not advance current sets",
      preparation: "Preparation",
      alreadyOwned: "refinement already owned",
      openRefinement: (refinement: string) => `Open: ${refinement}`,
      upgradeRefinement: (source: string, target: string) => `Refine: ${source} → ${target}`,
      noRelicPriceAction: "Value is not known yet",
      negativeRelicAction: "Not profitable for platinum",
      traces: (count: number) => `${count} Void Traces`,
      ownedRelicCopies: (count: number) => `Owned total: ${count}`,
      showAllRanked: (count: number) => `Show all (${count})`,
      showTopRanked: "Show the best",
      rankingDetails: "How this ranking works",
      rankingExplanation: "The ranking compares net expected value after the relic price. Void Traces are not tradable, so the comparison models 100 Traces as 2p and still respects the available balance. Rewards without confirmed prices are not replaced with zero.",
      noRankedRelics: "No owned relics can be evaluated. Refresh inventory and item data in Settings.",
      completableSetsTitle: "Sets these relics can help finish",
      owned: "Owned",
      perOpen: "Per opening",
      fromCopies: "Across copies",
      usefulDrops: "Useful rewards",
      needShort: "need",
      composition: "Set components",
      part: "Part",
      oneSetNeeds: "One set needs",
      ownedForSet: "Sellable",
      missingForSet: "For the next set",
      price: "Price",
      sellSet: "List set",
      openOrders: "Open my orders",
      connectAccount: "Connect Warframe Market",
      verifyAccount: "Verify account",
      orderTitle: "New set order",
      orderPrice: "Price, platinum",
      orderQuantity: "Set quantity",
      publishOrder: "Publish the order immediately",
      reviewOrder: "Review order",
      cancel: "Cancel",
      confirmTitle: "Confirm order",
      confirmSummary: (name: string, quantity: number, price: number) => `${name}: list ${quantity} at ${price}p each.`,
      confirmCheck: "I checked the set, price, and quantity",
      createOrder: "Create order",
      creatingOrder: "Creating order…",
      orderCreated: "Set listed on Warframe Market.",
      invalidOrder: "Use a whole-number price and do not exceed the number of sets available for sale.",
      setUnavailable: "The set is missing from the Warframe Market catalog. Refresh market data.",
      noResults: "No matching opportunities",
      noSearchResults: "No sets match this name.",
      noRelicResults: (count: number) => count > 0
        ? `${count} relic variants were recognized, but none currently drop your missing set parts.`
        : "No relics were recognized. Run the inventory scan in Settings.",
      noBuyResults: "No sets are currently profitable to finish by buying missing parts at reliable prices.",
      noReadyResults: "No complete sets are ready to sell.",
      ducatTitle: "Lowest platinum cost per ducat",
      ducatBody: "Parts with the lowest platinum value per ducat are shown first.",
      ducatWarning: "Check the quantity before trading: the in-game action cannot be undone.",
      primePart: "Prime part",
      sellable: "Available",
      ducats: "Ducats",
      platinumPerDucat: "Platinum / ducat",
      noDucats: "No recognized Prime parts have both a reliable price and ducat value.",
    },
  } as const;
  $: c = copy[$locale];

  let view: InsightsView | null = null;
  let loading = true;
  let errorMessage = "";
  let activeMode: InsightsViewMode = loadInsightsViewPreferences().mode;
  let relicScenario: RelicOverviewScenario = "solo";
  let setQuery = "";
  let showAllRankedRelics = false;
  let showAllSetRows = false;
  let showAllDucats = false;
  let expandedRelicSet = "";
  let marketStatus = "";
  let marketStatusSlug = "";
  let marketBusySlug = "";
  let accountView: AccountView | null = null;
  let listingSlug = "";
  let listingPrice = 1;
  let listingQuantity = 1;
  let listingVisible = true;
  let listingStage: "edit" | "confirm" = "edit";
  let listingConfirmed = false;
  let listingBusy = false;
  let listingError = "";
  let liveSetQuotes = new Map<string, LivePricingResult>();
  let liveSetErrors = new Map<string, string>();
  let liveSetPriceBusySlug = "";
  let expandedLiveSetSlug = "";

  $: marketSets = (view?.sets ?? []).map((row) => reservePublishedSetListings(
    row,
    accountView?.orders ?? [],
    view?.sets ?? [],
  ));
  $: allRelicRows = filterAndSortOpportunitySets(marketSets, view?.relics ?? [], "relics", "", $locale);
  $: allBuyRows = filterAndSortOpportunitySets(marketSets, view?.relics ?? [], "buy", "", $locale);
  $: allReadyRows = filterAndSortOpportunitySets(marketSets, view?.relics ?? [], "ready", "", $locale)
    .filter((row) => availableSetQuantity(row) > 0);
  $: relicRows = filterAndSortOpportunitySets(marketSets, view?.relics ?? [], "relics", setQuery, $locale);
  $: buyRows = filterAndSortOpportunitySets(marketSets, view?.relics ?? [], "buy", setQuery, $locale);
  $: readyRows = filterAndSortOpportunitySets(marketSets, view?.relics ?? [], "ready", setQuery, $locale)
    .filter((row) => availableSetQuantity(row) > 0)
    .sort((left, right) => availableSetQuantity(right) - availableSetQuantity(left));
  $: buyOpportunityCount = allBuyRows.length;
  $: readyOpportunityCount = allReadyRows.length;
  $: setRows = activeMode === "relics"
    ? relicRows
    : activeMode === "complete_sets"
      ? buyRows
      : activeMode === "sell_sets"
        ? readyRows
        : [];
  $: visibleSetRows = showAllSetRows ? setRows : setRows.slice(0, SET_PAGE_SIZE);
  $: remainingSetRows = Math.max(0, setRows.length - visibleSetRows.length);
  $: ducatRows = adjustDucatsForMarketReservations(view?.ducats ?? [], marketSets)
    .filter((row) => row.sellableQuantity > 0 && row.efficiency.credible);
  $: visibleDucatRows = showAllDucats ? ducatRows : ducatRows.slice(0, DUCAT_PAGE_SIZE);
  $: remainingDucatRows = Math.max(0, ducatRows.length - visibleDucatRows.length);
  $: rankedRelics = rankRelicsToOpen(view?.relics ?? [], marketSets, {
    availableTraces: view?.voidTraces,
    squadSize: relicScenario === "solo" ? 1 : 4,
  });
  $: visibleRankedRelics = showAllRankedRelics ? rankedRelics : rankedRelics.slice(0, 5);
  $: bestRelic = selectBestOverviewRelic(rankedRelics, relicScenario);
  $: bestBuyRow = allBuyRows.find((row) => safeOverviewSetPrice(row) !== null) ?? null;
  $: bestReadyRow = selectBestOverviewReadySet(allReadyRows);
  $: ownedRelicCount = (view?.relics ?? []).reduce((sum, relic) => sum + relic.ownedQuantity, 0);
  $: listedSetItemIds = new Set(
    (accountView?.orders ?? []).filter((order) => order.type === "sell").map((order) => order.itemId),
  );
  $: listingRow = marketSets.find((row) => row.definition.setSlug === listingSlug) ?? null;

  function formatProbability(value: number): string {
    return `${value.toLocaleString(localeCode($locale), { maximumFractionDigits: 1 })}%`;
  }

  function scenarioProbability(value: number): number {
    const solo = Math.min(100, Math.max(0, Number.isFinite(value) ? value : 0));
    return relicScenario === "solo"
      ? solo
      : 100 * (1 - (1 - solo / 100) ** 4);
  }

  function selectMode(mode: InsightsViewMode): void {
    activeMode = mode;
    saveInsightsViewPreferences({ mode });
    setQuery = "";
    expandedRelicSet = "";
    expandedLiveSetSlug = "";
    listingSlug = "";
    listingError = "";
    marketStatus = "";
    marketStatusSlug = "";
    showAllSetRows = false;
    showAllDucats = false;
  }

  function selectRelicScenario(scenario: RelicOverviewScenario): void {
    relicScenario = scenario;
    showAllRankedRelics = false;
  }

  function availableSetQuantity(row: SetInsightRow): number {
    return setOpportunity(row).sellableCompleteSets;
  }

  function relicValue(row: { expectedPlatinum: number | null; squadExpectedPlatinum: number | null }): number | null {
    return relicScenario === "solo" ? row.expectedPlatinum : row.squadExpectedPlatinum;
  }

  async function loadInsights(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      view = await invoke<InsightsView | null>("insights");
    } catch {
      errorMessage = c.loadError;
    } finally {
      loading = false;
    }
  }

  async function loadAccount(): Promise<void> {
    try {
      accountView = await invoke<AccountView>("account_status");
    } catch {
      // Сохраняем последнее успешное состояние: временный сетевой сбой не означает отключённый аккаунт.
    }
  }

  async function openMissingParts(row: SetInsightRow): Promise<void> {
    const slugs = setOpportunity(row).missingParts.map((part) => part.slug);
    if (slugs.length === 0) return;
    marketBusySlug = row.definition.setSlug;
    marketStatusSlug = row.definition.setSlug;
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

  async function checkLiveSetPrice(row: SetInsightRow): Promise<void> {
    const slug = row.definition.setSlug;
    expandedLiveSetSlug = slug;
    marketStatusSlug = slug;
    marketStatus = c.checkingLivePrice;
    const nextQuotes = new Map(liveSetQuotes);
    nextQuotes.delete(slug);
    liveSetQuotes = nextQuotes;
    const nextErrors = new Map(liveSetErrors);
    nextErrors.delete(slug);
    liveSetErrors = nextErrors;
    const key = row.setRecommendation?.key;
    if (!key) {
      liveSetErrors = new Map(liveSetErrors).set(slug, c.setUnavailable);
      marketStatus = c.setUnavailable;
      return;
    }

    liveSetPriceBusySlug = slug;
    try {
      const result = await invoke<LivePricingResult | null>("live_price_current_variant", {
        key,
        itemKind: "standard",
      });
      if (!result) {
        liveSetErrors = new Map(liveSetErrors).set(slug, c.livePriceUnavailable);
        marketStatus = c.livePriceUnavailable;
        return;
      }
      const sellOrders = setLiveSellOrders(result.orders);
      if (sellOrders.length === 0) {
        liveSetErrors = new Map(liveSetErrors).set(slug, c.liveSellOrdersUnavailable);
        marketStatus = c.liveSellOrdersUnavailable;
        return;
      }
      liveSetQuotes = new Map(liveSetQuotes).set(slug, result);
      marketStatus = c.livePriceChecked(formatPlatinum(sellOrders[0].pricePerSet, $locale), sellOrders.length);
    } catch {
      liveSetErrors = new Map(liveSetErrors).set(slug, c.livePriceError);
      marketStatus = c.livePriceError;
    } finally {
      liveSetPriceBusySlug = "";
    }
  }

  function startListing(row: SetInsightRow): void {
    listingError = "";
    expandedLiveSetSlug = "";
    marketStatus = "";
    marketStatusSlug = row.definition.setSlug;
    if (!accountView?.connected || !accountView.profile?.verification || (row.itemId && listedSetItemIds.has(row.itemId))) {
      onOpenMarketSales();
      return;
    }
    if (!row.itemId) {
      listingError = c.setUnavailable;
      listingSlug = row.definition.setSlug;
      return;
    }
    listingSlug = row.definition.setSlug;
    const liveMinimum = setLiveMinimumPrice(liveSetQuotes.get(row.definition.setSlug)?.orders ?? []);
    listingPrice = Math.max(1, Math.round(liveMinimum ?? row.setRecommendation?.listPrice ?? row.comparison.setFairValue ?? 1));
    listingQuantity = 1;
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
    const availableSets = listingRow ? availableSetQuantity(listingRow) : 0;
    if (!Number.isInteger(listingPrice) || listingPrice < 1 || !Number.isInteger(listingQuantity)
      || listingQuantity < 1 || listingQuantity > availableSets) {
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
    const cleanups: UnlistenFn[] = [];
    void loadInsights();
    void loadAccount();
    for (const event of ["game-metadata-updated", "market-data-updated", "inventory-updated"]) {
      void listen(event, () => void loadInsights()).then((cleanup) => {
        if (disposed) cleanup();
        else cleanups.push(cleanup);
      });
    }
    return () => {
      disposed = true;
      for (const cleanup of cleanups) cleanup();
    };
  });
</script>

<section class="opportunities" aria-label={c.region}>
  <div class="opportunity-navigation">
    <div class="mode-switcher" role="group" aria-label={c.filters}>
      <button type="button" aria-pressed={activeMode === "overview"} onclick={() => selectMode("overview")}>
        {c.overviewMode}
      </button>
      <button type="button" aria-pressed={activeMode === "resources"} onclick={() => selectMode("resources")}>
        {c.resourcesMode}
      </button>
      <button type="button" aria-pressed={activeMode === "relics"} onclick={() => selectMode("relics")}>
        {c.relicMode}<span>{rankedRelics.length}</span>
      </button>
      <button type="button" aria-pressed={activeMode === "complete_sets"} onclick={() => selectMode("complete_sets")}>
        {c.buyMode}<span>{buyOpportunityCount}</span>
      </button>
      <button type="button" aria-pressed={activeMode === "sell_sets"} onclick={() => selectMode("sell_sets")}>
        {c.readyMode}<span>{readyOpportunityCount}</span>
      </button>
      <button type="button" aria-pressed={activeMode === "ducats"} onclick={() => selectMode("ducats")}>
        {c.ducatMode}<span>{ducatRows.length}</span>
      </button>
    </div>
  </div>

  <div class="resource-mode" hidden={activeMode !== "resources"}>
    <ResourceConverter {onOpenSettings} />
  </div>

  {#if activeMode !== "resources"}
    <div class="data-status" role="status" aria-live="polite">
      {#if loading}{c.reading}{:else if view}{c.ready(view.metadata.fetchedAt.slice(0, 10))}{/if}
    </div>

    {#if errorMessage}
      <div class="message message--error" role="alert">
        <p>{errorMessage}</p>
        <button type="button" onclick={loadInsights}>{c.retry}</button>
      </div>
    {/if}

    {#if !loading && !view && !errorMessage}
      <div class="message">
        <h2>{c.noSnapshot}</h2>
        <p>{c.noSnapshotBody}</p>
        <button type="button" onclick={onOpenSettings}>{c.openSettings}</button>
      </div>
    {:else if view}
    {#if !view.inventoryAvailable}
      <div class="message message--action" role="note">
        <div><h2>{c.noInventory}</h2><p>{c.noInventoryBody}</p></div>
        <button type="button" onclick={onOpenSettings}>{c.openSettings}</button>
      </div>
    {/if}

    <div class="action-status sr-only" role="status" aria-live="polite">{marketStatus}</div>

    {#if activeMode === "overview"}
      <section class="opportunity-overview" aria-labelledby="opportunity-overview-title">
        <header class="overview-heading">
          <div>
            <p class="section-kicker">{c.overviewMode}</p>
            <h2 id="opportunity-overview-title">{c.overviewTitle}</h2>
            <p>{c.overviewBody}</p>
          </div>
        </header>

        {#if bestReadyRow || bestBuyRow || bestRelic}
        <div class="overview-grid">
          {#if bestReadyRow}
          <article class="overview-card">
            <p class="overview-card__kind">{c.overviewSell}</p>
              <div class="overview-card__identity">
                {#if bestReadyRow.imageUrl}<img src={bestReadyRow.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                <h3>{bestReadyRow.displayName}</h3>
              </div>
              <p class="overview-card__meta">{c.overviewReadyCount(availableSetQuantity(bestReadyRow))}</p>
              <dl><div><dt>{c.overviewEstimated}</dt><dd>{formatPlatinum(safeOverviewSetPrice(bestReadyRow), $locale)}</dd></div></dl>
            <button type="button" class="secondary" onclick={() => selectMode("sell_sets")}>{c.showReadySets}</button>
          </article>
          {/if}

          {#if bestBuyRow}
          {@const buyOpportunity = setOpportunity(bestBuyRow)}
          <article class="overview-card overview-card--accent">
            <p class="overview-card__kind">{c.overviewComplete}</p>
              <div class="overview-card__identity">
                {#if bestBuyRow.imageUrl}<img src={bestBuyRow.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                <h3>{bestBuyRow.displayName}</h3>
              </div>
              <p class="overview-card__meta">{c.overviewMissingCount(buyOpportunity.missingQuantity)}</p>
              <dl><div><dt>{c.overviewProfit}</dt><dd>{formatPlatinum(buyOpportunity.completionProfit, $locale)}</dd></div></dl>
            <button type="button" onclick={() => selectMode("complete_sets")}>{c.showCompleteSets}</button>
          </article>
          {/if}

          {#if bestRelic}
          <article class="overview-card">
            <p class="overview-card__kind">{c.overviewRelic}</p>
              <div class="overview-card__identity">
                {#if bestRelic.imageUrl}<img src={bestRelic.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                <h3>{bestRelic.displayName}</h3>
              </div>
              <p class="overview-card__meta">
                {c.overviewRelicCopies(bestRelic.sourceQuantity)} · {refinementLabel(bestRelic.sourceRefinement, $locale)}{#if bestRelic.sourceRefinement !== bestRelic.recommendedRefinement} → {refinementLabel(bestRelic.recommendedRefinement, $locale)}{/if}
              </p>
              <dl><div><dt>{c.overviewNet}</dt><dd>{formatPlatinum(relicValue(bestRelic), $locale)}</dd></div></dl>
            <button type="button" class="secondary" onclick={() => selectMode("relics")}>{c.showRelicRanking}</button>
          </article>
          {/if}
        </div>
        {/if}

        {#if !bestReadyRow && !bestBuyRow && !bestRelic}
          <div class="overview-empty">
            <h3>{$locale === "ru" ? "Пока нет подходящих вариантов" : "No suitable opportunities yet"}</h3>
            <p>{$locale === "ru" ? "Можно проверить, что доступно за накопленные ресурсы." : "See what you can buy with your resources."}</p>
            <button type="button" class="secondary" onclick={() => selectMode("resources")}>{$locale === "ru" ? "Посмотреть ресурсы" : "View resources"}</button>
          </div>
        {/if}
      </section>

    {:else if activeMode === "ducats"}
      <section class="ducat-panel" aria-labelledby="ducat-heading">
        <header>
          <div>
            <h2 id="ducat-heading">{c.ducatTitle}</h2>
            <p>{c.ducatBody}</p>
          </div>
          <p class="warning">{c.ducatWarning}</p>
        </header>
        <div class="table-scroll">
          <table>
            <thead><tr><th>{c.primePart}</th><th>{c.sellable}</th><th>{c.price}</th><th>{c.ducats}</th><th>{c.platinumPerDucat}</th></tr></thead>
            <tbody>
              {#each visibleDucatRows as row (row.metadata.slug)}
                <tr>
                  <th scope="row"><span class="item-name">{#if row.imageUrl}<img src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{row.displayName}</span></th>
                  <td data-label={c.sellable}>{row.sellableQuantity}</td>
                  <td data-label={c.price}>{formatPlatinum(row.efficiency.fairPrice, $locale)}</td>
                  <td data-label={c.ducats}>{row.efficiency.ducats}</td>
                  <td data-label={c.platinumPerDucat}>{formatRatio(row.efficiency.platinumPerDucat, $locale)}</td>
                </tr>
              {:else}
                <tr><td colspan="5">{c.noDucats}</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#if ducatRows.length > DUCAT_PAGE_SIZE}
          <div class="list-footer">
            <span>{c.setResultCount(visibleDucatRows.length, ducatRows.length)}</span>
            <button type="button" class="secondary" onclick={() => (showAllDucats = !showAllDucats)}>
              {showAllDucats ? c.showLess : c.showMore(remainingDucatRows)}
            </button>
          </div>
        {/if}
      </section>
    {:else}
      {#if activeMode === "relics"}
        <section class="relic-ranking" aria-labelledby="relic-ranking-title">
          <header class="relic-ranking__header">
            <div>
              <h2 id="relic-ranking-title">{c.openNowTitle}</h2>
              <p>{relicScenario === "solo" ? c.openNowBodySolo : c.openNowBodySquad}</p>
            </div>
            <div class="relic-ranking__controls">
              <div class="scenario-switcher" role="group" aria-label={c.relicScenario}>
                <button type="button" aria-pressed={relicScenario === "solo"} onclick={() => selectRelicScenario("solo")}>{c.soloScenario}</button>
                <button type="button" aria-pressed={relicScenario === "matching_squad"} onclick={() => selectRelicScenario("matching_squad")}>{c.squadScenario}</button>
              </div>
              {#if rankedRelics.length > 5}
                <button type="button" class="secondary" onclick={() => (showAllRankedRelics = !showAllRankedRelics)}>
                  {showAllRankedRelics ? c.showTopRanked : c.showAllRanked(rankedRelics.length)}
                </button>
              {/if}
            </div>
          </header>

          {#if visibleRankedRelics.length > 0}
            <ol class="relic-ranking__list">
              {#each visibleRankedRelics as recommendation, index (recommendation.relicSlug)}
                {#if relicValue(recommendation) === null && (index === 0 || relicValue(visibleRankedRelics[index - 1]) !== null)}
                  <li class="unrated-heading"><h3>{$locale === "ru" ? "Не хватает цен для оценки" : "Not enough prices to estimate"}</h3></li>
                {/if}
                <li class:relic-ranking__item--best={index === 0 && (relicValue(recommendation) ?? 0) > 0} class="relic-ranking__item">
                  <span class="relic-rank" aria-label={relicValue(recommendation) === null ? c.noNetEstimate : `№ ${index + 1}`}>{relicValue(recommendation) === null ? "—" : index + 1}</span>
                  <div class="relic-ranking__identity">
                    {#if recommendation.imageUrl}<img src={recommendation.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                    <div>
                      <h3>{recommendation.displayName}</h3>
                      <p>{c.ownedRelicCopies(recommendation.totalOwnedQuantity)} · {refinementLabel(recommendation.sourceRefinement, $locale)} ×{recommendation.sourceQuantity}</p>
                    </div>
                  </div>
                  <dl class="relic-ranking__metrics">
                    <div>
                      <dt>{relicScenario === "solo" ? c.soloNet : c.publicNet}</dt>
                      {#if relicValue(recommendation) !== null}
                        <dd>{formatPlatinum(relicValue(recommendation), $locale)}<small>{relicScenario === "solo" ? c.netAfterCosts : c.bestOfFour} · {c.pricedCoverage(formatProbability(recommendation.pricedChancePercent))}</small></dd>
                      {:else}
                        <dd class="metric-unavailable">{c.noNetEstimate}<small>{c.pricedCoverage(formatProbability(recommendation.pricedChancePercent))}</small></dd>
                      {/if}
                    </div>
                    <div>
                      <dt>{relicScenario === "solo" ? c.finishSetSolo : c.finishSetSquad}</dt>
                      <dd>{formatProbability(scenarioProbability(recommendation.completionChancePercent))}
                        <small>
                          {#if recommendation.completionTargets[0]}
                            {recommendation.completionTargets[0].displayName}
                          {:else if recommendation.progressChancePercent > 0}
                            {c.setProgress}: {formatProbability(scenarioProbability(recommendation.progressChancePercent))}
                          {:else}
                            {c.noSetProgress}
                          {/if}
                        </small>
                      </dd>
                    </div>
                    <div class="relic-ranking__action">
                      <dt>{c.preparation}</dt>
                      <dd>{refinementLabel(recommendation.recommendedRefinement, $locale)}
                        <small>{recommendation.traceCost > 0 ? c.traces(recommendation.traceCost) : c.alreadyOwned}</small>
                      </dd>
                    </div>
                  </dl>
                  <p class:relic-ranking__decision--muted={(relicValue(recommendation) ?? 0) <= 0} class="relic-ranking__decision">
                    {#if relicValue(recommendation) === null}
                      {c.noRelicPriceAction}
                    {:else if (relicValue(recommendation) ?? 0) <= 0}
                      {c.negativeRelicAction}
                    {:else}
                      {recommendation.traceCost > 0
                        ? c.upgradeRefinement(refinementLabel(recommendation.sourceRefinement, $locale), refinementLabel(recommendation.recommendedRefinement, $locale))
                        : c.openRefinement(refinementLabel(recommendation.recommendedRefinement, $locale))}
                    {/if}
                  </p>
                </li>
              {/each}
            </ol>
          {:else}
            <p class="relic-ranking__empty">{c.noRankedRelics}</p>
          {/if}

          <details class="relic-ranking__details">
            <summary>{c.rankingDetails}</summary>
            <p>{c.rankingExplanation}</p>
          </details>
        </section>

        <section class="mode-heading" aria-labelledby="relic-set-list-title">
          <div>
            <h2 id="relic-set-list-title">{c.completableSetsTitle}</h2>
            <p>{c.modeRelicsHint}</p>
          </div>
          <label class="set-search">
            <span>{c.search}</span>
            <input bind:value={setQuery} oninput={() => (showAllSetRows = false)} type="search" placeholder={c.searchPlaceholder} />
          </label>
        </section>
      {:else if activeMode === "complete_sets" || activeMode === "sell_sets"}
        <section class="mode-heading" aria-labelledby="set-mode-title">
          <div>
            <h2 id="set-mode-title">{activeMode === "complete_sets" ? c.modeCompleteTitle : c.modeReadyTitle}</h2>
            <p>{activeMode === "complete_sets" ? c.modeCompleteHint : c.modeReadyHint}</p>
          </div>
          <label class="set-search">
            <span>{c.search}</span>
            <input bind:value={setQuery} oninput={() => (showAllSetRows = false)} type="search" placeholder={c.searchPlaceholder} />
          </label>
        </section>
      {/if}
      <div class="set-list">
        {#each visibleSetRows as row (row.definition.setSlug)}
          {@const opportunity = setOpportunity(row)}
          {@const relicSupport = setRelicSupport(row, view.relics)}
          {@const availableSets = availableSetQuantity(row)}
          {@const liveQuote = liveSetQuotes.get(row.definition.setSlug)}
          {@const liveOrders = setLiveSellOrders(liveQuote?.orders ?? [])}
          {@const liveMinimum = setLiveMinimumPrice(liveQuote?.orders ?? [])}
          {@const displayedSetPrice = liveMinimum ?? opportunity.setFairValue}
          {@const displayedSetPremium = liveMinimum !== null && opportunity.partsFairValue !== null
            ? liveMinimum - opportunity.partsFairValue
            : opportunity.setPremiumValue}
          {@const displayedSetPremiumPercent = liveMinimum !== null
            ? opportunity.partsFairValue !== null && opportunity.partsFairValue > 0
              ? (liveMinimum - opportunity.partsFairValue) / opportunity.partsFairValue * 100
              : null
            : opportunity.setPremiumPercent}
          <article class="set-card">
            <header class="set-card__header">
              <div class="set-identity">
                {#if row.imageUrl}<img class="set-image" src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                <div>
                  <p class="set-context">{vaultLabel(row.definition.vaultStatus, $locale)}</p>
                  <h2>{row.displayName}</h2>
                </div>
              </div>
              {#if activeMode === "sell_sets"}
                <span
                  class:sale-route--set={row.comparison.recommendedMode === "set"}
                  class:sale-route--parts={row.comparison.recommendedMode === "parts"}
                  class="sale-route"
                >{c.saleAdvice(row.comparison.recommendedMode)}</span>
              {/if}
            </header>

            <dl class="set-metrics">
              {#if activeMode === "relics"}
                <div><dt>{availableSets > 0 ? c.missingNextSet : c.missing}</dt><dd>{opportunity.missingQuantity}</dd></div>
                <div><dt>{c.ownedRelics}</dt><dd>{relicSupport.ownedRelicCount}</dd></div>
                <div><dt>{c.usefulChance}</dt><dd>{formatProbability(relicSupport.aggregateChancePercent)}<small>{c.chanceHint}</small></dd></div>
                <div class:positive={(opportunity.setPremiumValue ?? 0) > 0}>
                  <dt>{c.setPremium}</dt>
                  {#if opportunity.setPremiumValue !== null}
                    <dd>{formatPlatinum(opportunity.setPremiumValue, $locale)}{#if opportunity.setPremiumPercent !== null}<small>{formatPercent(opportunity.setPremiumPercent, $locale)} · {c.setPremiumHint}</small>{/if}</dd>
                  {:else}
                    <dd class="metric-unavailable">{c.priceUnavailable}</dd>
                  {/if}
                </div>
              {:else if activeMode === "complete_sets"}
                <div><dt>{availableSets > 0 ? c.missingNextSet : c.missing}</dt><dd>{opportunity.missingQuantity}</dd></div>
                <div>
                  <dt>{c.buyFor}</dt>
                  <dd class:metric-unavailable={opportunity.completionCost === null}>{opportunity.completionCost === null ? c.priceUnavailable : formatPlatinum(opportunity.completionCost, $locale)}</dd>
                </div>
                <div>
                  <dt>{c.sellSetFor}</dt>
                  <dd class:metric-unavailable={opportunity.completionRevenue === null}>{opportunity.completionRevenue === null ? c.priceUnavailable : formatPlatinum(opportunity.completionRevenue, $locale)}</dd>
                </div>
                <div class:positive={(opportunity.completionProfit ?? 0) > 0}>
                  <dt>{c.completionProfit}</dt>
                  {#if opportunity.completionProfit !== null}
                    <dd>{formatPlatinum(opportunity.completionProfit, $locale)}<small>{c.profitHint}</small></dd>
                  {:else}
                    <dd class="metric-unavailable">{c.priceUnavailable}</dd>
                  {/if}
                </div>
              {:else}
                <div><dt>{c.readySets}</dt><dd>{availableSets}</dd></div>
                <div>
                  <dt>{c.setPrice}</dt>
                  <dd class:metric-unavailable={displayedSetPrice === null}>
                    {displayedSetPrice === null ? c.priceUnavailable : formatPlatinum(displayedSetPrice, $locale)}
                    {#if liveMinimum !== null && liveQuote}<small>{c.livePriceHint} · {liveQuoteLabel(liveQuote.quoteState, $locale)}</small>{/if}
                  </dd>
                </div>
                <div><dt>{c.partsPrice}</dt><dd class:metric-unavailable={opportunity.partsFairValue === null}>{opportunity.partsFairValue === null ? c.priceUnavailable : formatPlatinum(opportunity.partsFairValue, $locale)}</dd></div>
                <div class:positive={(displayedSetPremium ?? 0) > 0}>
                  <dt>{c.setPremium}</dt>
                  {#if displayedSetPremium !== null}
                    <dd>{formatPlatinum(displayedSetPremium, $locale)}{#if displayedSetPremiumPercent !== null}<small>{formatPercent(displayedSetPremiumPercent, $locale)} · {c.setPremiumHint}</small>{/if}</dd>
                  {:else}
                    <dd class="metric-unavailable">{c.priceUnavailable}</dd>
                  {/if}
                </div>
              {/if}
            </dl>

            {#if activeMode === "relics"}
              <p class="decision-copy">
                {relicSupport.allMissingPartsCovered ? c.allPartsCovered : c.somePartsCovered(relicSupport.coveredPartCount, relicSupport.missingPartCount)}
              </p>
            {:else if activeMode === "complete_sets" && (opportunity.completionCost === null || opportunity.completionRevenue === null || opportunity.completionProfit === null)}
              <p class="decision-copy">{c.buyUnknown}</p>
            {/if}

            {#if activeMode !== "sell_sets" && opportunity.missingParts.length > 0}
              <div class="missing-parts" aria-label={c.missing}>
                {#each opportunity.missingParts as part (part.slug)}
                  <span>{part.displayName} ×{part.quantity}<strong>{part.estimatedCost === null ? c.priceUnavailable : formatPlatinum(part.estimatedCost, $locale)}</strong></span>
                {/each}
              </div>
            {/if}

            <div class="card-actions">
              {#if activeMode === "relics"}
                <button type="button" onclick={() => (expandedRelicSet = expandedRelicSet === row.definition.setSlug ? "" : row.definition.setSlug)}>
                  {expandedRelicSet === row.definition.setSlug ? c.hideRelics : c.showRelics(relicSupport.matches.length)}
                </button>
                <button type="button" class="secondary" disabled={marketBusySlug === row.definition.setSlug} onclick={() => openMissingParts(row)}>
                  {marketBusySlug === row.definition.setSlug ? c.openingMarket : c.buyMissing(opportunity.missingParts.length)}
                </button>
              {:else if activeMode === "complete_sets"}
                <button type="button" disabled={marketBusySlug === row.definition.setSlug} onclick={() => openMissingParts(row)}>
                  {marketBusySlug === row.definition.setSlug ? c.openingMarket : c.buyMissing(opportunity.missingParts.length)}
                </button>
              {:else}
                <button
                  type="button"
                  class="secondary"
                  disabled={liveSetPriceBusySlug !== ""}
                  aria-busy={liveSetPriceBusySlug === row.definition.setSlug}
                  aria-controls={`live-set-orders-${row.definition.setSlug}`}
                  aria-expanded={expandedLiveSetSlug === row.definition.setSlug && Boolean(liveQuote || liveSetErrors.get(row.definition.setSlug))}
                  onclick={() => checkLiveSetPrice(row)}
                >
                  {liveSetPriceBusySlug === row.definition.setSlug ? c.checkingLivePrice : c.checkLivePrice}
                </button>
                <button type="button" onclick={() => row.itemId && listedSetItemIds.has(row.itemId) ? onOpenMarketSales() : startListing(row)}>
                  {#if row.itemId && listedSetItemIds.has(row.itemId)}{c.openOrders}{:else if !accountView}{c.openOrders}{:else if !accountView.connected}{c.connectAccount}{:else if !accountView.profile?.verification}{c.verifyAccount}{:else}{c.sellSet}{/if}
                </button>
              {/if}
            </div>

            {#if marketStatusSlug === row.definition.setSlug && marketStatus && marketBusySlug !== row.definition.setSlug && expandedLiveSetSlug !== row.definition.setSlug}
              <p class:card-status--error={marketStatus === c.marketOpenError} class="card-status">{marketStatus}</p>
            {/if}

            {#if activeMode === "sell_sets" && expandedLiveSetSlug === row.definition.setSlug}
              {@const liveError = liveSetErrors.get(row.definition.setSlug)}
              {#if liveError}
                <p id={`live-set-orders-${row.definition.setSlug}`} class="live-set-error">{liveError}</p>
              {:else if liveQuote && liveOrders.length > 0}
                <section
                  id={`live-set-orders-${row.definition.setSlug}`}
                  class="live-set-orders"
                  aria-labelledby={`live-set-orders-title-${row.definition.setSlug}`}
                >
                  <header>
                    <h3 id={`live-set-orders-title-${row.definition.setSlug}`}>{c.liveOrdersTitle}</h3>
                    <p>{c.liveOrdersOrder} · {liveQuoteLabel(liveQuote.quoteState, $locale)}</p>
                  </header>
                  <div class="table-scroll">
                    <table aria-labelledby={`live-set-orders-title-${row.definition.setSlug}`}>
                      <thead>
                        <tr><th>{c.liveOrderPrice}</th><th>{c.liveOrderQuantity}</th><th>{c.liveOrderStatus}</th></tr>
                      </thead>
                      <tbody>
                        {#each liveOrders as order, index (`${order.pricePerSet}:${order.quantity}:${order.userStatus}:${index}`)}
                          <tr>
                            <td><strong>{formatPlatinum(order.pricePerSet, $locale)}</strong></td>
                            <td>{c.liveOrderQuantityValue(order.quantity)}</td>
                            <td>{liveUserStatusLabel(order.userStatus, $locale)}</td>
                          </tr>
                        {/each}
                      </tbody>
                    </table>
                  </div>
                </section>
              {/if}
            {/if}

            {#if expandedRelicSet === row.definition.setSlug && activeMode === "relics"}
              <section class="relic-plan" aria-label={c.relicPlan}>
                <div class="relic-plan__summary">
                  <span>{c.aggregateChance}</span>
                  <strong>{formatProbability(relicSupport.aggregateChancePercent)}</strong>
                  <small>{c.probabilityNote}</small>
                </div>
                <div class="relic-list">
                  {#each relicSupport.matches as match (`${match.relic.definition.relicSlug}:${match.relic.definition.refinement}`)}
                    <article class="relic-row">
                      <div class="relic-identity">
                        {#if match.relic.imageUrl}<img src={match.relic.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                        <div><h3>{match.relic.displayName}</h3><p>{refinementLabel(match.relic.definition.refinement, $locale)}</p></div>
                      </div>
                      <dl>
                        <div><dt>{c.owned}</dt><dd>×{match.relic.ownedQuantity}</dd></div>
                        <div><dt>{c.perOpen}</dt><dd>{formatProbability(match.chancePerRelicPercent)}</dd></div>
                        <div><dt>{c.fromCopies}</dt><dd>{formatProbability(match.chanceFromOwnedPercent)}</dd></div>
                      </dl>
                      <div class="useful-rewards" aria-label={c.usefulDrops}>
                        {#each match.usefulRewards as reward (reward.slug)}
                          <span>{#if reward.imageUrl}<img src={reward.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{reward.displayName}<small>{formatProbability(reward.chancePercent)} · {c.needShort} ×{reward.quantityNeeded}</small></span>
                        {/each}
                      </div>
                    </article>
                  {/each}
                </div>
              </section>
            {/if}

            {#if listingSlug === row.definition.setSlug}
              <section class="order-panel" aria-labelledby={`set-order-${row.definition.setSlug}`}>
                <h3 id={`set-order-${row.definition.setSlug}`}>{listingStage === "edit" ? c.orderTitle : c.confirmTitle}</h3>
                {#if listingError}<p class="inline-error" role="alert">{listingError}</p>{/if}
                {#if listingStage === "edit"}
                  <form onsubmit={reviewListing}>
                    <div class="order-fields">
                      <label><span>{c.orderPrice}</span><input bind:value={listingPrice} type="number" min="1" step="1" inputmode="numeric" /></label>
                      <label><span>{c.orderQuantity}</span><input bind:value={listingQuantity} type="number" min="1" max={availableSets} step="1" inputmode="numeric" /></label>
                    </div>
                    <label class="order-visible"><input bind:checked={listingVisible} type="checkbox" /><span>{c.publishOrder}</span></label>
                    <div class="order-actions"><button type="submit">{c.reviewOrder}</button><button type="button" class="secondary" onclick={closeListing}>{c.cancel}</button></div>
                  </form>
                {:else}
                  <p>{c.confirmSummary(row.displayName, listingQuantity, listingPrice)}</p>
                  <label class="order-visible"><input bind:checked={listingConfirmed} type="checkbox" /><span>{c.confirmCheck}</span></label>
                  <div class="order-actions"><button type="button" disabled={listingBusy || !listingConfirmed} onclick={createSetListing}>{listingBusy ? c.creatingOrder : c.createOrder}</button><button type="button" class="secondary" disabled={listingBusy} onclick={closeListing}>{c.cancel}</button></div>
                {/if}
              </section>
            {/if}

            <details class="set-composition">
              <summary>{c.composition}</summary>
              <div class="table-scroll">
                <table>
                  <thead><tr><th>{c.part}</th><th>{c.oneSetNeeds}</th><th>{c.ownedForSet}</th><th>{c.missingForSet}</th><th>{c.price}</th></tr></thead>
                  <tbody>
                    {#each row.components as component (component.definition.slug)}
                      <tr>
                        <th scope="row"><span class="item-name">{#if component.imageUrl}<img src={component.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{component.displayName}</span></th>
                        <td>{component.definition.requiredQuantity}</td>
                        <td>{component.sellableQuantity}</td>
                        <td>{Math.max(0, component.definition.requiredQuantity * (opportunity.sellableCompleteSets + 1) - component.sellableQuantity)}</td>
                        <td>{formatPlatinum(component.recommendation?.fairPrice ?? null, $locale)}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            </details>
          </article>
        {:else}
          <div class="message empty-result">
            <h2>{c.noResults}</h2>
            <p>{setQuery ? c.noSearchResults : activeMode === "relics" ? c.noRelicResults(ownedRelicCount) : activeMode === "complete_sets" ? c.noBuyResults : c.noReadyResults}</p>
            {#if setQuery}<button type="button" onclick={() => (setQuery = "")}>{c.clearSearch}</button>{/if}
          </div>
        {/each}
      </div>
      {#if setRows.length > SET_PAGE_SIZE}
        <div class="list-footer">
          <span>{c.setResultCount(visibleSetRows.length, setRows.length)}</span>
          <button type="button" class="secondary" onclick={() => (showAllSetRows = !showAllSetRows)}>
            {showAllSetRows ? c.showLess : c.showMore(remainingSetRows)}
          </button>
        </div>
      {/if}
    {/if}
    {/if}
  {/if}
</section>

<style>
  .overview-empty { padding: 1rem; }
  .overview-empty p { color: var(--text-muted); margin: .5rem 0 .75rem; }
  .unrated-heading { list-style: none; padding: .7rem; background: var(--surface-2); }
  .opportunities {
    display: grid;
    min-width: 0;
    gap: .65rem;
  }

  .resource-mode[hidden] {
    display: none;
  }

  .opportunity-navigation {
    position: sticky;
    inset-block-start: -.1rem;
    z-index: 5;
    border: 1px solid var(--border);
    border-radius: .72rem;
    padding: .28rem;
    background: color-mix(in oklch, var(--surface-1) 92%, transparent);
    box-shadow: var(--shadow-sm);
    backdrop-filter: blur(.65rem);
  }

  .mode-switcher {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: .2rem;
  }

  .mode-switcher button,
  .scenario-switcher button {
    min-width: 0;
    border-color: transparent;
    background: transparent;
    color: var(--text-muted);
    transition: background-color 120ms ease-out, border-color 120ms ease-out, color 120ms ease-out;
  }

  .mode-switcher button {
    min-height: 2rem;
    overflow: hidden;
    border-radius: .48rem;
    padding: .35rem .48rem;
    font-size: .75rem;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .mode-switcher button:hover,
  .scenario-switcher button:hover {
    border-color: var(--border);
    background: var(--surface-2);
    color: var(--text);
  }

  .mode-switcher button[aria-pressed="true"],
  .scenario-switcher button[aria-pressed="true"] {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent-strong);
    box-shadow: inset 0 0 0 1px color-mix(in oklch, var(--accent) 26%, transparent);
  }

  .mode-switcher span {
    display: inline-grid;
    place-items: center;
    min-width: 1.25rem;
    margin-inline-start: .3rem;
    border-radius: 999px;
    padding-inline: .25rem;
    background: color-mix(in oklch, currentColor 10%, transparent);
    font-size: .75rem;
    font-variant-numeric: tabular-nums;
  }

  .data-status {
    min-height: 1rem;
    padding-inline: .15rem;
    color: var(--text-muted);
    font-size: .75rem;
  }

  .message {
    border: 1px solid var(--border);
    border-radius: .75rem;
    padding: .85rem;
    background: var(--surface-2);
    box-shadow: var(--shadow-sm);
  }

  .message h2,
  .message p {
    margin: 0;
  }

  .message h2 {
    font-size: 1rem;
  }

  .message p {
    max-width: 68ch;
    margin-block-start: .25rem;
    color: var(--text-muted);
    font-size: .8rem;
  }

  .message button {
    margin-block-start: .65rem;
  }

  .message--error {
    border-color: var(--danger);
    background: var(--danger-soft);
  }

  .message--action {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: .75rem;
  }

  .message--action button {
    flex: none;
    margin: 0;
  }

  .opportunity-overview,
  .relic-ranking,
  .mode-heading,
  .set-card,
  .ducat-panel {
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: .8rem;
    background: var(--surface-1);
    box-shadow: var(--shadow-sm);
  }

  .opportunity-overview,
  .relic-ranking,
  .ducat-panel {
    overflow: hidden;
  }

  .overview-heading {
    padding: .72rem .8rem;
    border-block-end: 1px solid var(--border);
    background: var(--surface-2);
  }

  .overview-heading h2,
  .overview-heading p,
  .overview-card h3,
  .overview-card p {
    margin: 0;
  }

  .overview-heading h2 {
    font-size: 1.02rem;
    line-height: 1.25;
  }

  .overview-heading > div > p:last-child {
    max-width: 72ch;
    margin-block-start: .2rem;
    color: var(--text-muted);
    font-size: .76rem;
  }

  .section-kicker,
  .overview-card__kind {
    color: var(--accent-strong);
    font-size: .75rem;
    font-weight: 800;
    letter-spacing: .055em;
    text-transform: uppercase;
  }

  .section-kicker {
    margin-block-end: .14rem !important;
  }

  .overview-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 16rem), 1fr));
    gap: .55rem;
    padding: .65rem;
  }

  .overview-card {
    display: grid;
    grid-template-rows: auto auto auto 1fr auto;
    align-content: start;
    min-width: 0;
    gap: .35rem;
    border: 1px solid var(--border);
    border-radius: .65rem;
    padding: .68rem;
    background: var(--surface-2);
  }

  .overview-card--accent {
    border-color: color-mix(in oklch, var(--accent) 55%, var(--border));
    background: color-mix(in oklch, var(--accent-soft) 68%, var(--surface-1));
    box-shadow: inset .2rem 0 0 var(--accent);
  }

  .overview-card h3 {
    overflow-wrap: anywhere;
    font-size: .94rem;
    line-height: 1.25;
  }

  .overview-card__identity {
    display: flex;
    align-items: center;
    min-width: 0;
    gap: .5rem;
  }

  .overview-card__identity img {
    flex: none;
    width: 2.5rem;
    height: 2.5rem;
    border-radius: .4rem;
    object-fit: contain;
    background: var(--surface-1);
    outline: 1px solid oklch(0 0 0 / .1);
    outline-offset: -1px;
  }

  .overview-card__meta {
    color: var(--text-muted);
    font-size: .75rem;
  }

  .overview-card dl {
    align-self: end;
    margin: .12rem 0 0;
  }

  .overview-card dl div {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: .5rem;
  }

  .overview-card dl dd {
    color: var(--accent-strong);
    font-size: 1.18rem;
  }

  .overview-card button {
    justify-self: start;
    margin-block-start: .25rem;
  }

  .mode-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: .8rem;
    padding: .68rem .75rem;
  }

  .mode-heading h2,
  .mode-heading p {
    margin: 0;
  }

  .mode-heading h2 {
    font-size: .98rem;
  }

  .mode-heading p {
    max-width: 72ch;
    margin-block-start: .18rem;
    color: var(--text-muted);
    font-size: .75rem;
  }

  .set-search {
    display: grid;
    flex: 0 1 18rem;
    gap: .2rem;
    color: var(--text);
    font-size: .75rem;
    font-weight: 700;
  }

  .set-search input {
    min-height: 2.1rem;
    width: 100%;
    border: 1px solid var(--border);
    border-radius: .48rem;
    padding-inline: .6rem;
    background: oklch(0.995 0.004 84);
    color: var(--text);
  }

  .relic-ranking__header {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: .75rem;
    padding: .72rem .8rem;
    background: var(--surface-2);
  }

  .relic-ranking__header h2,
  .relic-ranking__header p {
    margin: 0;
  }

  .relic-ranking__header h2 {
    font-size: 1rem;
  }

  .relic-ranking__header p {
    max-width: 70ch;
    margin-block-start: .18rem;
    color: var(--text-muted);
    font-size: .75rem;
  }

  .relic-ranking__controls {
    display: flex;
    flex: none;
    align-items: center;
    gap: .4rem;
  }

  .scenario-switcher {
    display: flex;
    gap: .15rem;
    border: 1px solid var(--border);
    border-radius: .5rem;
    padding: .15rem;
    background: var(--surface-1);
  }

  .scenario-switcher button {
    min-height: 1.9rem;
    border-radius: .36rem;
    padding: .3rem .48rem;
    font-size: .75rem;
  }

  .relic-ranking__list {
    display: grid;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .relic-ranking__item {
    display: grid;
    grid-template-columns: 1.55rem minmax(10rem, 1fr) minmax(20rem, 1.5fr) minmax(8rem, auto);
    align-items: center;
    gap: .55rem;
    min-width: 0;
    padding: .52rem .72rem;
    border-block-start: 1px solid var(--border);
  }

  .relic-ranking__item--best {
    background: var(--accent-soft);
    box-shadow: inset .2rem 0 0 var(--accent);
  }

  .relic-rank {
    display: grid;
    place-items: center;
    width: 1.4rem;
    height: 1.4rem;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--accent-strong);
    font-size: .75rem;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }

  .relic-ranking__item--best .relic-rank {
    background: var(--accent);
    color: oklch(0.985 0.009 84);
  }

  .relic-ranking__identity {
    display: flex;
    align-items: center;
    min-width: 0;
    gap: .48rem;
  }

  .relic-ranking__identity img {
    flex: none;
    width: 2.55rem;
    height: 2.55rem;
    border-radius: .35rem;
    object-fit: contain;
    outline: 1px solid oklch(0 0 0 / .1);
    outline-offset: -1px;
  }

  .relic-ranking__identity h3,
  .relic-ranking__identity p {
    margin: 0;
  }

  .relic-ranking__identity h3 {
    font-size: .84rem;
    line-height: 1.25;
  }

  .relic-ranking__identity p {
    margin-block-start: .1rem;
    color: var(--text-muted);
    font-size: .75rem;
  }

  .relic-ranking__metrics {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: .3rem;
    margin: 0;
  }

  .relic-ranking__metrics > div {
    min-width: 0;
    border-inline-start: 1px solid var(--border);
    padding-inline: .5rem;
  }

  .relic-ranking__metrics dd {
    font-size: .88rem;
  }

  .relic-ranking__metrics small {
    overflow-wrap: anywhere;
  }

  .relic-ranking__action dd {
    color: var(--accent-strong);
  }

  .relic-ranking__decision {
    min-width: 8.5rem;
    max-width: 12rem;
    margin: 0;
    border: 1px solid var(--accent);
    border-radius: .45rem;
    padding: .35rem .48rem;
    background: var(--surface-1);
    color: var(--accent-strong);
    font-size: .75rem;
    font-weight: 750;
    line-height: 1.25;
    text-align: center;
  }

  .relic-ranking__decision--muted {
    border-color: var(--border);
    background: var(--surface-2);
    color: var(--text-muted);
  }

  .relic-ranking__details {
    border-block-start: 1px solid var(--border);
    padding-inline: .8rem;
  }

  .relic-ranking__details p {
    max-width: 80ch;
    margin: 0 0 .65rem;
    color: var(--text-muted);
    font-size: .75rem;
  }

  .relic-ranking__empty {
    margin: 0;
    border-block-start: 1px solid var(--border);
    padding: .8rem;
    color: var(--text-muted);
    font-size: .78rem;
  }

  .set-list {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 32rem), 1fr));
    align-items: start;
    gap: .65rem;
  }

  .set-card {
    padding: .68rem;
  }

  .set-card__header {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: .65rem;
    margin-block-end: .55rem;
  }

  .set-identity {
    display: flex;
    align-items: center;
    min-width: 0;
    gap: .58rem;
  }

  .set-image {
    flex: none;
    width: 3rem;
    height: 3rem;
    border-radius: .42rem;
    object-fit: contain;
    outline: 1px solid oklch(0 0 0 / .1);
    outline-offset: -1px;
  }

  .set-context {
    margin: 0 0 .08rem;
    color: var(--text-muted);
    font-size: .75rem;
  }

  .set-identity h2 {
    margin: 0;
    overflow-wrap: anywhere;
    font-size: .98rem;
    line-height: 1.25;
  }

  .sale-route {
    flex: none;
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: .2rem .45rem;
    background: var(--surface-2);
    color: var(--text-muted);
    font-size: .75rem;
    font-weight: 750;
    white-space: nowrap;
  }

  .sale-route--set {
    border-color: color-mix(in oklch, var(--success) 55%, var(--border));
    background: var(--success-soft);
    color: oklch(0.34 0.08 145);
  }

  .sale-route--parts {
    border-color: color-mix(in oklch, var(--gold) 55%, var(--border));
    background: var(--accent-soft);
    color: var(--accent-strong);
  }

  .set-metrics {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: .35rem;
    margin: 0;
  }

  .set-metrics > div {
    min-width: 0;
    border-radius: .48rem;
    padding: .46rem;
    background: var(--surface-2);
  }

  dt {
    color: var(--text-muted);
    font-size: .75rem;
  }

  dd {
    margin: .12rem 0 0;
    font-size: .96rem;
    font-weight: 780;
    font-variant-numeric: tabular-nums;
  }

  dd small {
    display: block;
    margin-block-start: .08rem;
    color: var(--text-muted);
    font-size: .75rem;
    font-weight: 650;
    line-height: 1.25;
  }

  dd.metric-unavailable {
    color: var(--text-muted);
    font-size: .75rem;
    line-height: 1.25;
  }

  .positive dd {
    color: oklch(0.37 0.08 145);
  }

  .decision-copy {
    margin: .52rem 0 0;
    color: var(--text-muted);
    font-size: .75rem;
  }

  .missing-parts {
    display: flex;
    flex-wrap: wrap;
    gap: .32rem;
    margin-block-start: .5rem;
  }

  .missing-parts span {
    border-radius: 999px;
    padding: .2rem .45rem;
    background: var(--accent-soft);
    color: var(--accent-strong);
    font-size: .75rem;
  }

  .missing-parts strong {
    margin-inline-start: .3rem;
    font-variant-numeric: tabular-nums;
  }

  .card-actions,
  .order-actions {
    display: flex;
    flex-wrap: wrap;
    gap: .4rem;
    margin-block-start: .58rem;
  }

  .card-actions button,
  .order-actions button {
    flex: 0 1 auto;
    transition: scale 120ms ease-out, background-color 120ms ease-out, border-color 120ms ease-out;
  }

  .card-actions button:active,
  .order-actions button:active {
    scale: .97;
  }

  .card-status,
  .live-set-error {
    margin: .55rem 0 0;
    border-radius: .5rem;
    padding: .45rem .58rem;
    font-size: .75rem;
    font-weight: 680;
  }

  .card-status {
    background: var(--success-soft);
    color: var(--success);
  }

  .card-status--error,
  .live-set-error {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .live-set-orders {
    margin-block-start: .58rem;
    overflow: hidden;
    border-radius: .6rem;
    background: var(--surface-2);
    box-shadow: 0 0 0 1px var(--border);
  }

  .live-set-orders > header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: .7rem;
    padding: .5rem .6rem .32rem;
  }

  .live-set-orders h3,
  .live-set-orders p {
    margin: 0;
  }

  .live-set-orders h3 {
    font-size: .8rem;
  }

  .live-set-orders p {
    color: var(--text-muted);
    font-size: .75rem;
    text-align: end;
  }

  .live-set-orders table {
    font-size: .75rem;
  }

  .live-set-orders th,
  .live-set-orders td {
    padding: .35rem .6rem;
  }

  .live-set-orders tbody strong {
    color: var(--accent-strong);
    font-size: .8rem;
    font-variant-numeric: tabular-nums;
  }

  .relic-plan {
    margin-block-start: .65rem;
    border-radius: .6rem;
    padding: .6rem;
    background: var(--surface-2);
  }

  .relic-plan__summary {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: baseline;
    gap: .18rem .7rem;
  }

  .relic-plan__summary span {
    font-size: .75rem;
    font-weight: 700;
  }

  .relic-plan__summary strong {
    color: var(--accent-strong);
    font-size: 1.05rem;
    font-variant-numeric: tabular-nums;
  }

  .relic-plan__summary small {
    grid-column: 1 / -1;
    color: var(--text-muted);
    font-size: .75rem;
  }

  .relic-list {
    display: grid;
    gap: .45rem;
    margin-block-start: .55rem;
  }

  .relic-row {
    display: grid;
    grid-template-columns: minmax(11rem, 1.2fr) minmax(12rem, 1fr);
    gap: .5rem .7rem;
    border-radius: .5rem;
    padding: .5rem;
    background: var(--surface-1);
    box-shadow: 0 0 0 1px oklch(0 0 0 / .06);
  }

  .relic-identity {
    display: flex;
    align-items: center;
    min-width: 0;
    gap: .5rem;
  }

  .relic-identity img {
    flex: none;
    width: 2.55rem;
    height: 2.55rem;
    border-radius: .35rem;
    object-fit: contain;
    outline: 1px solid oklch(0 0 0 / .1);
    outline-offset: -1px;
  }

  .relic-identity h3,
  .relic-identity p {
    margin: 0;
  }

  .relic-identity h3 {
    font-size: .84rem;
  }

  .relic-identity p {
    margin-block-start: .1rem;
    color: var(--text-muted);
    font-size: .75rem;
  }

  .relic-row dl {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: .28rem;
    margin: 0;
  }

  .relic-row dl div {
    min-width: 0;
    border-radius: .4rem;
    padding: .35rem;
    background: var(--surface-2);
  }

  .relic-row dd {
    font-size: .82rem;
  }

  .useful-rewards {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: .32rem;
  }

  .useful-rewards > span {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: 0 .38rem;
    border-radius: .42rem;
    padding: .28rem .42rem;
    background: var(--success-soft);
    color: oklch(0.32 0.065 145);
    font-size: .75rem;
    font-weight: 700;
  }

  .useful-rewards img {
    grid-row: 1 / 3;
    width: 1.7rem;
    height: 1.7rem;
    object-fit: contain;
    outline: 1px solid oklch(0 0 0 / .1);
    outline-offset: -1px;
  }

  .useful-rewards small {
    color: oklch(0.43 0.05 145);
    font-size: .75rem;
    font-weight: 650;
  }

  .order-panel {
    margin-block-start: .62rem;
    border-radius: .6rem;
    padding: .6rem;
    background: var(--surface-2);
    box-shadow: 0 0 0 1px var(--border);
  }

  .order-panel h3,
  .order-panel p {
    margin: 0;
  }

  .order-panel h3 {
    font-size: .88rem;
  }

  .order-panel p {
    margin-block-start: .3rem;
    color: var(--text-muted);
    font-size: .76rem;
  }

  .order-panel form {
    display: grid;
    gap: .45rem;
    margin-block-start: .45rem;
  }

  .order-fields {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: .4rem;
  }

  .order-fields label {
    display: grid;
    gap: .22rem;
    font-size: .75rem;
    font-weight: 700;
  }

  .order-fields input {
    min-width: 0;
    min-height: 2.15rem;
    width: 100%;
    border: 1px solid var(--border);
    border-radius: .45rem;
    padding-inline: .52rem;
    background: oklch(0.995 0.004 84);
    color: var(--text);
  }

  .order-visible {
    display: flex;
    align-items: center;
    width: fit-content;
    min-height: 2.05rem;
    gap: .42rem;
    font-size: .75rem;
    font-weight: 700;
    cursor: pointer;
  }

  .order-visible input {
    width: 1.1rem;
    height: 1.1rem;
    accent-color: var(--accent);
  }

  .inline-error {
    color: var(--danger) !important;
    font-size: .75rem;
  }

  .set-composition {
    margin-block-start: .38rem;
  }

  summary {
    min-height: 2rem;
    padding-block: .38rem;
    color: var(--accent-strong);
    cursor: pointer;
    font-size: .75rem;
    font-weight: 700;
  }

  .table-scroll {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: .78rem;
  }

  th,
  td {
    border-block-end: 1px solid var(--border);
    padding: .42rem .48rem;
    text-align: start;
    font-variant-numeric: tabular-nums;
  }

  thead th {
    color: var(--text-muted);
    font-size: .75rem;
    text-transform: uppercase;
    letter-spacing: .035em;
  }

  tbody th {
    font-weight: 680;
  }

  .item-name {
    display: inline-flex;
    align-items: center;
    gap: .48rem;
  }

  .item-name img {
    flex: none;
    width: 1.9rem;
    height: 1.9rem;
    object-fit: contain;
    outline: 1px solid oklch(0 0 0 / .1);
    outline-offset: -1px;
  }

  .ducat-panel > header {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: .8rem;
    padding: .72rem .8rem;
    background: var(--surface-2);
  }

  .ducat-panel th:first-child, .set-composition th:first-child { width: 40%; }
  .ducat-panel tbody th, .set-composition tbody th { font-size: .875rem; letter-spacing: normal; text-transform: none; }

  .ducat-panel h2,
  .ducat-panel p {
    margin: 0;
  }

  .ducat-panel h2 {
    font-size: 1rem;
  }

  .ducat-panel header div p {
    margin-block-start: .2rem;
    color: var(--text-muted);
    font-size: .75rem;
  }

  .ducat-panel .warning {
    max-width: 28rem;
    color: var(--danger);
    font-size: .75rem;
    text-align: end;
  }

  .list-footer {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: .6rem;
    border-block-start: 1px solid var(--border);
    padding: .5rem .65rem;
    color: var(--text-muted);
    font-size: .75rem;
  }

  .set-list + .list-footer {
    width: fit-content;
    margin-inline: auto;
    border: 1px solid var(--border);
    border-radius: .58rem;
    background: var(--surface-1);
    box-shadow: var(--shadow-sm);
  }

  .empty-result {
    text-align: center;
  }

  :is(.mode-switcher, .scenario-switcher) button:focus-visible,
  .set-search input:focus-visible,
  summary:focus-visible {
    outline: .15rem solid color-mix(in oklch, var(--accent) 75%, white);
    outline-offset: .12rem;
  }

  @media (max-width: 70rem) {
    .mode-switcher {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }

    .relic-ranking__item {
      grid-template-columns: 1.55rem minmax(0, 1fr) auto;
    }

    .relic-ranking__metrics {
      grid-column: 2 / -1;
      width: 100%;
    }

    .relic-ranking__decision {
      grid-column: 3;
      grid-row: 1;
    }
  }

  @media (max-width: 54rem) {
    .message--action,
    .mode-heading,
    .relic-ranking__header {
      align-items: stretch;
      flex-direction: column;
    }

    .set-search {
      flex-basis: auto;
      width: 100%;
    }

    .message--action button {
      align-self: start;
    }

    .relic-ranking__controls {
      justify-content: space-between;
    }

    .relic-row {
      grid-template-columns: minmax(0, 1fr);
    }

    .useful-rewards {
      grid-column: 1;
    }
  }

  @media (max-width: 42rem) {
    .mode-switcher {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .relic-ranking__controls {
      align-items: stretch;
      flex-direction: column;
    }

    .scenario-switcher {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .relic-ranking__controls > button {
      align-self: start;
    }

    .relic-ranking__item {
      grid-template-columns: 1.55rem minmax(0, 1fr);
    }

    .relic-ranking__metrics {
      grid-column: 1 / -1;
    }

    .relic-ranking__decision {
      grid-column: 2;
      grid-row: auto;
      justify-self: start;
      max-width: none;
    }

    .set-metrics {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .card-actions button {
      flex: 1 1 11rem;
    }

    .live-set-orders > header,
    .ducat-panel > header {
      align-items: start;
      flex-direction: column;
    }

    .live-set-orders p,
    .ducat-panel .warning {
      text-align: start;
    }
  }

  @media (max-width: 30rem) {
    .order-fields,
    .relic-row dl,
    .relic-ranking__metrics {
      grid-template-columns: minmax(0, 1fr);
    }

    .relic-ranking__metrics > div {
      border-inline-start: 0;
      border-block-start: 1px solid var(--border);
      padding: .35rem 0 0;
    }

    .set-card {
      padding: .6rem;
    }

    .list-footer {
      align-items: stretch;
      flex-direction: column;
      text-align: center;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .mode-switcher button,
    .scenario-switcher button,
    .card-actions button,
    .order-actions button {
      transition: none;
    }
  }

  @media (forced-colors: active) {
    .opportunity-navigation,
    .overview-card,
    .set-card,
    .relic-row,
    .order-panel,
    .relic-ranking,
    .mode-switcher button[aria-pressed="true"],
    .scenario-switcher button[aria-pressed="true"] {
      outline: 1px solid CanvasText;
    }
  }
</style>
