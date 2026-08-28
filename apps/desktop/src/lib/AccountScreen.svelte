<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick } from "svelte";
  import { localeCode, useLocale } from "./i18n";

  import {
    accountActionErrorMessage,
    createListingInput,
    orderTypeLabel,
    validateListingNumbers,
    visibilityLabel,
    type AccountOrder,
    type AccountView,
    type CreateListingInput,
    type UpdateListingInput,
  } from "./account";
  import { formatPlatinum, variantLabel, type MarketSearchResult, type MarketSearchRow } from "./market";

  type PendingAction =
    | { kind: "create"; input: CreateListingInput; itemName: string }
    | { kind: "update"; id: string; input: UpdateListingInput; order: AccountOrder }
    | { kind: "delete"; id: string; order: AccountOrder };

  const locale = useLocale();
  const copy = {
    ru: {
      loadError: (r: string) => `Не удалось прочитать состояние аккаунта WFM. Проверьте подключение и повторите попытку. Техническая причина: ${r}`, connecting: "Подключаем аккаунт WFM…", connected: "Аккаунт WFM подключён. Пароль удалён из формы; сохранён только токен в хранилище Windows.", connectError: (r: string) => `Не удалось подключить аккаунт. Проверьте email и пароль WFM. Техническая причина: ${r}`,
      disconnecting: "Отключаем аккаунт WFM…", disconnected: "Сессия WFM завершена, локальный токен удалён.", localRemoved: "Локальный токен удалён. Сервер WFM не подтвердил завершение сессии.", disconnectError: (r: string) => `Не удалось удалить токен из защищённого хранилища Windows. Техническая причина: ${r}`,
      minSearch: "Введите не менее двух символов названия предмета.", noSearch: (q: string) => `По запросу «${q}» ничего не найдено.`, searchError: (r: string) => `Локальный поиск недоступен. Сначала обновите данные рынка. Техническая причина: ${r}`, selectExact: "Сначала найдите и выберите точный вариант предмета.", confirmCheck: "Подтвердите, что проверили действие и его параметры.", created: "Ордер создан на WFM.", updated: "Изменения ордера сохранены на WFM.", deleted: "Ордер удалён с WFM.", actionError: (r: string) => accountActionErrorMessage(r, "ru"),
      createTitle: "Проверьте новый ордер", updateTitle: "Проверьте изменения ордера", deleteTitle: "Подтвердите удаление ордера", createButton: "Создать ордер", updateButton: "Сохранить изменения", deleteButton: "Удалить ордер",
      kicker: "Явное подключение", heading: "Аккаунт Warframe Market", intro: "Читайте свои ордера и управляйте ими вручную. PlatScope не создаёт, не меняет и не удаляет ордера в фоне.", refresh: "Обновить ордера", reading: "Читаем токен из защищённого хранилища и проверяем WFM…", retry: "Повторить проверку",
      optIn: "Отключено по умолчанию", connectHeading: "Подключить существующий аккаунт WFM", connectBody: "Пароль используется один раз для входа и не сохраняется. Токен хранится в Windows Credential Manager, а не в базе PlatScope или логах.", password: "Пароль WFM", connectBusy: "Подключаем аккаунт…", connect: "Подключить аккаунт", legacyNote: "Вход использует переходный v1-маршрут WFM, потому что публичная регистрация OAuth-клиентов v2 пока недоступна. Остальные запросы выполняются через v2.", connectedKicker: "Подключено", crossplayOn: "Crossplay включён", crossplayOff: "Crossplay выключен", verified: "Аккаунт подтверждён", unverified: "Аккаунт не подтверждён", disconnect: "Отключить аккаунт", verifyNote: "WFM разрешает создание, изменение и удаление ордеров только подтверждённым аккаунтам. Просмотр ордеров остаётся доступен.",
      afterReview: "Только после проверки", newOrder: "Новый ордер на продажу", newOrderBody: "Выберите предмет из локального каталога. Точный ранг, subtype и звёзды перейдут в черновик.", findItem: "Найти предмет", searchExample: "Например, Поток Прайм", searching: "Ищем…", results: "Результаты поиска предмета", price: "Цена, платина", quantity: "Количество", perTrade: "За сделку, если bulk", notRequired: "Не требуется", publishNow: "Сразу опубликовать ордер", reviewNew: "Проверить новый ордер",
      current: "Текущие данные WFM", myOrders: "Мои ордера", notProvided: "не передан", updatedAt: "Обновлён", edit: "Изменить ордер", remove: "Удалить ордер", noOrders: "Ордеров пока нет", noOrdersBody: "Найдите предмет слева, задайте цену и проверьте черновик перед публикацией.", editDraft: "Черновик изменения", editHeading: "Изменить ордер", noSend: "На WFM ничего не отправится до отдельного подтверждения.", publish: "Опубликовать ордер", reviewChanges: "Проверить изменения", cancelEdit: "Отменить изменение",
      final: "Финальное подтверждение", orderWord: "Ордер", createSummary: (q: number, p: string, s: string) => `Продажа ${q} шт. по ${p}p. Статус: ${s}.`, updateSummary: (p: string, q: number, s: string) => `Цена ${p}p, количество ${q}, статус «${s}».`, deleteSummary: "будет безвозвратно удалён с WFM. Его придётся создавать заново.", checked: "Я проверил действие и параметры ордера", cancel: "Отменить",
    },
    en: {
      loadError: (r: string) => `Unable to read WFM account status. Check the connection and try again. Technical reason: ${r}`, connecting: "Connecting WFM account…", connected: "WFM account connected. The password was cleared; only the token was saved in Windows secure storage.", connectError: (r: string) => `Unable to connect the account. Check the WFM email and password. Technical reason: ${r}`,
      disconnecting: "Disconnecting WFM account…", disconnected: "WFM session ended and the local token was deleted.", localRemoved: "The local token was deleted. WFM did not confirm remote session revocation.", disconnectError: (r: string) => `Unable to delete the token from Windows secure storage. Technical reason: ${r}`,
      minSearch: "Enter at least two characters of an item name.", noSearch: (q: string) => `No results for “${q}”.`, searchError: (r: string) => `Local search unavailable. Refresh market data first. Technical reason: ${r}`, selectExact: "Find and select an exact item variant first.", confirmCheck: "Confirm that you reviewed the action and order parameters.", created: "Order created on WFM.", updated: "Order changes saved on WFM.", deleted: "Order deleted from WFM.", actionError: (r: string) => accountActionErrorMessage(r, "en"),
      createTitle: "Review new order", updateTitle: "Review order changes", deleteTitle: "Confirm order deletion", createButton: "Create order", updateButton: "Save changes", deleteButton: "Delete order",
      kicker: "Explicit connection", heading: "Warframe Market account", intro: "Read and manage your orders manually. PlatScope never creates, changes, or deletes orders in the background.", refresh: "Refresh orders", reading: "Reading the token from secure storage and checking WFM…", retry: "Check again",
      optIn: "Off by default", connectHeading: "Connect an existing WFM account", connectBody: "The password is used once for sign-in and is not saved. The token is stored in Windows Credential Manager, not in the PlatScope database or logs.", password: "WFM password", connectBusy: "Connecting account…", connect: "Connect account", legacyNote: "Sign-in uses the transitional WFM v1 route because public v2 OAuth client registration is not available. All other requests use v2.", connectedKicker: "Connected", crossplayOn: "Crossplay on", crossplayOff: "Crossplay off", verified: "Account verified", unverified: "Account not verified", disconnect: "Disconnect account", verifyNote: "WFM allows order creation, changes, and deletion only for verified accounts. Reading orders remains available.",
      afterReview: "Only after review", newOrder: "New sell order", newOrderBody: "Select an item from the local catalog. Exact rank, subtype, and stars are copied into the draft.", findItem: "Find item", searchExample: "For example, Primed Flow", searching: "Searching…", results: "Item search results", price: "Price, platinum", quantity: "Quantity", perTrade: "Per trade for bulk", notRequired: "Not required", publishNow: "Publish order immediately", reviewNew: "Review new order",
      current: "Current WFM data", myOrders: "My orders", notProvided: "not provided", updatedAt: "Updated", edit: "Edit order", remove: "Delete order", noOrders: "No orders yet", noOrdersBody: "Find an item, set a price, and review the draft before publishing.", editDraft: "Change draft", editHeading: "Edit order", noSend: "Nothing is sent to WFM until separate confirmation.", publish: "Publish order", reviewChanges: "Review changes", cancelEdit: "Cancel changes",
      final: "Final confirmation", orderWord: "Order", createSummary: (q: number, p: string, s: string) => `Sell ${q} at ${p}p each. Status: ${s}.`, updateSummary: (p: string, q: number, s: string) => `Price ${p}p, quantity ${q}, status “${s}”.`, deleteSummary: "will be permanently deleted from WFM. You will need to recreate it.", checked: "I reviewed the action and order parameters", cancel: "Cancel",
    },
  } as const;
  $: c = copy[$locale];

  let view: AccountView | null = null;
  let loading = true;
  let busy = false;
  let errorMessage = "";
  let statusMessage = "";
  let email = "";
  let password = "";

  let itemQuery = "";
  let itemResults: MarketSearchRow[] = [];
  let itemSearching = false;
  let itemSearchError = "";
  let selectedItem: MarketSearchRow | null = null;
  let createPlatinum = 1;
  let createQuantity = 1;
  let createPerTrade = "";
  let createVisible = false;
  let createError = "";

  let editingOrder: AccountOrder | null = null;
  let editPlatinum = 1;
  let editQuantity = 1;
  let editVisible = false;
  let editError = "";

  let pendingAction: PendingAction | null = null;
  let confirmationAccepted = false;
  let confirmationError = "";
  let confirmationHeading: HTMLElement;
  let confirmationCheckbox: HTMLInputElement;
  let confirmationTrigger: HTMLElement | null = null;

  async function loadAccount(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      view = await invoke<AccountView>("account_status");
    } catch (error) {
      view = null;
      errorMessage = c.loadError(String(error));
    } finally {
      loading = false;
    }
  }

  async function connectAccount(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    busy = true;
    errorMessage = "";
    statusMessage = c.connecting;
    try {
      view = await invoke<AccountView>("account_connect", { email, password });
      email = "";
      password = "";
      statusMessage = c.connected;
    } catch (error) {
      password = "";
      statusMessage = "";
      errorMessage = c.connectError(String(error));
    } finally {
      busy = false;
    }
  }

  async function disconnectAccount(): Promise<void> {
    busy = true;
    errorMessage = "";
    statusMessage = c.disconnecting;
    try {
      const remotelyRevoked = await invoke<boolean>("account_disconnect");
      view = { connected: false, profile: null, orders: [] };
      closeConfirmation();
      statusMessage = remotelyRevoked
        ? c.disconnected : c.localRemoved;
    } catch (error) {
      statusMessage = "";
      errorMessage = c.disconnectError(String(error));
    } finally {
      busy = false;
    }
  }

  async function searchItems(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    itemSearchError = "";
    if (itemQuery.trim().length < 2) {
      itemSearchError = c.minSearch;
      return;
    }
    itemSearching = true;
    try {
      const result = await invoke<MarketSearchResult>("search_market", {
        query: itemQuery.trim(),
        limit: 8,
      });
      itemResults = result.rows;
      if (!itemResults.length) itemSearchError = c.noSearch(itemQuery.trim());
    } catch (error) {
      itemResults = [];
      itemSearchError = c.searchError(String(error));
    } finally {
      itemSearching = false;
    }
  }

  function selectItem(row: MarketSearchRow): void {
    selectedItem = row;
    itemResults = [];
    itemQuery = row.displayName;
    createPlatinum = Math.max(1, Math.round(row.recommendation.listPrice ?? row.recommendation.fairPrice ?? 1));
    createError = "";
  }

  function prepareCreate(event: SubmitEvent): void {
    event.preventDefault();
    createError = "";
    if (!selectedItem) {
      createError = c.selectExact;
      return;
    }
    const perTrade = createPerTrade.trim() === "" ? null : Number(createPerTrade);
    const error = validateListingNumbers(createPlatinum, createQuantity, perTrade, $locale);
    if (error) {
      createError = error;
      return;
    }
    openConfirmation(
      {
        kind: "create",
        input: createListingInput(selectedItem, createPlatinum, createQuantity, createVisible, perTrade),
        itemName: selectedItem.displayName,
      },
      event.submitter as HTMLElement | null,
    );
  }

  function beginEdit(order: AccountOrder, trigger: HTMLElement): void {
    editingOrder = order;
    editPlatinum = order.platinum;
    editQuantity = order.quantity;
    editVisible = order.visible;
    editError = "";
    trigger.scrollIntoView({ block: "nearest" });
  }

  function orderItem(order: AccountOrder) {
    return order.itemId ? view?.orderItems?.[order.itemId] : undefined;
  }

  function prepareUpdate(event: SubmitEvent): void {
    event.preventDefault();
    if (!editingOrder) return;
    editError = validateListingNumbers(editPlatinum, editQuantity, null, $locale) ?? "";
    if (editError) return;
    openConfirmation(
      {
        kind: "update",
        id: editingOrder.id,
        order: editingOrder,
        input: {
          platinum: editPlatinum,
          quantity: editQuantity,
          visible: editVisible,
          perTrade: null,
          rank: null,
          charges: null,
          subtype: null,
          amberStars: null,
          cyanStars: null,
        },
      },
      event.submitter as HTMLElement | null,
    );
  }

  function prepareDelete(order: AccountOrder, trigger: HTMLElement): void {
    openConfirmation({ kind: "delete", id: order.id, order }, trigger);
  }

  async function openConfirmation(action: PendingAction, trigger: HTMLElement | null): Promise<void> {
    pendingAction = action;
    confirmationAccepted = false;
    confirmationError = "";
    confirmationTrigger = trigger;
    await tick();
    confirmationHeading?.focus();
  }

  function closeConfirmation(): void {
    pendingAction = null;
    confirmationAccepted = false;
    confirmationError = "";
    const trigger = confirmationTrigger;
    confirmationTrigger = null;
    void tick().then(() => trigger?.focus());
  }

  async function executePendingAction(): Promise<void> {
    if (!pendingAction) return;
    if (!confirmationAccepted) {
      confirmationError = c.confirmCheck;
      confirmationCheckbox?.focus();
      return;
    }
    busy = true;
    confirmationError = "";
    errorMessage = "";
    const action = pendingAction;
    try {
      if (action.kind === "create") {
        await invoke<AccountOrder>("account_create_listing", { input: action.input, confirmed: true });
        statusMessage = c.created;
        selectedItem = null;
        itemQuery = "";
      } else if (action.kind === "update") {
        await invoke<AccountOrder>("account_update_listing", {
          id: action.id,
          input: action.input,
          confirmed: true,
        });
        statusMessage = c.updated;
        editingOrder = null;
      } else {
        await invoke<AccountOrder>("account_delete_listing", { id: action.id, confirmed: true });
        statusMessage = c.deleted;
        if (editingOrder?.id === action.id) editingOrder = null;
      }
      closeConfirmation();
      await loadAccount();
    } catch (error) {
      confirmationError = c.actionError(String(error));
    } finally {
      busy = false;
    }
  }

  function pendingTitle(action: PendingAction): string {
    if (action.kind === "create") return c.createTitle;
    if (action.kind === "update") return c.updateTitle;
    return c.deleteTitle;
  }

  function pendingButtonLabel(action: PendingAction): string {
    if (action.kind === "create") return c.createButton;
    if (action.kind === "update") return c.updateButton;
    return c.deleteButton;
  }

  function formatDate(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.valueOf())
      ? value
      : new Intl.DateTimeFormat(localeCode($locale), { dateStyle: "short", timeStyle: "short" }).format(date);
  }

  onMount(() => void loadAccount());
