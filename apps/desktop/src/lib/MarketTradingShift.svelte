<script lang="ts">
  import MarketOrderTable from "./MarketOrderTable.svelte";
  import MarketTradeHistory from "./MarketTradeHistory.svelte";
  import { marketAnalyticsKey, summarizeMarketAnalytics, type MarketAnalyticsBatch, type MarketAnalyticsSummary } from "./marketAnalytics";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import { orderChange, orderMarketPrice, orderUnchanged, reviewedChanges, reviewedQuantitiesMatch, salesFilterRows, type SalesFilter, type ReviewedOrderChange } from "./marketSales";
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
  import { type LivePricingResult, type PriceRecommendation } from "./market";
  import {
    applyPriceCheckFailures,
    buildTradeShiftRows,
    filterTradeShiftRows,
    normalizeTradeName,
    pendingSaleEvents,
    planTradeReconciliation,
    recommendationIdentity,
    updateInput,
    type TradeEvent,
    type TradeSalesSummary,
    type TradeReconciliationAction,
    type TradeShiftRow,
  } from "./tradeShift";

  export let onOpenInventory: () => void;
  export let onBrowseMarket: () => void;
  export let view: "orders" | "history" = "orders";
  export let onHistory: () => void = () => {};
  let orderType: "sell" | "buy" = "sell";
  let analytics = new Map<string, MarketAnalyticsSummary>();
  let analyticsLoading = false;
  let analyticsError = "";
  let analyticsRevision = 0;
  let visibilityScope: "all" | "selected" = "selected";

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
  let editPerTrade: number | null = null;
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
  let refreshPromise: Promise<void> | null = null;
  let orderDialog: HTMLDialogElement;
  let batchDialog: HTMLDialogElement;
  let visibilityDialog: HTMLDialogElement;
  let undoDialog: HTMLDialogElement;
  let visibilityTargets: AccountOrder[] = [];
  let visibilityExpectedOrders: AccountOrder[] = [];
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
  async function confirmVisibility(visible: boolean, scope: "all" | "selected" = "selected"): Promise<void> {
    errorMessage = "";
    visibilityIntent = visible;
    visibilityScope = scope;
    visibilityExpectedOrders = rows.map(row => ({...row.order}));
    visibilityTargets = rows.filter(row => row.order.visible !== visible && (scope === "all" || selectedIds.has(row.order.id))).map(row => ({ ...row.order }));
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
    if (message.includes("Объявления изменились") || message.includes("Объявление уже изменилось")) return "Объявления изменились. Закройте окно и проверьте новые данные перед сохранением.";
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
        buildTradeShiftRows(account, inventory, recommendations, new Date(), orderType),
        failedPriceChecks,
      )
    : [];
  $: visibleRows = salesFilterRows(filterTradeShiftRows(rows, orderQuery), orderFilter);
  $: actionableRows = rows.filter((row) => row.needsAction && rowChange(row) !== null);
  $: selectedOrders = rows.filter(row => selectedIds.has(row.order.id));
  $: selectedRows = actionableRows.filter((row) => selectedIds.has(row.order.id));
  $: pendingEvents = pendingSaleEvents(events);
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
        read<TradeEvent[]>("market_trade_events"), read<TradeSalesSummary>("trade_sales_summary"),
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
      await Promise.all([loadSavedPrices(), loadAnalytics()]);
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
        invoke<TradeEvent[]>("market_trade_events"),
        invoke<TradeSalesSummary>("trade_sales_summary"),
      ]);
      historyUnavailable = false; summaryUnavailable = false;
      if (announce) actionMessage = "Сделка записана. Синхронизируем продажу с Warframe Market автоматически.";
    } catch {
      historyUnavailable = true;
      if (announce) actionMessage = "Сделка обнаружена, но журнал пока не открылся. Обновите ордера.";
    }
  }

  async function loadAnalytics(): Promise<void> {
    if (!account?.connected) return;
    const request = ++analyticsRevision;
    const source = [...buildTradeShiftRows(account, inventory, recommendations), ...buildTradeShiftRows(account, inventory, recommendations, new Date(), "buy")];
    const keys = source.flatMap(row => row.key ? [row.key] : []);
    analyticsLoading = true; analyticsError = "";
    try {
      const collected = new Map<string, MarketAnalyticsSummary>();
      for (let offset = 0; offset < keys.length; offset += 500) {
        const result = await read<MarketAnalyticsBatch>("market_history_batch", {keys: keys.slice(offset, offset + 500)});
        if (disposed || request !== analyticsRevision) return;
        for (const item of result.items) collected.set(marketAnalyticsKey(item.key), summarizeMarketAnalytics(item, result.asOf));
      }
      analytics = collected;
    } catch { if (!disposed && request === analyticsRevision) analyticsError = "Не удалось загрузить статистику. Повторите загрузку списка."; }
    finally { if (!disposed && request === analyticsRevision) analyticsLoading = false; }
  }

  async function toggleVisibility(row: TradeShiftRow): Promise<void> {
    if (applying || !account?.profile?.verification) return;
    applying = true; errorMessage = ""; actionMessage = "";
    try {
      await validateCurrentOrders([row.order]);
      const updated = await invoke<AccountOrder>("account_update_listing", {id:row.order.id,input:updateInput({visible:!row.order.visible}),expectedOrder:row.order,confirmed:true});
      if (account) account = {...account,orders:account.orders.map(order => order.id === updated.id ? updated : order)};
      actionMessage = updated.visible ? "Объявление показано на Warframe Market." : "Объявление скрыто. Цена и количество сохранены.";
    } catch (error) { errorMessage = changeError(error); }
    finally { applying = false; }
  }

  async function openMarket(row: TradeShiftRow): Promise<void> {
    if (!row.item) return;
    try { await invoke("open_market_items",{slugs:[row.item.slug]}); }
    catch { errorMessage = "Не удалось открыть Warframe Market."; }
  }

  function selectPage(ids: string[], selected: boolean): void {
    const next = new Set(selectedIds);
    ids.forEach(id => selected ? next.add(id) : next.delete(id)); selectedIds = next;
  }

  function switchType(type: "sell" | "buy"): void { orderType = type; selectedIds = new Set(); orderFilter = "all"; orderQuery = ""; actionMessage = ""; errorMessage = ""; }

  async function loadSavedPrices(): Promise<void> {
    if (!account?.connected) return;
    const next = new Map([...recommendations].filter(([key]) => Date.now() - (liveCheckedAt.get(key) ?? 0) < quoteTtlSeconds * 1000));
    const currentRevision = dataRevision;
    for (const row of [...buildTradeShiftRows(account, inventory, next), ...buildTradeShiftRows(account, inventory, next, new Date(), "buy")]) {
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

  }

  async function refreshCurrentPrices(): Promise<void> {
    if (!account?.connected || refreshingLive) return;
    refreshingLive = true;
    const currentRevision = dataRevision;
    stopLiveRefresh = false;
    errorMessage = "";
    const candidates = (selectedOrders.length ? selectedOrders : rows).filter((row, index, source) => row.key
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
        if (change.delete) await invoke<AccountOrder>("account_delete_listing", { id: before.id, expectedOrder: before, confirmed: true });
        else await invoke<AccountOrder>("account_update_listing", { id: before.id, expectedOrder: before, input: updateInput({
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
    editPerTrade = row.order.perTrade;
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
      editPerTrade,
      "ru",
      editingOrder?.type === "sell" && editVisible ? rows.find((row) => row.order.id === editingOrder?.id)?.inventory?.sellableQuantity ?? null : null,
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
        expectedOrder: editingOrder,
        input: updateInput({
          platinum: editPlatinum,
          quantity: editQuantity,
          perTrade: editPerTrade ?? undefined,
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
        expectedOrder: orderToRemove,
        confirmed: true,
      });
      actionMessage = `Объявление «${manualOrderName(orderToRemove)}» удалено.`;
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
    if (visibilityScope === "all") {
      try {
        const result = await invoke<{updated:number}>("account_set_orders_visibility", {orderType,visible:visibilityIntent,expectedOrders:visibilityExpectedOrders,confirmed:true});
        completed = result.updated;
      } catch (error) { errorMessage = changeError(error); }
    } else for (const row of targets) {
      applyProgress = `${completed + 1} из ${targets.length}`;
      try {
        await invoke<AccountOrder>("account_update_listing", {
          id: row.id,
          expectedOrder: row,
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
    ++analyticsRevision; analytics = new Map();
    stopLiveRefresh = true;
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
      await Promise.all([loadSavedPrices(),loadAnalytics()]);
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
    ++analyticsRevision; analytics = new Map();
    stopLiveRefresh = true;
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



  function summarize(source: TradeShiftRow[]) {
    return {
      total: source.length,
      visible: source.filter((row) => row.order.visible).length,
    };
  }

</script>

<section class="sales-workspace" aria-labelledby="sales-heading">
  <header class="sales-header">
    <div><h2 id="sales-heading">{view === "history" ? "История сделок" : "Мои объявления"}</h2><p>{view === "history" ? "Обмены из игры и ваши торговые партнёры." : "Цена, спрос и быстрые действия — в каждой строке."}</p></div>
    {#if account?.connected}<div class="sales-header__actions"><button class="secondary account-button" aria-expanded={accountPanelOpen} onclick={() => accountPanelOpen = !accountPanelOpen}><span class="connection-dot"></span><span translate="no">{account.profile?.ingameName ?? "Warframe Market"}</span></button>{#if view === "orders" && rows.length}<button onclick={orderType === "buy" ? onBrowseMarket : onOpenInventory}>{orderType === "buy" ? "Создать заявку на покупку" : "Выставить предмет"}</button>{/if}</div>{/if}
  </header>
  {#if errorMessage}<div class="inline-error" role="alert"><span>{errorMessage}</span>{#if !applying}<button class="secondary" onclick={() => loadAll()} disabled={loading}>Повторить загрузку</button>{/if}</div>{/if}
  {#if dataMessage}<p class="data-note" role="status">{dataMessage}</p>{/if}
  {#if accountPanelOpen && account?.connected}<section class="account-panel" aria-label="Подключение Warframe Market"><div><strong>Warframe Market · <span translate="no">{account.profile?.ingameName}</span></strong><p>{account.profile?.platform.toUpperCase() ?? "PC"} · {account.profile?.verification ? "Аккаунт подтверждён" : "Нужно подтверждение аккаунта"}</p></div><button class="secondary" disabled={accountBusy || applying} onclick={disconnectAccount}>Отключить аккаунт</button></section>{/if}
  {#if view === "history"}
    <MarketTradeHistory {events} {account} salesSummary={summaryUnavailable ? null : tradeSales} unavailable={historyUnavailable} busy={applying} onRetry={retryTrade} onIgnore={ignoreTrade} onRestore={restoreTradeEvent} onUndo={confirmUndo} onReload={() => loadEvents()} />
  {:else if loading && !account}
    <section class="sales-empty" role="status"><h3>Загружаем объявления…</h3><p>Сверяем остатки и последние сделки.</p><div class="skeleton"></div><div class="skeleton"></div></section>
  {:else if !account && errorMessage}<section class="sales-empty"><h3>Объявления пока недоступны</h3><p>Повторите загрузку или воспользуйтесь поиском предметов.</p><button class="secondary" onclick={onBrowseMarket}>Найти предмет</button></section>
  {:else if !account?.connected}
    <section class="welcome"><div><p class="eyebrow">Ваши продажи в одном месте</p><h3>Подключите Warframe Market</h3><p>Здесь появятся ваши объявления, подсказки по ценам и количество доступных копий.</p><ul><li>Сравнивайте свою цену с предложениями рынка.</li><li>Меняйте цену, количество и показ объявления.</li><li>Следите за сделками, подтверждёнными игрой.</li></ul><button class="text-button" onclick={onBrowseMarket}>Посмотреть цены без подключения →</button></div>
      <form class="connect-panel" onsubmit={connectAccount}><label for="market-wfm-email">Email Warframe Market<input id="market-wfm-email" type="email" name="username" autocomplete="username" spellcheck="false" bind:value={email} required maxlength="128" placeholder="name@example.com" /></label><label for="market-wfm-password">Пароль Warframe Market<input id="market-wfm-password" type="password" name="password" autocomplete="current-password" bind:value={password} required maxlength="128" /></label><button type="submit" disabled={accountBusy}>{accountBusy ? "Подключаем…" : "Подключить аккаунт"}</button><details class="security-details"><summary>Как хранятся данные входа</summary><p>Пароль не сохраняется. Ключ сессии хранится в защищённом хранилище Windows.</p></details></form>
    </section>
  {:else}
    {#if !account.profile?.verification}<p class="data-note">Аккаунт не подтверждён. После подтверждения на Warframe Market обновите список.</p>{/if}
    {#if pendingEvents.length}<div class="pending-notice"><span>Не учтено продаж из игры: <strong>{pendingEvents.length}</strong></span><button class="text-button" onclick={onHistory}>Проверить сделки →</button></div>{/if}
    <section class="orders-panel" aria-labelledby="orders-heading">
      <header class="orders-heading"><div class="order-type" role="group" aria-label="Тип объявлений"><button aria-pressed={orderType === "sell"} onclick={() => switchType("sell")}>Продажа <b>{account.orders.filter(o => o.type === "sell").length}</b></button><button aria-pressed={orderType === "buy"} onclick={() => switchType("buy")}>Покупка <b>{account.orders.filter(o => o.type === "buy").length}</b></button></div><h3 class="sr-only" id="orders-heading">{orderType === "sell" ? "Объявления на продажу" : "Заявки на покупку"}</h3><div class="sales-header__actions"><span class="updated">{lastUpdated ? "Обновлено " + new Date(lastUpdated).toLocaleTimeString("ru-RU", {hour:"2-digit",minute:"2-digit"}) : "Загрузка…"}</span><button class="text-button" disabled={loading || applying || refreshingLive} onclick={() => loadAll()}>{loading ? "Обновляем…" : "Обновить список"}</button>{#if refreshingLive}<button class="secondary" onclick={() => stopLiveRefresh = true}>Остановить проверку</button>{:else}<button class="secondary" disabled={!rows.length || loading || applying} onclick={refreshCurrentPrices}>{selectedOrders.length ? "Проверить выбранные · " + selectedOrders.length : "Проверить цены"}</button>{/if}</div></header>
      <div class="orders-toolbar"><label class="order-search"><span class="sr-only">Поиск объявления</span><input type="search" bind:value={orderQuery} maxlength="80" autocomplete="off" spellcheck="false" placeholder="Найти среди объявлений · русское или английское название" /></label><label class="status-filter"><span class="sr-only">Показать объявления</span><select bind:value={orderFilter}><option value="all">Все объявления · {rows.length}</option><option value="attention">Требуют внимания · {rows.filter(row => row.needsAction).length}</option><option value="hidden">Скрытые · {rows.filter(row => !row.order.visible).length}</option></select></label><details class="all-actions"><summary>Действия со всеми</summary><div><button class="text-button" disabled={!summary.visible || applying || !account.profile?.verification} onclick={() => confirmVisibility(false,"all")}>Скрыть все · {summary.visible}</button><button class="text-button" disabled={summary.visible === summary.total || applying || !account.profile?.verification} onclick={() => confirmVisibility(true,"all")}>Показать все · {summary.total - summary.visible}</button></div></details></div>
      {#if selectedOrders.length}<div class="batch-bar"><div><strong>Выбрано: {selectedOrders.length}</strong><button class="text-button" disabled={applying} onclick={() => selectedIds = new Set()}>Снять выбор</button></div><div class="sales-header__actions"><button class="secondary" disabled={applying || !selectedOrders.some(r => r.order.visible)} onclick={() => confirmVisibility(false)}>Скрыть выбранные</button><button class="secondary" disabled={applying || !selectedOrders.some(r => !r.order.visible)} onclick={() => confirmVisibility(true)}>Показать выбранные</button>{#if selectedRows.length}<button disabled={applying || refreshingLive} onclick={openBatch}>Проверить изменения · {selectedRows.length}</button>{/if}</div></div>{/if}
      {#if liveProgress}<p class="status-line" role="status">{liveProgress}</p>{/if}{#if actionMessage}<p class="status-line action-line" role="status">{actionMessage}</p>{/if}{#if analyticsError}<p class="status-line" role="status">{analyticsError}</p>{/if}
      {#if visibleRows.length}
        <MarketOrderTable rows={visibleRows} {selectedIds} {analytics} {analyticsLoading} {analyticsError} inventoryKnown={!!inventory} busy={applying || reviewOpen || !!editingOrder} verified={!!account.profile?.verification} onToggle={toggleSelected} onSelectPage={selectPage} onVisibility={toggleVisibility} onEdit={beginManualEdit} onMarket={openMarket}/>
      {:else if rows.length}<div class="sales-empty"><h3>Подходящих объявлений нет</h3><p>Измените название или отбор.</p><button class="secondary" onclick={() => {orderQuery="";orderFilter="all";}}>Показать все объявления</button></div>
      {:else}<div class="sales-empty"><h3>{orderType === "sell" ? "Нет объявлений на продажу" : "Нет заявок на покупку"}</h3><p>{orderType === "sell" ? "Выберите предмет из инвентаря, чтобы выставить его на Warframe Market." : "Найдите предмет на рынке и разместите заявку с нужной ценой."}</p><button class="secondary" onclick={orderType === "sell" ? onOpenInventory : onBrowseMarket}>{orderType === "sell" ? "Выставить предмет" : "Найти предмет"}</button></div>{/if}
    </section>
    <div class="workspace-footnote"><span>Список обновляется раз в минуту. Глаз — показ объявления; название или график — подробная статистика.</span><details><summary>Как читать статистику</summary><p>Графики показывают последние 7 завершённых дней UTC. Процент сравнивает их с предыдущей неделей. «Объём / день» — средний объём закрытых сделок Warframe Market, не все обмены в игре. Пропуски данных остаются пропусками. Ориентир цены — за штуку; цена партии подписана отдельно.</p></details></div>
  {/if}
</section>

<dialog class="sales-dialog" bind:this={orderDialog} oncancel={event => { if (applying) event.preventDefault(); }} onclose={() => { editingOrder = null; orderToRemove = null; editError = ""; }} aria-labelledby="order-editor-heading">
  {#if editingOrder}{@const editingRow = rows.find(row => row.order.id === editingOrder?.id)}
    <header class="dialog-heading"><div><p class="eyebrow">Warframe Market</p><h2 id="order-editor-heading">{orderToRemove ? "Снять объявление с продажи?" : "Изменить объявление"}</h2></div><button class="secondary" disabled={applying} onclick={closeEditor}>Закрыть</button></header>
    <div class="dialog-item">{#if editingRow?.item?.imageUrl}<img src={editingRow.item.imageUrl} alt="" />{/if}<div><strong>{manualOrderName(editingOrder)}</strong>{#if orderEnglishName(editingRow?.item ?? undefined)}<span translate="no">{orderEnglishName(editingRow?.item ?? undefined)}</span>{/if}<small>{editingRow?.key ? variantLabel(editingRow.key) : ""}</small></div></div>
    {#if editError || errorMessage}<p class="inline-error" role="alert">{editError || errorMessage}</p>{/if}
    {#if orderToRemove}<p class="dialog-description">Объявление исчезнет с Warframe Market. Чтобы вернуть его, потребуется новая публикация.</p><div class="confirm-actions"><button class="danger-primary" disabled={applying} onclick={removeManualOrder}>{applying ? "Удаляем…" : "Удалить объявление"}</button><button class="secondary" disabled={applying} onclick={() => orderToRemove = null}>Вернуться к редактированию</button></div>
    {:else}<form class="order-editor" onsubmit={reviewManualEdit}>
      <dl class="edit-context"><div><dt>Ориентир рынка</dt><dd>{editingRow ? money(orderMarketPrice(editingRow)) : "Нет оценки"}</dd></div>{#if editingOrder.type === "sell"}<div><dt>Доступно для продажи</dt><dd>{inventory ? (editingRow?.inventory?.sellableQuantity ?? 0) + " шт." : "Остаток неизвестен"}</dd></div>{/if}</dl>
      <div class="order-editor__fields"><label>Цена, платина{#if (editingOrder.perTrade ?? 1) > 1}<small>за {editingOrder.perTrade} шт.</small>{/if}<input type="number" inputmode="numeric" bind:value={editPlatinum} min="1" max="900000" step="1" required /></label><label>Количество, шт.<input type="number" inputmode="numeric" bind:value={editQuantity} min="1" max="9999" step="1" required /></label>{#if editingOrder.perTrade !== null}<label>В одной сделке, шт.<input type="number" inputmode="numeric" bind:value={editPerTrade} min="1" max="6" step="1" required /></label>{/if}</div>
      {#if editingRow && rowChange(editingRow) && !rowChange(editingRow)?.delete}<button class="text-button use-suggestion" type="button" onclick={() => { if (editingRow.suggestedPrice !== null) editPlatinum = editingRow.suggestedPrice; if (editingRow.suggestedQuantity !== null) editQuantity = editingRow.suggestedQuantity; }}>Подставить предложенные цену и количество</button>{/if}
      <label class="compact-check"><input type="checkbox" bind:checked={editVisible} /> Показывать на Warframe Market</label>
      <section class="edit-preview" aria-label="Изменения объявления"><strong>Будет сохранено</strong><dl><div><dt>Цена{(editingOrder.perTrade ?? 1) > 1 ? " за партию" : " за штуку"}</dt><dd>{money(editingOrder.platinum)} → {money(editPlatinum ?? null)}</dd></div><div><dt>Количество</dt><dd>{editingOrder.quantity} → {editQuantity ?? "—"} шт.</dd></div>{#if editPerTrade !== null}<div><dt>В одной сделке</dt><dd>{editingOrder.perTrade} → {editPerTrade} шт.</dd></div>{/if}<div><dt>Показ на рынке</dt><dd>{editVisible ? "Включён" : "Выключен"}</dd></div></dl></section>
      <div class="confirm-actions"><button type="submit" disabled={applying || (editPlatinum === editingOrder.platinum && editQuantity === editingOrder.quantity && editVisible === editingOrder.visible && editPerTrade === editingOrder.perTrade)}>{applying ? "Сохраняем…" : "Сохранить изменения"}</button><button class="text-button danger" type="button" disabled={applying} onclick={() => orderToRemove = editingOrder}>Удалить объявление</button></div>
    </form>{/if}
  {/if}
</dialog>
<dialog class="sales-dialog" bind:this={batchDialog} onclose={() => reviewOpen = false} oncancel={event => { if (applying) event.preventDefault(); }} aria-labelledby="batch-heading">
  <header class="dialog-heading"><div><p class="eyebrow">Проверка перед отправкой</p><h2 id="batch-heading">Изменить объявления · {reviewed.length}</h2></div><button class="secondary" disabled={applying} onclick={closeBatchReview}>Закрыть</button></header><p class="dialog-description">На Warframe Market будут отправлены только показанные ниже изменения.</p>
  {#if errorMessage}<p class="inline-error" role="alert">{errorMessage}</p>{/if}
  <div class="reviewed-list">{#each reviewed as proposal (proposal.before.id)}<article><strong>{proposal.name}</strong>{#if proposal.change.delete}<p class="danger">Удалить объявление — свободных копий нет.</p>{:else}<dl>{#if proposal.change.price !== null}<div><dt>Цена{(proposal.before.perTrade ?? 1) > 1 ? " за " + proposal.before.perTrade + " шт." : " за штуку"}</dt><dd>{money(proposal.before.platinum)} → <b>{money(proposal.change.price)}</b></dd></div>{/if}{#if proposal.change.quantity !== null}<div><dt>Количество</dt><dd>{proposal.before.quantity} → <b>{proposal.change.quantity} шт.</b></dd></div>{/if}</dl>{/if}</article>{/each}</div>
  {#if applyProgress}<p class="status-line" role="status">{applyProgress}</p>{/if}<div class="confirm-actions"><button disabled={applying || !reviewed.length} onclick={applySelectedChanges}>{applying ? "Отправляем…" : "Применить изменения"}</button><button class="secondary" disabled={applying} onclick={closeBatchReview}>Отмена</button></div>
</dialog>
<dialog class="sales-dialog" bind:this={visibilityDialog} onclose={() => visibilityIntent = null} oncancel={event => { if (applying) event.preventDefault(); }} aria-labelledby="visibility-heading">
  <h2 id="visibility-heading">{visibilityIntent ? "Показать" : "Скрыть"} объявления · {visibilityTargets.length}</h2><p class="dialog-description">{visibilityIntent ? "Выбранные объявления станут видны на Warframe Market." : "Эти объявления перестанут отображаться на рынке. Цены и количество сохранятся."}</p><ul class="visibility-items">{#each visibilityTargets as order}<li>{manualOrderName(order)}</li>{/each}</ul>
  {#if errorMessage}<p class="inline-error" role="alert">{errorMessage}</p>{/if}{#if applyProgress}<p role="status">{applyProgress}</p>{/if}<div class="confirm-actions"><button disabled={applying} onclick={applyVisibility}>{applying ? "Отправляем…" : visibilityIntent ? "Показать объявления" : "Скрыть объявления"}</button><button class="secondary" disabled={applying} onclick={() => visibilityDialog.close()}>Отмена</button></div>
</dialog>
<dialog class="sales-dialog" bind:this={undoDialog} onclose={() => tradeToUndo = null} oncancel={event => { if (applying) event.preventDefault(); }} aria-labelledby="trade-undo-heading">
  <h2 id="trade-undo-heading">Вернуть объявление к состоянию до сделки?</h2><p class="dialog-description">Используйте отмену, если предмет или количество были сопоставлены неправильно.</p>{#if tradeToUndo}<p>{soldItems(tradeToUndo)}</p>{/if}{#if errorMessage}<p class="inline-error" role="alert">{errorMessage}</p>{/if}<div class="confirm-actions"><button disabled={applying} onclick={() => tradeToUndo && undoTrade(tradeToUndo)}>Вернуть объявление</button><button class="secondary" disabled={applying} onclick={() => undoDialog.close()}>Отмена</button></div>
</dialog>

<style>
  .sales-workspace { display:grid; gap:.65rem; min-width:0; }
  h2,h3,p,dd,dl { margin:0; } p { font-size:.8125rem; line-height:1.5; color:var(--text-muted); } h2 { font-size:1.15rem; } h3 { font-size:1rem; }
  button { min-height:2rem; font-size:.8125rem; } .text-button { background:none; border-color:transparent; color:var(--accent); box-shadow:none; }
  .sales-header,.sales-header__actions,.account-panel,.orders-heading,.orders-toolbar,.batch-bar,.confirm-actions,.dialog-heading,.pending-notice { display:flex; align-items:center; justify-content:space-between; gap:.6rem; }
  .sales-header p { margin-top:.25rem; } .sales-header__actions,.confirm-actions { justify-content:flex-start; flex-wrap:wrap; }
  .account-button { display:flex; align-items:center; gap:.45rem; } .connection-dot { width:.4rem; height:.4rem; background:var(--success); border-radius:50%; }
  .account-panel { padding:.75rem; border:1px solid var(--border); border-radius:.5rem; background:var(--surface-1); } .account-panel strong { font-size:.875rem; }
  .inline-error,.data-note { margin:0; border:1px solid var(--border); border-radius:.5rem; padding:.6rem .8rem; font-size:.8125rem; line-height:1.5; }
  .inline-error { display:flex; align-items:center; justify-content:space-between; gap:1rem; background:var(--danger-soft); color:var(--danger); border-color:var(--danger); } .data-note { background:var(--surface-2); }
  .sales-empty { padding:1.5rem; border:1px solid var(--border); border-radius:.5rem; background:var(--surface-1); } .sales-empty p { margin:.5rem 0 .8rem; max-width:42rem; } .orders-panel .sales-empty { border:0; }
  .skeleton { height:2.5rem; border-radius:.3rem; background:var(--surface-2); margin-top:.5rem; }
  .welcome { display:grid; grid-template-columns:minmax(0,1fr) minmax(18rem,.75fr); gap:2rem; padding:1.5rem; border:1px solid var(--border); border-radius:.7rem; background:var(--surface-1); }
  .welcome h3 { font-size:1.3rem; margin:.6rem 0; } .welcome ul { padding-left:1.2rem; font-size:.875rem; line-height:1.9; color:var(--text-muted); }
  .eyebrow { font-size:.75rem; letter-spacing:.08em; text-transform:uppercase; color:var(--text-subtle); font-weight:700; }
  .connect-panel { display:grid; align-content:start; gap:1rem; padding:1rem; border-radius:.5rem; background:var(--surface-2); }
  .connect-panel label,.order-editor__fields label { display:grid; gap:.4rem; font-size:.8125rem; font-weight:600; }
  input:not([type=checkbox]),select { min-width:0; width:100%; min-height:2.1rem; border:1px solid var(--border-strong); border-radius:.35rem; padding:.35rem .55rem; background:var(--surface-1); color:var(--text); font:inherit; font-size:.8125rem; }
  input::placeholder { color:var(--text-subtle); font-weight:400; } input[type=checkbox] { accent-color:var(--accent); width:1rem; height:1rem; flex:none; }
  .security-details { font-size:.75rem; color:var(--text-muted); } .security-details summary { cursor:pointer; } .security-details p { margin-top:.5rem; }
  .orders-panel { min-width:0; border:1px solid var(--border); border-radius:.6rem; background:var(--surface-1); box-shadow:var(--shadow-sm); }
  .orders-heading { padding:.6rem .8rem; flex-wrap:wrap; border-bottom:1px solid var(--border); } .updated { font-size:.75rem; color:var(--text-muted); }
  .order-type { display:flex; gap:.3rem; } .order-type button { padding:.4rem .7rem; border:1px solid transparent; background:transparent; color:var(--text-muted); box-shadow:none; } .order-type button[aria-pressed=true] { border-color:var(--border-strong); background:var(--accent-soft); color:var(--text); } .order-type b { margin-left:.4rem; font-weight:500; }
  .orders-toolbar { padding:.6rem .8rem; } .order-search { flex:1; min-width:12rem; } .status-filter { width:13rem; }
  .all-actions { position:relative; font-size:.75rem; } .all-actions summary { padding:.5rem; white-space:nowrap; cursor:pointer; }
  .all-actions > div { position:absolute; z-index:5; right:0; top:100%; width:13rem; padding:.4rem; background:var(--surface-1); border:1px solid var(--border); box-shadow:var(--shadow-md); border-radius:.4rem; } .all-actions button { display:block; width:100%; text-align:left; }
  .batch-bar { border-block:1px solid var(--border); padding:.45rem .8rem; background:var(--accent-soft); flex-wrap:wrap; } .batch-bar > div { display:flex; align-items:center; gap:.6rem; } .batch-bar strong { font-size:.8125rem; }
  .status-line { margin:0; padding:.45rem .8rem; font-size:.75rem; line-height:1.4; color:var(--text-muted); border-bottom:1px solid var(--border); } .action-line { color:var(--success); }
  .pending-notice { padding:.5rem .8rem; font-size:.8125rem; background:var(--accent-soft); border-radius:.4rem; }
  .workspace-footnote { display:flex; align-items:start; gap:1rem; justify-content:space-between; color:var(--text-muted); font-size:.75rem; line-height:1.5; } .workspace-footnote details { max-width:28rem; } .workspace-footnote summary { cursor:pointer; white-space:nowrap; } .workspace-footnote p { margin-top:.5rem; font-size:.75rem; }
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

  @media(max-width:1100px) { .orders-toolbar { flex-wrap:wrap; } .order-search { flex-basis:100%; } .status-filter { flex:1; } .welcome { grid-template-columns:1fr; gap:1rem; } }
  @media(max-width:750px) { .sales-header,.workspace-footnote { flex-wrap:wrap; } .sales-header p { max-width:25rem; } .sales-header__actions { flex-wrap:wrap; } }
</style>
