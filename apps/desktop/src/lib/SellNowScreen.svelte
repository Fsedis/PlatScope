<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick } from "svelte";
  import { useLocale } from "./i18n";
  import {
    accountActionErrorMessage,
    createListingInputFromInventory,
    matchingSellOrder,
    validateListingNumbers,
    visibilityLabel,
    type AccountOrder,
    type AccountView,
    type CreateListingInput,
    type UpdateListingInput,
  } from "./account";
  import {
    INVENTORY_CATEGORIES,
    inventoryCategory,
    type InventoryCategoryFilter,
  } from "./inventory";

  import { formatChange, timingLabel } from "./history";
  import { confidenceLabel, formatPlatinum, formatVolume, liveQuoteLabel, priceReasonMessage, variantLabel } from "./market";
  import {
    filterAndSortSellNowRows,
    priorityLabel,
    priorityReasonMessages,
    sellNowRowIdentity,
    type LiveSellNowResult,
    type SellNowConfidenceFilter,
    type SellNowFilters,
    type SellNowPreset,
    type SellNowRow,
    type SellNowSortDirection,
    type SellNowSortKey,
    type SellNowTimingFilter,
    type SellNowView,
  } from "./sellNow";
  import {
    loadSellNowViewPreferences,
    saveSellNowViewPreferences,
  } from "./viewPreferences";

  export let onOpenInventory: () => void;
  export let onOpenAccount: () => void;

  type PendingListingAction =
    | { kind: "create"; input: CreateListingInput; itemName: string }
    | { kind: "update"; id: string; input: UpdateListingInput; order: AccountOrder; itemName: string }
    | { kind: "delete"; id: string; order: AccountOrder; itemName: string };

  const locale = useLocale();
  const categoryCopy = {
    ru: { mod: "Моды", arcane_enhancement: "Мистификаторы", relic: "Реликвии", component: "Компоненты", weapon: "Оружие", warframe: "Варфреймы", misc: "Прочее" },
    en: { mod: "Mods", arcane_enhancement: "Arcanes", relic: "Relics", component: "Components", weapon: "Weapons", warframe: "Warframes", misc: "Other" },
  } as const;
  const copy = {
    ru: {
      matching: "Сопоставляем инвентарь с локальными ценами…", shown: (visible: number, total: number) => `${visible} из ${total} кандидатов показано`,
      loadError: (reason: string) => `Не удалось собрать список продажи. Инвентарь и рыночные данные не изменены. Техническая причина: ${reason}`,
      missingVariant: "Точный вариант больше не доступен для продажи. Обновите список.", liveError: (reason: string) => `Текущая цена недоступна; локальная оценка сохранена. ${reason}`,
      noSignal: "Нет сигнала", retry: "Повторить расчёт", notImported: "Инвентарь не импортирован", addSnapshot: "Добавьте локальный снимок инвентаря",
      addSnapshotBody: "После импорта PlatScope покажет только подтверждённые копии, которые можно продать.", openInventory: "Открыть инвентарь",
      summary: "Сводка продажи", candidates: "Кандидаты", priced: "С ценой", highPriority: "Высокий priority", nominal: "Номинальная стоимость",
      notForecast: "Не прогноз выручки:", nominalBody: "номинальная стоимость — это sellable × fair. Она не гарантирует продажу всего объёма по этой цене.",
      noCandidatesLabel: "Кандидатов нет", noCandidates: "Нет подтверждённых предметов для продажи", noCandidatesBody: "Проверьте импорт, tradeable-статус и резерв копий. Неоднозначные варианты автоматически исключены.", checkInventory: "Проверить инвентарь",
      filters: "Поиск и фильтры продажи", search: "Поиск предмета", searchExample: "Например, Поток Прайм", category: "Тип предмета", allCategories: "Все типы", view: "Рабочий вид", allCandidates: "Все кандидаты", sellNow: "Продавать сейчас", unpriced: "Без цены",
      confidence: "Уверенность", any: "Любая", high: "Высокая", medium: "Средняя", low: "Низкая", unrated: "Нет оценки", timing: "Момент продажи", anyTiming: "Любой",
      queue: "Очередь продажи", exactSnapshot: (date: string) => `Точные варианты · снимок ${date}`, missingSnapshot: "не найден",
      tableCaption: "Предметы инвентаря, цены, ликвидность, момент и приоритет продажи", item: "Предмет", volumeTrend: "Сделки и цена", unknownVariant: "вариант не определён",
      dailyTrades: (value: string) => `Сделки за день: ${value}`,
      averageTrades: (value: string) => `Среднее за 7 дней: ${value}/день`,
      priceChange: (value: string) => `Цена за 7 дней: ${value}`,
      noFiltered: "Нет кандидатов для этих фильтров", changeFilters: "Измените тип предмета или другие фильтры.", reset: "Сбросить фильтры",
      candidate: "Кандидат на продажу", gettingLive: "Получаем текущую цену…", updateLive: "Обновить цену сейчас", getLive: "Получить цену сейчас",
      liveHint: "Один запрос для выбранного точного варианта. Quick Sell не подменяется исторической buy-ценой.", forSale: "Для продажи", of: "из", moment: "Момент",
      nominalWarning: "Номинальная стоимость не учитывает скорость продажи всего объёма.", whyPrice: "Почему такая цена?", noPriceSignal: "Надёжного ценового сигнала пока нет.", whyPriority: "Почему такой priority?", details: "Подробности продажи", selectCandidate: "Выберите кандидата в очереди.",
      ownedSellable: "Есть / для продажи", owned: "есть", sellableLabel: "для продажи", fairListQuick: "Цена",
      estimatedPrice: (value: string) => `Оценка: ${value}`,
      listingPrice: (value: string) => `Выставить: ${value}`,
      instantPrice: (value: string) => `Продать сразу: ${value}`,
      noPrice: "нет цены",
      confidenceTiming: "Уверенность / момент", priority: "Приоритет", fairPrice: "Справедливая цена", listPrice: "Цена размещения", quickSell: "Быстрая продажа", sell: "продажа", buy: "покупка",
      wfmOrder: "Ордер WFM", loadingOrders: "Проверяем ваши ордера…", accountUnavailable: "Не удалось загрузить ордера WFM.", retryOrders: "Повторить", accountDisconnected: "Аккаунт WFM не подключён", accountDisconnectedBody: "Подключите аккаунт, чтобы выставлять и менять ордера прямо здесь.", openAccount: "Подключить аккаунт WFM", unverifiedAccount: "WFM разрешает изменение ордеров только подтверждённому аккаунту.", noCurrentOrder: "Ордер не выставлен", currentOrder: (price: string, quantity: number, status: string, perTrade: number | null) => perTrade === null ? `Ваш ордер: ${price}p × ${quantity} · ${status}` : `Ваш ордер: ${quantity} шт., по ${perTrade} за сделку, цена сделки ${price}p · ${status}`,
      orderPrice: "Цена, платина", bulkOrderPrice: "Цена за сделку, платина", orderQuantity: "Всего предметов", orderPerTrade: "Предметов за одну сделку", publishOrder: "Опубликовать ордер", reviewCreate: "Проверить новый ордер", reviewUpdate: "Проверить изменения", removeOrder: "Снять ордер", variantUnavailable: "Точный WFM-вариант недоступен. Обновите рыночные данные.", createTitle: "Подтвердите новый ордер", updateTitle: "Подтвердите изменения", deleteTitle: "Подтвердите снятие ордера", confirmCreate: (name: string, price: number, quantity: number, perTrade: number | null) => perTrade === null ? `${name}: выставить ${quantity} шт. по ${price}p.` : `${name}: всего ${quantity} шт., по ${perTrade} за сделку, цена сделки ${price}p.`, confirmUpdate: (name: string, price: number, quantity: number, perTrade: number | null) => perTrade === null ? `${name}: изменить ордер на ${price}p × ${quantity}.` : `${name}: всего ${quantity} шт., по ${perTrade} за сделку, цена сделки ${price}p.`, confirmDelete: (name: string) => `${name}: ордер будет удалён с WFM.`, confirmChecked: "Я проверил предмет, вариант, цену, количество и размер сделки", createOrder: "Создать ордер", updateOrder: "Сохранить изменения", deleteOrder: "Снять ордер", cancelOrderAction: "Отменить", confirmRequired: "Подтвердите, что проверили параметры ордера.", orderCreated: "Ордер создан на WFM.", orderUpdated: "Ордер обновлён на WFM.", orderDeleted: "Ордер снят с WFM.", orderActionError: (reason: string) => accountActionErrorMessage(reason, "ru"),
    },
    en: {
      matching: "Matching inventory with local prices…", shown: (visible: number, total: number) => `${visible} of ${total} candidates shown`,
      loadError: (reason: string) => `Unable to build the sell queue. Inventory and market data were not changed. Technical reason: ${reason}`,
      missingVariant: "The exact variant is no longer sellable. Refresh the list.", liveError: (reason: string) => `Current price unavailable; the local estimate was preserved. ${reason}`,
      noSignal: "No signal", retry: "Recalculate", notImported: "Inventory not imported", addSnapshot: "Add a local inventory snapshot",
      addSnapshotBody: "After import, PlatScope shows only confirmed copies that can be sold.", openInventory: "Open inventory",
      summary: "Sell summary", candidates: "Candidates", priced: "Priced", highPriority: "High priority", nominal: "Nominal value",
      notForecast: "Not a revenue forecast:", nominalBody: "nominal value is sellable × fair. It does not guarantee that the full volume will sell at that price.",
      noCandidatesLabel: "No candidates", noCandidates: "No confirmed items to sell", noCandidatesBody: "Check the import, tradeability, and copy reserve. Ambiguous variants are excluded automatically.", checkInventory: "Check inventory",
      filters: "Sell queue search and filters", search: "Search items", searchExample: "For example, Primed Flow", category: "Item type", allCategories: "All types", view: "Working view", allCandidates: "All candidates", sellNow: "Sell now", unpriced: "Unpriced",
      confidence: "Confidence", any: "Any", high: "High", medium: "Medium", low: "Low", unrated: "Not rated", timing: "Timing", anyTiming: "Any",
      queue: "Sell queue", exactSnapshot: (date: string) => `Exact variants · snapshot ${date}`, missingSnapshot: "not found",
      tableCaption: "Inventory items, prices, liquidity, timing, and sell priority", item: "Item", volumeTrend: "Sales and price", unknownVariant: "variant unavailable",
      dailyTrades: (value: string) => `Trades per day: ${value}`,
      averageTrades: (value: string) => `7-day average: ${value}/day`,
      priceChange: (value: string) => `Price over 7 days: ${value}`,
      noFiltered: "No candidates match these filters", changeFilters: "Change the item type or another filter.", reset: "Reset filters",
      candidate: "Sell candidate", gettingLive: "Getting current price…", updateLive: "Refresh current price", getLive: "Get current price",
      liveHint: "One request for the selected exact variant. Quick Sell is never replaced with a historical buy price.", forSale: "For sale", of: "of", moment: "Timing",
      nominalWarning: "Nominal value does not account for how quickly the full volume may sell.", whyPrice: "Why this price?", noPriceSignal: "No reliable price signal is available yet.", whyPriority: "Why this priority?", details: "Sell details", selectCandidate: "Select a candidate from the queue.",
      ownedSellable: "Owned / sellable", owned: "owned", sellableLabel: "sellable", fairListQuick: "Price",
      estimatedPrice: (value: string) => `Estimate: ${value}`,
      listingPrice: (value: string) => `List at: ${value}`,
      instantPrice: (value: string) => `Sell now: ${value}`,
      noPrice: "no price",
      confidenceTiming: "Confidence / timing", priority: "Priority", fairPrice: "Fair price", listPrice: "List price", quickSell: "Quick Sell", sell: "sell", buy: "buy",
      wfmOrder: "WFM order", loadingOrders: "Checking your orders…", accountUnavailable: "Unable to load WFM orders.", retryOrders: "Try again", accountDisconnected: "WFM account not connected", accountDisconnectedBody: "Connect your account to create and manage orders here.", openAccount: "Connect WFM account", unverifiedAccount: "WFM allows order changes only for verified accounts.", noCurrentOrder: "Not listed", currentOrder: (price: string, quantity: number, status: string, perTrade: number | null) => perTrade === null ? `Your order: ${price}p × ${quantity} · ${status}` : `Your order: ${quantity} total, ${perTrade} per trade at ${price}p per trade · ${status}`,
      orderPrice: "Price, platinum", bulkOrderPrice: "Price per trade, platinum", orderQuantity: "Total items", orderPerTrade: "Items per trade", publishOrder: "Publish order", reviewCreate: "Review new order", reviewUpdate: "Review changes", removeOrder: "Remove order", variantUnavailable: "The exact WFM variant is unavailable. Refresh market data.", createTitle: "Confirm new order", updateTitle: "Confirm changes", deleteTitle: "Confirm order removal", confirmCreate: (name: string, price: number, quantity: number, perTrade: number | null) => perTrade === null ? `${name}: list ${quantity} at ${price}p each.` : `${name}: ${quantity} total, ${perTrade} per trade at ${price}p per trade.`, confirmUpdate: (name: string, price: number, quantity: number, perTrade: number | null) => perTrade === null ? `${name}: change the order to ${price}p × ${quantity}.` : `${name}: ${quantity} total, ${perTrade} per trade at ${price}p per trade.`, confirmDelete: (name: string) => `${name}: the order will be deleted from WFM.`, confirmChecked: "I reviewed the item, variant, price, quantity, and trade size", createOrder: "Create order", updateOrder: "Save changes", deleteOrder: "Remove order", cancelOrderAction: "Cancel", confirmRequired: "Confirm that you reviewed the order parameters.", orderCreated: "Order created on WFM.", orderUpdated: "Order updated on WFM.", orderDeleted: "Order removed from WFM.", orderActionError: (reason: string) => accountActionErrorMessage(reason, "en"),
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
  let errorMessage = "";
  let selectedIdentity = "";
  let liveIdentity = "";
  let liveResult: LiveSellNowResult | null = null;
  let liveLoading = false;
  let liveError = "";
  let query = "";
  let category: InventoryCategoryFilter = "all";
  let preset: SellNowPreset = "all";
  let confidence: SellNowConfidenceFilter = "all";
  let timing: SellNowTimingFilter = "all";
  let sortKey: SellNowSortKey = "priority";
  let sortDirection: SellNowSortDirection = "desc";
  let viewPreferencesReady = false;

  $: filters = {
    query,
    category,
    preset,
    confidence,
    timing,
    sortKey,
    sortDirection,
  } satisfies SellNowFilters;
  $: if (viewPreferencesReady) {
    saveSellNowViewPreferences({
      category,
      preset,
      confidence,
      timing,
      sortKey,
      sortDirection,
    });
  }
  $: visibleRows = filterAndSortSellNowRows(view?.rows ?? [], filters);
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

  async function loadAccountOrders(): Promise<void> {
    accountLoading = true;
    accountError = "";
    try {
      accountView = await invoke<AccountView>("account_status");
    } catch (error) {
      accountView = null;
      accountError = `${c.accountUnavailable} ${String(error)}`;
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
    orderPrice = order?.platinum ?? Math.max(
      1,
      Math.round(row.recommendation?.listPrice ?? row.recommendation?.fairPrice ?? 1),
    );
    orderQuantity = order?.quantity ?? Math.max(1, row.inventory.sellableQuantity);
    orderPerTrade = order?.perTrade ?? 1;
    orderVisible = order?.visible ?? true;
    orderFormError = "";
    listingConfirmationError = "";
  }

  function prepareListingAction(event: SubmitEvent): void {
    event.preventDefault();
    if (!selectedRow) return;
    const perTrade = selectedRow.inventory.bulkTradable ? orderPerTrade : null;
    orderFormError = validateListingNumbers(orderPrice, orderQuantity, perTrade, $locale) ?? "";
    if (orderFormError) return;
    const trigger = event.submitter as HTMLElement | null;
    if (currentOrder) {
      openListingConfirmation(
        {
          kind: "update",
          id: currentOrder.id,
          order: currentOrder,
          itemName: selectedRow.inventory.displayName,
          input: {
            platinum: orderPrice,
            quantity: orderQuantity,
            visible: orderVisible,
            perTrade,
            rank: null,
            charges: null,
            subtype: null,
            amberStars: null,
            cyanStars: null,
          },
        },
        trigger,
      );
      return;
    }
    const input = createListingInputFromInventory(
      selectedRow.inventory,
      orderPrice,
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

  function prepareRemoveOrder(trigger: HTMLElement): void {
    if (!selectedRow || !currentOrder) return;
    openListingConfirmation(
      {
        kind: "delete",
        id: currentOrder.id,
        order: currentOrder,
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
      if (action.kind === "create") {
        await invoke<AccountOrder>("account_create_listing", {
          input: action.input,
          confirmed: true,
        });
        orderStatusMessage = c.orderCreated;
      } else if (action.kind === "update") {
        await invoke<AccountOrder>("account_update_listing", {
          id: action.id,
          input: action.input,
          confirmed: true,
        });
        orderStatusMessage = c.orderUpdated;
      } else {
        await invoke<AccountOrder>("account_delete_listing", {
          id: action.id,
          confirmed: true,
        });
        orderStatusMessage = c.orderDeleted;
      }
      closeListingConfirmation();
      await loadAccountOrders();
    } catch (error) {
      listingConfirmationError = c.orderActionError(String(error));
    } finally {
      orderBusy = false;
    }
  }

  function listingConfirmationTitle(action: PendingListingAction): string {
    if (action.kind === "create") return c.createTitle;
    if (action.kind === "update") return c.updateTitle;
    return c.deleteTitle;
  }

  function listingConfirmationSummary(action: PendingListingAction): string {
    if (action.kind === "create") {
      return c.confirmCreate(action.itemName, action.input.platinum, action.input.quantity, action.input.perTrade);
    }
    if (action.kind === "update") {
      return c.confirmUpdate(
        action.itemName,
        action.input.platinum ?? action.order.platinum,
        action.input.quantity ?? action.order.quantity,
        action.input.perTrade ?? action.order.perTrade,
      );
    }
    return c.confirmDelete(action.itemName);
  }

  function listingConfirmationButton(action: PendingListingAction): string {
    if (action.kind === "create") return c.createOrder;
    if (action.kind === "update") return c.updateOrder;
    return c.deleteOrder;
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
    return {
      candidateRows: rows.length,
      pricedRows: rows.filter((row) => row.recommendation?.fairPrice !== null && row.recommendation !== null).length,
      highPriorityRows: rows.filter((row) => row.priority.band === "high").length,
      inventoryNominalValue: view?.summary.inventoryNominalValue ?? 0,
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
    return row.trend?.timing?.toUpperCase() ?? c.noSignal;
  }

  function timingDescription(row: SellNowRow): string {
    return row.trend?.timing ? timingLabel(row.trend.timing, $locale) : c.noSignal;
  }

  function displayPrice(value: number | null | undefined): string {
    return value === null || value === undefined ? c.noPrice : formatPlatinum(value, $locale);
  }

  onMount(() => {
    const savedView = loadSellNowViewPreferences();
    category = savedView.category;
    preset = savedView.preset;
    confidence = savedView.confidence;
    timing = savedView.timing;
    sortKey = savedView.sortKey;
    sortDirection = savedView.sortDirection;
    viewPreferencesReady = true;
    void loadSellNow();
    void loadAccountOrders();
  });
</script>

<div class="sell-now-status" role="status" aria-live="polite">{resultStatus}</div>

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
    <button type="button" onclick={onOpenInventory}>{c.openInventory}</button>
  </section>
{:else if view}
  <section class="sell-summary" aria-labelledby="sell-summary-heading">
    <h2 id="sell-summary-heading" class="sr-only">{c.summary}</h2>
    <dl>
      <div><dt>{c.candidates}</dt><dd>{view.summary.candidateRows}</dd></div>
      <div><dt>{c.priced}</dt><dd>{view.summary.pricedRows}</dd></div>
      <div><dt>{c.highPriority}</dt><dd>{view.summary.highPriorityRows}</dd></div>
      <div><dt>{c.nominal}</dt><dd>{formatPlatinum(view.summary.nominalValue, $locale)}</dd></div>
    </dl>
    <p><strong>{c.notForecast}</strong> {c.nominalBody}</p>
  </section>

  {#if view.rows.length === 0}
    <section class="empty-panel" aria-labelledby="sell-now-zero-heading">
      <p class="empty-panel__label">{c.noCandidatesLabel}</p>
      <h2 id="sell-now-zero-heading">{c.noCandidates}</h2>
      <p>{c.noCandidatesBody}</p>
      <button type="button" onclick={onOpenInventory}>{c.checkInventory}</button>
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
        <label for="sell-view">{c.view}</label>
        <select id="sell-view" bind:value={preset}>
          <option value="all">{c.allCandidates}</option>
          <option value="sell_now">{c.sellNow}</option>
          <option value="high_priority">{c.highPriority}</option>
          <option value="unpriced">{c.unpriced}</option>
        </select>
      </div>
      <div class="filter-field">
        <label for="sell-confidence">{c.confidence}</label>
        <select id="sell-confidence" bind:value={confidence}>
          <option value="all">{c.any}</option><option value="high">{c.high}</option><option value="medium">{c.medium}</option><option value="low">{c.low}</option><option value="unknown">{c.unrated}</option>
        </select>
      </div>
      <div class="filter-field">
        <label for="sell-timing">{c.timing}</label>
        <select id="sell-timing" bind:value={timing}>
          <option value="all">{c.anyTiming}</option>
          <option value="peak">PEAK</option>
          <option value="sell">SELL</option>
          <option value="neutral">NEUTRAL</option>
          <option value="hold">HOLD</option>
          <option value="unknown">{c.noSignal}</option>
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
                  <th scope="col" aria-sort={sortAria("volume", sortKey, sortDirection)}><button type="button" onclick={() => changeSort("volume")}>{c.volumeTrend} <span aria-hidden="true">{sortMarker("volume", sortKey, sortDirection)}</span></button></th>
                  <th scope="col" aria-sort={sortAria("trend", sortKey, sortDirection)}><button type="button" onclick={() => changeSort("trend")}>{c.confidenceTiming} <span aria-hidden="true">{sortMarker("trend", sortKey, sortDirection)}</span></button></th>
                  <th scope="col" aria-sort={sortAria("priority", sortKey, sortDirection)}><button type="button" onclick={() => changeSort("priority")}>{c.priority} <span aria-hidden="true">{sortMarker("priority", sortKey, sortDirection)}</span></button></th>
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
                        </span>
                      </button>
                    </td>
                    <td class="numeric stacked-cell" data-label={c.ownedSellable}><span>{row.inventory.ownedQuantity} {c.owned}</span><strong>{row.inventory.sellableQuantity} {c.sellableLabel}</strong></td>
                    <td class="numeric stacked-cell" data-label={c.fairListQuick}>
                      <span>{c.estimatedPrice(displayPrice(row.recommendation?.fairPrice))}</span>
                      <span>{c.listingPrice(displayPrice(row.recommendation?.listPrice))}</span>
                      <strong>{c.instantPrice(displayPrice(row.recommendation?.quickSell))}</strong>
                    </td>
                    <td class="numeric stacked-cell" data-label={c.volumeTrend}>
                      <span>{c.dailyTrades(formatVolume(row.recommendation?.closedVolume ?? null, $locale))}</span>
                      <span>{c.averageTrades(formatVolume(row.trend?.volumeAvg7d ?? null, $locale))}</span>
                      <strong>{c.priceChange(formatChange(row.trend?.change7d ?? null, $locale))}</strong>
                    </td>
                    <td class="stacked-cell" data-label={c.confidenceTiming}><span class={`confidence confidence--${row.recommendation?.confidence ?? "unknown"}`}>{confidenceLabel(row.recommendation?.confidence ?? "unknown", $locale)}</span><strong class={`timing-pill timing-pill--${row.trend?.timing ?? "unknown"}`} title={timingDescription(row)} aria-label={timingDescription(row)}>{shortTiming(row)}</strong></td>
                    <td class="numeric" data-label={c.priority}><span class={`priority priority--${row.priority.band}`}>{row.priority.score}<small>{priorityLabel(row.priority.band, $locale)}</small></span></td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else}
          <div class="no-results">
            <h3>{c.noFiltered}</h3><p>{c.changeFilters}</p><button type="button" onclick={() => { query = ""; category = "all"; preset = "all"; confidence = "all"; timing = "all"; }}>{c.reset}</button>
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
            <button type="button" disabled={liveLoading || !selectedRow.inventory.key} onclick={() => loadLive(selectedRow)}>{liveLoading ? c.gettingLive : activeLive ? c.updateLive : c.getLive}</button>
            <div class="live-status" aria-live="polite">
              {#if activeLive}
                <span>{liveQuoteLabel(activeLive.quoteState, $locale)} · {activeLive.sellOrderCount} {c.sell} / {activeLive.buyOrderCount} {c.buy}</span>
                {#if activeLive.warning}<strong>{activeLive.warning}</strong>{/if}
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

            {#if accountLoading}
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
                <button type="button" onclick={onOpenAccount}>{c.openAccount}</button>
              </div>
            {:else if !accountView.profile?.verification}
              <div class="wfm-order-empty">
                <strong>{c.unverifiedAccount}</strong>
                <button type="button" class="secondary" onclick={onOpenAccount}>{c.openAccount}</button>
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
              <form class="wfm-order-form" onsubmit={prepareListingAction}>
                <div class="wfm-order-fields">
                  <div class="filter-field">
                    <label for="sell-order-price">{selectedRow.inventory.bulkTradable ? c.bulkOrderPrice : c.orderPrice}</label>
                    <input id="sell-order-price" type="number" inputmode="numeric" bind:value={orderPrice} min="1" max="900000" step="1" required aria-describedby={orderFormError ? "sell-order-error" : undefined} aria-invalid={orderFormError ? "true" : undefined} />
                  </div>
                  <div class="filter-field">
                    <label for="sell-order-quantity">{c.orderQuantity}</label>
                    <input id="sell-order-quantity" type="number" inputmode="numeric" bind:value={orderQuantity} min="1" max="9999" step="1" required aria-describedby={orderFormError ? "sell-order-error" : undefined} aria-invalid={orderFormError ? "true" : undefined} />
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
                <div class="wfm-order-actions">
                  <button type="submit" disabled={orderBusy}>{currentOrder ? c.reviewUpdate : c.reviewCreate}</button>
                  {#if currentOrder}
                    <button type="button" class="danger-secondary" disabled={orderBusy} onclick={(event) => prepareRemoveOrder(event.currentTarget)}>{c.removeOrder}</button>
                  {/if}
                </div>
              </form>
            {/if}

            {#if pendingListingAction}
              <section class:destructive={pendingListingAction.kind === "delete"} class="wfm-order-confirmation" aria-labelledby="wfm-order-confirmation-heading">
                <h3 id="wfm-order-confirmation-heading" bind:this={listingConfirmationHeading} tabindex="-1">{listingConfirmationTitle(pendingListingAction)}</h3>
                <p>{listingConfirmationSummary(pendingListingAction)}</p>
                <label class="wfm-confirm-check"><input bind:this={listingConfirmationCheckbox} type="checkbox" bind:checked={listingConfirmationAccepted} aria-describedby={listingConfirmationError ? "sell-order-confirmation-error" : undefined} /> {c.confirmChecked}</label>
                {#if listingConfirmationError}<p id="sell-order-confirmation-error" class="inline-error" role="alert">{listingConfirmationError}</p>{/if}
                <div class="wfm-order-actions">
                  <button type="button" class:danger-primary={pendingListingAction.kind === "delete"} onclick={executeListingAction} disabled={orderBusy}>{listingConfirmationButton(pendingListingAction)}</button>
                  <button type="button" class="secondary" onclick={closeListingConfirmation} disabled={orderBusy}>{c.cancelOrderAction}</button>
                </div>
              </section>
            {/if}
          </section>

          <div class="detail-meta">
            <div><span>{c.forSale}</span><strong>{selectedRow.inventory.sellableQuantity} {c.of} {selectedRow.inventory.ownedQuantity}</strong></div>
            <div><span>{c.confidence}</span><strong>{confidenceLabel(selectedRow.recommendation?.confidence ?? "unknown", $locale)}</strong></div>
            <div><span>{c.moment}</span><strong>{selectedRow.trend?.timing ? timingLabel(selectedRow.trend.timing, $locale) : c.noSignal}</strong></div>
            <div><span>{c.priority}</span><strong>{selectedRow.priority.score}/100 · {priorityLabel(selectedRow.priority.band, $locale)}</strong></div>
            <div><span>{c.nominal}</span><strong>{formatPlatinum(selectedRow.nominalValue, $locale)}</strong></div>
          </div>
          <p class="nominal-warning">{c.nominalWarning}</p>

          <details class="explanation" open>
            <summary>{c.whyPrice}</summary>
            {#if selectedRow.recommendation?.reasons.length}
              <ul>{#each selectedRow.recommendation.reasons as reason}<li>{priceReasonMessage(reason, $locale)}</li>{/each}</ul>
            {:else}
              <p>{c.noPriceSignal}</p>
            {/if}
          </details>
          <details class="explanation">
            <summary>{c.whyPriority}</summary>
            <ul>{#each priorityReasonMessages(selectedRow, $locale) as reason}<li>{reason}</li>{/each}</ul>
          </details>
        {:else}
          <div class="detail-placeholder"><h2 id="sell-detail-heading">{c.details}</h2><p>{c.selectCandidate}</p></div>
        {/if}
      </aside>
    </div>
  {/if}
{/if}
