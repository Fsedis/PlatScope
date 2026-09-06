<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import { orderAdvice, orderChange, orderMarketPrice, orderUnchanged, reviewedChanges, reviewedQuantitiesMatch, salesFilterRows, type SalesFilter, type ReviewedOrderChange } from "./marketSales";
  import { variantLabel } from "./market";
  import { checkedLivePrice } from "./livePriceCheck";

  import {
    accountActionErrorMessage,
    orderEnglishName,
    validateListingNumbers,
    type AccountOrder,
    type AccountView,
    type CreateListingInput,
  } from "./account";
  import type { InventoryView } from "./inventory";
  import { formatPlatinum, type LivePricingResult, type PriceRecommendation } from "./market";
  import {
    applyPriceCheckFailures,
    buildTradeShiftRows,
    filterTradeShiftRows,
    isSaleTrade,
    normalizeTradeName,
    pendingSaleEvents,
    planTradeReconciliation,
    recommendationIdentity,
    updateInput,
    visibleTradeHistory,
    type TradeEvent,
    type TradeSalesSummary,
    type TradeReconciliationAction,
    type TradeShiftRow,
  } from "./tradeShift";

  export let onOpenInventory: () => void;
  export let onBrowseMarket: () => void;

  let account: AccountView | null = null;
  let inventory: InventoryView | null = null;
  let events: TradeEvent[] = [];
  let tradeSales: TradeSalesSummary = { saleCount: 0, platinumReceived: 0 };
  let recommendations = new Map<string, PriceRecommendation | null>();
  let liveCheckedAt = new Map<string, number>();
  let quoteTtlSeconds = 90;
  let loading = true;
  let refreshingLive = false;
  let stopLiveRefresh = false;
  let liveProgress = "";
  let failedPriceChecks = new Set<string>();
  let errorMessage = "";
  let actionMessage = "";
  let accountBusy = false;
  let accountPanelOpen = false;
  let email = "";
  let password = "";
  let selectedIds = new Set<string>();
  let reviewOpen = false;
  let visibilityIntent: boolean | null = null;
  let tradeToUndo: TradeEvent | null = null;
  let applying = false;
  let applyProgress = "";
  let editingOrder: AccountOrder | null = null;
  let editPlatinum = 1;
  let editQuantity = 1;
  let editVisible = false;
  let editError = "";
  let orderToRemove: AccountOrder | null = null;
  let orderQuery = "";
  let orderFilter: SalesFilter = "all";
  let reviewed: ReviewedOrderChange[] = [];
  let dataMessage = "";
  let historyUnavailable = false;
  let summaryUnavailable = false;
  let lastUpdated: number | null = null;
  let disposed = false;
  let dataRevision = 0;
  let suggestionsInitialized = false;
  let refreshPromise: Promise<void> | null = null;
  let orderDialog: HTMLDialogElement;
  let batchDialog: HTMLDialogElement;
  let visibilityDialog: HTMLDialogElement;
  let undoDialog: HTMLDialogElement;
  let visibilityTargets: AccountOrder[] = [];
  let historyFilter: "all" | "sell" | "buy" = "all";
  let historyExpanded = false;
  const rowChange = orderChange;
  const money = (value: number | null) => value == null ? "Нет оценки" : value.toLocaleString("ru-RU", { maximumFractionDigits: 1 }) + " пл.";

  async function read<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    let timer: ReturnType<typeof setTimeout>;
    try {
      return await Promise.race([invoke<T>(command, args), new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("Превышено время ожидания")), 20_000); })]);
    } finally { clearTimeout(timer!); }
  }
  async function openBatch(event: MouseEvent): Promise<void> {
    batchTrigger = event.currentTarget as HTMLElement;
    errorMessage = "";
    reviewed = reviewedChanges(selectedRows);
    reviewOpen = true;
    await tick();
    batchDialog.showModal();
  }
  async function confirmVisibility(visible: boolean): Promise<void> {
    errorMessage = "";
    visibilityIntent = visible;
    visibilityTargets = rows.filter(row => row.order.visible !== visible).map(row => ({ ...row.order }));
    await tick(); visibilityDialog.showModal();
  }
  async function confirmUndo(event: TradeEvent): Promise<void> {
    errorMessage = "";
    tradeToUndo = event; await tick(); undoDialog.showModal();
  }
  async function validateCurrentOrders(before: AccountOrder[]): Promise<AccountView> {
    ++dataRevision;
    const latest = await read<AccountView>("account_status");
    if (!latest.connected || !latest.profile?.verification) throw new Error("authorization");
    if (!before.every(order => orderUnchanged(order, latest.orders.find(candidate => candidate.id === order.id)))) {
      account = latest;
      throw new Error("Объявления изменились. Закройте окно и проверьте новые данные перед сохранением.");
    }
    return latest;
  }
  function changeError(error: unknown): string {
    const message = String(error);
    if (message.includes("Объявления изменились")) return "Объявления изменились. Закройте окно и проверьте новые данные перед сохранением.";
    if (message.includes("Остатки изменились")) return "Остатки изменились или недоступны. Закройте окно и обновите список перед сохранением.";
    return accountActionErrorMessage(message);
  }

  let editorTrigger: HTMLElement | null = null;
  let batchTrigger: HTMLElement | null = null;

  function closeEditor(): void {
    orderDialog?.close(); editingOrder = null; orderToRemove = null;
    editorTrigger?.focus({ preventScroll: true });
  }

  function closeBatchReview(): void {
    batchDialog?.close(); reviewOpen = false;
    batchTrigger?.focus({ preventScroll: true });
  }

  $: rows = account
    ? applyPriceCheckFailures(
        buildTradeShiftRows(account, inventory, recommendations),
        failedPriceChecks,
      )
    : [];
  $: visibleRows = salesFilterRows(filterTradeShiftRows(rows, orderQuery), orderFilter);
  $: actionableRows = rows.filter((row) => row.needsAction && rowChange(row) !== null);
  $: selectedRows = actionableRows.filter((row) => selectedIds.has(row.order.id));
  $: pendingEvents = pendingSaleEvents(events);
  $: filteredHistory = visibleTradeHistory(events, 100).filter(event => historyFilter === "all" || (historyFilter === "sell" ? isSaleTrade(event) : event.platinumGiven > 0 && event.platinumReceived === 0));
  $: historyEvents = filteredHistory.slice(0, historyExpanded ? 100 : 8);
  $: summary = summarize(rows);

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];
    void loadAll();
    void read<{ live_quote_ttl_seconds: number }>("load_settings").then(settings => {
      if (!disposed) quoteTtlSeconds = settings.live_quote_ttl_seconds;
    }).catch(() => { /* До загрузки настроек действует стандартный срок актуальности. */ });
    const autoRefresh = () => { if (!document.hidden && !loading && !accountBusy && !refreshingLive && !applying && !editingOrder && !reviewOpen && visibilityIntent === null && !tradeToUndo) void loadAll(true); };
    const refreshTimer = window.setInterval(autoRefresh, 60_000);
    document.addEventListener("visibilitychange", autoRefresh);
    void Promise.all([
      listen("trade-detected", () => void loadEvents(true)),
      listen("trade-reconciled", () => {
        actionMessage = "Продажа автоматически учтена в Warframe Market.";
        void loadAll();
      }),
      listen("trade-reconciliation-failed", () => void loadEvents()),
      listen("inventory-updated", () => void loadInventoryAndPrices()),
      listen("market-data-updated", () => void loadSavedPrices()),
    ]).then((items) => {
      if (disposed) items.forEach((unlisten) => unlisten());
      else unlisteners.push(...items);
    }).catch(() => { if (!disposed) dataMessage = "События игры временно недоступны. Список продолжит обновляться раз в минуту."; });
    return () => {
      disposed = true;
      window.clearInterval(refreshTimer);
      document.removeEventListener("visibilitychange", autoRefresh);
      stopLiveRefresh = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  });

  async function loadAll(background = false): Promise<void> {
    if (refreshPromise) return refreshPromise;
    refreshPromise = (async () => {
      if (!background) loading = true;
      const requestedRevision = ++dataRevision;
      errorMessage = "";
      dataMessage = "";
      const results = await Promise.allSettled([
        read<AccountView>("account_status"), read<InventoryView | null>("load_inventory"),
        read<TradeEvent[]>("trade_events"), read<TradeSalesSummary>("trade_sales_summary"),
      ]);
      if (disposed || requestedRevision !== dataRevision) return;
      const [accountResult, inventoryResult, eventsResult, summaryResult] = results;
      historyUnavailable = eventsResult.status === "rejected";
      summaryUnavailable = summaryResult.status === "rejected";
      if (accountResult.status === "fulfilled") {
        account = accountResult.value;
        lastUpdated = Date.now();
      } else {
        errorMessage = "Не удалось обновить объявления Warframe Market. Повторите загрузку.";
      }
      inventory = inventoryResult.status === "fulfilled" ? inventoryResult.value : null;
      if (eventsResult.status === "fulfilled") events = eventsResult.value;
      if (summaryResult.status === "fulfilled") tradeSales = summaryResult.value;
      if (inventoryResult.status === "rejected") dataMessage = "Не удалось проверить остатки. Количество в объявлениях пока не сравниваем.";
      if (eventsResult.status === "rejected" || summaryResult.status === "rejected") dataMessage += " История сделок временно недоступна.";
      loading = false;
      await loadSavedPrices();
    })().finally(() => { if (!disposed) loading = false; refreshPromise = null; });
    return refreshPromise;
  }

  async function loadInventoryAndPrices(): Promise<void> {
    try {
      inventory = await read<InventoryView | null>("load_inventory");
      await loadSavedPrices();
    } catch {
      actionMessage = "Инвентарь обновился, но сверить ордера не удалось.";
    }
  }

  async function loadEvents(announce = false): Promise<void> {
    try {
      [events, tradeSales] = await Promise.all([
        invoke<TradeEvent[]>("trade_events"),
        invoke<TradeSalesSummary>("trade_sales_summary"),
      ]);
      historyUnavailable = false; summaryUnavailable = false;
      if (announce) actionMessage = "Сделка записана. Синхронизируем продажу с Warframe Market автоматически.";
    } catch {
      historyUnavailable = true;
      if (announce) actionMessage = "Сделка обнаружена, но журнал пока не открылся. Обновите ордера.";
    }
  }

  async function loadSavedPrices(): Promise<void> {
    if (!account?.connected) return;
    const next = new Map([...recommendations].filter(([key]) => Date.now() - (liveCheckedAt.get(key) ?? 0) < quoteTtlSeconds * 1000));
    const currentRevision = dataRevision;
    for (const row of buildTradeShiftRows(account, inventory, next)) {
      if (disposed || currentRevision !== dataRevision) return;
      if (!row.key || next.has(recommendationIdentity(row.key))) continue;
      try {
        const result = await read<PriceRecommendation | null>("price_current_variant", {
          key: row.key,
          itemKind: row.itemKind,
        });
        next.set(recommendationIdentity(row.key), result);
      } catch {
        next.set(recommendationIdentity(row.key), null);
      }
    }
    if (disposed || currentRevision !== dataRevision) return;
    recommendations = next;
    if (!suggestionsInitialized) { selectSuggestedByDefault(buildTradeShiftRows(account, inventory, next)); suggestionsInitialized = true; }
  }

  async function refreshCurrentPrices(): Promise<void> {
    if (!account?.connected || refreshingLive) return;
    refreshingLive = true;
    const currentRevision = dataRevision;
    stopLiveRefresh = false;
    errorMessage = "";
    const candidates = rows.filter((row, index, source) => row.key
      && source.findIndex((candidate) => candidate.key
        && recommendationIdentity(candidate.key) === recommendationIdentity(row.key!)) === index);
    const next = new Map(recommendations);
    const candidateKeys = new Set(candidates.map((row) => recommendationIdentity(row.key!)));
    const failures = new Set([...failedPriceChecks].filter((key) => candidateKeys.has(key)));
    let checked = 0;
    let rateLimited = false;
    for (let index = 0; index < candidates.length; index += 1) {
      if (stopLiveRefresh) break;
      const row = candidates[index];
      liveProgress = `${index + 1} из ${candidates.length}: ${row.item?.displayName ?? "ордер"}`;
      try {
        const result = await read<LivePricingResult | null>("live_price_current_variant", {
          key: row.key,
          itemKind: row.itemKind,
        });
        if (checkedLivePrice(result).state === "failed") throw new Error("stale price fallback");
        if (row.key) failures.delete(recommendationIdentity(row.key));
        if (row.key) next.set(recommendationIdentity(row.key), result?.recommendation ?? null);
        if (row.key) liveCheckedAt.set(recommendationIdentity(row.key), Date.parse(result!.fetchedAt));
        checked += 1;
      } catch (error) {
        if (row.key) failures.add(recommendationIdentity(row.key));
        const reason = String(error).toLowerCase();
        rateLimited ||= reason.includes("rate limit") || reason.includes("429");
      }
      if (disposed || currentRevision !== dataRevision) { refreshingLive = false; return; }
      failedPriceChecks = new Set(failures);
      recommendations = new Map(next);
    }
    if (stopLiveRefresh) {
      liveProgress = `Проверка остановлена · проверено ${checked} из ${candidates.length}`;
    } else if (failures.size) {
      liveProgress = rateLimited
        ? `WFM ограничил запросы · проверено ${checked} из ${candidates.length}`
        : `Проверено: ${checked} из ${candidates.length} · не удалось: ${failures.size}`;
    } else {
      liveProgress = `Проверено: ${checked}`;
    }
    refreshingLive = false;
    selectSuggestedByDefault(applyPriceCheckFailures(
      buildTradeShiftRows(account, inventory, next),
      failures,
    ));
  }

  function selectSuggestedByDefault(source: TradeShiftRow[] = rows): void {
    selectedIds = new Set(
      source.filter((row) => row.needsAction && rowChange(row) !== null).map((row) => row.order.id),
    );
  }

  function toggleSelected(id: string): void {
    const next = new Set(selectedIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selectedIds = next;
  }

  async function applySelectedChanges(): Promise<void> {
    if (!account || !reviewed.length || applying) return;
    applying = true;
    errorMessage = "";
    let completed = 0;
    try {
      const latest = await validateCurrentOrders(reviewed.map(item => item.before));
      if (reviewed.some(item => item.change.quantity !== null)) {
        const currentInventory = await read<InventoryView | null>("load_inventory");
        inventory = currentInventory;
        if (!currentInventory || !reviewedQuantitiesMatch(reviewed, buildTradeShiftRows(latest, currentInventory, recommendations))) {
          throw new Error("Остатки изменились");
        }
      }
    }
    catch (error) { errorMessage = changeError(error); applying = false; return; }
    for (const proposal of reviewed) {
      const { before, change, name } = proposal;
      applyProgress = (completed + 1) + " из " + reviewed.length + ": " + name;
      try {
        if (change.delete) await invoke<AccountOrder>("account_delete_listing", { id: before.id, confirmed: true });
        else await invoke<AccountOrder>("account_update_listing", { id: before.id, input: updateInput({
          ...(change.price !== null ? { platinum: change.price } : {}),
          ...(change.quantity !== null ? { quantity: change.quantity } : {}),
        }), confirmed: true });
        completed += 1;
      } catch (error) { errorMessage = name + ": " + changeError(error); break; }
    }
    actionMessage = completed
      ? `Применено изменений: ${completed}.`
      : "Изменения не применены.";
    applying = false;
    closeBatchReview();
    applyProgress = "";
    await reloadAccount();
  }

  function beginManualEdit(row: TradeShiftRow): void {
    editorTrigger = document.activeElement as HTMLElement | null;
    editingOrder = { ...row.order };
    editPlatinum = row.order.platinum;
    editQuantity = row.order.quantity;
    editVisible = row.order.visible;
    editError = "";
    errorMessage = "";
    reviewOpen = false;
    void tick().then(() => orderDialog.showModal());
  }

  function reviewManualEdit(event: SubmitEvent): void {
    event.preventDefault();
    editError = validateListingNumbers(
      editPlatinum,
      editQuantity,
      editingOrder?.perTrade ?? null,
      "ru",
      rows.find((row) => row.order.id === editingOrder?.id)?.inventory?.sellableQuantity ?? null,
    ) ?? "";
    if (!editError) void applyManualEdit();
  }

  async function applyManualEdit(): Promise<void> {
    if (!editingOrder || applying) return;
    applying = true;
    errorMessage = "";
    try {
      await validateCurrentOrders([editingOrder]);
      await invoke<AccountOrder>("account_update_listing", {
        id: editingOrder.id,
        input: updateInput({
          platinum: editPlatinum,
          quantity: editQuantity,
          visible: editVisible,
        }),
        confirmed: true,
      });
      actionMessage = `Объявление «${manualOrderName(editingOrder)}» обновлено.`;
      closeEditor();
      await reloadAccount();
    } catch (error) {
      editError = changeError(error);
      } finally {
      applying = false;
    }
  }

  async function removeManualOrder(): Promise<void> {
    if (!orderToRemove || applying) return;
    applying = true;
    errorMessage = "";
    try {
      await validateCurrentOrders([orderToRemove]);
      await invoke<AccountOrder>("account_delete_listing", {
        id: orderToRemove.id,
        confirmed: true,
      });
      actionMessage = `Объявление «${manualOrderName(orderToRemove)}» снято с продажи.`;
      if (editingOrder?.id === orderToRemove.id) editingOrder = null;
      closeEditor();
      await reloadAccount();
    } catch (error) {
      errorMessage = changeError(error);
    } finally {
      applying = false;
    }
  }

  async function applyVisibility(): Promise<void> {
    if (!account || visibilityIntent === null || applying) return;
    const targets = visibilityTargets;
    applying = true;
    errorMessage = "";
    try { await validateCurrentOrders(targets); } catch (error) { errorMessage = changeError(error); applying = false; return; }
    let completed = 0;
    for (const row of targets) {
      applyProgress = `${completed + 1} из ${targets.length}`;
      try {
        await invoke<AccountOrder>("account_update_listing", {
          id: row.id,
          input: updateInput({ visible: visibilityIntent }),
          confirmed: true,
        });
        completed += 1;
      } catch (error) {
        errorMessage = changeError(error);
        break;
      }
    }
    actionMessage = visibilityIntent
      ? `Опубликовано объявлений: ${completed}.`
      : `Скрыто объявлений: ${completed}.`;
    applying = false;
    visibilityDialog.close(); visibilityIntent = null;
    applyProgress = "";
    await reloadAccount();
  }

  async function retryTrade(event: TradeEvent): Promise<void> {
    if (applying) return;
    applying = true;
    errorMessage = "";
    try {
      const completed = await invoke<boolean>("trade_event_retry", { id: event.id });
      actionMessage = completed
        ? "Продажа учтена в Warframe Market."
        : "Однозначный активный ордер пока не найден.";
    } catch (error) {
      errorMessage = changeError(error);
    } finally {
      applying = false;
      await reloadAccount();
      await loadEvents();
    }
  }

  async function undoTrade(event: TradeEvent): Promise<void> {
    if (!event.reconciliationJson || applying) return;
    let actions: TradeReconciliationAction[];
    try {
      actions = JSON.parse(event.reconciliationJson) as TradeReconciliationAction[];
    } catch {
      errorMessage = "Сохранённое изменение повреждено; отмена недоступна.";
      return;
    }
    if (actions.some((action) => action.kind === "close")) {
      errorMessage = "Продажа уже записана в статистику Warframe Market; отменить транзакцию через API нельзя.";
      return;
    }
    applying = true;
    try {
      for (const action of [...actions].reverse()) {
        if (action.kind === "delete") {
          const before = action.before;
          const input: CreateListingInput = {
            itemId: before.itemId ?? "",
            type: before.type,
            platinum: before.platinum,
            quantity: before.quantity,
            visible: before.visible,
            perTrade: before.perTrade,
            rank: before.rank,
            charges: before.charges,
            subtype: before.subtype,
            amberStars: before.amberStars,
            cyanStars: before.cyanStars,
          };
          await invoke<AccountOrder>("account_create_listing", { input, confirmed: true });
        } else {
          await invoke<AccountOrder>("account_update_listing", {
            id: action.before.id,
            input: updateInput({ quantity: action.before.quantity }),
            confirmed: true,
          });
        }
      }
      await invoke<boolean>("trade_event_restore", { id: event.id });
      actionMessage = "Изменение ордера отменено.";
    } catch (error) {
      errorMessage = changeError(error);
    }
    applying = false;
    undoDialog.close(); tradeToUndo = null;
    await reloadAccount();
    await loadEvents();
  }

  async function ignoreTrade(event: TradeEvent): Promise<void> {
    if (applying) return;
    applying = true;
    try { await invoke<boolean>("trade_event_ignore", { id: event.id }); await loadEvents(); }
    catch { errorMessage = "Не удалось пропустить сделку. Повторите действие."; }
    finally { applying = false; }
  }

  async function restoreTradeEvent(event: TradeEvent): Promise<void> {
    if (applying) return;
    applying = true;
    errorMessage = "";
    try {
      await invoke<boolean>("trade_event_restore", { id: event.id });
      const completed = await invoke<boolean>("trade_event_retry", { id: event.id });
      actionMessage = completed
        ? "Продажа учтена в Warframe Market."
        : "Ордер всё ещё нельзя определить однозначно.";
    } catch (error) {
      errorMessage = changeError(error);
    } finally {
      applying = false;
      await reloadAccount();
      await loadEvents();
    }
  }

  async function reloadAccount(): Promise<void> {
    const requestedRevision = ++dataRevision;
    try {
      const latest = await read<AccountView>("account_status");
      if (disposed || requestedRevision !== dataRevision) return;
      account = latest;
      lastUpdated = Date.now();
      await loadSavedPrices();
    } catch (error) {
      errorMessage = changeError(error);
    }
  }

  async function connectAccount(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    ++dataRevision;
    stopLiveRefresh = true;
    suggestionsInitialized = false;
    failedPriceChecks = new Set();
    liveProgress = "";
    accountBusy = true;
    errorMessage = "";
    actionMessage = "Подключаем Warframe Market…";
    try {
      account = await invoke<AccountView>("account_connect", { email, password });
      email = "";
      password = "";
      recommendations = new Map();
      liveCheckedAt = new Map();
      await loadSavedPrices();
      actionMessage = "Аккаунт Warframe Market подключён.";
    } catch {
      password = "";
      actionMessage = "";
      errorMessage = "Не удалось подключить Warframe Market. Проверьте email и пароль Warframe Market.";
    } finally {
      accountBusy = false;
    }
  }

  async function disconnectAccount(): Promise<void> {
    ++dataRevision;
    stopLiveRefresh = true;
    suggestionsInitialized = false;
    failedPriceChecks = new Set();
    liveProgress = "";
    accountBusy = true;
    errorMessage = "";
    actionMessage = "Отключаем аккаунт Warframe Market…";
    try {
      const remotelyRevoked = await invoke<boolean>("account_disconnect");
      account = { connected: false, profile: null, orders: [], orderItems: {} };
      recommendations = new Map();
      liveCheckedAt = new Map();
      selectedIds = new Set();
      accountPanelOpen = false;
      actionMessage = remotelyRevoked
        ? "Аккаунт Warframe Market отключён."
        : "Данные входа удалены с этого компьютера; WFM не подтвердил завершение сессии.";
    } catch {
      actionMessage = "";
      errorMessage = "Не удалось отключить аккаунт Warframe Market. Повторите попытку.";
    } finally {
      accountBusy = false;
    }
  }

  function manualOrderName(order: AccountOrder): string {
    return rows.find((row) => row.order.id === order.id)?.item?.displayName ?? "Ордер";
  }

  function eventTitle(event: TradeEvent): string {
    if (isSaleTrade(event)) return `Продажа · +${event.platinumReceived} пл.`;
    if (event.platinumGiven > 0 && event.platinumReceived === 0) return `Покупка · −${event.platinumGiven} пл.`;
    return "Обмен предметами";
  }

  function soldItems(event: TradeEvent): string {
    const matched = matchedSaleItems(event);
    if (matched) return matched;
    return event.givenItems.map((item) => `${localizedTradeName(item.name)} ×${item.quantity}`).join(", ") || "без предметов";
  }

  function matchedSaleItems(event: TradeEvent): string | null {
    let actions: TradeReconciliationAction[] = [];
    if (event.status === "pending" && account) {
      const plan = planTradeReconciliation(event, account);
      if (plan.unmatched.length === 0 && plan.unsafe.length === 0) actions = plan.actions;
    } else if (event.reconciliationJson) {
      try {
        actions = JSON.parse(event.reconciliationJson) as TradeReconciliationAction[];
      } catch {
        return null;
      }
    }
    return actions.length > 0
      ? actions.map((action) => `${action.itemName} ×${action.soldQuantity}`).join(", ")
      : null;
  }

  function wasClosedOnMarket(event: TradeEvent): boolean {
    if (!event.reconciliationJson) return false;
    try {
      const actions = JSON.parse(event.reconciliationJson) as TradeReconciliationAction[];
      return actions.some((action) => action.kind === "close");
    } catch {
      return false;
    }
  }

  function receivedItems(event: TradeEvent): string {
    return event.receivedItems.map((item) => `${localizedTradeName(item.name)} ×${item.quantity}`).join(", ") || "без предметов";
  }

  function eventItems(event: TradeEvent): string {
    if (isSaleTrade(event)) return soldItems(event);
    if (event.platinumGiven > 0 && event.platinumReceived === 0) return receivedItems(event);
    return `Отдано: ${soldItems(event)} · Получено: ${receivedItems(event)}`;
  }

  function localizedTradeName(name: string): string {
    const normalized = normalizeTradeName(name);
    for (const item of Object.values(account?.orderItems ?? {})) {
      if ([item.displayName, item.displayNameEn]
        .some((candidate) => normalizeTradeName(candidate) === normalized)) {
        return item.displayName;
      }
      const component = item.setComponents?.find((candidate) =>
        [candidate.displayName, candidate.displayNameEn]
          .some((candidateName) => normalizeTradeName(candidateName) === normalized)
      );
      if (component) return component.displayName;
    }
    return name;
  }

  function eventCanApply(event: TradeEvent): boolean {
    if (!account) return false;
    const plan = planTradeReconciliation(event, account);
    return plan.actions.length > 0 && plan.unmatched.length === 0 && plan.unsafe.length === 0;
  }

  function eventMatchStatus(event: TradeEvent): string {
    if (!account) return "Проверяем объявления";
    const plan = planTradeReconciliation(event, account);
    if (plan.actions.length > 0 && plan.unmatched.length === 0 && plan.unsafe.length === 0) {
      return "Подходящее объявление найдено";
    }
    if (plan.unsafe.length > 0) {
      return "Нужно проверить вариант или количество предметов";
    }
    return "Подходящего объявления нет в текущем списке";
  }

  function summarize(source: TradeShiftRow[]) {
    return {
      total: source.length,
      visible: source.filter((row) => row.order.visible).length,
    };
  }

  function saleCountLabel(count: number): string {
    const mod100 = count % 100;
    const mod10 = count % 10;
    const noun = mod100 >= 11 && mod100 <= 14
      ? "сделок"
      : mod10 === 1
        ? "сделка"
        : mod10 >= 2 && mod10 <= 4
          ? "сделки"
          : "сделок";
    return `${count} ${noun}`;
  }
</script>

<section class="sales-workspace" aria-labelledby="sales-heading">
  <header class="sales-header">
    <div><h2 id="sales-heading">Управляйте продажами</h2><p>Проверьте цену и остаток, затем обновите нужные объявления.</p></div>
    {#if account?.connected}<div class="sales-header__actions"><button class="secondary account-button" aria-expanded={accountPanelOpen} onclick={() => accountPanelOpen = !accountPanelOpen}><span class="connection-dot"></span><span translate="no">{account.profile?.ingameName ?? "Warframe Market"}</span></button><button onclick={onOpenInventory}>Выставить предмет</button></div>{/if}
  </header>
  {#if errorMessage}<div class="inline-error" role="alert"><span>{errorMessage}</span>{#if !applying}<button class="secondary" onclick={() => loadAll()} disabled={loading}>Повторить загрузку</button>{/if}</div>{/if}
  {#if actionMessage && !account?.connected}<p class="data-note" role="status">{actionMessage}</p>{/if}
  {#if dataMessage}<p class="data-note" role="status">{dataMessage}</p>{/if}
  {#if accountPanelOpen && account?.connected}<section class="account-panel" aria-label="Подключение Warframe Market"><div><strong>Warframe Market · <span translate="no">{account.profile?.ingameName}</span></strong><p>{account.profile?.platform.toUpperCase() ?? "PC"} · {account.profile?.verification ? "Аккаунт подтверждён" : "Нужно подтверждение аккаунта"}</p></div><button class="secondary" disabled={accountBusy || applying} onclick={disconnectAccount}>Отключить аккаунт</button></section>{/if}
  {#if loading && !account}
    <section class="sales-empty" role="status"><h3>Загружаем ваши объявления…</h3><p>Сверяем остатки и последние сделки.</p><div class="skeleton"></div><div class="skeleton"></div></section>
  {:else if !account && errorMessage}
    <section class="sales-empty"><h3>Объявления пока недоступны</h3><p>Повторите загрузку или воспользуйтесь поиском предметов.</p><button class="secondary" onclick={onBrowseMarket}>Найти предмет</button></section>
  {:else if !account?.connected}
    <section class="welcome"><div><p class="eyebrow">Ваши продажи в одном месте</p><h3>Подключите Warframe Market</h3><p>Здесь появятся ваши объявления, подсказки по ценам и количество доступных копий.</p><ul><li>Сравнивайте свою цену с предложениями рынка.</li><li>Меняйте цену, количество и показ объявления.</li><li>Следите за сделками, подтверждёнными игрой.</li></ul><button class="text-button" onclick={onBrowseMarket}>Посмотреть цены без подключения →</button></div>
      <form class="connect-panel" onsubmit={connectAccount}><label for="market-wfm-email">Email Warframe Market<input id="market-wfm-email" type="email" name="username" autocomplete="username" spellcheck="false" bind:value={email} required maxlength="128" placeholder="name@example.com" /></label><label for="market-wfm-password">Пароль Warframe Market<input id="market-wfm-password" type="password" name="password" autocomplete="current-password" bind:value={password} required maxlength="128" /></label><button type="submit" disabled={accountBusy}>{accountBusy ? "Подключаем…" : "Подключить аккаунт"}</button><details class="security-details"><summary>Как хранятся данные входа</summary><p>Пароль не сохраняется. Ключ сессии хранится в защищённом хранилище Windows.</p></details></form>
    </section>
  {:else}
    {#if !account.profile?.verification}<p class="data-note">Аккаунт Warframe Market не подтверждён. Объявления доступны для просмотра; после подтверждения обновите список.</p>{/if}
    {#if pendingEvents.length}<details class="priority-panel" open><summary><strong>Нужно проверить продажи из игры</strong><span>{pendingEvents.length}</span></summary><p class="section-hint">Эти сделки ещё не учтены на Warframe Market: не найдено подходящее объявление или произошла ошибка.</p><div class="trade-events">{#each pendingEvents as event (event.id)}<article class="pending"><div class="trade-event__copy"><strong>{eventTitle(event)}</strong><span>{soldItems(event)}{event.partner ? " · " + event.partner : ""}</span><small>{new Date(event.occurredAt).toLocaleString("ru-RU")}</small></div><div class="trade-event__actions"><span class="manual">{eventMatchStatus(event)}</span><button class="secondary" disabled={applying || !account.profile?.verification} onclick={() => retryTrade(event)}>{eventCanApply(event) ? "Учесть продажу" : "Найти объявление снова"}</button><button class="text-button" disabled={applying} onclick={() => ignoreTrade(event)}>Пропустить</button></div></article>{/each}</div></details>{/if}
    <section class="orders-panel" aria-labelledby="orders-heading">
      <header class="orders-heading"><div><h3 id="orders-heading">Объявления на продажу</h3><p>{lastUpdated ? "Список проверен в " + new Date(lastUpdated).toLocaleTimeString("ru-RU", {hour:"2-digit", minute:"2-digit"}) : "Получаем список"} · обновляется автоматически</p></div><div class="sales-header__actions"><button class="text-button" disabled={loading || applying || refreshingLive} onclick={() => loadAll()}>{loading ? "Обновляем…" : "Обновить список"}</button>{#if refreshingLive}<button class="secondary" onclick={() => stopLiveRefresh = true}>Остановить проверку</button>{:else if rows.length}<button class="secondary" disabled={loading || applying || !!editingOrder || reviewOpen} onclick={refreshCurrentPrices}>Проверить цены</button>{/if}</div></header>
      {#if rows.length}<div class="orders-toolbar"><div class="order-filters" role="group" aria-label="Отбор объявлений"><button aria-pressed={orderFilter === "all"} onclick={() => orderFilter = "all"}>Все <b>{rows.length}</b></button><button aria-pressed={orderFilter === "attention"} onclick={() => orderFilter = "attention"}>Требуют внимания <b>{rows.filter(row => row.needsAction).length}</b></button><button aria-pressed={orderFilter === "hidden"} onclick={() => orderFilter = "hidden"}>Скрытые <b>{rows.filter(row => !row.order.visible).length}</b></button></div><label class="order-search"><span class="sr-only">Поиск объявления</span><input type="search" bind:value={orderQuery} maxlength="80" autocomplete="off" spellcheck="false" placeholder="Название предмета на русском или английском" /></label></div>{/if}
      {#if liveProgress}<p class="status-line progress-line" role="status">{liveProgress}</p>{/if}{#if actionMessage}<p class="status-line action-line" role="status">{actionMessage}</p>{/if}
      {#if visibleRows.length}
        <div class="shift-table-wrap"><table class="shift-table"><caption class="sr-only">Ваши объявления, цены и доступные копии</caption><thead><tr>{#if actionableRows.length}<th class="check-column"><span class="sr-only">Выбрать изменение</span></th>{/if}<th>Предмет</th><th>Ваша цена</th><th>Ориентир рынка</th><th>Количество</th><th>Действие</th></tr></thead><tbody>
          {#each visibleRows as row (row.order.id)}<tr class:row-attention={row.needsAction}>
            {#if actionableRows.length}<td class="check-column">{#if rowChange(row)}<input type="checkbox" aria-label={"Выбрать изменение: " + (row.item?.displayName ?? "предмет")} checked={selectedIds.has(row.order.id)} onchange={() => toggleSelected(row.order.id)} disabled={applying || reviewOpen || !account.profile?.verification} />{/if}</td>{/if}
            <th scope="row" class="item-column"><div class="item-cell">{#if row.item?.imageUrl}<img src={row.item.imageUrl} alt="" loading="lazy" />{/if}<span><strong>{row.item?.displayName ?? "Неизвестный предмет"}</strong><small>{row.key ? (variantLabel(row.key) === "базовый вариант" ? "" : variantLabel(row.key)) : "Вариант не определён"}</small><span class="visibility-label" class:shown={row.order.visible}>{row.order.visible ? "Видно покупателям" : "Скрыто от покупателей"}</span></span></div></th>
            <td data-label="Ваша цена"><strong class="price">{money(row.order.platinum)}</strong><small>{(row.order.perTrade ?? 1) > 1 ? "за " + row.order.perTrade + " шт." : "за штуку"}</small></td>
            <td data-label="Ориентир рынка"><strong class="market-price">{money(orderMarketPrice(row))}</strong><small>{row.priceCheckFailed ? "Не удалось проверить" : row.recommendation ? ((row.order.perTrade ?? 1) > 1 ? "за " + row.order.perTrade + " шт." : "за штуку") : "Проверьте цены"}</small></td>
            <td data-label="Количество"><strong>{row.order.quantity} <small class="inline-small">в объявлении</small></strong>{#if inventory}<small class:mismatch={row.health === "inventory_mismatch"}>Доступно: {row.inventory?.sellableQuantity ?? 0}</small>{:else}<small>Остаток неизвестен</small>{/if}</td>
            <td class="advice-column"><span class={"health health--" + row.health}>{!inventory && row.health === "healthy" ? "Цена в пределах оценки" : orderAdvice(row)}</span>{#if row.suggestedQuantity !== null}<small>{row.suggestedQuantity === 0 ? "Нет свободных копий" : "Доступно для продажи: " + row.suggestedQuantity + " шт."}</small>{/if}<button class="secondary" disabled={applying || !account.profile?.verification} onclick={() => beginManualEdit(row)} aria-label={"Изменить объявление: " + (row.item?.displayName ?? "предмет")}>Изменить</button></td>
          </tr>{/each}
        </tbody></table></div>
      {:else if rows.length}<div class="sales-empty"><h3>{orderFilter === "attention" && !orderQuery ? "Объявления не требуют изменений" : orderFilter === "hidden" && !orderQuery ? "Скрытых объявлений нет" : "Ничего не найдено"}</h3><p>{orderFilter === "attention" && !orderQuery ? "По имеющимся данным цена и количество в порядке." : "Измените название предмета или выберите другой отбор."}</p><button class="secondary" onclick={() => { orderQuery = ""; orderFilter = "all"; }}>Показать все объявления</button></div>
      {:else}<div class="sales-empty"><h3>Пока нет объявлений на продажу</h3><p>Нажмите «Выставить предмет», чтобы выбрать его из инвентаря и опубликовать на Warframe Market.</p></div>{/if}
      {#if actionableRows.length}<footer class="batch-bar"><div><strong>Выбрано изменений: {selectedRows.length}</strong><small>Перед отправкой покажем, что изменится.</small></div><div class="sales-header__actions"><button class="text-button" disabled={applying} onclick={() => selectedIds = selectedIds.size ? new Set() : new Set(actionableRows.map(row => row.order.id))}>{selectedIds.size ? "Снять выбор" : "Выбрать предложенные"}</button><button disabled={!selectedRows.length || applying || !account.profile?.verification || refreshingLive} onclick={openBatch}>Посмотреть изменения</button></div></footer>{/if}
    </section>
    <div class="secondary-sections">
      {#if rows.length}<details class="management-panel"><summary><strong>Показ объявлений</strong><span>Видно {summary.visible} из {summary.total}</span></summary><div class="management-panel__body"><p>Скройте объявления, если сейчас не готовы торговать. Цены и количество сохранятся.</p><div class="sales-header__actions"><button class="secondary" disabled={!summary.visible || applying || !account.profile?.verification} onclick={() => confirmVisibility(false)}>Скрыть все объявления</button><button class="secondary" disabled={summary.visible === summary.total || applying || !account.profile?.verification} onclick={() => confirmVisibility(true)}>Показать все объявления</button></div></div></details>{/if}
      <details class="trade-history"><summary><strong>История сделок</strong><span>{historyUnavailable ? "Не удалось загрузить" : historyEvents.length ? "Последние обмены из игры" : "Пока пусто"}</span></summary><div class="history-toolbar">{#if summaryUnavailable}<p>Не удалось загрузить сумму продаж.</p>{:else}<p>Получено от продаж за всё время: <strong>{money(tradeSales.platinumReceived)}</strong> · {saleCountLabel(tradeSales.saleCount)}</p>{/if}<label>Показать<select aria-label="Показать сделки" bind:value={historyFilter}><option value="all">Все сделки</option><option value="sell">Продажи</option><option value="buy">Покупки</option></select></label></div><div class="trade-events">
        {#each historyEvents as event (event.id)}<article><div class="trade-event__copy"><strong>{eventTitle(event)}</strong><span>{eventItems(event)}{event.partner ? " · " + event.partner : ""}</span><small>{new Date(event.occurredAt).toLocaleString("ru-RU")}</small></div><div class="trade-event__actions">{#if event.status === "reconciled" && event.reconciliationJson}{#if wasClosedOnMarket(event)}<span class="done">Продажа учтена</span>{:else}<span class="done">Объявление обновлено</span><button class="text-button" disabled={applying} onclick={() => confirmUndo(event)}>Отменить изменение</button>{/if}{:else if event.status === "ignored" && isSaleTrade(event)}<span>Без изменения объявления</span><button class="text-button" disabled={applying} onclick={() => restoreTradeEvent(event)}>Вернуть к проверке</button>{:else}<span class="done">Записано</span>{/if}</div></article>{:else}<p class="history-empty">{historyUnavailable ? "Не удалось загрузить историю. Нажмите «Обновить список», чтобы повторить загрузку." : "Таких сделок пока нет. История пополняется после обменов в игре при запущенном PlatScope."}</p>{/each}
      </div>{#if !historyExpanded && filteredHistory.length > 8}<button class="text-button history-more" onclick={() => historyExpanded = true}>Показать больше сделок</button>{/if}</details>
    </div>
  {/if}
</section>

<dialog class="sales-dialog" bind:this={orderDialog} oncancel={event => { if (applying) event.preventDefault(); }} onclose={() => { editingOrder = null; orderToRemove = null; editError = ""; }} aria-labelledby="order-editor-heading">
  {#if editingOrder}{@const editingRow = rows.find(row => row.order.id === editingOrder?.id)}
    <header class="dialog-heading"><div><p class="eyebrow">Warframe Market</p><h2 id="order-editor-heading">{orderToRemove ? "Снять объявление с продажи?" : "Изменить объявление"}</h2></div><button class="secondary" disabled={applying} onclick={closeEditor}>Закрыть</button></header>
    <div class="dialog-item">{#if editingRow?.item?.imageUrl}<img src={editingRow.item.imageUrl} alt="" />{/if}<div><strong>{manualOrderName(editingOrder)}</strong>{#if orderEnglishName(editingRow?.item ?? undefined)}<span translate="no">{orderEnglishName(editingRow?.item ?? undefined)}</span>{/if}<small>{editingRow?.key ? variantLabel(editingRow.key) : ""}</small></div></div>
    {#if editError || errorMessage}<p class="inline-error" role="alert">{editError || errorMessage}</p>{/if}
    {#if orderToRemove}<p class="dialog-description">Объявление исчезнет с Warframe Market. Чтобы вернуть его, потребуется новая публикация.</p><div class="confirm-actions"><button class="danger-primary" disabled={applying} onclick={removeManualOrder}>{applying ? "Снимаем…" : "Снять с продажи"}</button><button class="secondary" disabled={applying} onclick={() => orderToRemove = null}>Вернуться к редактированию</button></div>
    {:else}<form class="order-editor" onsubmit={reviewManualEdit}>
      <dl class="edit-context"><div><dt>Ориентир рынка</dt><dd>{editingRow ? money(orderMarketPrice(editingRow)) : "Нет оценки"}</dd></div><div><dt>Доступно для продажи</dt><dd>{inventory ? (editingRow?.inventory?.sellableQuantity ?? 0) + " шт." : "Остаток неизвестен"}</dd></div></dl>
      <div class="order-editor__fields"><label>Цена, платина{#if (editingOrder.perTrade ?? 1) > 1}<small>за {editingOrder.perTrade} шт.</small>{/if}<input type="number" inputmode="numeric" bind:value={editPlatinum} min="1" max="900000" step="1" required /></label><label>Количество, шт.<input type="number" inputmode="numeric" bind:value={editQuantity} min="1" max="9999" step="1" required /></label></div>
      {#if editingRow && rowChange(editingRow) && !rowChange(editingRow)?.delete}<button class="text-button use-suggestion" type="button" onclick={() => { if (editingRow.suggestedPrice !== null) editPlatinum = editingRow.suggestedPrice; if (editingRow.suggestedQuantity !== null) editQuantity = editingRow.suggestedQuantity; }}>Подставить предложенные цену и количество</button>{/if}
      <label class="compact-check"><input type="checkbox" bind:checked={editVisible} /> Показывать покупателям</label>
      <section class="edit-preview" aria-label="Изменения объявления"><strong>Будет сохранено</strong><dl><div><dt>Цена{(editingOrder.perTrade ?? 1) > 1 ? " за партию" : " за штуку"}</dt><dd>{money(editingOrder.platinum)} → {money(editPlatinum ?? null)}</dd></div><div><dt>Количество</dt><dd>{editingOrder.quantity} → {editQuantity ?? "—"} шт.</dd></div><div><dt>Показ покупателям</dt><dd>{editVisible ? "Включён" : "Выключен"}</dd></div></dl></section>
      <div class="confirm-actions"><button type="submit" disabled={applying || (editPlatinum === editingOrder.platinum && editQuantity === editingOrder.quantity && editVisible === editingOrder.visible)}>{applying ? "Сохраняем…" : "Сохранить изменения"}</button><button class="text-button danger" type="button" disabled={applying} onclick={() => orderToRemove = editingOrder}>Снять с продажи</button></div>
    </form>{/if}
  {/if}
</dialog>
<dialog class="sales-dialog" bind:this={batchDialog} onclose={() => reviewOpen = false} oncancel={event => { if (applying) event.preventDefault(); }} aria-labelledby="batch-heading">
  <header class="dialog-heading"><div><p class="eyebrow">Проверка перед отправкой</p><h2 id="batch-heading">Изменить объявления · {reviewed.length}</h2></div><button class="secondary" disabled={applying} onclick={closeBatchReview}>Закрыть</button></header><p class="dialog-description">На Warframe Market будут отправлены только показанные ниже изменения.</p>
  {#if errorMessage}<p class="inline-error" role="alert">{errorMessage}</p>{/if}
  <div class="reviewed-list">{#each reviewed as proposal (proposal.before.id)}<article><strong>{proposal.name}</strong>{#if proposal.change.delete}<p class="danger">Снять с продажи — свободных копий нет.</p>{:else}<dl>{#if proposal.change.price !== null}<div><dt>Цена{(proposal.before.perTrade ?? 1) > 1 ? " за " + proposal.before.perTrade + " шт." : " за штуку"}</dt><dd>{money(proposal.before.platinum)} → <b>{money(proposal.change.price)}</b></dd></div>{/if}{#if proposal.change.quantity !== null}<div><dt>Количество</dt><dd>{proposal.before.quantity} → <b>{proposal.change.quantity} шт.</b></dd></div>{/if}</dl>{/if}</article>{/each}</div>
  {#if applyProgress}<p class="status-line" role="status">{applyProgress}</p>{/if}<div class="confirm-actions"><button disabled={applying || !reviewed.length} onclick={applySelectedChanges}>{applying ? "Отправляем…" : "Применить изменения"}</button><button class="secondary" disabled={applying} onclick={closeBatchReview}>Отмена</button></div>
</dialog>
<dialog class="sales-dialog" bind:this={visibilityDialog} onclose={() => visibilityIntent = null} oncancel={event => { if (applying) event.preventDefault(); }} aria-labelledby="visibility-heading">
  <h2 id="visibility-heading">{visibilityIntent ? "Показать" : "Скрыть"} объявления · {visibilityTargets.length}</h2><p class="dialog-description">{visibilityIntent ? "Выбранные объявления станут видны покупателям на Warframe Market." : "Покупатели перестанут видеть эти объявления. Цены и количество сохранятся."}</p><ul class="visibility-items">{#each visibilityTargets as order}<li>{manualOrderName(order)}</li>{/each}</ul>
  {#if errorMessage}<p class="inline-error" role="alert">{errorMessage}</p>{/if}{#if applyProgress}<p role="status">{applyProgress}</p>{/if}<div class="confirm-actions"><button disabled={applying} onclick={applyVisibility}>{applying ? "Отправляем…" : visibilityIntent ? "Показать объявления" : "Скрыть объявления"}</button><button class="secondary" disabled={applying} onclick={() => visibilityDialog.close()}>Отмена</button></div>
</dialog>
<dialog class="sales-dialog" bind:this={undoDialog} onclose={() => tradeToUndo = null} oncancel={event => { if (applying) event.preventDefault(); }} aria-labelledby="trade-undo-heading">
  <h2 id="trade-undo-heading">Вернуть объявление к состоянию до сделки?</h2><p class="dialog-description">Используйте отмену, если предмет или количество были сопоставлены неправильно.</p>{#if tradeToUndo}<p>{soldItems(tradeToUndo)}</p>{/if}{#if errorMessage}<p class="inline-error" role="alert">{errorMessage}</p>{/if}<div class="confirm-actions"><button disabled={applying} onclick={() => tradeToUndo && undoTrade(tradeToUndo)}>Вернуть объявление</button><button class="secondary" disabled={applying} onclick={() => undoDialog.close()}>Отмена</button></div>
</dialog>

<style>
  .sales-workspace { display:grid; gap:1rem; container:sales / inline-size; }
  h2,h3,p,dd,dl { margin:0; } p { font-size:.875rem; line-height:1.5; color:var(--text-muted); }
  button { min-height:2.4rem; } h2 { font-size:1.2rem; } h3 { font-size:1rem; }
  .sales-header,.sales-header__actions,.account-panel,.orders-heading,.orders-toolbar,.batch-bar,.confirm-actions,.dialog-heading { display:flex; align-items:center; justify-content:space-between; gap:.75rem; }
  .sales-header { margin:.1rem 0 .6rem; align-items:start; } .sales-header p { margin-top:.35rem; }
  .sales-header__actions,.confirm-actions { justify-content:flex-start; flex-wrap:wrap; }
  .account-button { display:flex; align-items:center; gap:.45rem; } .connection-dot { width:.4rem; height:.4rem; background:var(--success); border-radius:50%; }
  .account-panel { padding:1rem; border:1px solid var(--border); border-radius:.75rem; background:var(--surface-1); }
  .account-panel strong { font-size:.875rem; } .account-panel p { margin-top:.25rem; }
  .inline-error,.data-note { margin:0; border:1px solid var(--border); border-radius:.6rem; padding:.85rem 1rem; font-size:.8125rem; line-height:1.5; }
  .inline-error { display:flex; align-items:center; justify-content:space-between; gap:1rem; background:var(--danger-soft); color:var(--danger); border-color:var(--danger); }
  .data-note { background:var(--surface-2); }
  .sales-empty { padding:2rem; border:1px solid var(--border); border-radius:.7rem; background:var(--surface-1); }
  .sales-empty p { margin:.6rem 0 1rem; max-width:42rem; } .orders-panel .sales-empty { border:0; }
  .skeleton { height:3rem; border-radius:.4rem; background:var(--surface-2); margin-top:.75rem; }
  .welcome { display:grid; grid-template-columns:minmax(0,1fr) minmax(18rem,.75fr); gap:3rem; padding:2rem; border:1px solid var(--border); border-radius:1rem; background:var(--surface-1); }
  .welcome h3 { font-size:1.45rem; margin:.6rem 0; } .welcome ul { padding-left:1.2rem; font-size:.875rem; line-height:1.9; color:var(--text-muted); }
  .eyebrow { font-size:.75rem; letter-spacing:.08em; text-transform:uppercase; color:var(--text-subtle); font-weight:700; }
  .connect-panel { display:grid; align-content:start; gap:1rem; padding:1.25rem; border-radius:.75rem; background:var(--surface-2); }
  .connect-panel label,.order-editor__fields label { display:grid; gap:.45rem; font-size:.8125rem; font-weight:600; }
  input:not([type=checkbox]),select { min-width:0; width:100%; min-height:2.5rem; border:1px solid var(--border-strong); border-radius:.5rem; padding:.5rem .7rem; background:var(--surface-1); color:var(--text); font:inherit; font-size:.875rem; }
  input::placeholder { color:var(--text-subtle); font-weight:400; } input[type=checkbox] { accent-color:var(--accent); width:1rem; height:1rem; flex:none; }
  .security-details { font-size:.75rem; color:var(--text-muted); } .security-details summary { cursor:pointer; } .security-details p { margin-top:.5rem; font-size:.8125rem; }
  .orders-panel { min-width:0; overflow:hidden; border:1px solid var(--border); border-radius:.85rem; background:var(--surface-1); box-shadow:var(--shadow-sm); }
  .orders-heading { padding:1.15rem 1.15rem .85rem; flex-wrap:wrap; } .orders-heading p { margin-top:.3rem; font-size:.75rem; }
  .orders-toolbar { padding:.25rem 1.15rem 1rem; flex-wrap:wrap; gap:1rem; }
  .order-filters { display:flex; gap:.3rem; padding:.25rem; background:var(--surface-2); border:1px solid var(--border); border-radius:.55rem; flex-wrap:wrap; }
  .order-filters button { border:1px solid transparent; background:transparent; color:var(--text-muted); font-size:.8125rem; padding:.35rem .65rem; min-height:2.25rem; }
  .order-filters button[aria-pressed=true] { background:var(--surface-1); border-color:var(--border-strong); color:var(--text); box-shadow:var(--shadow-sm); }
  .order-filters b { font-weight:600; margin-left:.35rem; color:var(--accent); }
  .order-search { display:block; flex:1; min-width:15rem; max-width:31rem; }
  .shift-table-wrap { width:100%; overflow-x:auto; }
  .shift-table { width:100%; table-layout:auto; border-collapse:collapse; font-size:.875rem; }
  .shift-table th,.shift-table td { border-bottom:1px solid var(--border); padding:1rem .9rem; vertical-align:middle; text-align:left; }
  .shift-table thead th { background:var(--surface-2); font-size:.75rem; color:var(--text-muted); font-weight:650; text-transform:none; letter-spacing:0; }
  .shift-table tbody tr:last-child th,.shift-table tbody tr:last-child td { border-bottom:0; } .shift-table tbody tr:hover { background:var(--surface-2); }
  .shift-table .check-column { width:2.5rem; padding-right:0; }
  .shift-table .item-column { width:34%; text-transform:none; letter-spacing:0; font-weight:400; }
  .item-cell { display:flex; gap:.8rem; align-items:center; min-width:0; }
  .item-cell img { width:2.75rem; height:3.25rem; object-fit:contain; flex:none; }
  .item-cell strong { display:block; font-size:.9375rem; line-height:1.35; overflow-wrap:anywhere; }
  .item-cell small { display:block; font-size:.75rem; color:var(--text-muted); margin-top:.3rem; }
  .visibility-label { display:block; margin-top:.4rem; font-size:.75rem; color:var(--text-muted); } .visibility-label.shown { color:var(--success); }
  .shift-table td > strong { display:block; font-weight:650; font-variant-numeric:tabular-nums; }
  .shift-table td > small { display:block; margin-top:.35rem; font-size:.75rem; color:var(--text-muted); }
  .price { font-size:1.05rem; color:var(--text); white-space:nowrap; } .market-price { font-size:.9375rem; white-space:nowrap; }
  .inline-small { font-size:.75rem; font-weight:400; } .shift-table td > small.mismatch { color:var(--danger); }
  .advice-column { width:17%; min-width:10rem; } .advice-column button { display:block; margin-top:.65rem; min-height:2.15rem; }
  .health { display:block; font-size:.8125rem; font-weight:600; line-height:1.35; }
  .health--healthy { color:var(--success); font-weight:400; } .health--underpriced,.health--overpriced { color:var(--accent); }
  .health--inventory_mismatch,.health--price_check_failed { color:var(--danger); } .health--unknown,.health--hidden { color:var(--text-muted); font-weight:400; }
  .batch-bar { border-top:1px solid var(--border); background:var(--surface-2); padding:1rem 1.15rem; border-radius:0 0 .85rem .85rem; flex-wrap:wrap; }
  .batch-bar strong { display:block; font-size:.875rem; }.batch-bar small { display:block; margin-top:.25rem; color:var(--text-muted); font-size:.75rem; }
  .status-line { font-size:.8125rem; line-height:1.4; color:var(--text-muted); }
  .progress-line,.action-line { margin:0 1.15rem .9rem; padding:.7rem .85rem; border-radius:.5rem; background:var(--surface-2); } .action-line { color:var(--success); }
  .secondary-sections { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:1rem; align-items:start; }
  .secondary-sections > :only-child { grid-column:1 / -1; }
  .management-panel,.trade-history,.priority-panel { border:1px solid var(--border); border-radius:.7rem; background:var(--surface-1); min-width:0; }
  .management-panel summary,.trade-history summary,.priority-panel summary { padding:1rem; cursor:pointer; font-size:.875rem; }
  summary span { margin-left:.5rem; font-size:.75rem; color:var(--text-muted); }
  .management-panel__body,.history-toolbar { border-top:1px solid var(--border); padding:1rem; } .management-panel__body p { margin-bottom:1rem; }
  .history-toolbar label { display:flex; align-items:center; gap:.6rem; font-size:.8125rem; margin-top:.75rem; } .history-toolbar select { max-width:12rem; }
  .trade-events article { display:flex; justify-content:space-between; gap:1rem; border-top:1px solid var(--border); padding:1rem; }
  .trade-event__copy { min-width:0; } .trade-event__copy strong { display:block; font-size:.875rem; }
  .trade-event__copy span { display:block; margin-top:.3rem; font-size:.8125rem; line-height:1.4; overflow-wrap:anywhere; }
  .trade-event__copy small { display:block; margin-top:.3rem; font-size:.75rem; color:var(--text-muted); }
  .trade-event__actions { display:flex; align-items:flex-start; justify-content:flex-end; flex-wrap:wrap; gap:.5rem; max-width:22rem; font-size:.75rem; }
  .trade-event__actions .manual { flex-basis:100%; color:var(--danger); } .done { color:var(--success); } .section-hint { padding:0 1rem 1rem; font-size:.8125rem; }
  .history-empty { padding:1rem; font-size:.8125rem; } .history-more { margin:.3rem 1rem 1rem; }
  .sales-dialog { width:min(36rem,calc(100vw - 2rem)); max-height:calc(100dvh - 2rem); padding:1.5rem; border:1px solid var(--border); border-radius:1rem; background:var(--surface-1); color:var(--text); box-shadow:var(--shadow-md); overscroll-behavior:contain; }
  .sales-dialog::backdrop { background:rgb(0 0 0 / .45); } .sales-dialog h2 { font-size:1.35rem; line-height:1.3; }
  .dialog-heading { align-items:start; margin-bottom:1rem; } .dialog-heading button { flex:none; } .dialog-heading .eyebrow { margin-bottom:.35rem; }
  .dialog-item { display:flex; align-items:center; gap:.8rem; padding:1rem 0; border-block:1px solid var(--border); }
  .dialog-item img { width:3rem; height:3.5rem; object-fit:contain; } .dialog-item strong { display:block; font-size:1rem; line-height:1.4; }
  .dialog-item span,.dialog-item small { display:block; font-size:.75rem; margin-top:.3rem; color:var(--text-muted); }
  .order-editor { display:grid; gap:1rem; padding-top:1rem; } .edit-context { display:grid; grid-template-columns:1fr 1fr; gap:.8rem; }
  .edit-context dt { color:var(--text-muted); font-size:.75rem; margin-bottom:.3rem; }.edit-context dd { font-size:.9375rem; font-weight:600; }
  .order-editor__fields { display:grid; grid-template-columns:1fr 1fr; gap:1rem; align-items:end; } .order-editor__fields label small { font-size:.75rem; font-weight:400; }
  .compact-check { display:flex; align-items:center; gap:.5rem; font-size:.875rem; }
  .use-suggestion { justify-self:start; padding:0; min-height:1.7rem; border:0; color:var(--accent); text-align:left; }
  .edit-preview { background:var(--surface-2); border-radius:.65rem; padding:1rem; border:1px solid var(--border); } .edit-preview > strong { font-size:.8125rem; }
  .edit-preview dl div,.reviewed-list dl div { display:flex; gap:1rem; justify-content:space-between; font-size:.8125rem; margin-top:.5rem; }
  .edit-preview dt,.reviewed-list dt { color:var(--text-muted); } .edit-preview dd,.reviewed-list dd { text-align:right; }
  .confirm-actions { margin-top:1rem; } .order-editor .confirm-actions { margin-top:0; } .dialog-description { margin:1rem 0; } .sales-dialog .inline-error { margin:1rem 0; }
  .danger { color:var(--danger); } .reviewed-list article { border-bottom:1px solid var(--border); padding:1rem 0; } .reviewed-list article > strong { font-size:.875rem; } .reviewed-list p { font-size:.8125rem; margin-top:.4rem; }
  .visibility-items { padding-left:1.2rem; font-size:.875rem; line-height:1.8; }
  @container sales (max-width:65rem) { .sales-header { flex-wrap:wrap; } .secondary-sections { grid-template-columns:1fr; } .welcome { grid-template-columns:1fr; gap:1rem; } .order-search { max-width:none; } .shift-table .item-column { width:30%; } .shift-table th,.shift-table td { padding:.85rem .6rem; } }
  @container sales (max-width:48rem) {
    .shift-table,.shift-table tbody { display:block; }.shift-table thead { display:none; }
    .shift-table tbody tr { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:.75rem 1rem; padding:1rem; border-bottom:1px solid var(--border); }
    .shift-table tbody th,.shift-table tbody td { display:block; width:auto; border:0; padding:0; }
    .shift-table .item-column { grid-column:1 / -1; grid-row:1; width:auto; padding-right:2rem; }
    .shift-table .check-column { grid-column:2; grid-row:1; justify-self:end; width:auto; z-index:1; }
    .shift-table td[data-label]::before { content:attr(data-label); display:block; font-size:.75rem; color:var(--text-muted); margin-bottom:.3rem; }
    .shift-table .advice-column { min-width:0; width:auto; } .advice-column button { margin-top:.4rem; }
    .orders-toolbar { align-items:stretch; } .order-filters { width:100%; } .sales-header__actions { flex-wrap:wrap; } .trade-events article { flex-direction:column; } .trade-event__actions { justify-content:flex-start; }
  }
</style>
