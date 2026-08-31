<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import { localeCode, useLocale } from "./i18n";
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
    inventoryCategory,
    inventorySourceLabel,
    type InventoryCategoryFilter,
  } from "./inventory";

  import { formatChange, timingLabel, timingShortLabel } from "./history";
  import { formatPlatinum, formatVolume, liveQuoteLabel, priceReasonMessage, variantLabel } from "./market";
  import {
    filterAndSortSellNowRows,
    priorityReasonMessages,
    sellPriorityRanks,
    sellNowRowIdentity,
    type LiveSellNowResult,
    type EquippedFilter,
    type SellNowFilters,
    type SellNowPreset,
    type SellNowRow,
    type SellNowSortDirection,
    type SellNowSortKey,
    type SellNowView,
  } from "./sellNow";
  import {
    loadSellNowViewPreferences,
    saveSellNowViewPreferences,
  } from "./viewPreferences";

  export let onInventoryChange: (() => void) | undefined = undefined;
  export let onOpenMarketSales: () => void;
  type PendingListingAction = { kind: "create"; input: CreateListingInput; itemName: string };

  const locale = useLocale();
  const categoryCopy = {
    ru: { mod: "Моды", arcane_enhancement: "Мистификаторы", relic: "Реликвии", component: "Компоненты", weapon: "Оружие", warframe: "Варфреймы", misc: "Прочее" },
    en: { mod: "Mods", arcane_enhancement: "Arcanes", relic: "Relics", component: "Components", weapon: "Weapons", warframe: "Warframes", misc: "Other" },
  } as const;
  const copy = {
    ru: {
      matching: "Загружаем ваши предметы…", shown: (visible: number, total: number) => `${visible} из ${total} предметов показано`,
      loadError: (_reason: string) => "Не удалось открыть инвентарь. Сохранённые данные не изменились.",
      missingVariant: "Этот вариант больше не доступен для продажи. Обновите список.", liveError: (_reason: string) => "Не удалось получить текущую цену. Сохранённая оценка не изменилась.",
      noSignal: "Нет рекомендации", retry: "Повторить", notImported: "Инвентарь не обновлён", addSnapshot: "Сначала обновите инвентарь",
      addSnapshotBody: "Запустите Warframe и обновите инвентарь. PlatScope покажет все найденные предметы и рекомендации по продаже.", openInventory: "Обновить инвентарь",
      summary: "Мои предметы", totalCopies: "Всего копий", candidates: "Позиций к продаже", recommended: "Продавать сейчас", priced: "С рассчитанной ценой", highPriority: "В первую очередь", nominal: "Ориентировочная сумма",
      inventoryUpdated: "Инвентарь обновлён", scanInventory: "Обновить из Warframe", scanningInventory: "Обновляем…", scanError: "Не удалось обновить инвентарь. Запустите Warframe, войдите в игру и повторите.", reserve: "Оставлять копий", reserveError: "Не удалось изменить резерв копий.",
      notForecast: "Важно:", nominalBody: "это сумма по текущим оценкам, а не гарантированная выручка.",
      noCandidatesLabel: "Очередь пуста", noCandidates: "Нет предметов, готовых к продаже", noCandidatesBody: "Проверьте количество, возможность обмена и резерв копий в инвентаре.", checkInventory: "Открыть инвентарь",
      filters: "Поиск и фильтры", search: "Поиск предмета", searchExample: "Например, Поток Прайм", category: "Тип предмета", allCategories: "Все типы", view: "Показывать", allCandidates: "Весь торговый инвентарь", sellable: "Всё к продаже", sellNow: "Продавать сейчас", hold: "Лучше подождать", duplicates: "Дубликаты", unpriced: "Без цены", attention: "Требуют проверки", usage: "Использование", allUsage: "Все", freeOnly: "Не надеты", equippedOnly: "Надеты", equippedCount: (count: number) => `Надето: ${count}`,
      queue: "Мои предметы", exactSnapshot: (date: string) => `Цены рынка от ${date}`, missingSnapshot: "дата неизвестна",
      tableCaption: "Предметы инвентаря, рекомендуемая цена, продажи, тренд цены и очерёдность", item: "Предмет", salesPerDay: "Продажи / день", priceTrend90: "Тренд цены / 90 дней", unknownVariant: "вариант не определён",
      dailyTrades: (value: string) => `${value} сделок/день`,
      priceUp: (value: string) => `Цена растёт ↑ ${value} за 90 дней`, priceDown: (value: string) => `Цена падает ↓ ${value} за 90 дней`, priceFlat: "Цена стабильна →", noPriceTrend: "Нет данных о цене за 90 дней",
      noFiltered: "Нет кандидатов для этих фильтров", changeFilters: "Измените тип предмета или другие фильтры.", reset: "Сбросить фильтры",
      candidate: "Предмет инвентаря", gettingLive: "Проверяем текущие ордера…", updateLive: "Обновить ордера", getLive: "Проверить ордера сейчас",
      liveHint: "Покажет активные ордера на продажу и покупку для этого варианта.", forSale: "Можно выставить", of: "из", moment: "Когда продавать",
      nominalWarning: "Скорость продажи зависит от спроса и вашей цены.", whyPrice: "Как рассчитана цена?", noPriceSignal: "Сделок пока недостаточно для расчёта цены.", whyPriority: "Почему этот предмет выше или ниже?", details: "Подробности продажи", selectCandidate: "Выберите предмет в очереди.",
      ownedSellable: "Есть / продать", owned: "есть", sellableLabel: "можно продать", fairListQuick: "Выставить по цене",
      noPrice: "нет данных",
      priority: "Приоритет продажи", priorityHint: "№1 — предмет, который стоит выставить первым.", priorityPosition: (rank: number | null, score: number) => rank === null ? `Приоритет не рассчитан · ${score}/100` : `№${rank} в очереди · ${score}/100`, fairPrice: "Ориентир рынка", listPrice: "Рекомендуемый ордер", quickSell: "Лучшая покупка сейчас", sell: "на продажу", buy: "на покупку",
      wfmOrder: "Ордер Warframe Market", loadingOrders: "Проверяем ваши ордера…", accountUnavailable: "Не удалось загрузить ордера Warframe Market.", retryOrders: "Повторить", accountDisconnected: "Warframe Market не подключён", accountDisconnectedBody: "Подключите аккаунт, чтобы выставить этот предмет.", openAccount: "Подключить Warframe Market", unverifiedAccount: "Подтвердите игровой аккаунт на Warframe Market, чтобы менять ордера.", notSellable: "После резерва нет подтверждённых копий для продажи.", noCurrentOrder: "Ордер ещё не выставлен", currentOrder: (price: string, quantity: number, status: string, perTrade: number | null) => perTrade === null ? `Выставлено: ${price}p × ${quantity} · ${status}` : `Выставлено: ${quantity} шт., по ${perTrade} за сделку за ${price}p · ${status}`,
      manageOrder: "Управлять ордером", orderPrice: "Цена, платина", bulkOrderPrice: "Цена за 1 предмет, платина", orderQuantity: "Всего предметов", orderPerTrade: "Предметов за одну сделку", publishOrder: "Сразу показать ордер на рынке", reviewCreate: "Проверить ордер", variantUnavailable: "Этот вариант не найден на Warframe Market. Обновите рыночные данные.", createTitle: "Подтвердите новый ордер", confirmCreate: (name: string, price: number, quantity: number, perTrade: number | null) => perTrade === null ? `${name}: выставить ${quantity} шт. по ${price}p.` : `${name}: всего ${quantity} шт., по ${perTrade} за сделку за ${price}p за лот.`, confirmChecked: "Я проверил предмет, цену и количество", createOrder: "Создать ордер", cancelOrderAction: "Отменить", confirmRequired: "Подтвердите, что проверили параметры ордера.", orderCreated: "Ордер создан на Warframe Market.", orderActionError: (reason: string) => accountActionErrorMessage(reason, "ru"),
    },
    en: {
      matching: "Loading your items…", shown: (visible: number, total: number) => `${visible} of ${total} items shown`,
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
      tableCaption: "Inventory items, recommended price, sales, price trend, and priority", item: "Item", salesPerDay: "Sales / day", priceTrend90: "Price trend / 90 days", unknownVariant: "variant unavailable",
      dailyTrades: (value: string) => `${value} trades/day`,
      priceUp: (value: string) => `Price rising ↑ ${value} over 90 days`, priceDown: (value: string) => `Price falling ↓ ${value} over 90 days`, priceFlat: "Price stable →", noPriceTrend: "No 90-day price data",
      noFiltered: "No candidates match these filters", changeFilters: "Change the item type or another filter.", reset: "Reset filters",
      candidate: "Inventory item", gettingLive: "Getting current price…", updateLive: "Refresh current price", getLive: "Get current price",
      liveHint: "One request for the selected exact variant. Quick Sell is never replaced with a historical buy price.", forSale: "For sale", of: "of", moment: "Timing",
      nominalWarning: "Nominal value does not account for how quickly the full volume may sell.", whyPrice: "Why this price?", noPriceSignal: "There are not enough completed trades to calculate a price.", whyPriority: "Why this priority?", details: "Sell details", selectCandidate: "Select a candidate from the queue.",
      ownedSellable: "Owned / list", owned: "owned", sellableLabel: "can list", fairListQuick: "Recommended price",
      noPrice: "no price",
      priority: "Sale priority", priorityHint: "No. 1 is the item to list first.", priorityPosition: (rank: number | null, score: number) => rank === null ? `Priority unavailable · ${score}/100` : `No. ${rank} in the queue · ${score}/100`, fairPrice: "Fair price", listPrice: "List price", quickSell: "Quick Sell", sell: "sell", buy: "buy",
      wfmOrder: "Warframe Market order", loadingOrders: "Checking your orders…", accountUnavailable: "Unable to load Warframe Market orders.", retryOrders: "Try again", accountDisconnected: "Warframe Market is not connected", accountDisconnectedBody: "Connect your account to list this item.", openAccount: "Connect Warframe Market", unverifiedAccount: "Verify your game account on Warframe Market to change orders.", notSellable: "There are no confirmed copies to sell after the reserve.", noCurrentOrder: "Not listed yet", currentOrder: (price: string, quantity: number, status: string, perTrade: number | null) => perTrade === null ? `Listed: ${price}p × ${quantity} · ${status}` : `Listed: ${quantity} total, ${perTrade} per trade for ${price}p · ${status}`,
      manageOrder: "Manage order", orderPrice: "Price, platinum", bulkOrderPrice: "Price per item, platinum", orderQuantity: "Total items", orderPerTrade: "Items per trade", publishOrder: "Show order on the market immediately", reviewCreate: "Review order", variantUnavailable: "This variant is not available on Warframe Market. Refresh market data.", createTitle: "Confirm new order", confirmCreate: (name: string, price: number, quantity: number, perTrade: number | null) => perTrade === null ? `${name}: list ${quantity} at ${price}p each.` : `${name}: ${quantity} total, ${perTrade} per trade for ${price}p per lot.`, confirmChecked: "I reviewed the item, price, and quantity", createOrder: "Create order", cancelOrderAction: "Cancel", confirmRequired: "Confirm that you reviewed the order parameters.", orderCreated: "Order created on Warframe Market.", orderActionError: (reason: string) => accountActionErrorMessage(reason, "en"),
    },
  } as const;
  $: c = copy[$locale];
  $: categoryLabels = categoryCopy[$locale];

  let view: SellNowView | null = null;
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
  let listingConfirmationAccepted = false;
  let listingConfirmationError = "";
  let listingConfirmationHeading: HTMLElement;
  let listingConfirmationCheckbox: HTMLInputElement;
  let listingConfirmationTrigger: HTMLElement | null = null;
  let loading = true;
  let scanning = false;
  let reserveUpdating = false;
  let errorMessage = "";
  let selectedIdentity = "";
  let liveIdentity = "";
  let liveResult: LiveSellNowResult | null = null;
  let liveLoading = false;
  let liveError = "";
  let query = "";
  let category: InventoryCategoryFilter = "all";
  let preset: SellNowPreset = "sell_now";
  let equipped: EquippedFilter = "all";
  let sortKey: SellNowSortKey = "priority";
  let sortDirection: SellNowSortDirection = "desc";
  let viewPreferencesReady = false;

  $: filters = {
    query,
    category,
    preset,
    equipped,
    sortKey,
    sortDirection,
  } satisfies SellNowFilters;
  $: if (viewPreferencesReady) {
    saveSellNowViewPreferences({
      category,
      preset,
      equipped,
      sortKey,
      sortDirection,
    });
  }
  $: visibleRows = filterAndSortSellNowRows(view?.rows ?? [], filters);
  $: priorityRanks = sellPriorityRanks(view?.rows ?? []);
  $: recommendedCount = filterAndSortSellNowRows(view?.rows ?? [], { ...filters, query: "", category: "all", preset: "sell_now" }).length;
  $: categories = INVENTORY_CATEGORIES.filter((candidate) =>
    view?.rows.some((row) => inventoryCategory(row.inventory) === candidate),
  );
  $: selectedRow =
    visibleRows.find((row) => sellNowRowIdentity(row) === selectedIdentity) ??
    visibleRows[0] ??
    null;
  $: currentOrder = selectedRow
    ? matchingSellOrder(selectedRow.inventory, accountView)
    : null;
  $: listingDraftSource = selectedRow
    ? `${sellNowRowIdentity(selectedRow)}:${currentOrder?.id ?? "new"}`
    : "";
  $: if (
    selectedRow &&
    listingDraftSource &&
    listingDraftSource !== orderDraftSeed &&
    !pendingListingAction
  ) {
    seedListingDraft(selectedRow, currentOrder, listingDraftSource);
  }
  $: activeLive =
    liveResult && selectedRow && liveIdentity === sellNowRowIdentity(selectedRow)
      ? liveResult
      : null;
  $: resultStatus = loading
    ? c.matching
    : view
      ? c.shown(visibleRows.length, view.rows.length)
      : "";

  async function loadSellNow(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      const result = await invoke<SellNowView | null>("sell_now");
      view = result;
      const stillExists = result?.rows.some(
        (row) => sellNowRowIdentity(row) === selectedIdentity,
      );
      if (!stillExists) selectedIdentity = result?.rows[0] ? sellNowRowIdentity(result.rows[0]) : "";
      liveResult = null;
      liveIdentity = "";
      liveError = "";
    } catch (error) {
      view = null;
      errorMessage = c.loadError(String(error));
    } finally {
      loading = false;
    }
  }

  async function scanWarframe(): Promise<void> {
    scanning = true;
    errorMessage = "";
    try {
      await invoke("scan_read_only_inventory");
      await loadSellNow();
      onInventoryChange?.();
    } catch {
      errorMessage = c.scanError;
    } finally {
      scanning = false;
    }
  }

  async function updateReserve(event: Event): Promise<void> {
    const keepCopies = Number((event.currentTarget as HTMLSelectElement).value);
    reserveUpdating = true;
    errorMessage = "";
    try {
      await invoke("set_inventory_keep_copies", { keepCopies });
      await loadSellNow();
      onInventoryChange?.();
    } catch {
      errorMessage = c.reserveError;
    } finally {
      reserveUpdating = false;
    }
  }

  async function loadAccountOrders(): Promise<void> {
    accountLoading = true;
    accountError = "";
    try {
      accountView = await invoke<AccountView>("account_status");
    } catch (error) {
      accountView = null;
      accountError = c.accountUnavailable;
    } finally {
      accountLoading = false;
    }
  }

  function seedListingDraft(
    row: SellNowRow,
    order: AccountOrder | null,
    seed: string,
  ): void {
    orderDraftSeed = seed;
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
      selectedRow.inventory.sellableQuantity,
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
      },
      trigger,
    );
  }

  async function openListingConfirmation(
    action: PendingListingAction,
    trigger: HTMLElement | null,
  ): Promise<void> {
    pendingListingAction = action;
    listingConfirmationAccepted = false;
    listingConfirmationError = "";
    listingConfirmationTrigger = trigger;
    await tick();
    listingConfirmationHeading?.focus();
  }

  function closeListingConfirmation(): void {
    pendingListingAction = null;
    listingConfirmationAccepted = false;
    listingConfirmationError = "";
    const trigger = listingConfirmationTrigger;
    listingConfirmationTrigger = null;
    void tick().then(() => trigger?.focus());
  }

  async function executeListingAction(): Promise<void> {
    if (!pendingListingAction) return;
    if (!listingConfirmationAccepted) {
      listingConfirmationError = c.confirmRequired;
      listingConfirmationCheckbox?.focus();
      return;
    }
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
    selectedIdentity = sellNowRowIdentity(row);
    liveError = "";
  }

  async function loadLive(row: SellNowRow): Promise<void> {
    if (!row.inventory.key) return;
    const identity = sellNowRowIdentity(row);
    liveLoading = true;
    liveError = "";
    try {
      const result = await invoke<LiveSellNowResult | null>("sell_now_live", {
        key: row.inventory.key,
      });
      if (identity !== sellNowRowIdentity(selectedRow ?? row)) return;
      if (!result) {
        liveError = c.missingVariant;
        return;
      }
      liveResult = result;
      liveIdentity = identity;
      if (view) {
        const rows = view.rows.map((candidate) =>
          sellNowRowIdentity(candidate) === identity ? result.row : candidate,
        );
        view = { ...view, rows, summary: summarize(rows) };
      }
    } catch (error) {
      liveResult = null;
      liveIdentity = "";
      liveError = c.liveError(String(error));
    } finally {
      liveLoading = false;
    }
  }

  function summarize(rows: SellNowRow[]): SellNowView["summary"] {
    const candidates = rows.filter((row) =>
      row.inventory.sellableQuantity > 0 && row.inventory.resolution === "resolved"
    );
    return {
      candidateRows: candidates.length,
      pricedRows: candidates.filter((row) => row.recommendation?.fairPrice !== null && row.recommendation !== null).length,
      highPriorityRows: rows.filter((row) => row.priority.band === "high").length,
      nominalValue: rows.reduce((sum, row) => sum + (row.nominalValue ?? 0), 0),
    };
  }

  function changeSort(nextKey: SellNowSortKey): void {
    if (sortKey === nextKey) {
      sortDirection = sortDirection === "asc" ? "desc" : "asc";
    } else {
      sortKey = nextKey;
      sortDirection = nextKey === "name" ? "asc" : "desc";
    }
  }

  function sortAria(
    key: SellNowSortKey,
    activeKey: SellNowSortKey,
    direction: SellNowSortDirection,
  ): "none" | "ascending" | "descending" {
    if (activeKey !== key) return "none";
    return direction === "asc" ? "ascending" : "descending";
  }

  function sortMarker(
    key: SellNowSortKey,
    activeKey: SellNowSortKey,
    direction: SellNowSortDirection,
  ): string {
    if (activeKey !== key) return "";
    return direction === "asc" ? "↑" : "↓";
  }

  function shortTiming(row: SellNowRow): string {
    return row.trend?.timing ? timingShortLabel(row.trend.timing, $locale) : c.noSignal;
  }

  function timingDescription(row: SellNowRow): string {
    return row.trend?.timing ? timingLabel(row.trend.timing, $locale) : c.noSignal;
  }

  function priceTrendText(value: number | null | undefined): string {
    if (value === null || value === undefined || !Number.isFinite(value)) return c.noPriceTrend;
    if (Math.abs(value) < 1) return c.priceFlat;
    const magnitude = formatChange(Math.abs(value), $locale).replace(/^\+/, "");
    return value > 0 ? c.priceUp(magnitude) : c.priceDown(magnitude);
  }

  function priceTrendClass(value: number | null | undefined): "up" | "down" | "flat" | "unknown" {
    if (value === null || value === undefined || !Number.isFinite(value)) return "unknown";
    if (Math.abs(value) < 1) return "flat";
    return value > 0 ? "up" : "down";
  }

  function displayPrice(value: number | null | undefined): string {
    return value === null || value === undefined ? c.noPrice : formatPlatinum(value, $locale);
  }

  function priorityRank(row: SellNowRow): number | null {
    return priorityRanks.get(sellNowRowIdentity(row)) ?? null;
  }

  function priorityVisualBand(rank: number | null): "high" | "medium" | "low" | "none" {
    if (rank === null) return "none";
    if (rank <= 10) return "high";
    if (rank <= 50) return "medium";
    return "low";
  }

  onMount(() => {
    let disposed = false;
    let unlistenMarket: UnlistenFn | undefined;
    let unlistenInventory: UnlistenFn | undefined;
    const savedView = loadSellNowViewPreferences();
    category = savedView.category;
    preset = savedView.preset;
    equipped = savedView.equipped;
    sortKey = savedView.sortKey;
    sortDirection = savedView.sortDirection;
    viewPreferencesReady = true;
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
      unlistenMarket?.();
      unlistenInventory?.();
    };
  });
