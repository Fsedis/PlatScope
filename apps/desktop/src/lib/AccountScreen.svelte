<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick } from "svelte";
  import { localeCode, useLocale } from "./i18n";

  import {
    accountActionErrorMessage,
    orderEnglishName,
    orderTypeLabel,
    validateListingNumbers,
    visibilityLabel,
    type AccountOrder,
    type AccountView,
    type UpdateListingInput,
  } from "./account";

  export let onOpenSellQueue: () => void;

  type PendingAction =
    | { kind: "update"; id: string; input: UpdateListingInput; order: AccountOrder }
    | { kind: "delete"; id: string; order: AccountOrder };

  const locale = useLocale();
  const copy = {
    ru: {
      loading: "Проверяем подключение…",
      loadError: "Не удалось проверить подключение. Повторите попытку.",
      connecting: "Подключаем аккаунт…",
      connected: "Аккаунт подключён. Теперь можно выставлять предметы из очереди продажи.",
      connectError: "Не удалось войти. Проверьте email и пароль Warframe Market.",
      disconnecting: "Отключаем аккаунт…",
      disconnected: "Аккаунт отключён от PlatScope.",
      localRemoved: "Данные входа удалены с этого компьютера. Warframe Market не подтвердил завершение сессии.",
      disconnectError: "Не удалось удалить данные входа. Повторите попытку.",
      retry: "Проверить ещё раз",
      connectHeading: "Подключить Warframe Market",
      connectBody: "Подключение нужно, чтобы выставлять предметы и менять свои ордера прямо в PlatScope.",
      email: "Email Warframe Market",
      password: "Пароль Warframe Market",
      connectBusy: "Подключаем…",
      connect: "Подключить аккаунт",
      securitySummary: "Как хранятся данные входа",
      securityBody: "Пароль используется только для входа и не сохраняется. Ключ сессии хранится в защищённом хранилище Windows.",
      connectionDetails: "Подробности подключения",
      legacyNote: "Для входа используется маршрут Warframe Market v1, потому что публичная регистрация OAuth-клиентов v2 недоступна. Работа с ордерами выполняется через v2.",
      connectedLabel: "Подключено",
      crossplayOn: "Общий рынок включён",
      crossplayOff: "Общий рынок выключен",
      verified: "Аккаунт подтверждён",
      unverified: "Аккаунт не подтверждён",
      disconnect: "Отключить аккаунт",
      verifyNote: "Warframe Market разрешает менять ордера только после подтверждения игрового аккаунта. Просматривать их можно уже сейчас.",
      ordersHeading: "Выставленные ордера",
      ordersBody: "Здесь собраны ваши текущие ордера. Новые создаются из предметов в очереди продажи.",
      openSellQueue: "Открыть очередь продажи",
      refresh: "Обновить ордера",
      notProvided: "Предмет не определён",
      updatedAt: "Обновлён",
      edit: "Изменить",
      remove: "Снять ордер",
      noOrders: "Нет выставленных ордеров",
      noOrdersBody: "Выберите предмет в очереди продажи, проверьте цену и создайте первый ордер.",
      editHeading: "Изменить ордер",
      price: "Цена, платина",
      quantity: "Количество",
      publish: "Показывать ордер на рынке",
      reviewChanges: "Проверить изменения",
      cancelEdit: "Отменить",
      updateTitle: "Подтвердите изменения",
      deleteTitle: "Подтвердите снятие ордера",
      updateSummary: (name: string, p: string, q: number, s: string) => `${name}: цена ${p}p, количество ${q}, статус «${s}».`,
      deleteSummary: (name: string) => `${name}: ордер будет снят с Warframe Market.`,
      checked: "Я проверил предмет, цену и количество",
      confirmCheck: "Подтвердите, что проверили параметры ордера.",
      updateButton: "Сохранить изменения",
      deleteButton: "Снять ордер",
      cancel: "Отменить",
      updated: "Изменения сохранены на Warframe Market.",
      deleted: "Ордер снят с Warframe Market.",
      technical: "Технические подробности",
      actionError: (r: string) => accountActionErrorMessage(r, "ru"),
    },
    en: {
      loading: "Checking connection…",
      loadError: "Unable to check the connection. Try again.",
      connecting: "Connecting account…",
      connected: "Account connected. You can now list items from the sell queue.",
      connectError: "Unable to sign in. Check your Warframe Market email and password.",
      disconnecting: "Disconnecting account…",
      disconnected: "Account disconnected from PlatScope.",
      localRemoved: "Sign-in data was removed from this computer. Warframe Market did not confirm that the remote session ended.",
      disconnectError: "Unable to remove sign-in data. Try again.",
      retry: "Check again",
      connectHeading: "Connect Warframe Market",
      connectBody: "Connect your account to list items and manage your orders directly in PlatScope.",
      email: "Warframe Market email",
      password: "Warframe Market password",
      connectBusy: "Connecting…",
      connect: "Connect account",
      securitySummary: "How sign-in data is stored",
      securityBody: "Your password is used only to sign in and is not saved. The session key is stored in Windows secure storage.",
      connectionDetails: "Connection details",
      legacyNote: "Sign-in uses the Warframe Market v1 route because public v2 OAuth client registration is unavailable. Order requests use v2.",
      connectedLabel: "Connected",
      crossplayOn: "Shared market on",
      crossplayOff: "Shared market off",
      verified: "Account verified",
      unverified: "Account not verified",
      disconnect: "Disconnect account",
      verifyNote: "Warframe Market allows order changes only after the game account is verified. You can still view orders now.",
      ordersHeading: "Listed orders",
      ordersBody: "Your current orders are shown here. Create new ones from items in the sell queue.",
      openSellQueue: "Open sell queue",
      refresh: "Refresh orders",
      notProvided: "Unknown item",
      updatedAt: "Updated",
      edit: "Edit",
      remove: "Remove order",
      noOrders: "No listed orders",
      noOrdersBody: "Choose an item in the sell queue, review the price, and create your first order.",
      editHeading: "Edit order",
      price: "Price, platinum",
      quantity: "Quantity",
      publish: "Show order on the market",
      reviewChanges: "Review changes",
      cancelEdit: "Cancel",
      updateTitle: "Confirm changes",
      deleteTitle: "Confirm order removal",
      updateSummary: (name: string, p: string, q: number, s: string) => `${name}: price ${p}p, quantity ${q}, status “${s}”.`,
      deleteSummary: (name: string) => `${name}: the order will be removed from Warframe Market.`,
      checked: "I reviewed the item, price, and quantity",
      confirmCheck: "Confirm that you reviewed the order parameters.",
      updateButton: "Save changes",
      deleteButton: "Remove order",
      cancel: "Cancel",
      updated: "Changes saved on Warframe Market.",
      deleted: "Order removed from Warframe Market.",
      technical: "Technical details",
      actionError: (r: string) => accountActionErrorMessage(r, "en"),
    },
  } as const;
  $: c = copy[$locale];

  let view: AccountView | null = null;
  let loading = true;
  let busy = false;
  let errorMessage = "";
  let errorDetail = "";
  let statusMessage = "";
  let email = "";
  let password = "";
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
    errorDetail = "";
    try {
      view = await invoke<AccountView>("account_status");
    } catch (error) {
      view = null;
      errorMessage = c.loadError;
      errorDetail = String(error);
    } finally {
      loading = false;
    }
  }

  async function connectAccount(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    busy = true;
    errorMessage = "";
    errorDetail = "";
    statusMessage = c.connecting;
    try {
      view = await invoke<AccountView>("account_connect", { email, password });
      email = "";
      password = "";
      statusMessage = c.connected;
    } catch (error) {
      password = "";
      statusMessage = "";
      errorMessage = c.connectError;
      errorDetail = String(error);
    } finally {
      busy = false;
    }
  }

  async function disconnectAccount(): Promise<void> {
    busy = true;
    errorMessage = "";
    errorDetail = "";
    statusMessage = c.disconnecting;
    try {
      const remotelyRevoked = await invoke<boolean>("account_disconnect");
      view = { connected: false, profile: null, orders: [] };
      closeConfirmation();
      editingOrder = null;
      statusMessage = remotelyRevoked ? c.disconnected : c.localRemoved;
    } catch (error) {
      statusMessage = "";
      errorMessage = c.disconnectError;
      errorDetail = String(error);
    } finally {
      busy = false;
    }
  }

  function orderItem(order: AccountOrder) {
    return order.itemId ? view?.orderItems?.[order.itemId] : undefined;
  }

  function beginEdit(order: AccountOrder, trigger: HTMLElement): void {
    editingOrder = order;
    editPlatinum = order.platinum;
    editQuantity = order.quantity;
    editVisible = order.visible;
    editError = "";
    trigger.scrollIntoView({ block: "nearest" });
  }

  function prepareUpdate(event: SubmitEvent): void {
    event.preventDefault();
    if (!editingOrder) return;
    editError = validateListingNumbers(editPlatinum, editQuantity, null, $locale) ?? "";
    if (editError) return;
    openConfirmation({
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
    }, event.submitter as HTMLElement | null);
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
    const action = pendingAction;
    try {
      if (action.kind === "update") {
        await invoke<AccountOrder>("account_update_listing", { id: action.id, input: action.input, confirmed: true });
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

  function itemName(order: AccountOrder): string {
    return orderItem(order)?.displayName ?? c.notProvided;
  }

  function formatDate(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.valueOf())
      ? value
      : new Intl.DateTimeFormat(localeCode($locale), { dateStyle: "short", timeStyle: "short" }).format(date);
  }

  onMount(() => void loadAccount());
</script>

<section class="account-shell" aria-busy={loading || busy}>
  <div class="account-status" role="status" aria-live="polite">
    {loading ? c.loading : statusMessage}
  </div>

  {#if errorMessage}
    <div class="account-error" role="alert">
      <p>{errorMessage}</p>
      {#if errorDetail}
        <details><summary>{c.technical}</summary><code>{errorDetail}</code></details>
      {/if}
      <button type="button" onclick={loadAccount}>{c.retry}</button>
    </div>
  {/if}

  {#if !loading && !view?.connected}
    <form class="connect-card" onsubmit={connectAccount}>
      <div>
        <h2>{c.connectHeading}</h2>
        <p>{c.connectBody}</p>
      </div>
      <div class="connect-fields">
        <div class="field-group">
          <label for="wfm-email">{c.email}</label>
          <input id="wfm-email" name="username" type="email" autocomplete="username" spellcheck="false" bind:value={email} required maxlength="128" placeholder="name@example.com" />
        </div>
        <div class="field-group">
          <label for="wfm-password">{c.password}</label>
          <input id="wfm-password" name="password" type="password" autocomplete="current-password" bind:value={password} required maxlength="128" />
        </div>
      </div>
      <button type="submit" disabled={busy}>{busy ? c.connectBusy : c.connect}</button>
      <details class="security-details">
        <summary>{c.securitySummary}</summary>
        <p>{c.securityBody}</p>
        <details>
          <summary>{c.connectionDetails}</summary>
          <p>{c.legacyNote}</p>
        </details>
      </details>
    </form>
  {:else if view?.connected && view.profile}
    <section class="profile-card" aria-labelledby="profile-heading">
      <div>
        <p class="section-kicker">{c.connectedLabel}</p>
        <h2 id="profile-heading"><span translate="no">{view.profile.ingameName}</span></h2>
        <div class="profile-state">
          <span>{view.profile.platform.toUpperCase()}</span>
          <span>{view.profile.crossplay ? c.crossplayOn : c.crossplayOff}</span>
          <span class:warning={!view.profile.verification}>{view.profile.verification ? c.verified : c.unverified}</span>
        </div>
      </div>
      <button type="button" class="danger-secondary" onclick={disconnectAccount} disabled={busy}>{c.disconnect}</button>
    </section>

    {#if !view.profile.verification}
      <div class="account-note" role="note">{c.verifyNote}</div>
    {/if}

    <section class="orders-card" aria-labelledby="orders-heading">
      <div class="orders-heading">
        <div>
          <h2 id="orders-heading">{c.ordersHeading}</h2>
          <p>{c.ordersBody}</p>
        </div>
        <span class="count-badge" aria-label={`${c.ordersHeading}: ${view.orders.length}`}>{view.orders.length}</span>
      </div>
      <div class="orders-toolbar">
        <button type="button" onclick={onOpenSellQueue}>{c.openSellQueue}</button>
        <button type="button" class="secondary" onclick={loadAccount} disabled={busy}>{c.refresh}</button>
      </div>

      <div class="order-list">
        {#each view.orders as order (order.id)}
          {@const item = orderItem(order)}
          {@const englishName = orderEnglishName(item)}
          <article class="order-row">
            <div class="order-art" aria-hidden="true">
              {#if item?.imageUrl}<img src={item.imageUrl} alt="" loading="lazy" decoding="async" />{:else}<span>{item?.displayName?.slice(0, 1) ?? "?"}</span>{/if}
            </div>
            <div class="order-main">
              <h3>
                <span>{item?.displayName ?? c.notProvided}</span>
                {#if englishName}<span class="order-name-en" lang="en">{englishName}</span>{/if}
              </h3>
              <div>
                <strong>{orderTypeLabel(order.type, $locale)} · {order.platinum.toLocaleString(localeCode($locale))}p × {order.quantity.toLocaleString(localeCode($locale))}</strong>
                <span class:visible={order.visible}>{visibilityLabel(order.visible, $locale)}</span>
              </div>
              <small>{c.updatedAt} {formatDate(order.updatedAt)}</small>
            </div>
            <div class="order-actions">
              <button type="button" onclick={(event) => beginEdit(order, event.currentTarget)} disabled={!view.profile?.verification}>{c.edit}</button>
              <button type="button" class="danger-secondary" onclick={(event) => prepareDelete(order, event.currentTarget)} disabled={!view.profile?.verification}>{c.remove}</button>
            </div>
          </article>
        {:else}
          <div class="orders-empty">
            <h3>{c.noOrders}</h3>
            <p>{c.noOrdersBody}</p>
            <button type="button" onclick={onOpenSellQueue}>{c.openSellQueue}</button>
          </div>
        {/each}
      </div>
    </section>

    {#if editingOrder && !pendingAction}
      <form class="edit-card" onsubmit={prepareUpdate} aria-labelledby="edit-heading">
        <div>
          <h2 id="edit-heading">{c.editHeading}</h2>
          <p>{itemName(editingOrder)}</p>
        </div>
        <div class="numeric-fields">
          <div class="field-group"><label for="edit-platinum">{c.price}</label><input id="edit-platinum" type="number" inputmode="numeric" bind:value={editPlatinum} min="1" max="900000" step="1" required /></div>
          <div class="field-group"><label for="edit-quantity">{c.quantity}</label><input id="edit-quantity" type="number" inputmode="numeric" bind:value={editQuantity} min="1" max="9999" step="1" required /></div>
        </div>
        <label class="check-field"><input type="checkbox" bind:checked={editVisible} /> {c.publish}</label>
        {#if editError}<p class="inline-error" role="alert">{editError}</p>{/if}
        <div class="form-actions"><button type="submit">{c.reviewChanges}</button><button type="button" class="secondary" onclick={() => (editingOrder = null)}>{c.cancelEdit}</button></div>
      </form>
    {/if}

    {#if pendingAction}
      <section class:destructive={pendingAction.kind === "delete"} class="confirmation-card" aria-labelledby="confirmation-heading">
        <h2 id="confirmation-heading" bind:this={confirmationHeading} tabindex="-1">{pendingAction.kind === "update" ? c.updateTitle : c.deleteTitle}</h2>
        <p>{pendingAction.kind === "update"
          ? c.updateSummary(itemName(pendingAction.order), pendingAction.input.platinum?.toLocaleString(localeCode($locale)) ?? "—", pendingAction.input.quantity ?? pendingAction.order.quantity, visibilityLabel(pendingAction.input.visible ?? false, $locale))
          : c.deleteSummary(itemName(pendingAction.order))}</p>
        <label class="check-field confirmation-check"><input bind:this={confirmationCheckbox} type="checkbox" bind:checked={confirmationAccepted} aria-describedby={confirmationError ? "confirmation-error" : undefined} /> {c.checked}</label>
        {#if confirmationError}<p id="confirmation-error" class="inline-error" role="alert">{confirmationError}</p>{/if}
        <div class="form-actions">
          <button type="button" class:danger-primary={pendingAction.kind === "delete"} onclick={executePendingAction} disabled={busy}>{pendingAction.kind === "update" ? c.updateButton : c.deleteButton}</button>
          <button type="button" class="secondary" onclick={closeConfirmation} disabled={busy}>{c.cancel}</button>
        </div>
      </section>
    {/if}
  {/if}
</section>

<style>
  .account-shell { display: grid; gap: .7rem; }
  .profile-card, .connect-card, .orders-card, .edit-card, .confirmation-card { border-radius: .75rem; padding: .75rem; background: var(--surface-1); box-shadow: var(--shadow-sm); }
  .profile-card, .orders-heading { display: flex; align-items: start; justify-content: space-between; gap: .8rem; }
  .profile-card h2, .connect-card h2, .orders-card h2, .edit-card h2, .confirmation-card h2 { margin-block-end: .35rem; font-size: 1.2rem; }
  .connect-card p, .orders-heading p, .edit-card p, .confirmation-card p { max-width: 68ch; margin: 0; color: var(--text-muted); line-height: 1.5; }
  .section-kicker { margin-block-end: .3rem; color: var(--accent-strong); font-size: .76rem; font-weight: 750; letter-spacing: .08em; text-transform: uppercase; }
  .account-status { min-height: 1.5rem; color: var(--text-muted); }
  .account-error, .account-note { border: 1px solid var(--border); border-radius: .6rem; padding: .75rem; background: var(--surface-2); }
  .account-error { border-color: var(--danger); background: var(--danger-soft); }
  .account-error p { margin-block: 0 .75rem; }
  .account-error details { margin-block-end: .75rem; }
  .account-error code { display: block; margin-block-start: .5rem; overflow-wrap: anywhere; color: var(--text-muted); }
  .connect-card { display: grid; gap: .7rem; max-width: 46rem; }
  .connect-fields, .numeric-fields { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .55rem; }
  .field-group { display: grid; gap: .4rem; min-width: 0; }
  .field-group label, .check-field { color: var(--text); font-size: .86rem; font-weight: 650; }
  input { min-width: 0; min-height: 2.25rem; border: 1px solid var(--border); border-radius: .5rem; padding: .4rem .6rem; background: oklch(0.995 0.004 84); color: var(--text); }
  input::placeholder { color: var(--text-subtle); }
  .check-field { display: flex; align-items: center; gap: .5rem; width: fit-content; min-height: 2.125rem; cursor: pointer; }
  .check-field input { min-width: 1.25rem; min-height: 1.25rem; accent-color: var(--accent); }
  .security-details { border-radius: .55rem; padding: .6rem; background: var(--surface-2); color: var(--text-muted); }
  .security-details summary { color: var(--text); font-weight: 700; cursor: pointer; }
  .security-details p { margin-block-start: .65rem; }
  .security-details details { margin-block-start: .75rem; }
  .profile-card { align-items: center; }
  .profile-state { display: flex; flex-wrap: wrap; gap: .45rem; }
  .profile-state span, .count-badge { border-radius: 999px; padding: .18rem .42rem; background: var(--success-soft); color: oklch(0.37 0.08 145); font-size: .6875rem; font-weight: 700; }
  .profile-state span.warning { background: oklch(0.92 0.055 78); color: oklch(0.43 0.075 68); }
  .orders-card, .edit-card, .confirmation-card { display: grid; gap: .7rem; min-width: 0; }
  .count-badge { min-width: 2rem; background: var(--accent-soft); color: var(--accent-strong); font-variant-numeric: tabular-nums; text-align: center; }
  .orders-toolbar, .form-actions { display: flex; flex-wrap: wrap; gap: .5rem; }
  .order-list { display: grid; gap: .45rem; }
  .order-row { display: grid; grid-template-columns: 3.75rem minmax(0, 1fr) auto; align-items: center; gap: .7rem; border-radius: .6rem; padding: .65rem; background: var(--surface-2); box-shadow: 0 0 0 1px oklch(0 0 0 / .06); }
  .order-art { display: grid; place-items: center; width: 3.75rem; height: 3.75rem; border-radius: .55rem; background: var(--surface-1); }
  .order-art img { width: 3.4rem; height: 3.4rem; object-fit: contain; outline: 1px solid oklch(0 0 0 / 0.1); outline-offset: -1px; }
  .order-art span { color: var(--accent); font-size: 1.35rem; font-weight: 800; }
  .order-main { display: grid; gap: .35rem; min-width: 0; }
  .order-main h3 { display: flex; align-items: baseline; flex-wrap: wrap; column-gap: .5rem; row-gap: .1rem; margin: 0; color: var(--text); font-size: 1rem; }
  .order-name-en { color: var(--text-muted); font-size: .8125rem; font-weight: 600; }
  .order-name-en::before { content: "·"; margin-inline-end: .5rem; color: var(--border-strong); }
  .order-main > div { display: flex; align-items: center; flex-wrap: wrap; gap: .5rem; }
  .order-main > div > span { border-radius: 999px; padding: .15rem .4rem; background: oklch(0.92 0.055 78); color: oklch(0.43 0.075 68); font-size: .6875rem; font-weight: 700; }
  .order-main > div > span.visible { background: var(--success-soft); color: oklch(0.37 0.08 145); }
  .order-main small { color: var(--text-muted); }
  .order-actions { display: flex; align-items: start; gap: .6rem; }
  .orders-empty { display: grid; justify-items: start; gap: .55rem; border-radius: .55rem; padding: .8rem; background: var(--surface-2); }
  .orders-empty h3, .orders-empty p { margin: 0; }
  .orders-empty p { color: var(--text-muted); }
  button.secondary, button.danger-secondary { background: transparent; }
  button.danger-secondary { border-color: var(--danger); color: var(--danger); }
  button.danger-primary { border-color: var(--danger); background: var(--danger); color: oklch(0.985 0.009 84); }
  .confirmation-card { box-shadow: 0 0 0 1px var(--accent), var(--shadow-sm); background: var(--accent-soft); scroll-margin-block: 1rem; }
  .confirmation-card.destructive { box-shadow: 0 0 0 1px var(--danger), var(--shadow-sm); background: var(--danger-soft); }
  .confirmation-card h2:focus-visible { outline: .1875rem solid var(--gold); outline-offset: .1875rem; }
  .confirmation-check { border-radius: .6rem; padding: .45rem .6rem; background: oklch(1 0 0 / 0.35); }
  .inline-error { margin: 0; color: var(--danger); font-size: .86rem; line-height: 1.45; }
  @media (max-width: 44rem) {
    input, .check-field { min-height: 2.5rem; }
    input { font-size: 1rem; }
    .profile-card, .order-row { align-items: stretch; grid-template-columns: minmax(0, 1fr); flex-direction: column; }
    .profile-card button, .orders-toolbar button { width: 100%; }
    .connect-fields, .numeric-fields { grid-template-columns: minmax(0, 1fr); }
    .order-art { width: 100%; height: 6rem; }
    .order-actions { flex-direction: column; }
    .order-actions button { width: 100%; }
  }
  @media (forced-colors: active) {
    .profile-card, .connect-card, .orders-card, .edit-card, .confirmation-card, .order-row { border: 1px solid CanvasText; box-shadow: none; }
  }
</style>
