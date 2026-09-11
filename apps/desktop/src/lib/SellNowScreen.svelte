<script module lang="ts">
  import type { SellNowView as CachedSellNowView } from "./sellNow";

  let cachedSellNowView: CachedSellNowView | null = null;
  let cachedItemMode: "inventory" | "mastery" = "inventory";
  const cachedChecks = new Map<string, import("./sellNow").LiveSellNowResult>();
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import { revealCompactDetail, revealElement } from "./detailNavigation";
  import { localeCode, useLocale, type AppSettings } from "./i18n";
  import MasteryScreen from "./MasteryScreen.svelte";
  import InventoryAutoRefresh from "./InventoryAutoRefresh.svelte";
  import KeepCopiesControl from "./KeepCopiesControl.svelte";
  import { inventoryScanErrorMessage } from "./inventoryRefresh";
  let refreshingInBackground = false;
  import { masteryStore } from "./mastery";
  import {
    accountActionErrorMessage,
    createListingInputFromInventory,
    matchingSellOrder,
    validateListingNumbers,
    visibilityLabel,
    type AccountOrder,
    type AccountView,
    type CreateListingInput,
  } from "./account";
  import {
    INVENTORY_CATEGORIES,
    inventoryListingQuantity,
    listingReserveWarning,
    inventoryCategory,
    inventorySourceLabel,
    resolutionLabel,
    type InventoryCategoryFilter,
  } from "./inventory";
  import { inventoryForNewListing } from "./tradeShift";

  import {
    formatPlatinum,
    variantLabel,
  } from "./market";
  import {
    filterAndSortSellNowRows,
    resolveSellNowSelection,
    sellNowRowDomKey,
    sellNowRowIdentity,
    withCheckedPrice,
    isCheckedPriceCurrent,
    inventoryPage,
    inventoryUnitPrice,
    type LiveSellNowResult,
    type SellNowPreset,
    type SellNowRow,
    type SellNowSortDirection,
    type SellNowSortKey,
    type SellNowView,
  } from "./sellNow";
  import {
    DEFAULT_SELL_NOW_VIEW,
    loadSellNowViewPreferences,
    saveSellNowViewPreferences,
  } from "./viewPreferences";

  export let initialQuery = "";
  export let onInventoryChange: (() => void) | undefined = undefined;
  export let onOpenMarketSales: () => void;
  type PendingListingAction = { kind: "create"; input: CreateListingInput; itemName: string; reserveWarning: string | null };

  const locale = useLocale();
  const categoryCopy = {
    ru: { mod: "Моды", arcane_enhancement: "Мистификаторы", relic: "Реликвии", component: "Компоненты", weapon: "Оружие", warframe: "Варфреймы", misc: "Прочее" },
    en: { mod: "Mods", arcane_enhancement: "Arcanes", relic: "Relics", component: "Components", weapon: "Weapons", warframe: "Warframes", misc: "Other" },
  } as const;
  const copy = {
    ru: {
      matching: "Загружаем ваши предметы…", refreshingItems: "Обновляем цены и расчёты…", shown: (visible: number, total: number) => `${visible} из ${total} предметов показано`,
      loadError: (_reason: string) => "Не удалось открыть инвентарь. Сохранённые данные не изменились.",
      missingVariant: "Этот вариант больше не найден в инвентаре. Обновите список.", liveError: (_reason: string) => "Не удалось получить текущую цену. Сохранённая оценка не изменилась.",
      noSignal: "Нет рекомендации", retry: "Повторить", notImported: "Инвентарь не обновлён", addSnapshot: "Сначала обновите инвентарь",
      addSnapshotBody: "Запустите Warframe и обновите инвентарь. PlatScope покажет все найденные предметы и рекомендации по продаже.", openInventory: "Обновить инвентарь",
      summary: "Мои предметы", totalCopies: "Всего копий", candidates: "Позиций к продаже", recommended: "Продавать сейчас", priced: "С рассчитанной ценой", highPriority: "В первую очередь", nominal: "Ориентировочная сумма",
      inventoryUpdated: "Инвентарь обновлён", scanInventory: "Обновить из Warframe", scanningInventory: "Обновляем…", scanError: "Не удалось обновить инвентарь. Запустите Warframe, войдите в игру и повторите.", reserve: "Оставлять копий", reserveError: "Не удалось изменить резерв копий.",
      notForecast: "Важно:", nominalBody: "это сумма по текущим оценкам, а не гарантированная выручка.",
      noCandidatesLabel: "Очередь пуста", noCandidates: "Нет предметов, готовых к продаже", noCandidatesBody: "Проверьте количество, возможность обмена и резерв копий в инвентаре.", checkInventory: "Открыть инвентарь",
      filters: "Поиск и фильтры", search: "Поиск предмета", searchExample: "Например, Поток Прайм", category: "Тип предмета", allCategories: "Все типы", view: "Показывать", allCandidates: "Весь торговый инвентарь", sellable: "Всё к продаже", sellNow: "Продавать сейчас", hold: "Лучше подождать", duplicates: "Дубликаты", unpriced: "Без цены", attention: "Требуют проверки", usage: "Использование", allUsage: "Все", freeOnly: "Не надеты", equippedOnly: "Надеты", equippedCount: (count: number) => `Надето: ${count}`,
      queue: "Мои предметы", exactSnapshot: (date: string) => `Цены рынка от ${date}`, missingSnapshot: "дата неизвестна",
      tableCaption: "Предметы инвентаря, количество, рекомендуемая цена, продажи и время продажи", item: "Предмет", salesPerDay: "Продажи / день", priceTrend90: "Тренд цены / 90 дней", unknownVariant: "вариант не определён",
      dailyTrades: (value: string) => `${value} сделок/день`,
      priceUp: (value: string) => `Цена растёт ↑ ${value} за 90 дней`, priceDown: (value: string) => `Цена падает ↓ ${value} за 90 дней`, priceFlat: "Цена стабильна →", noPriceTrend: "Нет данных о цене за 90 дней",
      noFiltered: "Предметы не найдены", changeFilters: "Измените запрос или сбросьте фильтры.", reset: "Сбросить фильтры",
      candidate: "Предмет инвентаря", gettingLive: "Проверяем текущие цены…", updateLive: "Обновить текущие цены", getLive: "Проверить текущие цены",
      liveHint: "Покажет активные ордера на продажу и покупку для этого варианта.", forSale: "Можно выставить", of: "из", moment: "Когда продавать",
      nominalWarning: "Скорость продажи зависит от спроса и вашей цены.", whyPrice: "Как рассчитана цена?", noPriceSignal: "Сделок пока недостаточно для расчёта цены.", whyPriority: "Почему этот предмет выше или ниже?", details: "Подробности продажи", selectCandidate: "Выберите предмет в очереди.",
      ownedSellable: "Есть / продать", owned: "есть", sellableLabel: "можно продать", fairListQuick: "Выставить по цене",
      noPrice: "нет данных",
      priority: "Приоритет продажи", priorityHint: "№1 — предмет, который стоит выставить первым.", priorityPosition: (rank: number | null, score: number) => rank === null ? `Приоритет не рассчитан · ${score}/100` : `№${rank} в очереди · ${score}/100`, fairPrice: "Оценка рынка", listPrice: "Ориентир размещения", closedVolume: "Закрытые сделки", lowestAsk: "Минимальная цена продажи", depthThree: "Средняя цена до 3 шт.", depthPrice: "Средняя цена до 5 шт.", quickSell: "Лучшая заявка на покупку", sell: "на продажу", buy: "на покупку", currentOrders: "Ордера игроков в игре", side: "Тип", price: "Цена", quantityLot: "Количество · лот", playerStatus: "Статус", sellOrder: "Продажа", buyOrder: "Покупка", noActiveOrders: "Сейчас в игре нет ордеров для этого варианта.", freshness: "Актуальность", dataDate: "Цена рассчитана по данным от",
      wfmOrder: "Ордер Warframe Market", loadingOrders: "Проверяем ваши ордера…", accountUnavailable: "Не удалось загрузить ордера Warframe Market.", retryOrders: "Повторить", accountDisconnected: "Warframe Market не подключён", accountDisconnectedBody: "Подключите аккаунт, чтобы выставить этот предмет.", openAccount: "Подключить Warframe Market", unverifiedAccount: "Подтвердите игровой аккаунт на Warframe Market, чтобы менять ордера.", notSellable: "После резерва нет подтверждённых копий для продажи.", noCurrentOrder: "Ордер ещё не выставлен", currentOrder: (price: string, quantity: number, status: string, perTrade: number | null) => perTrade === null ? `Выставлено: ${price}p × ${quantity} · ${status}` : `Выставлено: ${quantity} шт., по ${perTrade} за сделку за ${price}p · ${status}`,
      manageOrder: "Управлять ордером", orderPrice: "Цена, платина", bulkOrderPrice: "Цена за 1 предмет, платина", orderQuantity: "Всего предметов", orderPerTrade: "Предметов за одну сделку", publishOrder: "Сразу показать ордер на рынке", reviewCreate: "Проверить ордер", variantUnavailable: "Этот вариант не найден на Warframe Market. Обновите рыночные данные.", createTitle: "Подтвердите новый ордер", confirmCreate: (name: string, price: number, quantity: number, perTrade: number | null) => perTrade === null ? `${name}: выставить ${quantity} шт. по ${price}p.` : `${name}: всего ${quantity} шт., по ${perTrade} за сделку за ${price}p за лот.`, confirmChecked: "Я проверил предмет, цену и количество", createOrder: "Создать ордер", cancelOrderAction: "Отменить", confirmRequired: "Подтвердите, что проверили параметры ордера.", orderCreated: "Ордер создан на Warframe Market.", orderActionError: (reason: string) => accountActionErrorMessage(reason, "ru"),
    },
    en: {
      matching: "Loading your items…", refreshingItems: "Refreshing prices and calculations…", shown: (visible: number, total: number) => `${visible} of ${total} items shown`,
      loadError: (_reason: string) => "Unable to open inventory. Saved data was not changed.",
      missingVariant: "The exact variant is no longer sellable. Refresh the list.", liveError: (reason: string) => `Current price unavailable; the local estimate was preserved. ${reason}`,
      noSignal: "No signal", retry: "Recalculate", notImported: "Inventory not imported", addSnapshot: "Add a local inventory snapshot",
      addSnapshotBody: "Start Warframe and update inventory to see all items and sell recommendations.", openInventory: "Update inventory",
      summary: "My items", totalCopies: "Total copies", candidates: "Sellable items", recommended: "Sell now", priced: "Priced", highPriority: "High priority", nominal: "Nominal value",
      inventoryUpdated: "Inventory updated", scanInventory: "Update from Warframe", scanningInventory: "Updating…", scanError: "Unable to update inventory. Start Warframe, sign in, and try again.", reserve: "Keep copies", reserveError: "Unable to change the copy reserve.",
      notForecast: "Not a revenue forecast:", nominalBody: "nominal value is sellable × fair. It does not guarantee that the full volume will sell at that price.",
      noCandidatesLabel: "No candidates", noCandidates: "No confirmed items to sell", noCandidatesBody: "Check the import, tradeability, and copy reserve. Ambiguous variants are excluded automatically.", checkInventory: "Check inventory",
      filters: "Item search and filters", search: "Search items", searchExample: "For example, Primed Flow", category: "Item type", allCategories: "All types", view: "Show", allCandidates: "All market inventory", sellable: "All sellable", sellNow: "Sell now", hold: "Better to wait", duplicates: "Duplicates", unpriced: "Unpriced", attention: "Needs review", usage: "Usage", allUsage: "All", freeOnly: "Not equipped", equippedOnly: "Equipped", equippedCount: (count: number) => `Equipped: ${count}`,
      queue: "My items", exactSnapshot: (date: string) => `Market prices from ${date}`, missingSnapshot: "not found",
      tableCaption: "Inventory items, quantity, recommended price, sales, and sale timing", item: "Item", salesPerDay: "Sales / day", priceTrend90: "Price trend / 90 days", unknownVariant: "variant unavailable",
      dailyTrades: (value: string) => `${value} trades/day`,
      priceUp: (value: string) => `Price rising ↑ ${value} over 90 days`, priceDown: (value: string) => `Price falling ↓ ${value} over 90 days`, priceFlat: "Price stable →", noPriceTrend: "No 90-day price data",
      noFiltered: "No candidates match these filters", changeFilters: "Change the item type or another filter.", reset: "Reset filters",
      candidate: "Inventory item", gettingLive: "Getting current price…", updateLive: "Refresh current price", getLive: "Get current price",
      liveHint: "One request for the selected exact variant. Quick Sell is never replaced with a historical buy price.", forSale: "For sale", of: "of", moment: "Timing",
      nominalWarning: "Nominal value does not account for how quickly the full volume may sell.", whyPrice: "Why this price?", noPriceSignal: "There are not enough completed trades to calculate a price.", whyPriority: "Why this priority?", details: "Sell details", selectCandidate: "Select a candidate from the queue.",
      ownedSellable: "Owned / list", owned: "owned", sellableLabel: "can list", fairListQuick: "Recommended price",
      noPrice: "no price",
      priority: "Sale priority", priorityHint: "No. 1 is the item to list first.", priorityPosition: (rank: number | null, score: number) => rank === null ? `Priority unavailable · ${score}/100` : `No. ${rank} in the queue · ${score}/100`, fairPrice: "Fair price", listPrice: "List price", closedVolume: "Closed trades", lowestAsk: "Lowest ask", depthThree: "Up to 3 units average", depthPrice: "Up to 5 units average", quickSell: "Best buy order", sell: "sell", buy: "buy", currentOrders: "Orders from players in game", side: "Side", price: "Price", quantityLot: "Quantity · lot", playerStatus: "Status", sellOrder: "Sell", buyOrder: "Buy", noActiveOrders: "No players in game have orders for this exact variant.", freshness: "Freshness", dataDate: "Price data from",
      wfmOrder: "Warframe Market order", loadingOrders: "Checking your orders…", accountUnavailable: "Unable to load Warframe Market orders.", retryOrders: "Try again", accountDisconnected: "Warframe Market is not connected", accountDisconnectedBody: "Connect your account to list this item.", openAccount: "Connect Warframe Market", unverifiedAccount: "Verify your game account on Warframe Market to change orders.", notSellable: "There are no confirmed copies to sell after the reserve.", noCurrentOrder: "Not listed yet", currentOrder: (price: string, quantity: number, status: string, perTrade: number | null) => perTrade === null ? `Listed: ${price}p × ${quantity} · ${status}` : `Listed: ${quantity} total, ${perTrade} per trade for ${price}p · ${status}`,
      manageOrder: "Manage order", orderPrice: "Price, platinum", bulkOrderPrice: "Price per item, platinum", orderQuantity: "Total items", orderPerTrade: "Items per trade", publishOrder: "Show order on the market immediately", reviewCreate: "Review order", variantUnavailable: "This variant is not available on Warframe Market. Refresh market data.", createTitle: "Confirm new order", confirmCreate: (name: string, price: number, quantity: number, perTrade: number | null) => perTrade === null ? `${name}: list ${quantity} at ${price}p each.` : `${name}: ${quantity} total, ${perTrade} per trade for ${price}p per lot.`, confirmChecked: "I reviewed the item, price, and quantity", createOrder: "Create order", cancelOrderAction: "Cancel", confirmRequired: "Confirm that you reviewed the order parameters.", orderCreated: "Order created on Warframe Market.", orderActionError: (reason: string) => accountActionErrorMessage(reason, "en"),
    },
  } as const;
  $: c = copy[$locale];
  $: categoryLabels = categoryCopy[$locale];

  const labels = {
    ru: { all: "Все предметы", total: "Позиций в инвентаре", value: "Оценка доступных копий", search: "Название на русском или английском", free: "Есть свободные копии", price: "Оценка / шт.", check: "Проверить цену", checked: "Проверено", count: "Количество", available: "Для продажи", protected: "Почему доступно не всё?", reserve: "Резерв и защита копий", reserveHint: "Резерв задаёт рекомендуемый запас для каждого варианта. При выставлении этих копий появится предупреждение — вы сможете продолжить. Надетые и непередаваемые копии защищены отдельно.", breakdown: "Состав количества", owned: "Всего есть", tradeable: "Можно передавать", untradeable: "Нельзя передавать", unknown: "Обмен не подтверждён", equipped: "Надето", saved: "Оставлять себе", insufficient: "Нет данных о надетых модах. Обновите инвентарь, чтобы не продать используемую копию.", details: "Цена и спрос подробнее", back: "← К списку предметов", page: "Страницы инвентаря", previous: "Назад", next: "Дальше", filterCount: "Найдено", partial: "Не все доступные позиции имеют цену; сумма оценочная.", empty: "В этом снимке пока нет предметов", emptyHint: "Повторите обновление после входа в Warframe.", selectionReset: "Инвентарь изменился. Проверьте количество и подтвердите ордер заново.", show: "Показывать", reset: "Сбросить фильтры" },
    en: { all: "All items", total: "Inventory entries", value: "Estimated sellable value", search: "Russian or English item name", free: "Has unequipped copies", price: "Estimate / item", check: "Check price", checked: "Checked", count: "Quantity", available: "For sale", protected: "Why are some copies unavailable?", reserve: "Keep copies and protection", reserveHint: "The reserve is your preferred stock for each variant. Listing these copies shows a warning and lets you continue. Equipped and untradeable copies remain protected.", breakdown: "Quantity details", owned: "Owned", tradeable: "Tradeable", untradeable: "Untradeable", unknown: "Tradeability unconfirmed", equipped: "Equipped", saved: "Keep copies", insufficient: "Equipped mod data is missing. Refresh inventory to protect copies in use.", details: "Price and demand details", back: "← Back to items", page: "Inventory pages", previous: "Previous", next: "Next", filterCount: "Found", partial: "Some sellable entries have no price; this is an estimate.", empty: "This snapshot has no items yet", emptyHint: "Refresh after logging into Warframe.", selectionReset: "Inventory changed. Check the quantity and confirm the order again.", show: "Show", reset: "Reset filters" },
  } as const;
  $: u = labels[$locale];

  let view: SellNowView | null = cachedSellNowView;
  let accountView: AccountView | null = null;
  let accountLoading = true;
  let accountError = "";
  let orderBusy = false;
  let orderStatusMessage = "";
  let orderPrice = 1;
  let orderQuantity = 1;
  let orderPerTrade = 1;
  let orderVisible = true;
  let orderFormError = "";
  let orderDraftSeed = "";
  let currentOrder: AccountOrder | null = null;
  let pendingListingAction: PendingListingAction | null = null;
  let listingConfirmationError = "";
  let listingConfirmationHeading: HTMLElement;
  let listingConfirmationTrigger: HTMLElement | null = null;
  let loading = cachedSellNowView === null;
  let refreshing = false;
  let scanning = false;
  let itemMode: "inventory" | "mastery" = initialQuery ? "inventory" : cachedItemMode;
  function selectItemMode(mode: "inventory" | "mastery") { itemMode = mode; cachedItemMode = mode; }
  let errorMessage = "";
  let selectedIdentity = "";
  let checkedPrices = new Map(cachedChecks);
  let quoteNow = Date.now();
  let quoteTtlSeconds = 90;
  let liveLoading = false;
  let liveError = "";
  let query = initialQuery;
  let category: InventoryCategoryFilter = "all";
  let preset: SellNowPreset = "all";
  let sortKey: SellNowSortKey = "name";
  let detailTrigger: HTMLElement | null = null;
  let sortDirection: SellNowSortDirection = "asc";
  let viewPreferencesReady = false;
  let page = 1;
  let listScroll: HTMLDivElement | undefined;
  let detailOpen = false;
  $: t = $locale === "ru" ? (ru: string, _en: string) => ru : (_ru: string, en: string) => en;
  $: filterOptions = [
    { value: "all", label: t("Без ограничений", "No restrictions"), hint: "" },
    { value: "sellable", label: t("Есть копии для продажи", "Has sellable copies"), hint: t("Есть хотя бы одна копия после резерва и защиты надетых предметов. Наличие цены не обязательно.", "At least one copy remains after reserves and protection. A price is not required.") },
    { value: "unavailable", label: t("Нет копий для продажи", "No sellable copies"), hint: t("Копии оставлены себе, защищены или пока не распознаны для рынка.", "Copies are reserved, protected, or not yet matched to the market.") },
    { value: "duplicates", label: t("Две копии и больше", "Two or more copies"), hint: t("От двух копий одного варианта, включая оставленные себе. Разные ранги считаются отдельно.", "At least two copies of the same variant, including reserved copies. Ranks are counted separately.") },
    { value: "equipped", label: t("Есть надетые копии", "Has equipped copies"), hint: t("Предмет используется в сборках. Другие его копии могут быть доступны для продажи.", "Used in loadouts. Other copies may still be sellable.") },
    { value: "unpriced", label: t("Нет оценки цены", "No price estimate"), hint: t("Для этих вариантов нет оценки цены. Это не означает, что их нельзя продать.", "These variants have no price estimate. This does not mean they cannot be sold.") },
    { value: "attention", label: t("Не распознаны для рынка", "Needs market matching"), hint: t("Не определён точный вариант или возможность обмена. Причина указана в карточке предмета.", "The exact variant or tradeability is unknown. See the item details for the reason.") },
  ] as const;
  $: filterHint = filterOptions.find(option => option.value === preset)?.hint ?? "";
  $: sortOptions = [
    { key: "name", direction: "asc", label: t("По названию: А → Я", "Name: A → Z") },
    { key: "fair", direction: "desc", label: t("Сначала дороже", "Highest price first") },
    { key: "fair", direction: "asc", label: t("Сначала дешевле", "Lowest price first") },
    { key: "owned", direction: "desc", label: t("Больше копий в наличии", "Most owned copies first") },
    { key: "sellable", direction: "desc", label: t("Больше копий к продаже", "Most sellable copies first") },
  ] as const;
  let viewRequest = 0;
  let accountRequest = 0;
  let liveRequest = 0;
  let autoQuoteIdentity = "";
  let destroyed = false;
  let orderPriceEdited = false;
  $: filterSignature = JSON.stringify([query, category, preset, sortKey, sortDirection]);
  $: if (filterSignature) { page = 1; detailOpen = false; selectedIdentity = ""; liveError = ""; resetListScroll(); }
  $: hasFilters = Boolean(query.trim() || category !== "all" || preset !== "all");
  $: canReset = hasFilters || sortKey !== "name" || sortDirection !== "asc";
  $: displayRows = (view?.rows ?? []).map(row => withCheckedPrice(row, checkedPrices.get(sellNowRowIdentity(row)), quoteNow, quoteTtlSeconds));

  $: if (viewPreferencesReady) {
    saveSellNowViewPreferences({
      category,
      preset,
      sortKey,
      sortDirection,
    });
  }
  $: visibleRows = filterAndSortSellNowRows(displayRows, {
    query,
    category,
    preset,
    sortKey,
    sortDirection,
  });
  $: categories = INVENTORY_CATEGORIES.filter(candidate => candidate === category || view?.rows.some(row => inventoryCategory(row.inventory) === candidate));
  $: pageView = inventoryPage(visibleRows, page);
  $: selectedRow = resolveSellNowSelection(visibleRows, selectedIdentity, pageView.rows);
  $: currentOrder = selectedRow
    ? matchingSellOrder(selectedRow.inventory, accountView)
    : null;
  $: listingInventory = selectedRow && accountView && view
    ? inventoryForNewListing(selectedRow.inventory, view.rows.map(row => row.inventory), accountView)
    : selectedRow?.inventory ?? null;
  $: listingDraftSource = selectedRow
    ? `${sellNowRowIdentity(selectedRow)}:${currentOrder?.id ?? "new"}`
    : "";
  $: if (
    selectedRow &&
    listingDraftSource &&
    listingDraftSource !== orderDraftSeed &&
    !orderBusy
  ) {
    pendingListingAction = null;
    seedListingDraft(selectedRow, currentOrder, listingDraftSource);
  }
  $: if (!selectedRow) { pendingListingAction = null; orderDraftSeed = ""; detailOpen = false; }
  $: selectedQuote = selectedRow ? checkedPrices.get(sellNowRowIdentity(selectedRow)) : undefined;
  $: activeLive = selectedQuote && isCheckedPriceCurrent(selectedQuote, quoteNow, quoteTtlSeconds) ? selectedQuote : null;
  $: sellerOrders = (activeLive?.orders ?? [])
    .filter(order => order.side === "sell")
    .sort((a, b) => a.platinum / Math.max(1, a.perTrade) - b.platinum / Math.max(1, b.perTrade));
  $: if (selectedRow && sellNowRowIdentity(selectedRow) !== autoQuoteIdentity) {
    autoQuoteIdentity = sellNowRowIdentity(selectedRow);
    ++liveRequest;
    liveLoading = false;
    liveError = "";
    const quote = checkedPrices.get(autoQuoteIdentity);
    if (!quote || !isCheckedPriceCurrent(quote, quoteNow, quoteTtlSeconds)) void loadLive(selectedRow);
  }
  $: if (!selectedRow) autoQuoteIdentity = "";
  $: resultStatus = loading
    ? c.matching
    : refreshing
      ? c.refreshingItems
    : view
      ? c.shown(visibleRows.length, view.rows.length)
      : "";

  async function loadSellNow(): Promise<void> {
    const request = ++viewRequest;
    const hasCurrentView = view !== null;
    loading = !hasCurrentView;
    refreshing = hasCurrentView;
    errorMessage = "";
    try {
      const result = await invoke<SellNowView | null>("sell_now");
      if (destroyed || request !== viewRequest) return;
      if (pendingListingAction && !orderBusy && (result?.inventoryMetadata.checksumSha256 !== view?.inventoryMetadata.checksumSha256 || result?.keepCopies !== view?.keepCopies
        || result?.rows.find(row => sellNowRowIdentity(row) === selectedIdentity)?.inventory.sellableQuantity !== selectedRow?.inventory.sellableQuantity)) {
        pendingListingAction = null;
        orderFormError = u.selectionReset;
      }
      view = result;
      cachedSellNowView = result;
      const stillExists = result?.rows.some(
        (row) => sellNowRowIdentity(row) === selectedIdentity,
      );
      if (!stillExists) selectedIdentity = "";
      quoteNow = Date.now();
    } catch (error) {
      if (destroyed || request !== viewRequest) return;
      if (!hasCurrentView) view = null;
      errorMessage = c.loadError(String(error));
    } finally {
      if (!destroyed && request === viewRequest) { loading = false; refreshing = false; }
    }
  }

  async function scanWarframe(): Promise<void> {
    scanning = true;
    errorMessage = "";
    try {
      await invoke("scan_read_only_inventory");
      await Promise.all([loadSellNow(), masteryStore.refresh()]);
      onInventoryChange?.();
    } catch (error) {
      errorMessage = inventoryScanErrorMessage(error, c.scanError, $locale === "ru");
    } finally {
      scanning = false;
    }
  }

  async function loadAccountOrders(): Promise<void> {
    const request = ++accountRequest;
    accountLoading = true;
    accountError = "";
    try {
      const result = await invoke<AccountView>("account_status");
      if (destroyed || request !== accountRequest) return;
      accountView = result;
    } catch (error) {
      if (destroyed || request !== accountRequest) return;
      accountView = null;
      accountError = c.accountUnavailable;
    } finally {
      if (!destroyed && request === accountRequest) accountLoading = false;
    }
  }

  function seedListingDraft(
    row: SellNowRow,
    order: AccountOrder | null,
    seed: string,
  ): void {
    orderDraftSeed = seed;
    liveError = "";
    orderPriceEdited = false;
    orderPrice = order
      ? Math.max(1, Math.round(order.platinum / (order.perTrade ?? 1)))
      : Math.max(1, Math.round(row.recommendation?.listPrice ?? row.recommendation?.fairPrice ?? 1));
    orderQuantity = order?.quantity ?? Math.max(1, row.inventory.sellableQuantity);
    orderPerTrade = order?.perTrade ?? 1;
    orderVisible = order?.visible ?? true;
    orderFormError = "";
    listingConfirmationError = "";
  }

  function prepareListingAction(event: SubmitEvent): void {
    event.preventDefault();
    if (!selectedRow || currentOrder) return;
    const perTrade = selectedRow.inventory.bulkTradable ? orderPerTrade : null;
    const listingPlatinum = orderPrice * (perTrade ?? 1);
    orderFormError = validateListingNumbers(
      listingPlatinum,
      orderQuantity,
      perTrade,
      $locale,
      inventoryListingQuantity(listingInventory),
    ) ?? "";
    if (orderFormError) return;
    const trigger = event.submitter as HTMLElement | null;
    const input = createListingInputFromInventory(
      selectedRow.inventory,
      listingPlatinum,
      orderQuantity,
      orderVisible,
      perTrade,
    );
    if (!input) {
      orderFormError = c.variantUnavailable;
      return;
    }
    openListingConfirmation(
      {
        kind: "create",
        input,
        itemName: selectedRow.inventory.displayName,
        reserveWarning: listingReserveWarning(listingInventory, orderQuantity, $locale),
      },
      trigger,
    );
  }

  async function openListingConfirmation(
    action: PendingListingAction,
    trigger: HTMLElement | null,
  ): Promise<void> {
    pendingListingAction = action;
    listingConfirmationError = "";
    listingConfirmationTrigger = trigger;
    await tick();
    listingConfirmationHeading?.focus();
  }

  function closeListingConfirmation(): void {
    pendingListingAction = null;
    listingConfirmationError = "";
    const trigger = listingConfirmationTrigger;
    listingConfirmationTrigger = null;
    void tick().then(() => trigger?.focus());
  }

  async function executeListingAction(): Promise<void> {
    if (!pendingListingAction || orderBusy) return;
    orderBusy = true;
    listingConfirmationError = "";
    const action = pendingListingAction;
    try {
      await invoke<AccountOrder>("account_create_listing", {
        input: action.input,
        confirmed: true,
      });
      orderStatusMessage = c.orderCreated;
      closeListingConfirmation();
      await loadAccountOrders();
    } catch (error) {
      listingConfirmationError = c.orderActionError(String(error));
    } finally {
      orderBusy = false;
    }
  }

  function selectRow(row: SellNowRow): void {
    detailTrigger = document.activeElement as HTMLElement | null;
    selectedIdentity = sellNowRowIdentity(row);
    detailOpen = true;
    liveError = "";
    void revealCompactDetail("sell-detail-heading", "(max-width: 68rem)");
  }

  async function backToList(): Promise<void> {
    detailOpen = false;
    await tick();
    await revealElement(detailTrigger?.isConnected ? detailTrigger : document.getElementById("sell-results-heading"));
  }

  async function changePage(next: number): Promise<void> {
    page = next;
    detailOpen = false;
    selectedIdentity = "";
    await tick();
    resetListScroll();
    await revealElement(document.getElementById("sell-results-heading"));
  }

  function resetListScroll(): void {
    if (listScroll) listScroll.scrollTop = 0;
  }

  function resetFilters(): void {
    query = ""; category = DEFAULT_SELL_NOW_VIEW.category; preset = DEFAULT_SELL_NOW_VIEW.preset;
    sortKey = DEFAULT_SELL_NOW_VIEW.sortKey; sortDirection = DEFAULT_SELL_NOW_VIEW.sortDirection;
    page = 1; detailOpen = false;
  }

  function meaningfulVariant(row: SellNowRow): string {
    const key = row.inventory.key;
    if (!key) return c.unknownVariant;
    return key.rank !== null || key.charges !== null || key.subtype !== null || key.amberStars !== null || key.cyanStars !== null ? variantLabel(key, $locale) : "";
  }

  function changeCategory(event: Event): void {
    category = (event.currentTarget as HTMLSelectElement).value as InventoryCategoryFilter;
    selectedIdentity = "";
    liveError = "";
  }

  async function loadLive(row: SellNowRow): Promise<void> {
    if (!row.inventory.key) return;
    const identity = sellNowRowIdentity(row);
    // Проверка может изменить порядок цен и страницу строки, но не выбранный предмет.
    selectedIdentity = identity;
    const request = ++liveRequest;
    liveLoading = true;
    liveError = "";
    try {
      const result = await invoke<LiveSellNowResult | null>("sell_now_live", {
        key: row.inventory.key,
      });
      if (destroyed || request !== liveRequest) return;
      if (!result) {
        if (selectedRow && identity === sellNowRowIdentity(selectedRow)) liveError = c.missingVariant;
        return;
      }
      if (!isCheckedPriceCurrent(result, Date.now(), quoteTtlSeconds)) {
        if (selectedRow && identity === sellNowRowIdentity(selectedRow)) liveError = $locale === "ru"
          ? "Рынок вернул устаревшие данные. Попробуйте проверить цену позже."
          : "The market returned stale data. Try checking the price later.";
        return;
      }
      checkedPrices = new Map(checkedPrices).set(identity, result);
      cachedChecks.set(identity, result);
      quoteNow = Date.now();
      if (identity === sellNowRowIdentity(selectedRow ?? row) && !orderPriceEdited && !currentOrder && !pendingListingAction) {
        orderPrice = Math.max(1, Math.round(result.row.recommendation?.listPrice ?? result.row.recommendation?.fairPrice ?? orderPrice));
      }
    } catch (error) {
      if (!destroyed && request === liveRequest && identity === sellNowRowIdentity(selectedRow ?? row)) liveError = c.liveError(String(error));
    } finally {
      if (!destroyed && request === liveRequest) liveLoading = false;
    }
  }

  function changeSort(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    const option = sortOptions.find(option => `${option.key}:${option.direction}` === value);
    if (option) { sortKey = option.key; sortDirection = option.direction; }
  }

  function sortAria(
    key: SellNowSortKey,
    activeKey: SellNowSortKey,
    direction: SellNowSortDirection,
  ): "none" | "ascending" | "descending" {
    if (activeKey !== key) return "none";
    return direction === "asc" ? "ascending" : "descending";
  }

  function displayPrice(value: number | null | undefined): string {
    return value === null || value === undefined ? c.noPrice : formatPlatinum(value, $locale);
  }

  onMount(() => {
    let disposed = false;
    let unlistenMarket: UnlistenFn | undefined;
    let unlistenInventory: UnlistenFn | undefined;
    const quoteTimer = setInterval(() => {
      quoteNow = Date.now();
      for (const [key, quote] of cachedChecks) if (!isCheckedPriceCurrent(quote, quoteNow, quoteTtlSeconds)) cachedChecks.delete(key);
      if (cachedChecks.size !== checkedPrices.size) checkedPrices = new Map(cachedChecks);
    }, 5_000);
    const savedView = loadSellNowViewPreferences();
    category = initialQuery ? "all" : savedView.category;
    preset = initialQuery ? "all" : savedView.preset;
    sortKey = savedView.sortKey;
    sortDirection = savedView.sortDirection;
    viewPreferencesReady = true;
    void invoke<AppSettings>("load_settings").then(settings => {
      if (!disposed) quoteTtlSeconds = settings.live_quote_ttl_seconds;
    }).catch(() => { /* Без настроек используется стандартный срок актуальности. */ });
    void loadSellNow();
    void loadAccountOrders();
    void listen("market-data-updated", () => void loadSellNow()).then((cleanup) => {
      if (disposed) cleanup();
      else unlistenMarket = cleanup;
    });
    void listen("inventory-updated", () => void loadSellNow()).then((cleanup) => {
      if (disposed) cleanup();
      else unlistenInventory = cleanup;
    });
    return () => {
      disposed = true;
      destroyed = true;
      ++viewRequest; ++accountRequest; ++liveRequest;
      clearInterval(quoteTimer);
      unlistenMarket?.();
      unlistenInventory?.();
    };
  });