</script>

<div class="sell-now-status" role="status" aria-live="polite">{scanning ? c.scanningInventory : resultStatus}</div>

{#if errorMessage}
  <div class="error-block" role="alert">
    <p>{errorMessage}</p>
    <button type="button" onclick={loadSellNow}>{c.retry}</button>
  </div>
{/if}

{#if !loading && !view}
  <section class="empty-panel" aria-labelledby="sell-now-empty-heading">
    <p class="empty-panel__label">{c.notImported}</p>
    <h2 id="sell-now-empty-heading">{c.addSnapshot}</h2>
    <p>{c.addSnapshotBody}</p>
    <button type="button" onclick={scanWarframe} disabled={scanning}>{scanning ? c.scanningInventory : c.openInventory}</button>
  </section>
{:else if view}
  <section class="inventory-command-bar" aria-label={c.inventoryUpdated}>
    <div class="inventory-command-bar__status">
      <strong>{c.inventoryUpdated}</strong>
      <span>{inventorySourceLabel(view.inventoryMetadata.source, $locale)} · <time datetime={view.inventoryMetadata.observedAt}>{new Date(view.inventoryMetadata.observedAt).toLocaleString(localeCode($locale))}</time></span>
    </div>
    <label class="inventory-reserve-control" for="keep-copies">
      <span>{c.reserve}</span>
      <select id="keep-copies" value={String(view.keepCopies)} disabled={reserveUpdating || scanning} onchange={updateReserve}>
        <option value="0">0</option><option value="1">1</option><option value="2">2</option>
      </select>
    </label>
    <button type="button" onclick={scanWarframe} disabled={loading || scanning}>{scanning ? c.scanningInventory : c.scanInventory}</button>
  </section>

  <section class="sell-summary" aria-labelledby="sell-summary-heading">
    <h2 id="sell-summary-heading" class="sr-only">{c.summary}</h2>
    <dl>
      <div><dt>{c.totalCopies}</dt><dd>{view.inventorySummary.ownedQuantity.toLocaleString(localeCode($locale))}</dd></div>
      <div><dt>{c.candidates}</dt><dd>{view.summary.candidateRows.toLocaleString(localeCode($locale))}</dd></div>
      <div><dt>{c.recommended}</dt><dd>{recommendedCount.toLocaleString(localeCode($locale))}</dd></div>
      <div><dt>{c.nominal}</dt><dd>{formatPlatinum(view.summary.nominalValue, $locale)}</dd></div>
    </dl>
    <p><strong>{c.notForecast}</strong> {c.nominalBody}</p>
  </section>

  {#if view.rows.length === 0}
    <section class="empty-panel" aria-labelledby="sell-now-zero-heading">
      <p class="empty-panel__label">{c.noCandidatesLabel}</p>
      <h2 id="sell-now-zero-heading">{c.noCandidates}</h2>
      <p>{c.noCandidatesBody}</p>
      <button type="button" onclick={() => (preset = "all")}>{c.allCandidates}</button>
    </section>
  {:else}
    <section class="sell-toolbar" aria-labelledby="sell-filter-heading">
      <h2 id="sell-filter-heading" class="sr-only">{c.filters}</h2>
      <div class="search-field">
        <label for="sell-search">{c.search}</label>
        <input id="sell-search" type="search" bind:value={query} maxlength="80" autocomplete="off" placeholder={c.searchExample} />
      </div>
      <div class="filter-field">
        <label for="sell-category">{c.category}</label>
        <select id="sell-category" bind:value={category}>
          <option value="all">{c.allCategories}</option>
          {#each categories as itemCategory}
            <option value={itemCategory}>{categoryLabels[itemCategory]}</option>
          {/each}
        </select>
      </div>
      <div class="filter-field">
        <label for="sell-equipped">{c.usage}</label>
        <select id="sell-equipped" bind:value={equipped}>
          <option value="all">{c.allUsage}</option>
          <option value="free">{c.freeOnly}</option>
          <option value="equipped">{c.equippedOnly}</option>
        </select>
      </div>
      <div class="filter-field">
        <label for="sell-view">{c.view}</label>
        <select id="sell-view" bind:value={preset}>
          <option value="all">{c.allCandidates}</option>
          <option value="sellable">{c.sellable}</option>
          <option value="sell_now">{c.sellNow}</option>
          <option value="hold">{c.hold}</option>
          <option value="duplicates">{c.duplicates}</option>
          <option value="unpriced">{c.unpriced}</option>
          <option value="attention">{c.attention}</option>
        </select>
      </div>
    </section>

    <div class="sell-now-layout">
      <section class="results-panel sell-results" aria-labelledby="sell-results-heading">
        <div class="panel-heading">
          <div>
            <h2 id="sell-results-heading">{c.queue}</h2>
            <p>{c.exactSnapshot(view.marketSnapshot?.sourceDate ?? c.missingSnapshot)}</p>
          </div>
          <span class="result-count">{visibleRows.length}</span>
        </div>

        {#if visibleRows.length}
          <div class="table-wrap">
            <table class="sell-table">
              <caption class="sr-only">{c.tableCaption}</caption>
              <thead>
                <tr>
                  <th scope="col" aria-sort={sortAria("name", sortKey, sortDirection)}><button type="button" onclick={() => changeSort("name")}>{c.item} <span aria-hidden="true">{sortMarker("name", sortKey, sortDirection)}</span></button></th>
                  <th scope="col" aria-sort={sortAria("sellable", sortKey, sortDirection)}><button type="button" onclick={() => changeSort("sellable")}>{c.ownedSellable} <span aria-hidden="true">{sortMarker("sellable", sortKey, sortDirection)}</span></button></th>
                  <th scope="col" aria-sort={sortAria("fair", sortKey, sortDirection)}><button type="button" onclick={() => changeSort("fair")}>{c.fairListQuick} <span aria-hidden="true">{sortMarker("fair", sortKey, sortDirection)}</span></button></th>
                  <th scope="col" aria-sort={sortAria("volume", sortKey, sortDirection)}><button type="button" onclick={() => changeSort("volume")}>{c.salesPerDay} <span aria-hidden="true">{sortMarker("volume", sortKey, sortDirection)}</span></button></th>
                  <th scope="col" aria-sort={sortAria("trend", sortKey, sortDirection)}><button type="button" onclick={() => changeSort("trend")}>{c.priceTrend90} <span aria-hidden="true">{sortMarker("trend", sortKey, sortDirection)}</span></button></th>
                  <th scope="col">{c.moment}</th>
                  <th scope="col" aria-sort={sortAria("priority", sortKey, sortDirection)}><button type="button" title={c.priorityHint} onclick={() => changeSort("priority")}>{c.priority} <span aria-hidden="true">{sortMarker("priority", sortKey, sortDirection)}</span></button></th>
                </tr>
              </thead>
              <tbody>
                {#each visibleRows as row (sellNowRowIdentity(row))}
                  <tr class:selected={selectedRow && sellNowRowIdentity(row) === sellNowRowIdentity(selectedRow)}>
                    <td data-label={c.item}>
                      <button class="item-button" type="button" onclick={() => selectRow(row)}>
                        {#if row.inventory.imageUrl}
                          <img class="item-thumb" src={row.inventory.imageUrl} alt="" loading="lazy" decoding="async" />
                        {/if}
                        <span class="item-button__copy">
                        <span>{row.inventory.displayName}</span>
                        <small>{row.inventory.key ? variantLabel(row.inventory.key, $locale) : c.unknownVariant}</small>
                        {#if row.inventory.equippedQuantity > 0}<small class="equipped-note">{c.equippedCount(row.inventory.equippedQuantity)}</small>{/if}
                        </span>
                      </button>
                    </td>
                    <td class="numeric stacked-cell" data-label={c.ownedSellable}><span>{row.inventory.ownedQuantity} {c.owned}</span><strong>{row.inventory.sellableQuantity} {c.sellableLabel}</strong></td>
                    <td class="numeric stacked-cell" data-label={c.fairListQuick}>
                      <strong>{displayPrice(row.recommendation?.listPrice ?? row.recommendation?.fairPrice)}</strong>
                    </td>
                    <td class="numeric stacked-cell" data-label={c.salesPerDay}>
                      <strong>{c.dailyTrades(formatVolume(row.recommendation?.closedVolume ?? null, $locale))}</strong>
                    </td>
                    <td class="numeric stacked-cell" data-label={c.priceTrend90}>
                      <strong class={`price-trend price-trend--${priceTrendClass(row.trend?.change90d)}`}>{priceTrendText(row.trend?.change90d)}</strong>
                    </td>
                    <td class="stacked-cell" data-label={c.moment}><strong class={`timing-pill timing-pill--${row.trend?.timing ?? "unknown"}`} title={timingDescription(row)} aria-label={timingDescription(row)}>{shortTiming(row)}</strong></td>
                    <td class="numeric" data-label={c.priority}>
                      <span class={`priority priority--${priorityVisualBand(priorityRank(row))}`} title={c.priorityPosition(priorityRank(row), row.priority.score)} aria-label={c.priorityPosition(priorityRank(row), row.priority.score)}>
                        {priorityRank(row) === null ? "—" : `№${priorityRank(row)}`}<small>{row.priority.score}/100</small>
                      </span>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else}
          <div class="no-results">
            <h3>{c.noFiltered}</h3><p>{c.changeFilters}</p><button type="button" onclick={() => { query = ""; category = "all"; preset = "sell_now"; equipped = "all"; }}>{c.reset}</button>
          </div>
        {/if}
      </section>

      <aside class="detail-panel sell-detail" aria-labelledby="sell-detail-heading">
        {#if selectedRow}
          <div class="detail-heading">
            {#if selectedRow.inventory.imageUrl}<img class="detail-art" src={selectedRow.inventory.imageUrl} alt="" decoding="async" />{/if}
            <p>{c.candidate}</p>
            <h2 id="sell-detail-heading">{selectedRow.inventory.displayName}</h2>
            <span>{selectedRow.inventory.key ? variantLabel(selectedRow.inventory.key, $locale) : c.unknownVariant}</span>
          </div>

          <div class="live-actions">
            <button type="button" disabled={liveLoading || !selectedRow.inventory.key || selectedRow.inventory.sellableQuantity <= 0} onclick={() => loadLive(selectedRow)}>{liveLoading ? c.gettingLive : activeLive ? c.updateLive : c.getLive}</button>
            <div class="live-status" aria-live="polite">
              {#if activeLive}
                <span>{liveQuoteLabel(activeLive.quoteState, $locale)} · {activeLive.sellOrderCount} {c.sell} / {activeLive.buyOrderCount} {c.buy}</span>
                {#if activeLive.warning}
                  <strong>{$locale === "ru" ? "Часть текущих ордеров недоступна. Проверьте список перед публикацией." : "Some current orders are unavailable. Review the list before publishing."}</strong>
                {/if}
              {:else if liveError}
                <strong>{liveError}</strong>
              {:else}
                <span>{c.liveHint}</span>
              {/if}
            </div>
          </div>

          <dl class="price-grid sell-price-grid">
            <div class="price-grid__primary"><dt>{c.fairPrice}</dt><dd>{formatPlatinum(selectedRow.recommendation?.fairPrice ?? null, $locale)}</dd></div>
            <div><dt>{c.listPrice}</dt><dd>{formatPlatinum(selectedRow.recommendation?.listPrice ?? null, $locale)}</dd></div>
            <div><dt>{c.quickSell}</dt><dd>{formatPlatinum(selectedRow.recommendation?.quickSell ?? null, $locale)}</dd></div>
          </dl>

          <section class="wfm-order-panel" aria-labelledby="wfm-order-heading" aria-busy={accountLoading || orderBusy}>
            <div class="wfm-order-heading">
              <h3 id="wfm-order-heading">{c.wfmOrder}</h3>
              {#if currentOrder}
                <span class:published={currentOrder.visible}>{visibilityLabel(currentOrder.visible, $locale)}</span>
              {/if}
            </div>
            <div class="wfm-order-status" role="status" aria-live="polite">{orderStatusMessage}</div>

            {#if selectedRow.inventory.sellableQuantity <= 0}
              <p>{c.notSellable}</p>
            {:else if accountLoading}
              <p>{c.loadingOrders}</p>
            {:else if accountError}
              <div class="wfm-order-error" role="alert">
                <p>{accountError}</p>
                <button type="button" class="secondary" onclick={loadAccountOrders}>{c.retryOrders}</button>
              </div>
            {:else if !accountView?.connected}
              <div class="wfm-order-empty">
                <strong>{c.accountDisconnected}</strong>
                <p>{c.accountDisconnectedBody}</p>
                <button type="button" onclick={onOpenMarketSales}>{c.openAccount}</button>
              </div>
            {:else if !accountView.profile?.verification}
              <div class="wfm-order-empty">
                <strong>{c.unverifiedAccount}</strong>
                <button type="button" class="secondary" onclick={onOpenMarketSales}>{c.openAccount}</button>
              </div>
            {:else if !selectedRow.inventory.itemId || !selectedRow.inventory.key}
              <p class="inline-error" role="alert">{c.variantUnavailable}</p>
            {:else}
              <p class="current-order-line">
                {currentOrder
                  ? c.currentOrder(
                      currentOrder.platinum.toLocaleString($locale === "ru" ? "ru-RU" : "en-US"),
                      currentOrder.quantity,
                      visibilityLabel(currentOrder.visible, $locale),
                      currentOrder.perTrade,
                    )
                  : c.noCurrentOrder}
              </p>
              {#if currentOrder}
                <button type="button" onclick={onOpenMarketSales}>{c.manageOrder}</button>
              {:else if pendingListingAction}
                <section class="wfm-order-confirmation" aria-labelledby="wfm-order-confirmation-heading">
                  <h3 id="wfm-order-confirmation-heading" bind:this={listingConfirmationHeading} tabindex="-1">{c.createTitle}</h3>
                  <p>{c.confirmCreate(pendingListingAction.itemName, pendingListingAction.input.platinum, pendingListingAction.input.quantity, pendingListingAction.input.perTrade)}</p>
                  <label class="wfm-confirm-check"><input bind:this={listingConfirmationCheckbox} type="checkbox" bind:checked={listingConfirmationAccepted} aria-describedby={listingConfirmationError ? "sell-order-confirmation-error" : undefined} /> {c.confirmChecked}</label>
                  {#if listingConfirmationError}<p id="sell-order-confirmation-error" class="inline-error" role="alert">{listingConfirmationError}</p>{/if}
                  <div class="wfm-order-actions">
                    <button type="button" onclick={executeListingAction} disabled={orderBusy}>{c.createOrder}</button>
                    <button type="button" class="secondary" onclick={closeListingConfirmation} disabled={orderBusy}>{c.cancelOrderAction}</button>
                  </div>
                </section>
              {:else}
                <form class="wfm-order-form" onsubmit={prepareListingAction}>
                  <div class="wfm-order-fields">
                    <div class="filter-field">
                      <label for="sell-order-price">{selectedRow.inventory.bulkTradable ? c.bulkOrderPrice : c.orderPrice}</label>
                      <input id="sell-order-price" type="number" inputmode="numeric" bind:value={orderPrice} min="1" max="900000" step="1" required aria-describedby={orderFormError ? "sell-order-error" : undefined} aria-invalid={orderFormError ? "true" : undefined} />
                    </div>
                    <div class="filter-field">
                      <label for="sell-order-quantity">{c.orderQuantity}</label>
                      <input id="sell-order-quantity" type="number" inputmode="numeric" bind:value={orderQuantity} min="1" max={selectedRow.inventory.sellableQuantity} step="1" required aria-describedby={orderFormError ? "sell-order-error" : undefined} aria-invalid={orderFormError ? "true" : undefined} />
                    </div>
                    {#if selectedRow.inventory.bulkTradable}
                      <div class="filter-field bulk-trade-field">
                        <label for="sell-order-per-trade">{c.orderPerTrade}</label>
                        <input id="sell-order-per-trade" type="number" inputmode="numeric" bind:value={orderPerTrade} min="1" max="6" step="1" required aria-describedby={orderFormError ? "sell-order-error" : undefined} aria-invalid={orderFormError ? "true" : undefined} />
                      </div>
                    {/if}
                  </div>
                  <label class="wfm-order-visible"><input type="checkbox" bind:checked={orderVisible} /> {c.publishOrder}</label>
                  {#if orderFormError}<p id="sell-order-error" class="inline-error" role="alert">{orderFormError}</p>{/if}
                  <button type="submit" disabled={orderBusy}>{c.reviewCreate}</button>
                </form>
              {/if}
            {/if}

          </section>

          <div class="detail-meta">
            <div><span>{c.forSale}</span><strong>{selectedRow.inventory.sellableQuantity} {c.of} {selectedRow.inventory.ownedQuantity}</strong></div>
            <div><span>{c.moment}</span><strong>{selectedRow.trend?.timing ? timingLabel(selectedRow.trend.timing, $locale) : c.noSignal}</strong></div>
            <div><span>{c.priority}</span><strong>{c.priorityPosition(priorityRank(selectedRow), selectedRow.priority.score)}</strong></div>
            <div><span>{c.nominal}</span><strong>{formatPlatinum(selectedRow.nominalValue, $locale)}</strong></div>
          </div>
          <p class="nominal-warning">{c.nominalWarning}</p>

          <details class="explanation">
            <summary>{c.whyPrice}</summary>
            {#if selectedRow.recommendation?.reasons.length}
              <ul>{#each selectedRow.recommendation.reasons as reason}<li>{priceReasonMessage(reason, $locale)}</li>{/each}</ul>
            {:else}
              <p>{c.noPriceSignal}</p>
            {/if}
          </details>
          <details class="explanation">
            <summary>{c.whyPriority}</summary>
            <ul>{#each priorityReasonMessages(selectedRow, $locale, priorityRank(selectedRow)) as reason}<li>{reason}</li>{/each}</ul>
          </details>
        {:else}
          <div class="detail-placeholder"><h2 id="sell-detail-heading">{c.details}</h2><p>{c.selectCandidate}</p></div>
        {/if}
      </aside>
    </div>
  {/if}
{/if}
