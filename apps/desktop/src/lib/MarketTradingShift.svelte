<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
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
    type OrderHealth,
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
  let manualReviewOpen = false;
  let orderToRemove: AccountOrder | null = null;
  let orderQuery = "";
  let editorHeading: HTMLHeadingElement;
  let batchHeading: HTMLHeadingElement;
  let manualHeading: HTMLHeadingElement;
  let removeHeading: HTMLHeadingElement;
  let editorTrigger: HTMLElement | null = null;
  let batchTrigger: HTMLElement | null = null;

  function reveal(element: HTMLElement | undefined): void {
    element?.scrollIntoView({ block: "center" });
    element?.focus({ preventScroll: true });
  }

  $: if (reviewOpen) void tick().then(() => reveal(batchHeading));
  $: if (manualReviewOpen) void tick().then(() => reveal(manualHeading));
  $: if (orderToRemove) void tick().then(() => reveal(removeHeading));

  function closeEditor(): void {
    editingOrder = null;
    manualReviewOpen = false;
    reveal(editorTrigger ?? undefined);
  }

  function closeBatchReview(): void {
    reviewOpen = false;
    reveal(batchTrigger ?? undefined);
  }

  $: rows = account
    ? applyPriceCheckFailures(
        buildTradeShiftRows(account, inventory, recommendations),
        failedPriceChecks,
      )
    : [];
  $: visibleRows = filterTradeShiftRows(rows, orderQuery);
  $: actionableRows = rows.filter((row) => row.needsAction && rowChange(row) !== null);
  $: selectedRows = actionableRows.filter((row) => selectedIds.has(row.order.id));
  $: pendingEvents = pendingSaleEvents(events);
  $: historyEvents = visibleTradeHistory(events);
  $: attentionCount = rows.filter((row) => row.needsAction).length + pendingEvents.length;
  $: summary = summarize(rows);

  onMount(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    void loadAll();
    void Promise.all([
      listen("trade-detected", () => void loadEvents(true)),
      listen("trade-reconciled", () => {
        actionMessage = "Продажа автоматически учтена в Warframe Market.";
        void loadAll();
      }),
      listen("trade-reconciliation-failed", () => void loadEvents()),
      listen("inventory-updated", () => void loadInventoryAndPrices()),
    ]).then((items) => {
      if (disposed) items.forEach((unlisten) => unlisten());
      else unlisteners.push(...items);
    });
    return () => {
      disposed = true;
      stopLiveRefresh = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  });

  async function loadAll(): Promise<void> {
    loading = true;
    errorMessage = "";
    actionMessage = "";
    try {
      const [nextAccount, nextInventory, nextEvents, nextTradeSales] = await Promise.all([
        invoke<AccountView>("account_status"),
        invoke<InventoryView | null>("load_inventory"),
        invoke<TradeEvent[]>("trade_events"),
        invoke<TradeSalesSummary>("trade_sales_summary"),
      ]);
      account = nextAccount;
      inventory = nextInventory;
      events = nextEvents;
      tradeSales = nextTradeSales;
      selectedIds = new Set();
      reviewOpen = false;
      await loadSavedPrices();
    } catch (error) {
      errorMessage = "Не удалось собрать торговую смену. " + accountActionErrorMessage(String(error));
    } finally {
      loading = false;
    }
  }

  async function loadInventoryAndPrices(): Promise<void> {
    try {
      inventory = await invoke<InventoryView | null>("load_inventory");
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
      if (announce) actionMessage = "Сделка записана. Синхронизируем продажу с Warframe Market автоматически.";
    } catch {
      if (announce) actionMessage = "Сделка обнаружена, но журнал пока не открылся. Обновите ордера.";
    }
  }

  async function loadSavedPrices(): Promise<void> {
    if (!account?.connected) return;
    const next = new Map(recommendations);
    for (const row of buildTradeShiftRows(account, inventory, next)) {
      if (!row.key || next.has(recommendationIdentity(row.key))) continue;
      try {
        const result = await invoke<PriceRecommendation | null>("price_current_variant", {
          key: row.key,
          itemKind: row.itemKind,
        });
        next.set(recommendationIdentity(row.key), result);
      } catch {
        next.set(recommendationIdentity(row.key), null);
      }
    }
    recommendations = next;
    selectSuggestedByDefault(buildTradeShiftRows(account, inventory, next));
  }

  async function refreshCurrentPrices(): Promise<void> {
    if (!account?.connected || refreshingLive) return;
    refreshingLive = true;
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
        const result = await invoke<LivePricingResult | null>("live_price_current_variant", {
          key: row.key,
          itemKind: row.itemKind,
        });
        if (checkedLivePrice(result).state === "failed") throw new Error("stale price fallback");
        if (row.key) failures.delete(recommendationIdentity(row.key));
        if (row.key) next.set(recommendationIdentity(row.key), result?.recommendation ?? null);
        checked += 1;
      } catch (error) {
        if (row.key) failures.add(recommendationIdentity(row.key));
        const reason = String(error).toLowerCase();
        rateLimited ||= reason.includes("rate limit") || reason.includes("429");
      }
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

  function rowChange(row: TradeShiftRow): { price: number | null; quantity: number | null; delete: boolean } | null {
    const quantity = row.suggestedQuantity !== null && row.suggestedQuantity !== row.order.quantity
      ? row.suggestedQuantity
      : null;
    const price = row.suggestedPrice !== null && row.suggestedPrice !== row.order.platinum
      ? row.suggestedPrice
      : null;
    if (quantity === 0) return { price, quantity, delete: true };
    if (price === null && quantity === null) return null;
    return { price, quantity, delete: false };
  }

  async function applySelectedChanges(): Promise<void> {
    if (!account || !selectedRows.length || applying) return;
    applying = true;
    errorMessage = "";
    let completed = 0;
    for (const row of selectedRows) {
      const change = rowChange(row);
      if (!change) continue;
      applyProgress = `${completed + 1} из ${selectedRows.length}: ${row.item?.displayName ?? "ордер"}`;
      try {
        if (change.delete) {
          await invoke<AccountOrder>("account_delete_listing", {
            id: row.order.id,
            confirmed: true,
          });
        } else {
          await invoke<AccountOrder>("account_update_listing", {
            id: row.order.id,
            input: updateInput({
              ...(change.price !== null ? { platinum: change.price } : {}),
              ...(change.quantity !== null ? { quantity: change.quantity } : {}),
            }),
            confirmed: true,
          });
        }
        completed += 1;
      } catch (error) {
        errorMessage = `${row.item?.displayName ?? "Ордер"}: ${accountActionErrorMessage(String(error))}`;
        break;
      }
    }
    actionMessage = completed
      ? `Применено изменений: ${completed}.`
      : "Изменения не применены.";
    applying = false;
    reviewOpen = false;
    applyProgress = "";
    await reloadAccount();
  }

  function beginManualEdit(row: TradeShiftRow): void {
    editorTrigger = document.activeElement as HTMLElement | null;
    editingOrder = row.order;
    editPlatinum = row.order.platinum;
    editQuantity = row.order.quantity;
    editVisible = row.order.visible;
    editError = "";
    manualReviewOpen = false;
    reviewOpen = false;
    void tick().then(() => reveal(editorHeading));
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
    if (!editError) manualReviewOpen = true;
  }

  async function applyManualEdit(): Promise<void> {
    if (!editingOrder || applying) return;
    applying = true;
    errorMessage = "";
    try {
      await invoke<AccountOrder>("account_update_listing", {
        id: editingOrder.id,
        input: updateInput({
          platinum: editPlatinum,
          quantity: editQuantity,
          visible: editVisible,
        }),
        confirmed: true,
      });
      actionMessage = `Ордер «${manualOrderName(editingOrder)}» обновлён.`;
      editingOrder = null;
      manualReviewOpen = false;
      await reloadAccount();
    } catch (error) {
      editError = accountActionErrorMessage(String(error));
      manualReviewOpen = false;
    } finally {
      applying = false;
    }
  }

  async function removeManualOrder(): Promise<void> {
    if (!orderToRemove || applying) return;
    applying = true;
    errorMessage = "";
    try {
      await invoke<AccountOrder>("account_delete_listing", {
        id: orderToRemove.id,
        confirmed: true,
      });
      actionMessage = `Ордер «${manualOrderName(orderToRemove)}» снят.`;
      if (editingOrder?.id === orderToRemove.id) editingOrder = null;
      orderToRemove = null;
      await reloadAccount();
    } catch (error) {
      errorMessage = accountActionErrorMessage(String(error));
    } finally {
      applying = false;
    }
  }

  async function applyVisibility(): Promise<void> {
    if (!account || visibilityIntent === null || applying) return;
    const targets = rows.filter((row) => row.order.visible !== visibilityIntent);
    applying = true;
    let completed = 0;
    for (const row of targets) {
      applyProgress = `${completed + 1} из ${targets.length}`;
      try {
        await invoke<AccountOrder>("account_update_listing", {
          id: row.order.id,
          input: updateInput({ visible: visibilityIntent }),
          confirmed: true,
        });
        completed += 1;
      } catch (error) {
        errorMessage = accountActionErrorMessage(String(error));
        break;
      }
    }
    actionMessage = visibilityIntent
      ? `Опубликовано ордеров: ${completed}.`
      : `Скрыто ордеров: ${completed}.`;
    applying = false;
    visibilityIntent = null;
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
      errorMessage = accountActionErrorMessage(String(error));
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
      errorMessage = accountActionErrorMessage(String(error));
    }
    applying = false;
    tradeToUndo = null;
    await reloadAccount();
    await loadEvents();
  }

  async function ignoreTrade(event: TradeEvent): Promise<void> {
    await invoke<boolean>("trade_event_ignore", { id: event.id });
    await loadEvents();
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
      errorMessage = accountActionErrorMessage(String(error));
    } finally {
      applying = false;
      await reloadAccount();
      await loadEvents();
    }
  }

  async function reloadAccount(): Promise<void> {
    try {
      account = await invoke<AccountView>("account_status");
      await loadSavedPrices();
    } catch (error) {
      errorMessage = accountActionErrorMessage(String(error));
    }
  }

  async function connectAccount(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    accountBusy = true;
    errorMessage = "";
    actionMessage = "Подключаем аккаунт WFM…";
    try {
      account = await invoke<AccountView>("account_connect", { email, password });
      email = "";
      password = "";
      recommendations = new Map();
      await loadSavedPrices();
      actionMessage = "Аккаунт WFM подключён.";
    } catch {
      password = "";
      actionMessage = "";
      errorMessage = "Не удалось подключить WFM. Проверьте email и пароль Warframe Market.";
    } finally {
      accountBusy = false;
    }
  }

  async function disconnectAccount(): Promise<void> {
    accountBusy = true;
    errorMessage = "";
    actionMessage = "Отключаем аккаунт WFM…";
    try {
      const remotelyRevoked = await invoke<boolean>("account_disconnect");
      account = { connected: false, profile: null, orders: [], orderItems: {} };
      recommendations = new Map();
      selectedIds = new Set();
      accountPanelOpen = false;
      actionMessage = remotelyRevoked
        ? "Аккаунт WFM отключён."
        : "Данные входа удалены с этого компьютера; WFM не подтвердил завершение сессии.";
    } catch {
      actionMessage = "";
      errorMessage = "Не удалось отключить аккаунт WFM. Повторите попытку.";
    } finally {
      accountBusy = false;
    }
  }

  function healthLabel(health: OrderHealth): string {
    return ({
      inventory_mismatch: "Количество не сходится",
      overpriced: "Цена выше рынка",
      underpriced: "Можно повысить цену",
        price_check_failed: "Не удалось проверить цену",
      hidden: "Скрыт",
      healthy: "В порядке",
      unknown: "Нет надёжной цены",
    } satisfies Record<OrderHealth, string>)[health];
  }

  function manualOrderName(order: AccountOrder): string {
    return rows.find((row) => row.order.id === order.id)?.item?.displayName ?? "Ордер";
  }

  function eventTitle(event: TradeEvent): string {
    if (isSaleTrade(event)) return `Продажа · +${event.platinumReceived}p`;
    if (event.platinumGiven > 0 && event.platinumReceived === 0) return `Покупка · −${event.platinumGiven}p`;
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
    if (!account) return "Проверяем ордера WFM";
    const plan = planTradeReconciliation(event, account);
    if (plan.actions.length > 0 && plan.unmatched.length === 0 && plan.unsafe.length === 0) {
      return "Точный ордер найден";
    }
    if (plan.unsafe.length > 0) {
      return "Ордер найден, но вариант или количество требуют повторной проверки";
    }
    return "Точного ордера нет в текущем списке WFM";
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

<section class="sales-workspace" aria-labelledby="sales-heading" aria-busy={loading || applying || accountBusy}>
  <header class="sales-header">
    <div>
      <h2 id="sales-heading">Мои продажи</h2>
      <p>Все ордера показаны сразу; те, что требуют решения, стоят первыми.</p>
    </div>
    {#if account?.connected}
      <div class="sales-header__actions">
        <button class="secondary compact" type="button" aria-expanded={accountPanelOpen} onclick={() => (accountPanelOpen = !accountPanelOpen)}>WFM · <span translate="no">{account.profile?.ingameName ?? "аккаунт"}</span></button>
        <button class="secondary compact" type="button" disabled={loading || applying} onclick={loadAll}>Обновить ордера</button>
        {#if refreshingLive}
          <button class="secondary compact" type="button" onclick={() => (stopLiveRefresh = true)}>Остановить проверку</button>
        {:else}
          <button class="compact" type="button" disabled={!rows.length} onclick={refreshCurrentPrices}>Проверить цены</button>
        {/if}
      </div>
    {/if}
  </header>

  <div class="status-line" aria-live="polite">
    {#if liveProgress}{liveProgress}{:else if actionMessage}{actionMessage}{/if}
  </div>
  {#if errorMessage}<p class="inline-error" role="alert">{errorMessage}</p>{/if}

  {#if accountPanelOpen && account?.connected}
    <section class="account-panel" aria-labelledby="wfm-account-heading">
      <div>
        <h3 id="wfm-account-heading"><span translate="no">{account.profile?.ingameName ?? "Warframe Market"}</span></h3>
        <p>{account.profile?.platform.toUpperCase() ?? "PC"} · {account.profile?.crossplay ? "общий рынок включён" : "общий рынок выключен"} · {account.profile?.verification ? "аккаунт подтверждён" : "аккаунт не подтверждён"}</p>
      </div>
      <div class="account-panel__actions">
        <button class="secondary compact" type="button" disabled={accountBusy} onclick={reloadAccount}>Обновить статус</button>
        <button class="danger-secondary compact" type="button" disabled={accountBusy} onclick={disconnectAccount}>Отключить WFM</button>
      </div>
    </section>
  {/if}

  {#if loading}
    <p class="sales-empty">Загружаем ордера, остатки и последние сделки…</p>
  {:else if !account?.connected}
    <form class="connect-panel" onsubmit={connectAccount}>
      <div class="connect-panel__copy"><h3>Подключить Warframe Market</h3><p>После подключения здесь появятся ордера и подтверждённые игрой продажи.</p></div>
      <div class="connect-fields">
        <label for="market-wfm-email">Email Warframe Market<input id="market-wfm-email" name="username" type="email" autocomplete="username" spellcheck="false" bind:value={email} required maxlength="128" placeholder="name@example.com" /></label>
        <label for="market-wfm-password">Пароль Warframe Market<input id="market-wfm-password" name="password" type="password" autocomplete="current-password" bind:value={password} required maxlength="128" /></label>
      </div>
      <button class="compact" type="submit" disabled={accountBusy}>{accountBusy ? "Подключаем…" : "Подключить WFM"}</button>
      <details class="security-details"><summary>Как хранятся данные входа</summary><p>Пароль используется только для входа и не сохраняется. Ключ сессии хранится в защищённом хранилище Windows.</p></details>
    </form>
  {:else}
    <dl class="sales-summary">
      <div class:attention={attentionCount > 0}><dt>Требуют действий</dt><dd>{attentionCount}</dd></div>
      <div><dt>Активные ордера</dt><dd>{summary.visible} <small>из {summary.total}</small></dd></div>
      <div><dt>Получено по зафиксированным продажам</dt><dd>{tradeSales.platinumReceived ? `${tradeSales.platinumReceived}p` : "—"} <small>{tradeSales.saleCount ? saleCountLabel(tradeSales.saleCount) : ""}</small></dd></div>
    </dl>

    {#if !account.profile?.verification}
      <div class="verification-note" role="note"><span>Аккаунт WFM не подтверждён: ордера доступны для просмотра, но менять их нельзя.</span><button class="text-button compact" type="button" disabled={accountBusy} onclick={reloadAccount}>Обновить статус</button></div>
    {/if}

    {#if pendingEvents.length}
      <section class="priority-panel" aria-labelledby="pending-sales-heading">
        <header class="section-heading">
          <div><p class="section-kicker">Не синхронизированы</p><h3 id="pending-sales-heading">Продажи из игры</h3></div>
          <span class="count-badge">{pendingEvents.length}</span>
        </header>
        <p class="section-hint">Обычно PlatScope отмечает продажу на WFM сам. Здесь остаются только сделки без однозначного ордера или после сетевой ошибки.</p>
        <div class="trade-events">
          {#each pendingEvents as event (event.id)}
            <article class="pending">
              <div class="trade-event__copy">
                <strong>{eventTitle(event)}</strong>
                <span>{soldItems(event)}{event.partner ? ` · ${event.partner}` : ""}</span>
                <small>{new Date(event.occurredAt).toLocaleString("ru-RU")}</small>
              </div>
              <div class="trade-event__actions">
                <span class="manual">{eventMatchStatus(event)}</span>
                <button class="compact" type="button" disabled={applying} onclick={() => retryTrade(event)}>
                  {eventCanApply(event) ? "Учесть продажу" : "Найти ордер снова"}
                </button>
                <button class="text-button compact" type="button" onclick={() => ignoreTrade(event)}>Пропустить</button>
              </div>
            </article>
          {/each}
        </div>
      </section>
    {/if}

    <section class="orders-panel" aria-labelledby="orders-heading">
      <header class="orders-toolbar">
        <div>
          <h3 id="orders-heading">Ордера на продажу</h3>
          <span id="orders-result-count">{orderQuery.trim() ? `${visibleRows.length} из ${rows.length}` : `${rows.length} всего`}</span>
        </div>
        <label class="order-search" for="order-search">
          <span>Поиск ордера</span>
          <input id="order-search" type="search" bind:value={orderQuery} maxlength="80" autocomplete="off" spellcheck="false" placeholder="Например, Рино Прайм или Rhino Prime" aria-describedby="orders-result-count" disabled={!rows.length} />
        </label>
        <div class="orders-toolbar__actions">
          <button class="compact" type="button" disabled={!selectedRows.length || applying} onclick={(event) => { batchTrigger = event.currentTarget; reviewOpen = true; }}>Посмотреть изменения ({selectedRows.length})</button>
        </div>
      </header>

      {#if visibleRows.length}
        <div class="shift-table-wrap">
          <table class="shift-table">
            <caption class="sr-only">Состояние ордеров Warframe Market</caption>
            <colgroup><col class="col-check" /><col class="col-item" /><col class="col-order" /><col class="col-market" /><col class="col-stock" /><col class="col-status" /></colgroup>
            <thead><tr><th class="check-column"><span class="sr-only">Выбрать</span></th><th>Предмет</th><th>В ордере</th><th>Рекомендуется</th><th>Можно продать</th><th>Что сделать</th></tr></thead>
            <tbody>
              {#each visibleRows as row (row.order.id)}
                {@const englishName = orderEnglishName(row.item ?? undefined)}
                <tr class:row-attention={row.needsAction}>
                  <td>
                    {#if rowChange(row)}
                      <input aria-label={`Выбрать изменение для ${row.item?.displayName ?? "ордера"}`} type="checkbox" checked={selectedIds.has(row.order.id)} onchange={() => toggleSelected(row.order.id)} />
                    {/if}
                  </td>
                  <th scope="row">
                    <span class="item-cell">
                      {#if row.item?.imageUrl}<img src={row.item.imageUrl} alt="" loading="lazy" />{/if}
                      <span>
                        <strong>{row.item?.displayName ?? "Неизвестный предмет"}</strong>
                        {#if englishName}<span class="item-name-en" translate="no">{englishName}</span>{/if}
                        <small>{row.order.visible ? "виден покупателям" : "скрыт"}</small>
                      </span>
                    </span>
                  </th>
                  <td data-label="В ордере"><strong>{formatPlatinum(row.order.platinum)}</strong><small>×{row.order.quantity}</small></td>
                  <td data-label="Рекомендуется">
                    {#if row.suggestedPrice !== null && row.suggestedPrice !== row.order.platinum}
                      <strong class="suggestion">{formatPlatinum(row.suggestedPrice)}</strong>
                    {:else}
                      <span>{formatPlatinum(row.recommendation?.listPrice ?? null)}</span>
                    {/if}
                    {#if row.priceCheckFailed}<small class="price-check-error">Цена не проверена</small>{/if}
                  </td>
                  <td data-label="Можно продать">
                    {#if inventory}
                      <strong class:mismatch={row.inventory && row.order.quantity > row.inventory.sellableQuantity}>{row.inventory?.sellableQuantity ?? 0}</strong>
                    {:else}
                      <button class="text-button compact" type="button" onclick={onOpenInventory}>Загрузить</button>
                    {/if}
                  </td>
                  <td><span class="order-action-cell"><span class={`health health--${row.health}`}>{row.health === "inventory_mismatch" && inventory ? `Поставить ${row.inventory?.sellableQuantity ?? 0} шт.` : healthLabel(row.health)}</span><button class="text-button compact" type="button" disabled={!account.profile?.verification} onclick={() => beginManualEdit(row)}>Изменить</button></span></td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else if rows.length}
        <div class="sales-empty sales-empty--action"><div><strong>По запросу ничего не найдено</strong><span>Проверьте русское или английское название предмета.</span></div><button class="secondary compact" type="button" onclick={() => (orderQuery = "")}>Очистить поиск</button></div>
      {:else}
        <div class="sales-empty sales-empty--action"><div><strong>Нет ордеров на продажу</strong><span>Найдите предмет, проверьте цену и выставьте его из раздела «Мои предметы».</span></div><button class="secondary compact" type="button" onclick={onBrowseMarket}>Найти предмет</button></div>
      {/if}

      {#if editingOrder}
        <form class="order-editor" aria-labelledby="order-editor-heading" onsubmit={reviewManualEdit}>
          <div class="order-editor__heading">
            <div><h3 id="order-editor-heading" tabindex="-1" bind:this={editorHeading}>Изменить ордер</h3><p>{manualOrderName(editingOrder)}</p></div>
            <button class="text-button compact" type="button" onclick={closeEditor}>Закрыть</button>
          </div>
          <div class="order-editor__fields">
            <label>Цена, платина<input type="number" inputmode="numeric" bind:value={editPlatinum} min="1" max="900000" step="1" required /></label>
            <label>Количество<input type="number" inputmode="numeric" bind:value={editQuantity} min="1" max="9999" step="1" required /></label>
            <label class="compact-check"><input type="checkbox" bind:checked={editVisible} /> Видно покупателям</label>
          </div>
          {#if editError}<p class="editor-error" role="alert">{editError}</p>{/if}
          <div class="order-editor__actions"><button class="compact" type="submit" disabled={applying}>Проверить изменения</button><button class="danger-secondary compact" type="button" disabled={applying} onclick={() => (orderToRemove = editingOrder)}>Снять ордер</button></div>
        </form>
      {/if}

      {#if manualReviewOpen && editingOrder}
        <section class="confirm-panel" aria-labelledby="manual-review-heading">
          <h3 id="manual-review-heading" tabindex="-1" bind:this={manualHeading}>Сохранить изменения?</h3>
          <p><strong>{manualOrderName(editingOrder)}</strong>: {editingOrder.platinum}p → {editPlatinum}p, {editingOrder.quantity} → {editQuantity} шт., {editVisible ? "показывать покупателям" : "скрыть от покупателей"}.</p>
          <div class="confirm-actions"><button type="button" disabled={applying} onclick={applyManualEdit}>Сохранить</button><button class="secondary" type="button" disabled={applying} onclick={() => (manualReviewOpen = false)}>Вернуться</button></div>
        </section>
      {/if}

      {#if orderToRemove}
        <section class="confirm-panel confirm-panel--danger" aria-labelledby="remove-order-heading">
          <h3 id="remove-order-heading" tabindex="-1" bind:this={removeHeading}>Снять ордер?</h3>
          <p>«{manualOrderName(orderToRemove)}» исчезнет с Warframe Market. Вернуть ордер можно будет только новой публикацией.</p>
          <div class="confirm-actions"><button class="danger-primary" type="button" disabled={applying} onclick={removeManualOrder}>Снять ордер</button><button class="secondary" type="button" disabled={applying} onclick={() => (orderToRemove = null)}>Отмена</button></div>
        </section>
      {/if}

      {#if reviewOpen}
        <section class="confirm-panel" aria-labelledby="batch-heading">
          <h3 id="batch-heading" tabindex="-1" bind:this={batchHeading}>Проверьте изменения перед отправкой</h3>
          <ul>
            {#each selectedRows as row}
              {@const change = rowChange(row)}
              <li><strong>{row.item?.displayName ?? "Ордер"}</strong>: {change?.delete ? "закрыть ордер — доступный остаток равен нулю" : `${change?.price !== null ? `${row.order.platinum}p → ${change?.price}p` : "цена без изменений"}${change?.quantity !== null ? `, ${row.order.quantity} → ${change?.quantity} шт.` : ""}`}</li>
            {/each}
          </ul>
          <p>PlatScope отправит только перечисленные изменения и остановится при первой ошибке.</p>
          <div class="confirm-actions"><button type="button" disabled={applying} onclick={applySelectedChanges}>Применить ({selectedRows.length})</button><button class="secondary" type="button" disabled={applying} onclick={closeBatchReview}>Отмена</button></div>
          {#if applyProgress}<span class="status-line">{applyProgress}</span>{/if}
        </section>
      {/if}
    </section>

    <div class="secondary-sections">
      <details class="management-panel">
        <summary>Видимость ордеров <span>{summary.visible} из {summary.total} опубликовано</span></summary>
        <div class="management-panel__body">
          <p>Одним действием скройте ордера, когда не готовы торговать, или опубликуйте их снова.</p>
          <div class="management-panel__actions"><button class="secondary compact" type="button" disabled={!summary.visible || applying} onclick={() => (visibilityIntent = false)}>Скрыть все</button><button class="secondary compact" type="button" disabled={summary.visible === summary.total || applying} onclick={() => (visibilityIntent = true)}>Опубликовать все</button></div>
        </div>
      </details>

      <details class="trade-history" open>
        <summary>Завершённые сделки <span>{historyEvents.length ? `${historyEvents.length} последних` : "пока пусто"}</span></summary>
        <div class="trade-events">
          {#each historyEvents as event (event.id)}
            <article>
              <div class="trade-event__copy"><strong>{eventTitle(event)}</strong><span>{eventItems(event)}{event.partner ? ` · ${event.partner}` : ""}</span><small>{new Date(event.occurredAt).toLocaleString("ru-RU")}</small></div>
              <div class="trade-event__actions">
                {#if event.status === "reconciled" && event.reconciliationJson}
                  {#if wasClosedOnMarket(event)}
                    <span class="done">Продажа учтена WFM</span>
                  {:else}
                    <span class="done">Ордер обновлён</span><button class="text-button compact" type="button" onclick={() => (tradeToUndo = event)}>Отменить</button>
                  {/if}
                {:else if event.status === "ignored" && isSaleTrade(event)}<span class="done">Без изменения ордера</span><button class="text-button compact" type="button" onclick={() => restoreTradeEvent(event)}>Вернуть</button>{:else}<span class="done">Записано</span>{/if}
              </div>
            </article>
          {:else}
            <p class="history-empty">Завершённые обмены текущей игровой сессии появятся здесь. Более ранние сессии Warframe восстановить нельзя.</p>
          {/each}
        </div>
      </details>
    </div>

    {#if visibilityIntent !== null}
      <section class="confirm-panel" aria-labelledby="visibility-heading">
        <h3 id="visibility-heading">{visibilityIntent ? "Опубликовать" : "Скрыть"} все ордера на продажу?</h3>
        <p>{visibilityIntent ? "Ордера снова станут видны покупателям." : "Цены и количество сохранятся, но покупатели временно не увидят ордера."}</p>
        <div class="confirm-actions"><button type="button" disabled={applying} onclick={applyVisibility}>{visibilityIntent ? "Опубликовать" : "Скрыть"}</button><button class="secondary" type="button" disabled={applying} onclick={() => (visibilityIntent = null)}>Отмена</button></div>
      </section>
    {/if}

    {#if tradeToUndo}
      <section class="confirm-panel" aria-labelledby="trade-undo-heading">
        <h3 id="trade-undo-heading">Вернуть состояние ордера до сделки?</h3>
        <p>Используйте отмену, только если PlatScope неверно сопоставил предмет или количество.</p>
        <div class="confirm-actions"><button type="button" disabled={applying} onclick={() => tradeToUndo && undoTrade(tradeToUndo)}>Вернуть ордер</button><button class="secondary" type="button" disabled={applying} onclick={() => (tradeToUndo = null)}>Отмена</button></div>
      </section>
    {/if}
  {/if}
</section>

<style>
  .sales-workspace { display: grid; gap: .65rem; }
  .sales-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 1rem; padding: .1rem; }
  .sales-header h2 { margin-bottom: .12rem; font-size: 1.08rem; }
  .sales-header p { max-width: 65ch; margin: 0; color: var(--text-muted); font-size: .76rem; }
  .sales-header__actions, .account-panel__actions, .orders-toolbar__actions, .confirm-actions, .trade-event__actions, .management-panel__actions { display: flex; align-items: center; gap: .4rem; flex-wrap: wrap; }
  button.compact { min-height: 2rem; padding: .3rem .6rem; font-size: .8125rem; }
  .status-line { min-height: .9rem; padding: 0 .1rem; color: var(--text-muted); font-size: .75rem; }
  .status-line:empty { display: none; }
  .inline-error { margin: 0; border-radius: .45rem; padding: .5rem .6rem; background: var(--danger-soft); color: var(--danger); font-size: .75rem; font-weight: 650; }
  .account-panel { display: flex; align-items: center; justify-content: space-between; gap: .8rem; border: 1px solid var(--border); border-radius: .55rem; padding: .55rem .65rem; background: var(--surface-2); }
  .account-panel h3, .account-panel p { margin: 0; }
  .account-panel h3 { font-size: .82rem; }
  .account-panel p { margin-block-start: .1rem; color: var(--text-muted); font-size: .75rem; }
  .connect-panel { display: grid; justify-items: start; gap: .65rem; border: 1px solid var(--border); border-radius: .6rem; padding: .8rem; background: var(--surface-1); box-shadow: var(--shadow-sm); }
  .connect-panel__copy h3, .connect-panel__copy p { margin: 0; }
  .connect-panel__copy h3 { font-size: .92rem; }
  .connect-panel__copy p { margin-block-start: .12rem; color: var(--text-muted); font-size: .75rem; }
  .connect-fields { display: grid; grid-template-columns: repeat(2, minmax(12rem, 1fr)); gap: .6rem; width: min(100%, 38rem); }
  .connect-fields label { display: grid; gap: .22rem; color: var(--text-muted); font-size: .75rem; font-weight: 700; }
  .connect-fields input { min-width: 0; min-height: 2.1rem; width: 100%; border: 1px solid var(--border); border-radius: .45rem; padding: .35rem .55rem; background: oklch(0.995 0.004 84); color: var(--text); }
  .security-details { width: min(100%, 38rem); color: var(--text-muted); font-size: .75rem; }
  .security-details summary { cursor: pointer; font-weight: 700; }
  .security-details p { margin: .4rem 0 0; line-height: 1.45; }
  .sales-empty { margin: 0; border: 1px solid var(--border); border-radius: .6rem; padding: .85rem; background: var(--surface-1); color: var(--text-muted); font-size: .78rem; }
  .sales-empty--action { display: flex; align-items: center; justify-content: space-between; gap: .8rem; }
  .sales-empty strong, .sales-empty span { display: block; }
  .sales-empty strong { margin-bottom: .12rem; color: var(--text); font-size: .85rem; }
  .sales-summary { display: grid; grid-template-columns: repeat(3, minmax(8rem, 1fr)); margin: 0; border: 1px solid var(--border); border-radius: .6rem; background: var(--surface-1); box-shadow: var(--shadow-sm); overflow: clip; }
  .sales-summary div { min-width: 0; padding: .55rem .7rem; border-inline-end: 1px solid var(--border); }
  .sales-summary div:last-child { border-inline-end: 0; }
  .sales-summary dt { color: var(--text-muted); font-size: .75rem; }
  .sales-summary dd { margin: .08rem 0 0; font-size: 1rem; font-weight: 800; font-variant-numeric: tabular-nums; }
  .sales-summary dd small { color: var(--text-muted); font-size: .75rem; font-weight: 600; }
  .sales-summary .attention dd { color: var(--danger); }
  .verification-note { display: flex; align-items: center; justify-content: space-between; gap: .6rem; border: 1px solid color-mix(in oklch, var(--gold), var(--border) 55%); border-radius: .5rem; padding: .45rem .6rem; background: var(--accent-soft); color: var(--accent-strong); font-size: .75rem; font-weight: 650; }
  .priority-panel, .orders-panel { border: 1px solid var(--border); border-radius: .6rem; background: var(--surface-1); box-shadow: var(--shadow-sm); overflow: clip; }
  .priority-panel { border-color: color-mix(in oklch, var(--gold), var(--border) 55%); }
  .section-heading, .orders-toolbar { gap: .75rem; padding: .55rem .7rem; background: var(--surface-2); }
  .section-heading { display: flex; align-items: center; justify-content: space-between; }
  .orders-toolbar { display: grid; grid-template-columns: minmax(9rem, .35fr) minmax(15rem, 1fr) auto; align-items: end; }
  .section-heading h3, .orders-toolbar h3 { margin: 0; font-size: .9rem; }
  .section-kicker { margin: 0 0 .05rem; color: var(--accent-strong); font-size: .75rem; font-weight: 800; letter-spacing: .07em; text-transform: uppercase; }
  .count-badge { min-width: 1.55rem; border-radius: 999px; padding: .16rem .42rem; background: var(--accent); color: var(--surface-1); font-size: .75rem; font-weight: 800; text-align: center; }
  .section-hint { margin: 0; border-block-start: 1px solid var(--border); padding: .45rem .7rem; color: var(--text-muted); font-size: .75rem; }
  .orders-toolbar > div:first-child span { color: var(--text-muted); font-size: .75rem; }
  .order-search { display: grid; gap: .18rem; min-width: 0; color: var(--text-muted); font-size: .75rem; font-weight: 700; }
  .order-search input { min-width: 0; min-height: 1.8rem; width: 100%; border: 1px solid var(--border); border-radius: .45rem; padding: .28rem .5rem; background: var(--surface-1); color: var(--text); font-size: .75rem; }
  .orders-toolbar__actions { justify-content: flex-end; }
  .compact-check { display: inline-flex; align-items: center; gap: .35rem; min-height: 1.8rem; font-size: .75rem; font-weight: 650; }
  .compact-check input, .shift-table input { width: 1rem; height: 1rem; accent-color: var(--accent); }
  .shift-table-wrap { max-height: 26rem; overflow: auto; border-top: 1px solid var(--border); }
  .shift-table { width: 100%; table-layout: fixed; border-collapse: collapse; font-size: .875rem; }
  .shift-table .col-check { width: 2.2rem; }
  .shift-table .col-item { width: 31%; }
  .shift-table .col-order { width: 12%; }
  .shift-table .col-market { width: 14%; }
  .shift-table .col-stock { width: 13%; }
  .shift-table .col-status { width: 24%; }
  .shift-table th, .shift-table td { border-bottom: 1px solid var(--border); padding: .42rem .55rem; text-align: start; vertical-align: middle; }
  .shift-table th:nth-child(5), .shift-table td:nth-child(5) { display: table-cell; }
  .shift-table thead th { position: sticky; top: 0; z-index: 1; background: var(--surface-2); color: var(--text-muted); font-size: .75rem; font-weight: 650; }
  .shift-table tbody tr:hover { background: var(--surface-hover); }
  .shift-table td strong, .shift-table td small { display: block; font-variant-numeric: tabular-nums; }
  .shift-table td small { margin-top: .15rem; color: var(--text-muted); font-size: .75rem; }
  .item-cell { display: flex; align-items: center; gap: .45rem; min-width: 0; }
  .item-cell > span { min-width: 0; }
  .item-cell img { width: 2rem; height: 2rem; border: 1px solid var(--border); border-radius: .35rem; object-fit: contain; background: var(--surface-2); }
  .item-cell strong, .item-cell small { display: block; }
  .item-cell strong { display: -webkit-box; overflow: hidden; line-height: 1.2; text-transform: none; letter-spacing: 0; overflow-wrap: anywhere; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; }
  .item-cell .item-name-en { display: block; margin-block-start: .08rem; overflow: hidden; color: var(--text-muted); font-size: .75rem; font-weight: 600; line-height: 1.2; text-overflow: ellipsis; text-transform: none; white-space: nowrap; }
  .item-cell small { margin-top: .1rem; color: var(--text-subtle); font-size: .75rem; font-weight: 500; }
  .suggestion { color: var(--success); }
  .mismatch { color: var(--danger); }
  .health { display: inline-flex; border: 1px solid var(--border); border-radius: 999px; padding: .16rem .42rem; background: var(--surface-2); font-size: .75rem; font-weight: 750; white-space: nowrap; }
  .health--inventory_mismatch, .health--underpriced { border-color: color-mix(in oklch, var(--danger), var(--border) 60%); background: var(--danger-soft); color: var(--danger); }
  .health--overpriced { border-color: color-mix(in oklch, var(--gold), var(--border) 55%); background: var(--accent-soft); color: var(--accent-strong); }
  .health--price_check_failed { border-color: color-mix(in oklch, var(--danger), var(--border) 55%); background: color-mix(in oklch, var(--danger), transparent 90%); color: var(--danger); }
  .health--healthy { border-color: color-mix(in oklch, var(--success), var(--border) 60%); background: var(--success-soft); color: var(--success); }
  .order-action-cell { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: .35rem; }
  .order-editor { display: grid; gap: .55rem; margin: .55rem; border: 1px solid var(--border-strong); border-radius: .5rem; padding: .65rem; background: var(--surface-2); }
  .order-editor__heading { display: flex; align-items: flex-start; justify-content: space-between; gap: .6rem; }
  .order-editor__heading h3, .order-editor__heading p { margin: 0; }
  .order-editor__heading h3 { font-size: .86rem; }
  .order-editor__heading p { margin-top: .08rem; color: var(--text-muted); font-size: .75rem; }
  .order-editor__fields { display: grid; grid-template-columns: minmax(7rem, 10rem) minmax(7rem, 10rem) minmax(10rem, 1fr); align-items: end; gap: .5rem; }
  .order-editor__fields > label:not(.compact-check) { display: grid; gap: .2rem; color: var(--text-muted); font-size: .75rem; font-weight: 650; }
  .order-editor__fields input[type="number"] { min-height: 2rem; width: 100%; border: 1px solid var(--border-strong); border-radius: .4rem; padding: .35rem .5rem; background: var(--surface-1); color: var(--text); font-size: .875rem; }
  .order-editor__actions { display: flex; align-items: center; gap: .4rem; }
  .editor-error { margin: 0; color: var(--danger); font-size: .75rem; font-weight: 650; }
  .confirm-panel { margin: 0; border: 1px solid var(--accent); border-radius: .55rem; padding: .7rem; background: var(--accent-soft); }
  .confirm-panel--danger { border-color: var(--danger); background: var(--danger-soft); }
  .orders-panel .confirm-panel { margin: .55rem; }
  .confirm-panel h3 { margin-bottom: .25rem; font-size: .9rem; }
  .confirm-panel p, .confirm-panel li { font-size: .75rem; line-height: 1.4; }
  .confirm-panel ul { margin: .4rem 0; padding-inline-start: 1.2rem; }
  .confirm-panel .status-line { padding: .35rem 0 0; }
  .trade-events article { display: flex; align-items: center; justify-content: space-between; gap: .8rem; padding: .55rem .7rem; border-top: 1px solid var(--border); }
  .trade-events article.pending { background: color-mix(in oklch, var(--accent-soft), transparent 55%); }
  .trade-event__copy { min-width: 0; }
  .trade-event__copy strong, .trade-event__copy span, .trade-event__copy small { display: block; }
  .trade-event__copy strong { font-size: .78rem; }
  .trade-event__copy span { margin-top: .1rem; font-size: .75rem; overflow-wrap: anywhere; }
  .trade-event__copy small { margin-top: .12rem; color: var(--text-subtle); font-size: .75rem; }
  .manual { color: var(--danger); font-size: .75rem; font-weight: 700; }
  .done { color: var(--success); font-size: .75rem; font-weight: 700; }
  .secondary-sections { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .65rem; }
  .management-panel, .trade-history { min-width: 0; border: 1px solid var(--border); border-radius: .55rem; background: var(--surface-1); }
  .management-panel summary, .trade-history summary { min-height: 2.2rem; padding: .55rem .65rem; cursor: pointer; font-size: .77rem; font-weight: 800; }
  .management-panel summary span, .trade-history summary span { margin-inline-start: .3rem; color: var(--text-muted); font-size: .75rem; font-weight: 600; }
  .management-panel__body { border-top: 1px solid var(--border); padding: .6rem .7rem; }
  .management-panel__body p { margin-bottom: .5rem; color: var(--text-muted); font-size: .75rem; }
  .trade-history .trade-events { max-height: 16rem; overflow: auto; }
  .history-empty { margin: 0; border-top: 1px solid var(--border); padding: .7rem; color: var(--text-muted); font-size: .75rem; }
  .sales-workspace .health { font-size: .75rem; white-space: normal; border-radius: .4rem; }
  .sales-workspace .item-cell small, .sales-workspace .item-name-en { font-size: .75rem; color: var(--text-muted); }
  .sales-workspace .order-editor__fields > label { font-size: .8125rem; }
  .sales-workspace .sales-summary dt { font-size: .75rem; }
  .sales-workspace .health--underpriced { background: var(--accent-soft); color: var(--accent-strong); border-color: var(--border-strong); }
  @media (max-width: 60rem) { .secondary-sections { grid-template-columns: minmax(0, 1fr); } }
  @media (max-width: 50rem) { .sales-header, .account-panel, .sales-empty--action, .verification-note { align-items: stretch; flex-direction: column; } .sales-summary, .order-editor__fields, .connect-fields, .orders-toolbar { grid-template-columns: minmax(0, 1fr); } .orders-toolbar__actions { justify-content: flex-start; } .sales-summary div { border-inline-end: 0; border-block-end: 1px solid var(--border); } .sales-summary div:last-child { border-block-end: 0; } .shift-table-wrap { max-height: none; } .trade-events article { align-items: flex-start; flex-direction: column; } }
  @media (max-width: 46rem) {
    .shift-table, .shift-table tbody { display: block; }
    .shift-table colgroup, .shift-table thead { display: none; }
    .shift-table tbody tr { display: grid; grid-template-columns: 1.5rem minmax(0, 1fr) minmax(6.5rem, auto); gap: .35rem .55rem; margin: 0; border-radius: 0; padding: .6rem; background: var(--surface-1); }
    .shift-table tbody tr:hover { background: var(--surface-hover); }
    .shift-table tbody th, .shift-table tbody td, .shift-table tbody td:nth-child(5) { display: block; width: auto; min-width: 0; border: 0; padding: 0; text-align: start; }
    .shift-table tbody td::before { content: none; }
    .shift-table tbody td[data-label]::before { content: attr(data-label); display: block; color: var(--text-muted); font-size: .75rem; margin-bottom: .2rem; }
    .shift-table tbody td:nth-child(1) { grid-column: 1; grid-row: 1; }
    .shift-table tbody th:nth-child(2) { grid-column: 2 / 4; grid-row: 1; }
    .shift-table tbody td:nth-child(3) { grid-column: 2; grid-row: 2; }
    .shift-table tbody td:nth-child(4) { grid-column: 3; grid-row: 2; }
    .shift-table tbody td:nth-child(5) { grid-column: 2; grid-row: 3; }
    .shift-table tbody td:nth-child(6) { grid-column: 3; grid-row: 3; align-self: end; }
    .health { white-space: normal; }
  }
</style>