</script>


<div class="inventory-topbar">
  <div class="item-mode-switch" role="group" aria-label={t("Мои предметы", "My items")}>
    <button type="button" aria-pressed={itemMode === "inventory"} onclick={() => selectItemMode("inventory")}>{t("Инвентарь", "Inventory")}</button>
    <button type="button" aria-pressed={itemMode === "mastery"} onclick={() => selectItemMode("mastery")}>{t("Освоение", "Mastery")}</button>
  </div>
  {#if itemMode === "inventory" && view}
    <div class="inventory-sync">
      <time datetime={view.inventoryMetadata.observedAt} title={inventorySourceLabel(view.inventoryMetadata.source, $locale) + " · " + new Date(view.inventoryMetadata.observedAt).toLocaleString(localeCode($locale))}>
        {t("Обновлено", "Updated")} {new Date(view.inventoryMetadata.observedAt).toLocaleString(localeCode($locale), {day:"numeric",month:"short",hour:"2-digit",minute:"2-digit"})}
      </time>
      <InventoryAutoRefresh compact onBusy={(busy) => refreshingInBackground = busy} />
      <KeepCopiesControl value={view.keepCopies} disabled={scanning || refreshingInBackground}
        onSaved={async () => { await loadSellNow(); onInventoryChange?.(); }} />
      <button class="secondary refresh-inventory" type="button" onclick={scanWarframe} disabled={loading || scanning || refreshingInBackground} title={c.scanInventory}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 7v5h-5M4 17v-5h5M6 7a7 7 0 0 1 12-1l2 3M4 15l2 3a7 7 0 0 0 12-1" /></svg>
        {scanning || refreshingInBackground ? c.scanningInventory : t("Обновить", "Refresh")}
      </button>
    </div>
  {/if}
</div>

{#if itemMode === "mastery"}
  {#if errorMessage}<div class="error-block" role="alert"><p>{errorMessage}</p></div>{/if}
  <MasteryScreen {scanning} onScan={scanWarframe} />
{:else}
  <div class="sell-now-status" class:sr-only={!loading || !!view} role="status" aria-live="polite">{scanning ? c.scanningInventory : resultStatus}</div>
  {#if errorMessage}
    <div class="error-block" role="alert"><p>{errorMessage}</p><button type="button" class="secondary" onclick={loadSellNow}>{c.retry}</button></div>
  {/if}
  {#if !loading && !view && !errorMessage}
    <section class="empty-panel" aria-labelledby="sell-now-empty-heading">
      <h2 id="sell-now-empty-heading">{c.addSnapshot}</h2>
      <p>{c.addSnapshotBody}</p>
      <InventoryAutoRefresh onBusy={(busy) => refreshingInBackground = busy} />
      <button type="button" onclick={scanWarframe} disabled={scanning || refreshingInBackground}>{scanning || refreshingInBackground ? c.scanningInventory : c.openInventory}</button>
    </section>
  {:else if view}
    {#if !view.rows.length}
      <section class="empty-panel" aria-labelledby="sell-now-zero-heading"><h2 id="sell-now-zero-heading">{u.empty}</h2><p>{u.emptyHint}</p></section>
    {:else}
      <div class="sell-now-layout" class:detail-open={detailOpen}>
        <section class="results-panel sell-results" aria-labelledby="sell-results-heading">
          <h2 id="sell-results-heading" class="sr-only" tabindex="-1">{t("Предметы инвентаря", "Inventory items")}</h2>
          <div class="inventory-filters" role="group" aria-label={c.filters}>
            <label class="inventory-search-field" for="sell-search">
              <span>{c.search}</span>
              <div class="inventory-search">
                <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></svg>
                <input id="sell-search" type="search" bind:value={query} maxlength="80" autocomplete="off" placeholder={c.searchExample} />
              </div>
            </label>
            <label for="sell-category">
              <span>{c.category}</span>
              <select id="sell-category" value={category} onchange={changeCategory}>
                <option value="all">{c.allCategories}</option>
                {#each categories as itemCategory}<option value={itemCategory}>{categoryLabels[itemCategory]}</option>{/each}
              </select>
            </label>
            <label for="sell-filter">
              <span>{t("Отбор предметов", "Item filter")}</span>
              <select id="sell-filter" bind:value={preset} aria-describedby={filterHint ? "inventory-filter-hint" : undefined}>
                {#each filterOptions as option}<option value={option.value}>{option.label}</option>{/each}
              </select>
            </label>
            <label for="inventory-sort">
              <span>{t("Порядок в списке", "List order")}</span>
              <select id="inventory-sort" value={sortKey + ":" + sortDirection} onchange={changeSort}>
                {#each sortOptions as option}<option value={option.key + ":" + option.direction}>{option.label}</option>{/each}
              </select>
            </label>
          </div>
          <div class="inventory-result-bar">
            <span>{t("Найдено", "Found")} <strong>{visibleRows.length.toLocaleString(localeCode($locale))}</strong> {t("из", "of")} {displayRows.length.toLocaleString(localeCode($locale))} {t("позиций", "entries")}</span>
            {#if canReset}<button type="button" class="text-action" onclick={resetFilters}>{t("Сбросить всё", "Reset all")}</button>{/if}
          </div>
          {#if filterHint}<p class="inventory-filter-hint" id="inventory-filter-hint">{filterHint}</p>{/if}
          {#if visibleRows.length}
            <div class="table-wrap" bind:this={listScroll}>
              <table class="sell-table">
                <caption class="sr-only">{t("Предметы, общее и доступное количество, оценка цены за штуку", "Items, owned and sellable quantities, price estimate per item")}</caption>
                <thead><tr>
                  <th scope="col" aria-sort={sortAria("name", sortKey, sortDirection)}>{c.item}</th>
                  <th scope="col" aria-sort={sortAria("owned", sortKey, sortDirection)}>{t("Всего", "Owned")}</th>
                  <th scope="col" aria-sort={sortAria("sellable", sortKey, sortDirection)}>{t("К продаже", "For sale")}</th>
                  <th scope="col" aria-sort={sortAria("fair", sortKey, sortDirection)}>{u.price}</th>
                </tr></thead>
                <tbody>
                  {#each pageView.rows as row, rowIndex (sellNowRowDomKey(row, rowIndex))}
                    <tr class:selected={selectedRow && sellNowRowIdentity(row) === sellNowRowIdentity(selectedRow)}>
                      <td data-label={c.item}>
                        <button class="item-button" type="button" onclick={() => selectRow(row)} aria-pressed={selectedRow && sellNowRowIdentity(row) === sellNowRowIdentity(selectedRow)}>
                          {#if row.inventory.imageUrl}<img class="item-thumb" src={row.inventory.imageUrl} alt="" loading="lazy" decoding="async" />{:else}<span class="item-placeholder" aria-hidden="true">◇</span>{/if}
                          <span class="item-button__copy">
                            <span>{row.inventory.displayName}</span>
                            {#if meaningfulVariant(row) || row.inventory.equippedQuantity}
                              <small>{meaningfulVariant(row)}{meaningfulVariant(row) && row.inventory.equippedQuantity ? " · " : ""}{row.inventory.equippedQuantity ? c.equippedCount(row.inventory.equippedQuantity) : ""}</small>
                            {/if}
                          </span>
                          <span class="item-chevron" aria-hidden="true">›</span>
                        </button>
                      </td>
                      <td class="numeric inventory-owned" data-label={t("Всего", "Owned")}>{row.inventory.ownedQuantity.toLocaleString(localeCode($locale))}</td>
                      <td class="numeric inventory-available" class:zero={!row.inventory.sellableQuantity} data-label={t("К продаже", "For sale")}>{row.inventory.sellableQuantity.toLocaleString(localeCode($locale))}</td>
                      <td class="numeric inventory-estimate" data-label={u.price}>
                        <strong>{inventoryUnitPrice(row) !== null ? displayPrice(inventoryUnitPrice(row)) : "—"}</strong>
                        {#if checkedPrices.has(sellNowRowIdentity(row)) && isCheckedPriceCurrent(checkedPrices.get(sellNowRowIdentity(row))!, quoteNow, quoteTtlSeconds)}<small class="checked-price">✓ {u.checked}</small>{/if}
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
            <nav class="inventory-pagination" aria-label={u.page}>
              <span>{pageView.from}–{pageView.to} {t("из", "of")} {visibleRows.length.toLocaleString(localeCode($locale))}</span>
              {#if pageView.count > 1}
                <div><button type="button" class="secondary" disabled={pageView.page === 1} onclick={() => changePage(pageView.page - 1)}>{u.previous}</button><span>{pageView.page} / {pageView.count}</span><button type="button" class="secondary" disabled={pageView.page === pageView.count} onclick={() => changePage(pageView.page + 1)}>{u.next}</button></div>
              {/if}
            </nav>
          {:else}
            <div class="no-results"><span class="empty-symbol" aria-hidden="true">⌕</span><h3>{c.noFiltered}</h3><p>{t("Измените поиск или выбранные условия выше.", "Change the search or the conditions above.")}</p></div>
          {/if}
        </section>

        {#if selectedRow}
          <aside class="detail-panel sell-detail" aria-labelledby="sell-detail-heading">
            <button type="button" class="text-action inventory-back" onclick={backToList}>{u.back}</button>
            <div class="detail-heading">
              {#if selectedRow.inventory.imageUrl}<img class="detail-art" src={selectedRow.inventory.imageUrl} alt="" decoding="async" />{:else}<span class="detail-placeholder-art" aria-hidden="true">◇</span>{/if}
              <div><h2 id="sell-detail-heading" tabindex="-1">{selectedRow.inventory.displayName}</h2>{#if meaningfulVariant(selectedRow)}<span>{meaningfulVariant(selectedRow)}</span>{/if}</div>
            </div>
            <section class="inventory-quantity" aria-label={u.count}>
              <p><strong>{selectedRow.inventory.sellableQuantity}</strong> {t("к продаже из", "for sale out of")} {selectedRow.inventory.ownedQuantity}</p>
              <details>
                <summary>{selectedRow.inventory.sellableQuantity < selectedRow.inventory.ownedQuantity ? t("Почему не все?", "Why not all?") : t("Подробнее", "Details")}</summary>
                {#if selectedRow.inventory.resolution !== "resolved"}<p>{resolutionLabel(selectedRow.inventory.resolution, $locale)}</p>{/if}
                {#if !view.modUsageScanned && inventoryCategory(selectedRow.inventory) === "mod"}<p>{u.insufficient}</p>{/if}
                <dl class="quantity-breakdown">
                  <div><dt>{u.tradeable}</dt><dd>{selectedRow.inventory.tradeableQuantity}</dd></div>
                  {#if selectedRow.inventory.untradeableQuantity}<div><dt>{u.untradeable}</dt><dd>{selectedRow.inventory.untradeableQuantity}</dd></div>{/if}
                  {#if selectedRow.inventory.unknownQuantity}<div><dt>{u.unknown}</dt><dd>{selectedRow.inventory.unknownQuantity}</dd></div>{/if}
                  {#if selectedRow.inventory.equippedQuantity}<div><dt>{u.equipped}</dt><dd>{selectedRow.inventory.equippedQuantity}</dd></div>{/if}
                  <div><dt>{u.saved}</dt><dd>{view.keepCopies}</dd></div>
                  {#if selectedRow.inventory.personalReservedQuantity}<div><dt>{t("Для личных сборок", "For personal goals")}</dt><dd>{selectedRow.inventory.personalReservedQuantity}</dd></div>{/if}
                </dl><p>{u.reserveHint}</p>
              </details>
            </section>

            <div class="inventory-price-line">
              <div><span>{t("Оценка за штуку", "Estimate per item")}</span><strong>{formatPlatinum(selectedRow.recommendation?.listPrice ?? selectedRow.recommendation?.fairPrice ?? null, $locale)}</strong>
                {#if activeLive}<small class="checked-price">✓ {u.checked} {new Date(activeLive.fetchedAt).toLocaleTimeString(localeCode($locale), {hour:"2-digit",minute:"2-digit"})}</small>{/if}
              </div>
            </div>

            <section class="inventory-sellers" aria-label={t("Цены продавцов", "Seller prices")} aria-busy={liveLoading}>
              <div class="seller-heading">
                <h3>{t("Продавцы в игре", "In-game sellers")}</h3>
                <button type="button" class="text-action" disabled={liveLoading || !selectedRow.inventory.key} onclick={() => loadLive(selectedRow)}>{liveLoading ? t("Загрузка…", "Loading…") : t("Обновить цены", "Refresh prices")}</button>
              </div>
              <div class="live-status" aria-live="polite">
                {#if activeLive?.warning}<strong>{t("Часть ордеров недоступна. Оценка может быть неполной.", "Some orders are unavailable. The estimate may be incomplete.")}</strong>{/if}
                {#if liveError}<strong class="inline-error">{liveError}</strong>{/if}
              </div>
              {#if activeLive && sellerOrders.length}
                <table class="seller-prices">
                  <thead><tr><th>{t("Цена / шт.", "Price / item")}</th><th>{t("В наличии", "Available")}</th>{#if selectedRow.inventory.bulkTradable}<th>{t("За сделку", "Per trade")}</th>{/if}</tr></thead>
                  <tbody>{#each sellerOrders as order, index (index)}
                    <tr><td>{formatPlatinum(order.platinum / Math.max(1, order.perTrade), $locale)}</td><td>{order.quantity} {t("шт.", "pcs")}</td>{#if selectedRow.inventory.bulkTradable}<td>{order.perTrade} {t("шт.", "pcs")}</td>{/if}</tr>
                  {/each}</tbody>
                </table>
              {:else if !liveError}
                <p class="seller-empty" role="status">{liveLoading ? t("Загружаем цены продавцов…", "Loading seller prices…") : activeLive ? t("Сейчас нет продавцов в игре.", "No sellers in game right now.") : !selectedRow.inventory.key ? t("Предмет не найден на рынке.", "Item not matched on the market.") : t("Цены устарели. Обновите список.", "Prices expired. Refresh the list.")}</p>
              {/if}
            </section>

            <section class="wfm-order-panel" aria-label={c.wfmOrder} aria-busy={accountLoading || orderBusy}>
              <div class="wfm-order-status" role="status" aria-live="polite">{orderStatusMessage}</div>
              {#if currentOrder && !accountLoading && !accountError}
                <p>{c.currentOrder(String(currentOrder.platinum), currentOrder.quantity, visibilityLabel(currentOrder.visible, $locale), currentOrder.perTrade)}</p>
                <button type="button" onclick={onOpenMarketSales}>{c.manageOrder}</button>
              {:else if inventoryListingQuantity(listingInventory) <= 0}
                <p class="no-sale">{t("Нет доступных копий для продажи.", "No copies available for sale.")}</p>
              {:else if accountLoading}<p>{c.loadingOrders}</p>
              {:else if accountError}
                <div class="wfm-order-error" role="alert"><p>{accountError}</p><button type="button" class="secondary" onclick={loadAccountOrders}>{c.retryOrders}</button></div>
              {:else if !accountView?.connected}
                <p>{c.accountDisconnectedBody}</p><button type="button" onclick={onOpenMarketSales}>{c.openAccount}</button>
              {:else if !accountView.profile?.verification}
                <p>{c.unverifiedAccount}</p><button type="button" class="secondary" onclick={onOpenMarketSales}>{c.openAccount}</button>
              {:else if !selectedRow.inventory.itemId || !selectedRow.inventory.key}<p class="inline-error" role="alert">{c.variantUnavailable}</p>
              {:else if pendingListingAction}
                <section class="wfm-order-confirmation" aria-labelledby="wfm-order-confirmation-heading">
                  <h3 id="wfm-order-confirmation-heading" bind:this={listingConfirmationHeading} tabindex="-1">{pendingListingAction.input.visible ? t("Опубликовать ордер?", "Publish this order?") : t("Создать скрытый ордер?", "Create a hidden order?")}</h3>
                  <p>{c.confirmCreate(pendingListingAction.itemName, pendingListingAction.input.platinum, pendingListingAction.input.quantity, pendingListingAction.input.perTrade)}</p>
                  {#if pendingListingAction.reserveWarning}<p class="reserve-warning" role="alert">{pendingListingAction.reserveWarning}</p>{/if}
                  <p class="confirmation-visibility">{pendingListingAction.input.visible ? t("Будет виден покупателям.", "Visible to buyers.") : t("Сохранится скрытым от покупателей.", "Hidden from buyers.")}</p>
                  {#if listingConfirmationError}<p class="inline-error" role="alert">{listingConfirmationError}</p>{/if}
                  <div class="wfm-order-actions"><button type="button" onclick={executeListingAction} disabled={orderBusy}>{orderBusy ? t("Создаём ордер…", "Creating order…") : pendingListingAction.input.visible ? t("Подтвердить публикацию", "Confirm publishing") : t("Подтвердить создание", "Confirm creation")}</button><button type="button" class="secondary" onclick={closeListingConfirmation} disabled={orderBusy}>{t("Назад к цене", "Edit price")}</button></div>
                </section>
              {:else}
                <form class="wfm-order-form" onsubmit={prepareListingAction}>
                  <div class="wfm-order-fields">
                    <div class="filter-field"><label for="sell-order-price">{t("Цена за штуку, платина", "Price per item, platinum")}</label><input id="sell-order-price" type="number" inputmode="numeric" bind:value={orderPrice} oninput={() => orderPriceEdited = true} min="1" max="900000" step="1" required aria-describedby={orderFormError ? "sell-order-error" : undefined} aria-invalid={orderFormError ? "true" : undefined} /></div>
                    <div class="filter-field"><label for="sell-order-quantity">{t("Количество", "Quantity")}</label><input id="sell-order-quantity" type="number" inputmode="numeric" bind:value={orderQuantity} min="1" max={Math.min(9999, inventoryListingQuantity(listingInventory))} step="1" required aria-describedby={orderFormError ? "sell-order-error" : undefined} aria-invalid={orderFormError ? "true" : undefined} /></div>
                    {#if selectedRow.inventory.bulkTradable}<div class="filter-field bulk-trade-field"><label for="sell-order-per-trade">{c.orderPerTrade}</label><input id="sell-order-per-trade" type="number" inputmode="numeric" bind:value={orderPerTrade} min="1" max="6" step="1" required aria-describedby={orderFormError ? "sell-order-error" : undefined} aria-invalid={orderFormError ? "true" : undefined} /></div>{/if}
                  </div>
                  <label class="wfm-order-visible"><input type="checkbox" bind:checked={orderVisible} />{t("Показывать покупателям", "Visible to buyers")}</label>
                  {#if orderFormError}<p id="sell-order-error" class="inline-error" role="alert">{orderFormError}</p>{/if}
                  <button type="submit" disabled={orderBusy}>{orderVisible ? t("Выставить на рынок", "List on market") : t("Создать скрытый ордер", "Create hidden order")}</button>
                </form>
              {/if}
            </section>

          </aside>
        {/if}
      </div>
    {/if}
  {/if}
{/if}

<style>
  .reserve-warning { padding:.75rem; border:1px solid var(--warning, #956422); border-radius:.5rem; background:var(--surface-2); color:var(--text); }
  .inventory-topbar { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:.75rem; margin:0 0 1.15rem; border-bottom:1px solid var(--border); padding:0 0 .75rem; }
  .item-mode-switch { display:flex; gap:.25rem; align-items:center; }
  .item-mode-switch button { border:0; background:transparent; color:var(--text-muted); border-radius:.45rem; padding:.55rem .9rem; box-shadow:none; }
  .item-mode-switch button[aria-pressed="true"] { background:var(--surface-1); color:var(--text); box-shadow:0 1px 3px #3b251718; }
  .inventory-sync { display:flex; flex-wrap:wrap; align-items:center; gap:.6rem 1rem; font-size:.78rem; }
  .inventory-sync time { color:var(--text-muted); font-size:.73rem; }
  .refresh-inventory { display:flex; gap:.4rem; align-items:center; justify-content:center; white-space:nowrap; }
  svg { width:1rem; height:1rem; fill:none; stroke:currentColor; stroke-width:1.65; stroke-linecap:round; stroke-linejoin:round; flex-shrink:0; }
  .sell-now-layout { display:grid; grid-template-columns:minmax(0,1fr) 23.5rem; gap:1.1rem; align-items:start; }
  .sell-results,.sell-detail { background:var(--surface-1); border:1px solid var(--border); border-radius:.85rem; box-shadow:0 2px 6px #3b25170a; }
  .sell-results { overflow:visible; min-width:0; }
  .sell-results:only-child { grid-column:1/-1; }
  .inventory-filters { display:grid; grid-template-columns:minmax(12rem,1.6fr) repeat(3,minmax(11rem,1fr)); gap:.75rem; padding:1rem 1rem .75rem; }
  .inventory-filters > label { display:flex; flex-direction:column; gap:.4rem; min-width:0; font-size:.75rem; color:var(--text-muted); font-weight:600; }
  .inventory-filters select { width:100%; min-width:0; min-height:2.6rem; padding:.45rem .65rem; border:1px solid var(--border); border-radius:.5rem; background:var(--surface-1); color:var(--text); font-size:.82rem; font-weight:400; }
  .inventory-search { display:flex; align-items:center; gap:.5rem; min-width:0; min-height:2.6rem; border:1px solid var(--border); border-radius:.5rem; padding:0 .65rem; background:var(--surface-1); color:var(--text-muted); }
  .inventory-search:focus-within { border-color:var(--accent); outline:2px solid var(--accent-soft); }
  .inventory-search input { width:100%; min-width:0; border:0; outline:none; background:transparent; padding:.6rem 0; font-size:.82rem; font-weight:400; box-shadow:none; }
  .inventory-search input:focus-visible { outline:none; }
  .inventory-result-bar { display:flex; flex-wrap:wrap; justify-content:space-between; align-items:center; gap:.5rem; padding:0 1rem .75rem; color:var(--text-muted); font-size:.78rem; min-height:1.7rem; }
  .inventory-result-bar strong { color:var(--text); }
  .inventory-filter-hint { font-size:.78rem; line-height:1.5; color:var(--text-muted); background:var(--surface-2); margin:0; padding:.65rem 1rem; border-top:1px solid var(--border); }
  .text-action { display:inline-flex; align-items:center; width:auto; padding:.25rem 0; border:0; background:transparent; color:var(--accent-strong); box-shadow:none; font-size:.78rem; font-weight:600; }
  .text-action:hover:not(:disabled) { background:transparent; text-decoration:underline; }
  .sell-results > .table-wrap { max-height:min(70vh,52rem); overflow:auto; overscroll-behavior:contain; scrollbar-gutter:stable; border-top:1px solid var(--border); }
  .sell-table { table-layout:fixed; width:100%; }
  .sell-table thead { position:sticky; top:0; z-index:1; background:var(--surface-1); }
  .sell-table th { padding:.65rem 1rem; font-size:.68rem; letter-spacing:.045em; color:var(--text-muted); border-bottom:1px solid var(--border); }
  .sell-table th:first-child { width:52%; }
  .sell-table th:nth-child(2) { width:12%; }
  .sell-table th:nth-child(3) { width:16%; }
  .sell-table th:nth-child(4) { width:20%; }
  .sell-table th:not(:first-child),.sell-table td.numeric { text-align:right; }
  .sell-table td { padding:.55rem 1rem; height:3.75rem; border-color:color-mix(in srgb,var(--border) 55%,transparent); }
  .sell-table tr.selected { background:color-mix(in srgb,var(--accent-soft) 45%,var(--surface-1)); box-shadow:3px 0 0 var(--accent) inset; }
  .item-button { width:100%; display:flex; align-items:center; gap:.7rem; min-width:0; padding:0; }
  .sell-table .item-button { font-size:.875rem; scroll-margin-block-start:3rem; scroll-margin-block-end:1rem; }
  .item-button__copy { min-width:0; flex:1; text-align:left; }
  .item-button__copy > span { line-height:1.35; font-weight:650; }
  .sell-table .item-button__copy small { color:var(--text-muted); margin-top:.2rem; font-size:.72rem; }
  .item-thumb,.item-placeholder { width:2.5rem; height:2.5rem; flex:0 0 2.5rem; border-radius:.45rem; object-fit:contain; background:transparent; outline:0; }
  .item-placeholder { display:grid; place-items:center; color:var(--text-muted); font-size:1.4rem; background:var(--surface-2); }
  .item-chevron { font-size:1.3rem; color:var(--text-muted); opacity:0; }
  .item-button:hover .item-chevron,.item-button:focus-visible .item-chevron,.selected .item-chevron { opacity:1; }
  .inventory-owned { color:var(--text-muted); }
  .inventory-available { font-weight:700; color:var(--text); }
  .inventory-available.zero { color:var(--text-muted); font-weight:400; }
  .inventory-estimate strong { font-size:.9rem; color:var(--text); font-variant-numeric:tabular-nums; }
  .checked-price { display:block; margin-top:.2rem; color:var(--success,#46613d); font-size:.68rem; font-weight:500; }
  .inventory-pagination { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:.5rem; padding:.7rem 1rem; border-top:1px solid var(--border); font-size:.75rem; color:var(--text-muted); }
  .inventory-pagination > div { display:flex; align-items:center; gap:.7rem; }
  .inventory-pagination button { padding:.35rem .6rem; font-size:.75rem; }
  .sell-detail { position:sticky; top:1rem; max-height:calc(100dvh - 2rem); overflow:auto; scrollbar-gutter:stable; padding:1.15rem; min-height:0; }
  .detail-heading { display:flex; gap:.85rem; align-items:center; margin-bottom:1rem; }
  .detail-heading > div { min-width:0; }
  .detail-heading h2 { font-size:1.12rem; line-height:1.4; text-wrap:initial; overflow-wrap:anywhere; margin:0; }
  .detail-heading span { color:var(--text-muted); font-size:.75rem; }
  .detail-art,.detail-placeholder-art { width:3.6rem; height:3.6rem; flex:0 0 3.6rem; margin:0; object-fit:contain; background:var(--surface-2); border-radius:.6rem; outline:0; filter:none; }
  .detail-placeholder-art { display:grid; place-items:center; font-size:1.75rem; }
  .inventory-back { display:none; margin-bottom:1rem; }
  .inventory-quantity { display:flex; flex-wrap:wrap; align-items:baseline; justify-content:space-between; gap:.3rem .6rem; border-bottom:1px solid var(--border); padding-bottom:.85rem; margin-bottom:1rem; }
  .inventory-quantity > p { margin:0; font-size:.82rem; color:var(--text-muted); }
  .inventory-quantity > p strong { font-size:1.15rem; color:var(--text); padding-right:.15rem; }
  .inventory-quantity details { font-size:.75rem; }
  .inventory-quantity summary { color:var(--text-muted); cursor:pointer; }
  .inventory-quantity details[open] { flex-basis:100%; padding-top:.4rem; }
  .inventory-quantity details p { margin:.6rem 0 0; color:var(--text-muted); line-height:1.6; }
  .quantity-breakdown { display:grid; gap:.4rem; margin:.7rem 0; }
  .quantity-breakdown > div { display:flex; justify-content:space-between; gap:.75rem; }
  .quantity-breakdown dt { color:var(--text-muted); }
  .quantity-breakdown dd { margin:0; font-weight:650; }
  .inventory-price-line { display:flex; justify-content:space-between; align-items:center; gap:.5rem; }
  .inventory-price-line > div > span { font-size:.75rem; color:var(--text-muted); }
  .inventory-price-line > div > strong { display:block; font-size:1.85rem; line-height:1.25; color:var(--text); font-variant-numeric:tabular-nums; margin:.15rem 0; }
  .live-status { margin-top:.5rem; font-size:.73rem; color:var(--text-muted); }
  .live-status:empty { display:none; }
  .wfm-order-panel { margin:1rem 0 0; padding:1rem 0 0; border:0; border-top:1px solid var(--border); background:transparent; border-radius:0; box-shadow:none; }
  .wfm-order-panel p { font-size:.8rem; line-height:1.5; margin:0 0 .75rem; }
  .wfm-order-status { min-height:0; margin:0; }
  .wfm-order-status:empty { display:none; }
  .wfm-order-panel > button,.wfm-order-form > button { width:100%; }
  .wfm-order-form { border:0; padding:0; margin:0; background:transparent; gap:.8rem; }
  .wfm-order-fields { grid-template-columns:minmax(0,1.4fr) minmax(0,.8fr); gap:.6rem; }
  .wfm-order-fields label { font-size:.73rem; font-weight:500; }
  .wfm-order-fields input { font-size:1rem; min-height:2.55rem; border-radius:.5rem; }
  .bulk-trade-field { grid-column:1/-1; }
  .wfm-order-visible { min-height:0; font-size:.75rem; gap:.4rem; padding:0; }
  .wfm-order-confirmation { border:0; padding:0; background:transparent; }
  .wfm-order-confirmation h3 { font-size:1rem; margin:0 0 .65rem; }
  .wfm-order-actions { display:grid; gap:.5rem; }
  .wfm-order-panel .confirmation-visibility { font-size:.75rem; color:var(--text-muted); }
  .inventory-sellers { margin-top:.85rem; }
  .seller-heading { display:flex; align-items:center; justify-content:space-between; gap:.5rem; margin-bottom:.5rem; }
  .seller-heading h3 { font-size:.8rem; margin:0; }
  .seller-heading .text-action { font-size:.72rem; white-space:nowrap; }
  .seller-prices { width:100%; table-layout:fixed; font-size:.78rem; font-variant-numeric:tabular-nums; }
  .seller-prices th,.seller-prices td { padding:.35rem .5rem; text-align:right; }
  .seller-prices th { font-size:.65rem; letter-spacing:0; text-transform:none; color:var(--text-muted); background:var(--surface-2); }
  .seller-prices th:first-child,.seller-prices td:first-child { text-align:left; }
  .seller-prices td:first-child { font-weight:650; }
  .seller-prices tbody tr:first-child { background:color-mix(in srgb,var(--accent-soft) 28%,var(--surface-1)); }
  .seller-empty { font-size:.75rem; color:var(--text-muted); margin:.5rem 0; }
  .no-results { padding:3rem 1rem; text-align:center; }
  .no-results h3 { font-size:1rem; }
  .no-results p { font-size:.85rem; }
  .empty-symbol { display:block; color:var(--text-muted); font-size:2.5rem; margin-bottom:.6rem; }
  @media (max-width:90rem) {
    .sell-now-layout { grid-template-columns:minmax(0,1fr) 21rem; gap:.75rem; }
    .inventory-filters { grid-template-columns:repeat(2,minmax(0,1fr)); }
    .sell-table td,.sell-table th { padding-left:.7rem; padding-right:.7rem; }
    .sell-table th:first-child { width:46%; }.sell-table th:nth-child(2) { width:13%; }.sell-table th:nth-child(3) { width:17%; }.sell-table th:nth-child(4) { width:24%; }
    .inventory-sync { gap:.5rem .75rem; }
  }
  @media (max-width:68rem) {
    .sell-now-layout { grid-template-columns:minmax(0,1fr); }
    .sell-detail { display:none; position:static; max-height:none; overflow:visible; }
    .detail-open .sell-results { display:none; }.detail-open .sell-detail { display:block; }
    .inventory-back { display:inline-flex; }
  }
  @media (max-width:46rem) {
    .inventory-filters { grid-template-columns:minmax(0,1fr); }
    .inventory-sync time { flex-basis:100%; }
    .sell-table thead { display:table-header-group; }.sell-table tbody tr { display:table-row; }
    .sell-table td { display:table-cell; }.sell-table td::before { display:none; }
    .sell-table th:first-child { width:46%; }.sell-table th:nth-child(2) { width:12%; }.sell-table th:nth-child(3) { width:17%; }.sell-table th:nth-child(4) { width:25%; }
  }
</style>