</script>

<section class="account-shell" aria-labelledby="account-heading" aria-busy={loading || busy}>
  <div class="account-intro">
    <div>
      <p class="section-kicker">{c.kicker}</p><h2 id="account-heading">{c.heading}</h2><p>{c.intro}</p>
    </div>
    {#if view?.connected}
      <button type="button" class="secondary" onclick={loadAccount} disabled={busy}>{c.refresh}</button>
    {/if}
  </div>

  <div class="account-status" role="status" aria-live="polite">
    {#if loading}{c.reading}{:else}{statusMessage}{/if}
  </div>

  {#if errorMessage}
    <div class="account-error" role="alert">
      <p>{errorMessage}</p>
      <button type="button" onclick={loadAccount}>{c.retry}</button>
    </div>
  {/if}

  {#if !loading && !view?.connected}
    <form class="connect-card" onsubmit={connectAccount}>
      <div>
        <p class="section-kicker">{c.optIn}</p><h3>{c.connectHeading}</h3><p>{c.connectBody}</p>
      </div>
      <div class="field-group">
        <label for="wfm-email">Email WFM</label>
        <input id="wfm-email" name="username" type="email" autocomplete="username" spellcheck="false" bind:value={email} required maxlength="128" placeholder="name@example.com" />
      </div>
      <div class="field-group">
        <label for="wfm-password">{c.password}</label>
        <input id="wfm-password" name="password" type="password" autocomplete="current-password" bind:value={password} required maxlength="128" />
      </div>
      <button type="submit" disabled={busy}>{busy ? c.connectBusy : c.connect}</button><p class="security-note">{c.legacyNote}</p>
    </form>
  {:else if view?.connected && view.profile}
    <section class="profile-card" aria-labelledby="profile-heading">
      <div>
        <p class="section-kicker">{c.connectedKicker}</p>
        <h3 id="profile-heading"><span translate="no">{view.profile.ingameName}</span></h3>
        <p>{view.profile.platform.toUpperCase()} · {view.profile.crossplay ? c.crossplayOn : c.crossplayOff} · {view.profile.verification ? c.verified : c.unverified}</p>
      </div>
      <button type="button" class="danger-secondary" onclick={disconnectAccount} disabled={busy}>{c.disconnect}</button>
    </section>

    {#if !view.profile.verification}
      <div class="account-note" role="note">{c.verifyNote}</div>
    {/if}

    <div class="account-grid">
      <section class="create-card" aria-labelledby="create-heading">
        <div>
          <p class="section-kicker">{c.afterReview}</p><h3 id="create-heading">{c.newOrder}</h3><p>{c.newOrderBody}</p>
        </div>

        <form class="item-search" onsubmit={searchItems}>
          <div class="field-group grow">
            <label for="account-item-search">{c.findItem}</label><input id="account-item-search" type="search" name="account-item-search" autocomplete="off" bind:value={itemQuery} minlength="2" maxlength="80" placeholder={c.searchExample} aria-describedby={itemSearchError ? "account-item-search-error" : undefined} aria-invalid={itemSearchError ? "true" : undefined} />
          </div>
          <button type="submit" disabled={itemSearching}>{itemSearching ? c.searching : c.findItem}</button>
        </form>
        {#if itemSearchError}<p id="account-item-search-error" class="inline-error">{itemSearchError}</p>{/if}
        {#if itemResults.length}
          <ul class="item-results" aria-label={c.results}>
            {#each itemResults as row (`${row.itemId}:${variantLabel(row.recommendation.key, $locale)}`)}
              <li>
                <button type="button" onclick={() => selectItem(row)}>
                  {#if row.imageUrl}<img src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                  <span class="item-result-copy">
                  <span>{row.displayName}</span>
                  <small>{variantLabel(row.recommendation.key, $locale)} · fair {formatPlatinum(row.recommendation.fairPrice, $locale)}</small>
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}

        {#if selectedItem}
          <form class="listing-form" onsubmit={prepareCreate}>
            <div class="selected-item">
              {#if selectedItem.imageUrl}<img src={selectedItem.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
              <span class="selected-item-copy">
              <strong>{selectedItem.displayName}</strong>
              <span>{variantLabel(selectedItem.recommendation.key, $locale)} · WFM ID <span translate="no">{selectedItem.itemId}</span></span>
              </span>
            </div>
            <div class="numeric-fields">
              <div class="field-group">
                <label for="create-platinum">{c.price}</label>
                <input id="create-platinum" type="number" inputmode="numeric" bind:value={createPlatinum} min="1" max="900000" step="1" required />
              </div>
              <div class="field-group">
                <label for="create-quantity">{c.quantity}</label>
                <input id="create-quantity" type="number" inputmode="numeric" bind:value={createQuantity} min="1" max="9999" step="1" required />
              </div>
              <div class="field-group">
                <label for="create-per-trade">{c.perTrade}</label><input id="create-per-trade" type="number" inputmode="numeric" value={createPerTrade} oninput={(event) => (createPerTrade = event.currentTarget.value)} min="1" max="6" step="1" placeholder={c.notRequired} />
              </div>
            </div>
            <label class="check-field"><input type="checkbox" bind:checked={createVisible} /> {c.publishNow}</label>
            {#if createError}<p class="inline-error" role="alert">{createError}</p>{/if}
            <button type="submit" disabled={!view.profile.verification}>{c.reviewNew}</button>
          </form>
        {/if}
      </section>

      <section class="orders-card" aria-labelledby="orders-heading">
        <div class="orders-heading">
          <div>
            <p class="section-kicker">{c.current}</p><h3 id="orders-heading">{c.myOrders}</h3>
          </div>
          <span class="count-badge">{view.orders.length}</span>
        </div>

        <div class="order-list">
          {#each view.orders as order (order.id)}
            {@const item = orderItem(order)}
            <article class="order-row">
              <div class="order-art" aria-hidden="true">
                {#if item?.imageUrl}
                  <img src={item.imageUrl} alt="" loading="lazy" decoding="async" />
                {:else}
                  <span>{item?.displayName?.slice(0, 1) ?? "?"}</span>
                {/if}
              </div>
              <div class="order-main">
                <h4>{item?.displayName ?? c.notProvided}</h4>
                <div>
                  <strong>{orderTypeLabel(order.type, $locale)} · {order.platinum.toLocaleString(localeCode($locale))}p × {order.quantity.toLocaleString(localeCode($locale))}</strong><span class:visible={order.visible}>{visibilityLabel(order.visible, $locale)}</span>
                </div>
                <p>{item?.slug ?? order.itemId ?? c.notProvided}</p><small>{c.updatedAt} {formatDate(order.updatedAt)}</small>
              </div>
              <div class="order-actions">
                <button type="button" onclick={(event) => beginEdit(order, event.currentTarget)} disabled={!view.profile?.verification}>{c.edit}</button><button type="button" class="danger-secondary" onclick={(event) => prepareDelete(order, event.currentTarget)} disabled={!view.profile?.verification}>{c.remove}</button>
              </div>
            </article>
          {:else}
            <div class="orders-empty">
              <h4>{c.noOrders}</h4><p>{c.noOrdersBody}</p>
            </div>
          {/each}
        </div>
      </section>
    </div>

    {#if editingOrder}
      <form class="edit-card" onsubmit={prepareUpdate} aria-labelledby="edit-heading">
        <div>
          <p class="section-kicker">{c.editDraft}</p><h3 id="edit-heading">{c.editHeading}</h3><p>ID <span translate="no">{editingOrder.id}</span>. {c.noSend}</p>
        </div>
        <div class="numeric-fields">
          <div class="field-group"><label for="edit-platinum">{c.price}</label><input id="edit-platinum" type="number" inputmode="numeric" bind:value={editPlatinum} min="1" max="900000" step="1" required /></div><div class="field-group"><label for="edit-quantity">{c.quantity}</label><input id="edit-quantity" type="number" inputmode="numeric" bind:value={editQuantity} min="1" max="9999" step="1" required /></div>
        </div>
        <label class="check-field"><input type="checkbox" bind:checked={editVisible} /> {c.publish}</label>
        {#if editError}<p class="inline-error" role="alert">{editError}</p>{/if}
        <div class="form-actions"><button type="submit">{c.reviewChanges}</button><button type="button" class="secondary" onclick={() => (editingOrder = null)}>{c.cancelEdit}</button></div>
      </form>
    {/if}

    {#if pendingAction}
      <section class:destructive={pendingAction.kind === "delete"} class="confirmation-card" aria-labelledby="confirmation-heading">
        <div>
          <p class="section-kicker">{c.final}</p>
          <h3 id="confirmation-heading" bind:this={confirmationHeading} tabindex="-1">{pendingTitle(pendingAction)}</h3>
          {#if pendingAction.kind === "create"}
            <p><strong>{pendingAction.itemName}</strong>: {c.createSummary(pendingAction.input.quantity, pendingAction.input.platinum.toLocaleString(localeCode($locale)), visibilityLabel(pendingAction.input.visible, $locale))}</p>
          {:else if pendingAction.kind === "update"}
            <p>{c.orderWord} <span translate="no">{pendingAction.id}</span>: {c.updateSummary(pendingAction.input.platinum?.toLocaleString(localeCode($locale)) ?? "—", pendingAction.input.quantity ?? pendingAction.order.quantity, visibilityLabel(pendingAction.input.visible ?? false, $locale))}</p>
          {:else}
            <p>{c.orderWord} <span translate="no">{pendingAction.id}</span> {c.deleteSummary}</p>
          {/if}
        </div>
        <label class="check-field confirmation-check"><input bind:this={confirmationCheckbox} type="checkbox" bind:checked={confirmationAccepted} aria-describedby={confirmationError ? "confirmation-error" : undefined} /> {c.checked}</label>
        {#if confirmationError}<p id="confirmation-error" class="inline-error" role="alert">{confirmationError}</p>{/if}
        <div class="form-actions">
          <button type="button" class:danger-primary={pendingAction.kind === "delete"} onclick={executePendingAction} disabled={busy}>{busy ? `${pendingButtonLabel(pendingAction)}…` : pendingButtonLabel(pendingAction)}</button>
          <button type="button" class="secondary" onclick={closeConfirmation} disabled={busy}>{c.cancel}</button>
        </div>
      </section>
    {/if}
  {/if}
</section>

<style>
  .account-shell { display: grid; gap: 1rem; }
  .account-intro, .profile-card, .connect-card, .create-card, .orders-card, .edit-card, .confirmation-card { border: 1px solid #283752; border-radius: .8rem; padding: 1rem; background: #111b2f; box-shadow: 0 .75rem 2rem rgb(0 0 0 / 14%); }
  .account-intro, .profile-card, .orders-heading { display: flex; align-items: start; justify-content: space-between; gap: 1.25rem; }
  .account-intro h2 { margin-block-end: .35rem; font-size: 1.25rem; }
  .account-intro p, .profile-card p, .connect-card p, .create-card > div > p, .edit-card > div > p, .confirmation-card p { max-width: 68ch; margin-block-end: 0; color: #9ba9bd; line-height: 1.5; }
  .section-kicker { margin-block-end: .3rem !important; color: #72a7ff !important; font-size: .78rem; font-weight: 750; letter-spacing: .08em; text-transform: uppercase; }
  .account-status { min-height: 1.5rem; color: #9ba9bd; }
  .account-error, .account-note { border: 1px solid #34496b; border-radius: .7rem; padding: 1rem; background: #0c1526; }
  .account-error { border-color: #9c5555; background: #2b1719; }
  .account-error p { margin-block-end: .75rem; }
  .connect-card { display: grid; gap: 1rem; max-width: 42rem; }
  .security-note { border-inline-start: .2rem solid #5d92a0; padding-inline-start: .75rem; font-size: .84rem; }
  .field-group { display: grid; gap: .4rem; min-width: 0; }
  .field-group label, .check-field { color: #c5d5da; font-size: .86rem; font-weight: 650; }
  input { min-width: 0; min-height: 2.6rem; border: 1px solid #3b4e70; border-radius: .5rem; padding: .55rem .7rem; background: #0b1323; color: #f4f7fb; }
  input::placeholder { color: #70848c; }
  .check-field { display: flex; align-items: center; gap: .65rem; width: fit-content; min-height: 2.75rem; cursor: pointer; }
  .check-field input { min-width: 1.25rem; min-height: 1.25rem; accent-color: #72a7ff; }
  .account-grid { display: grid; grid-template-columns: minmax(20rem, .85fr) minmax(24rem, 1.15fr); gap: 1rem; align-items: start; }
  .create-card, .orders-card, .edit-card, .confirmation-card { display: grid; gap: 1rem; min-width: 0; }
  .item-search { display: flex; align-items: end; gap: .75rem; }
  .grow { flex: 1 1 auto; }
  .inline-error { margin: 0; color: #ffc0ba; font-size: .86rem; line-height: 1.45; }
  .item-results { display: grid; gap: .45rem; margin: 0; padding: 0; list-style: none; }
  .item-results button { display: flex; align-items: center; gap: .65rem; width: 100%; text-align: start; }
  .item-results img, .selected-item img { flex: none; width: 3rem; height: 3rem; object-fit: contain; outline: 1px solid rgb(255 255 255 / 10%); outline-offset: -1px; }
  .item-result-copy, .selected-item-copy { display: grid; gap: .2rem; min-width: 0; }
  .item-results small, .selected-item span, .order-main small { color: #9eb1b8; font-weight: 500; }
  .listing-form { display: grid; gap: .9rem; border: 1px solid #283752; border-radius: .65rem; padding: .85rem; background: #172238; }
  .selected-item { display: flex; align-items: center; gap: .75rem; overflow-wrap: anywhere; }
  .numeric-fields { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: .75rem; }
  .profile-card { align-items: center; }
  .profile-card h3 { margin-block-end: .25rem; font-size: 1.2rem; }
  .count-badge { min-width: 2rem; border-radius: 999px; padding: .3rem .55rem; background: #183842; color: #c1e7ef; font-variant-numeric: tabular-nums; text-align: center; }
  .order-list { display: grid; gap: .65rem; }
  .order-row { display: grid; grid-template-columns: 4.5rem minmax(0, 1fr) auto; align-items: center; gap: 1rem; border: 1px solid #283752; border-radius: .75rem; padding: .85rem; background: #172238; }
  .order-art { display: grid; place-items: center; width: 4.5rem; height: 4.5rem; border-radius: .65rem; background: #0b171d; box-shadow: 0 0 0 1px rgb(255 255 255 / 8%) inset; }
  .order-art img { width: 4rem; height: 4rem; object-fit: contain; filter: drop-shadow(0 .35rem .4rem rgb(0 0 0 / 28%)); outline: 1px solid rgb(255 255 255 / 10%); outline-offset: -1px; }
  .order-art span { color: #75c8d8; font-size: 1.35rem; font-weight: 800; }
  .order-main { display: grid; gap: .35rem; min-width: 0; }
  .order-main h4 { margin: 0; color: #f3f8fa; font-size: 1rem; }
  .order-main > div { display: flex; align-items: center; flex-wrap: wrap; gap: .5rem; }
  .order-main > div > span { border-radius: 999px; padding: .2rem .5rem; background: #3a2e1d; color: #e8cc94; font-size: .75rem; font-weight: 700; }
  .order-main > div > span.visible { background: #163b2e; color: #a8e4c8; }
  .order-main p { margin: 0; overflow-wrap: anywhere; color: #a7bac1; font-size: .82rem; }
  .order-actions { display: flex; align-items: start; gap: .6rem; }
  .orders-empty { border-radius: .65rem; padding: 1.2rem; background: #111f27; text-align: center; }
  .orders-empty h4 { margin-block: 0 0 .35rem; }
  .orders-empty p { margin: 0; color: #9eb1b8; }
  .form-actions { display: flex; flex-wrap: wrap; gap: .75rem; }
  button.secondary, button.danger-secondary { background: transparent; }
  button.danger-secondary { border-color: #815256; color: #efbfc2; }
  button.danger-primary { border-color: #c56a70; background: #722d34; color: #fff5f5; }
  .confirmation-card { border-color: #5c8994; background: #11232b; scroll-margin-block: 1rem; }
  .confirmation-card.destructive { border-color: #a05d62; background: #28171a; }
  .confirmation-card h3:focus-visible { outline: .1875rem solid #f3c969; outline-offset: .1875rem; }
  .confirmation-check { border-radius: .5rem; padding: .45rem .6rem; background: rgb(255 255 255 / 4%); }
  @media (max-width: 64rem) {
    .account-grid { grid-template-columns: minmax(0, 1fr); }
  }
  @media (max-width: 44rem) {
    .account-intro, .profile-card, .item-search, .order-row { align-items: stretch; grid-template-columns: minmax(0, 1fr); flex-direction: column; }
    .order-art { width: 100%; height: 6rem; }
    .account-intro button, .profile-card button, .item-search button { width: 100%; }
    .numeric-fields { grid-template-columns: minmax(0, 1fr); }
    .order-actions { flex-direction: column; }
    .order-actions button { width: 100%; }
  }
  @media (forced-colors: active) {
    .account-intro, .profile-card, .connect-card, .create-card, .orders-card, .edit-card, .confirmation-card, .order-row, .listing-form { border: 1px solid CanvasText; }
  }
</style>
