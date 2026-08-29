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
  import type { InventoryView } from "./inventory";
  import { formatPlatinum, type LivePricingResult, type PriceRecommendation } from "./market";
  import {
    buildTradeShiftRows,
    normalizeTradeName,
    planTradeReconciliation,
    recommendationIdentity,
    updateInput,
    type OrderHealth,
    type TradeEvent,
    type TradeReconciliationAction,
    type TradeShiftRow,
  } from "./tradeShift";

  export let onOpenAccount: () => void;
  export let onOpenInventory: () => void;

  let account: AccountView | null = null;
  let inventory: InventoryView | null = null;
  let events: TradeEvent[] = [];
  let recommendations = new Map<string, PriceRecommendation | null>();
  let loading = true;
  let refreshingLive = false;
  let stopLiveRefresh = false;
  let liveProgress = "";
  let errorMessage = "";
  let actionMessage = "";
  let showAll = false;
  let selectedIds = new Set<string>();
  let reviewOpen = false;
  let visibilityIntent: boolean | null = null;
  let tradeToApply: TradeEvent | null = null;
  let tradeToUndo: TradeEvent | null = null;
  let applying = false;
  let applyProgress = "";

  $: rows = account
    ? buildTradeShiftRows(account, inventory, recommendations)
    : [];
  $: visibleRows = showAll ? rows : rows.filter((row) => row.health !== "healthy");
  $: actionableRows = rows.filter((row) => row.needsAction && rowChange(row) !== null);
  $: selectedRows = actionableRows.filter((row) => selectedIds.has(row.order.id));
  $: pendingEvents = events.filter((event) => event.status === "pending");
  $: summary = summarize(rows);

  onMount(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    void loadAll();
    void Promise.all([
      listen("trade-detected", () => void loadEvents()),
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
      const [nextAccount, nextInventory, nextEvents] = await Promise.all([
        invoke<AccountView>("account_status"),
        invoke<InventoryView | null>("load_inventory"),
        invoke<TradeEvent[]>("trade_events"),
      ]);
      account = nextAccount;
      inventory = nextInventory;
      events = nextEvents;
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

  async function loadEvents(): Promise<void> {
    try {
      events = await invoke<TradeEvent[]>("trade_events");
      actionMessage = "Игра подтвердила сделку. Проверьте изменение ордера.";
    } catch {
      actionMessage = "Сделка обнаружена, но журнал пока не открылся.";
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
    const candidates = rows.filter((row) => row.key);
    const next = new Map(recommendations);
    for (let index = 0; index < candidates.length; index += 1) {
      if (stopLiveRefresh) break;
      const row = candidates[index];
      liveProgress = `${index + 1} из ${candidates.length}: ${row.item?.displayName ?? "ордер"}`;
      try {
        const result = await invoke<LivePricingResult | null>("live_price_current_variant", {
          key: row.key,
          itemKind: row.itemKind,
        });
        if (result && row.key) next.set(recommendationIdentity(row.key), result.recommendation);
      } catch {
        // One unavailable item must not discard successful checks for the rest of the shift.
      }
      recommendations = new Map(next);
    }
    liveProgress = stopLiveRefresh ? "Проверка остановлена" : `Проверено: ${candidates.length}`;
    refreshingLive = false;
    selectSuggestedByDefault(buildTradeShiftRows(account, inventory, next));
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

  async function applyTrade(event: TradeEvent): Promise<void> {
    if (!account || applying) return;
    const plan = planTradeReconciliation(event, account);
    if (plan.actions.length !== 1 || plan.unmatched.length || plan.unsafe.length) return;
    applying = true;
    const completed: TradeReconciliationAction[] = [];
    for (const action of plan.actions) {
      try {
        if (action.kind === "delete") {
          await invoke<AccountOrder>("account_delete_listing", {
            id: action.before.id,
            confirmed: true,
          });
        } else {
          await invoke<AccountOrder>("account_update_listing", {
            id: action.before.id,
            input: updateInput({ quantity: action.before.quantity - action.soldQuantity }),
            confirmed: true,
          });
        }
        completed.push(action);
      } catch (error) {
        errorMessage = `${action.itemName}: ${accountActionErrorMessage(String(error))}`;
        break;
      }
    }
    if (completed.length === plan.actions.length) {
      await invoke<boolean>("trade_event_reconciled", {
        id: event.id,
        orderId: completed.length === 1 ? completed[0].before.id : null,
        reconciliationJson: JSON.stringify(completed),
      });
      actionMessage = "Продажа отражена в ордерах.";
    } else if (completed.length) {
      actionMessage = `Обновлено ${completed.length} из ${plan.actions.length}. Проверьте оставшиеся ордера вручную.`;
    }
    applying = false;
    tradeToApply = null;
    await reloadAccount();
    await loadEvents();
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
    await invoke<boolean>("trade_event_restore", { id: event.id });
    await loadEvents();
  }

  async function reloadAccount(): Promise<void> {
    try {
      account = await invoke<AccountView>("account_status");
      recommendations = new Map();
      await loadSavedPrices();
    } catch (error) {
      errorMessage = accountActionErrorMessage(String(error));
    }
  }

  function healthLabel(health: OrderHealth): string {
    return ({
      inventory_mismatch: "Количество не сходится",
      overpriced: "Цена выше рынка",
      underpriced: "Можно не сливать",
      stale: "Давно не проверялся",
      hidden: "Скрыт",
      healthy: "В порядке",
      unknown: "Нет надёжной цены",
    } satisfies Record<OrderHealth, string>)[health];
  }

  function eventTitle(event: TradeEvent): string {
    if (event.platinumReceived > 0 && event.platinumGiven === 0) return `Продажа · +${event.platinumReceived}p`;
    if (event.platinumGiven > 0 && event.platinumReceived === 0) return `Покупка · −${event.platinumGiven}p`;
    return "Обмен предметами";
  }

  function soldItems(event: TradeEvent): string {
    return event.givenItems.map((item) => `${localizedTradeName(item.name)} ×${item.quantity}`).join(", ") || "без предметов";
  }

  function localizedTradeName(name: string): string {
    const matched = Object.values(account?.orderItems ?? {}).find(
      (item) => normalizeTradeName(item.displayNameEn) === normalizeTradeName(name),
    );
    return matched?.displayName ?? name;
  }

  function eventCanApply(event: TradeEvent): boolean {
    if (!account) return false;
    const plan = planTradeReconciliation(event, account);
    return plan.actions.length === 1 && plan.unmatched.length === 0 && plan.unsafe.length === 0;
  }

  function summarize(source: TradeShiftRow[]) {
    return {
      total: source.length,
      visible: source.filter((row) => row.order.visible).length,
      price: source.filter((row) => row.health === "overpriced" || row.health === "underpriced" || row.health === "stale").length,
      inventory: source.filter((row) => row.health === "inventory_mismatch").length,
    };
  }
</script>

<section class="shift" aria-labelledby="shift-heading" aria-busy={loading || applying}>
  <header class="shift__header">
    <div>
      <p class="eyebrow">Перед торговлей</p>
      <h2 id="shift-heading">Торговая смена</h2>
      <p>Проблемные ордера и подтверждённые игрой сделки — без повторного просмотра всего списка.</p>
    </div>
    {#if account?.connected}
      <div class="shift__actions">
        <button class="secondary compact" type="button" disabled={loading || applying} onclick={loadAll}>Обновить ордера</button>
        {#if refreshingLive}
          <button class="secondary compact" type="button" onclick={() => (stopLiveRefresh = true)}>Остановить</button>
        {:else}
          <button class="compact" type="button" disabled={!rows.length} onclick={refreshCurrentPrices}>Проверить цены сейчас</button>
        {/if}
      </div>
    {/if}
  </header>

  <div class="status-line" aria-live="polite">
    {#if liveProgress}{liveProgress}{:else if actionMessage}{actionMessage}{/if}
  </div>

  {#if errorMessage}<p class="inline-error" role="alert">{errorMessage}</p>{/if}

  {#if loading}
    <p class="shift__empty">Собираем ордера, остатки и цены…</p>
  {:else if !account?.connected}
    <div class="shift__empty">
      <strong>Подключите Warframe Market</strong>
      <span>После подключения здесь появятся только ордера, которым действительно нужно внимание.</span>
      <button class="compact" type="button" onclick={onOpenAccount}>Подключить аккаунт</button>
    </div>
  {:else}
    <dl class="shift-summary">
      <div><dt>Ордеров на продажу</dt><dd>{summary.total}</dd></div>
      <div><dt>Опубликовано</dt><dd>{summary.visible}</dd></div>
      <div class:attention={summary.price > 0}><dt>Проверить цену</dt><dd>{summary.price}</dd></div>
      <div class:attention={summary.inventory > 0}><dt>Не сходится остаток</dt><dd>{summary.inventory}</dd></div>
      <div class:attention={pendingEvents.length > 0}><dt>Сделки ждут сверки</dt><dd>{pendingEvents.length}</dd></div>
    </dl>

    <div class="shift-toolbar">
      <label class="compact-check"><input type="checkbox" bind:checked={showAll} /> Показать исправные</label>
      <span>{showAll ? `${visibleRows.length} ордеров` : `${visibleRows.length} требуют внимания`}</span>
      <div class="shift-toolbar__actions">
        <button class="secondary compact" type="button" disabled={!summary.visible || applying} onclick={() => (visibilityIntent = false)}>Скрыть все</button>
        <button class="secondary compact" type="button" disabled={summary.visible === summary.total || applying} onclick={() => (visibilityIntent = true)}>Опубликовать все</button>
        <button class="compact" type="button" disabled={!selectedRows.length || applying} onclick={() => (reviewOpen = true)}>Проверить изменения ({selectedRows.length})</button>
      </div>
    </div>

    {#if visibleRows.length}
      <div class="shift-table-wrap">
        <table class="shift-table">
          <caption class="sr-only">Состояние ордеров Warframe Market</caption>
          <colgroup><col class="col-check" /><col class="col-item" /><col class="col-order" /><col class="col-market" /><col class="col-stock" /><col class="col-status" /></colgroup>
          <thead><tr><th class="check-column"><span class="sr-only">Выбрать</span></th><th>Предмет</th><th>Ордер</th><th>Рынок</th><th>Остаток</th><th>Статус</th></tr></thead>
          <tbody>
            {#each visibleRows as row (row.order.id)}
              <tr class:row-attention={row.needsAction}>
                <td>
                  {#if rowChange(row)}
                    <input aria-label={`Выбрать ${row.item?.displayName ?? "ордер"}`} type="checkbox" checked={selectedIds.has(row.order.id)} onchange={() => toggleSelected(row.order.id)} />
                  {/if}
                </td>
                <th scope="row">
                  <span class="item-cell">
                    {#if row.item?.imageUrl}<img src={row.item.imageUrl} alt="" loading="lazy" />{/if}
                    <span><strong>{row.item?.displayName ?? "Неизвестный предмет"}</strong><small>{row.order.visible ? "опубликован" : "скрыт"}</small></span>
                  </span>
                </th>
                <td><strong>{formatPlatinum(row.order.platinum)}</strong><small>×{row.order.quantity}</small></td>
                <td>
                  {#if row.suggestedPrice !== null && row.suggestedPrice !== row.order.platinum}
                    <strong class="suggestion">→ {formatPlatinum(row.suggestedPrice)}</strong>
                  {:else}
                    <span>{formatPlatinum(row.recommendation?.listPrice ?? null)}</span>
                  {/if}
                </td>
                <td>
                  {#if inventory}
                    <strong class:mismatch={row.inventory && row.order.quantity > row.inventory.sellableQuantity}>{row.inventory?.sellableQuantity ?? 0}</strong>
                    <small>можно продать</small>
                  {:else}
                    <button class="text-button compact" type="button" onclick={onOpenInventory}>Нет снимка</button>
                  {/if}
                </td>
                <td><span class={`health health--${row.health}`}>{row.health === "inventory_mismatch" && inventory ? `Остаток ${row.inventory?.sellableQuantity ?? 0}, в ордере ${row.order.quantity}` : healthLabel(row.health)}</span></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <p class="all-good">Все опубликованные ордера совпадают с остатками и сохранённой оценкой рынка.</p>
    {/if}

    {#if reviewOpen}
      <section class="confirm-panel" aria-labelledby="batch-heading">
        <h3 id="batch-heading">Проверьте изменения</h3>
        <ul>
          {#each selectedRows as row}
            {@const change = rowChange(row)}
            <li><strong>{row.item?.displayName ?? "Ордер"}</strong>: {change?.delete ? "закрыть ордер — доступный остаток равен нулю" : `${change?.price !== null ? `${row.order.platinum}p → ${change?.price}p` : "цена без изменений"}${change?.quantity !== null ? `, ${row.order.quantity} → ${change?.quantity} шт.` : ""}`}</li>
          {/each}
        </ul>
        <p>PlatScope отправит только показанные изменения. При первой ошибке пакет остановится.</p>
        <div class="confirm-actions"><button type="button" disabled={applying} onclick={applySelectedChanges}>Применить ({selectedRows.length})</button><button class="secondary" type="button" disabled={applying} onclick={() => (reviewOpen = false)}>Отмена</button></div>
        {#if applyProgress}<span class="status-line">{applyProgress}</span>{/if}
      </section>
    {/if}

    {#if visibilityIntent !== null}
      <section class="confirm-panel" aria-labelledby="visibility-heading">
        <h3 id="visibility-heading">{visibilityIntent ? "Опубликовать" : "Скрыть"} все sell-ордера?</h3>
        <p>{visibilityIntent ? "Ордера снова станут видны покупателям." : "Цены и количество сохранятся; покупатели временно не увидят ордера."}</p>
        <div class="confirm-actions"><button type="button" disabled={applying} onclick={applyVisibility}>{visibilityIntent ? "Опубликовать" : "Скрыть"}</button><button class="secondary" type="button" disabled={applying} onclick={() => (visibilityIntent = null)}>Отмена</button></div>
      </section>
    {/if}

    <details class="trade-log" open={pendingEvents.length > 0}>
      <summary>Сделки из игры <span>{pendingEvents.length ? `${pendingEvents.length} ждут сверки` : "всё сверено"}</span></summary>
      <p class="trade-log__hint">EE.log читается только вперёд. Ордер меняется лишь после вашего подтверждения.</p>
      <div class="trade-events">
        {#each events.slice(0, 8) as event (event.id)}
          <article class:pending={event.status === "pending"}>
            <div class="trade-event__copy">
              <strong>{eventTitle(event)}</strong>
              <span>{soldItems(event)}{event.partner ? ` · ${event.partner}` : ""}</span>
              <small>{new Date(event.occurredAt).toLocaleString("ru-RU")}</small>
            </div>
            <div class="trade-event__actions">
              {#if event.status === "pending"}
                {#if eventCanApply(event)}
                  <button class="compact" type="button" onclick={() => (tradeToApply = event)}>Отразить в ордере</button>
                {:else}
                  <span class="manual">Нужна ручная сверка</span>
                {/if}
                <button class="text-button compact" type="button" onclick={() => ignoreTrade(event)}>Не учитывать</button>
              {:else if event.status === "reconciled" && event.reconciliationJson}
                <span class="done">Отражено</span>
                <button class="text-button compact" type="button" onclick={() => (tradeToUndo = event)}>Отменить</button>
              {:else}
                <span class="done">Пропущено</span>
                <button class="text-button compact" type="button" onclick={() => restoreTradeEvent(event)}>Вернуть</button>
              {/if}
            </div>
          </article>
        {:else}
          <p class="all-good">Новых сделок в этой сессии ещё нет.</p>
        {/each}
      </div>
    </details>

    {#if tradeToApply}
      {@const plan = account ? planTradeReconciliation(tradeToApply, account) : null}
      <section class="confirm-panel" aria-labelledby="trade-apply-heading">
        <h3 id="trade-apply-heading">Отразить продажу в ордере?</h3>
        <ul>{#each plan?.actions ?? [] as action}<li><strong>{action.itemName}</strong>: {action.kind === "delete" ? "закрыть ордер" : `${action.before.quantity} → ${action.before.quantity - action.soldQuantity} шт.`}</li>{/each}</ul>
        <p>Это меняет только ваш ордер WFM. Инвентарь обновится своим обычным источником.</p>
        <div class="confirm-actions"><button type="button" disabled={applying} onclick={() => tradeToApply && applyTrade(tradeToApply)}>Подтвердить</button><button class="secondary" type="button" disabled={applying} onclick={() => (tradeToApply = null)}>Отмена</button></div>
      </section>
    {/if}

    {#if tradeToUndo}
      <section class="confirm-panel" aria-labelledby="trade-undo-heading">
        <h3 id="trade-undo-heading">Вернуть состояние ордера до сделки?</h3>
        <p>Используйте отмену, только если PlatScope неверно сопоставил предмет или количество.</p>
        <div class="confirm-actions"><button type="button" disabled={applying} onclick={() => tradeToUndo && undoTrade(tradeToUndo)}>Вернуть</button><button class="secondary" type="button" disabled={applying} onclick={() => (tradeToUndo = null)}>Отмена</button></div>
      </section>
    {/if}
  {/if}
</section>

<style>
  .shift { border: 1px solid var(--border); border-radius: .7rem; margin-block-end: .8rem; background: var(--surface-1); box-shadow: var(--shadow-sm); overflow: clip; }
  .shift__header { display: flex; align-items: flex-start; justify-content: space-between; gap: 1rem; padding: .8rem .9rem .65rem; }
  .shift__header h2 { margin-bottom: .15rem; font-size: 1.08rem; }
  .shift__header p:not(.eyebrow) { margin: 0; color: var(--text-muted); font-size: .78rem; }
  .eyebrow { margin: 0 0 .15rem; color: var(--accent); font-size: .65rem; font-weight: 800; letter-spacing: .1em; text-transform: uppercase; }
  .shift__actions, .shift-toolbar__actions, .confirm-actions, .trade-event__actions { display: flex; align-items: center; gap: .4rem; flex-wrap: wrap; }
  button.compact { min-height: 1.8rem; padding: .22rem .5rem; font-size: .75rem; }
  .status-line { min-height: 1rem; padding: 0 .9rem .35rem; color: var(--text-muted); font-size: .72rem; }
  .inline-error { margin: 0 .9rem .55rem; border-radius: .4rem; padding: .45rem .55rem; background: var(--danger-soft); color: var(--danger); font-size: .76rem; font-weight: 650; }
  .shift__empty { display: flex; align-items: center; gap: .65rem; margin: 0; padding: .9rem; border-top: 1px solid var(--border); color: var(--text-muted); font-size: .8rem; }
  .shift__empty strong { color: var(--text); }
  .shift-summary { display: grid; grid-template-columns: repeat(5, minmax(7rem, 1fr)); margin: 0; border-block: 1px solid var(--border); background: var(--surface-2); }
  .shift-summary div { min-width: 0; padding: .55rem .7rem; border-inline-end: 1px solid var(--border); }
  .shift-summary div:last-child { border-inline-end: 0; }
  .shift-summary dt { color: var(--text-muted); font-size: .65rem; }
  .shift-summary dd { margin: .1rem 0 0; font-size: 1.05rem; font-weight: 800; font-variant-numeric: tabular-nums; }
  .shift-summary .attention dd { color: var(--danger); }
  .shift-toolbar { display: flex; align-items: center; gap: .7rem; padding: .55rem .7rem; }
  .shift-toolbar > span { color: var(--text-muted); font-size: .72rem; }
  .shift-toolbar__actions { margin-inline-start: auto; }
  .compact-check { display: inline-flex; align-items: center; gap: .35rem; font-size: .75rem; font-weight: 650; }
  .compact-check input, .shift-table input { width: 1rem; height: 1rem; accent-color: var(--accent); }
  .shift-table-wrap { max-height: 23rem; overflow: auto; border-top: 1px solid var(--border); }
  .shift-table { width: 100%; table-layout: fixed; border-collapse: collapse; font-size: .76rem; }
  .shift-table .col-check { width: 2.2rem; }
  .shift-table .col-item { width: 31%; }
  .shift-table .col-order { width: 12%; }
  .shift-table .col-market { width: 12%; }
  .shift-table .col-stock { width: 15%; }
  .shift-table .col-status { width: 23%; }
  .shift-table th, .shift-table td { border-bottom: 1px solid var(--border); padding: .42rem .55rem; text-align: start; vertical-align: middle; }
  .shift-table th:nth-child(5), .shift-table td:nth-child(5) { display: table-cell; }
  .shift-table thead th { position: sticky; top: 0; z-index: 1; background: var(--surface-2); color: var(--text-muted); font-size: .63rem; letter-spacing: .05em; text-transform: uppercase; }
  .shift-table tbody tr:hover { background: var(--surface-hover); }
  .shift-table .check-column { width: 2.1rem; }
  .shift-table td strong, .shift-table td small { display: block; font-variant-numeric: tabular-nums; }
  .shift-table td small { margin-top: .08rem; color: var(--text-subtle); font-size: .65rem; }
  .item-cell { display: flex; align-items: center; gap: .45rem; min-width: 0; }
  .item-cell > span { min-width: 0; }
  .item-cell img { width: 2rem; height: 2rem; border: 1px solid var(--border); border-radius: .35rem; object-fit: contain; background: var(--surface-2); }
  .item-cell strong, .item-cell small { display: block; }
  .item-cell strong { display: -webkit-box; overflow: hidden; line-height: 1.2; text-transform: none; letter-spacing: 0; overflow-wrap: anywhere; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; }
  .item-cell small { margin-top: .1rem; color: var(--text-subtle); font-size: .64rem; font-weight: 500; }
  .suggestion { color: var(--success); }
  .mismatch { color: var(--danger); }
  .health { display: inline-flex; border: 1px solid var(--border); border-radius: 999px; padding: .16rem .42rem; background: var(--surface-2); font-size: .65rem; font-weight: 750; white-space: nowrap; }
  .health--inventory_mismatch, .health--underpriced { border-color: color-mix(in oklch, var(--danger), var(--border) 60%); background: var(--danger-soft); color: var(--danger); }
  .health--overpriced, .health--stale { border-color: color-mix(in oklch, var(--gold), var(--border) 55%); background: var(--accent-soft); color: var(--accent-strong); }
  .health--healthy { border-color: color-mix(in oklch, var(--success), var(--border) 60%); background: var(--success-soft); color: var(--success); }
  .all-good { margin: 0; padding: .8rem .9rem; border-top: 1px solid var(--border); color: var(--success); font-size: .78rem; font-weight: 650; }
  .confirm-panel { margin: .55rem .7rem .7rem; border: 1px solid var(--accent); border-radius: .55rem; padding: .7rem; background: var(--accent-soft); }
  .confirm-panel h3 { font-size: .9rem; }
  .confirm-panel p, .confirm-panel li { font-size: .75rem; line-height: 1.4; }
  .confirm-panel ul { margin: .45rem 0; padding-inline-start: 1.2rem; }
  .confirm-panel .status-line { padding: .4rem 0 0; }
  .trade-log { border-top: 1px solid var(--border); }
  .trade-log summary { padding: .65rem .8rem; cursor: pointer; font-size: .8rem; font-weight: 800; }
  .trade-log summary span { margin-inline-start: .35rem; color: var(--text-muted); font-size: .68rem; font-weight: 600; }
  .trade-log__hint { margin: -.2rem .8rem .55rem; color: var(--text-muted); font-size: .7rem; }
  .trade-events article { display: flex; align-items: center; justify-content: space-between; gap: .8rem; padding: .55rem .8rem; border-top: 1px solid var(--border); }
  .trade-events article.pending { background: color-mix(in oklch, var(--accent-soft), transparent 55%); }
  .trade-event__copy strong, .trade-event__copy span, .trade-event__copy small { display: block; }
  .trade-event__copy strong { font-size: .78rem; }
  .trade-event__copy span { margin-top: .1rem; font-size: .72rem; }
  .trade-event__copy small { margin-top: .12rem; color: var(--text-subtle); font-size: .64rem; }
  .manual { color: var(--danger); font-size: .68rem; font-weight: 700; }
  .done { color: var(--success); font-size: .68rem; font-weight: 700; }
  @media (max-width: 70rem) { .shift-summary { grid-template-columns: repeat(3, 1fr); } .shift-summary div { border-bottom: 1px solid var(--border); } }
  @media (max-width: 50rem) { .shift__header, .shift-toolbar { align-items: stretch; flex-direction: column; } .shift-toolbar__actions { margin-inline-start: 0; } .shift-summary { grid-template-columns: repeat(2, 1fr); } .shift-table-wrap { max-height: none; } .trade-events article { align-items: flex-start; flex-direction: column; } }
  @media (max-width: 46rem) {
    .shift-table, .shift-table tbody { display: block; }
    .shift-table colgroup, .shift-table thead { display: none; }
    .shift-table tbody tr { display: grid; grid-template-columns: 1.5rem minmax(0, 1fr) minmax(6.5rem, auto); gap: .35rem .55rem; margin: 0; border-radius: 0; padding: .6rem; background: var(--surface-1); }
    .shift-table tbody tr:hover { background: var(--surface-hover); }
    .shift-table tbody th, .shift-table tbody td, .shift-table tbody td:nth-child(5) { display: block; width: auto; min-width: 0; border: 0; padding: 0; text-align: start; }
    .shift-table tbody td::before { content: none; }
    .shift-table tbody td:nth-child(1) { grid-column: 1; grid-row: 1; }
    .shift-table tbody th:nth-child(2) { grid-column: 2 / 4; grid-row: 1; }
    .shift-table tbody td:nth-child(3) { grid-column: 2; grid-row: 2; }
    .shift-table tbody td:nth-child(4) { grid-column: 3; grid-row: 2; }
    .shift-table tbody td:nth-child(5) { grid-column: 2; grid-row: 3; }
    .shift-table tbody td:nth-child(6) { grid-column: 3; grid-row: 3; align-self: end; }
    .health { white-space: normal; }
  }
</style>
