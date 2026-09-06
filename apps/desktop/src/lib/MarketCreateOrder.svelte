<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import { accountActionErrorMessage, validateListingNumbers, type AccountView } from "./account";
  import { variantLabel, type MarketSearchRow } from "./market";
  import { buyListingInput, existingBuyOrder } from "./marketBuyOrder";

  export let row: MarketSearchRow;
  export let onCreated: (() => void | Promise<void>) | undefined = undefined;

  let dialog: HTMLDialogElement;
  let draft: MarketSearchRow | null = null;
  let account: AccountView | null = null;
  let loading = false;
  let saving = false;
  let loadError = "";
  let error = "";
  let success = "";
  let price: number | undefined = undefined;
  let quantity: number | undefined = 1;
  let visible = true;
  let request = 0;
  const money = (value: number) => value.toLocaleString("ru-RU", { maximumFractionDigits: 1 }) + " пл.";

  $: duplicate = draft && account ? existingBuyOrder(draft, account.orders) : null;
  $: validation = validateListingNumbers(price ?? NaN, quantity ?? NaN, null);
  $: total = price !== undefined && quantity !== undefined && !validation ? price * quantity : null;
  $: canCreate = !!account?.connected && !!account.profile?.verification && !duplicate && !validation && !loading && !saving;

  async function readAccount(): Promise<AccountView> {
    let timeout: ReturnType<typeof setTimeout>;
    try {
      return await Promise.race([
        invoke<AccountView>("account_status"),
        new Promise<never>((_, reject) => { timeout = setTimeout(() => reject(new Error("account_timeout")), 20_000); }),
      ]);
    } finally { clearTimeout(timeout!); }
  }

  async function loadAccount(): Promise<void> {
    const generation = ++request;
    loading = true;
    loadError = "";
    try {
      const result = await readAccount();
      if (generation === request) account = result;
    } catch {
      if (generation === request) loadError = "Не удалось проверить аккаунт Warframe Market. Проверьте подключение к интернету и повторите загрузку.";
    } finally { if (generation === request) loading = false; }
  }

  async function open(): Promise<void> {
    draft = structuredClone(row);
    account = null;
    error = "";
    success = "";
    visible = true;
    quantity = 1;
    const estimate = draft.recommendation.quickSell;
    price = estimate !== null && estimate >= 1 ? Math.round(estimate) : undefined;
    await tick();
    dialog.showModal();
    await loadAccount();
  }

  function close(): void { if (!saving) { request += 1; dialog.close(); } }

  async function save(): Promise<void> {
    if (!canCreate || !draft || price === undefined || quantity === undefined) return;
    saving = true;
    error = "";
    const accountId = account?.profile?.id;
    const input = buyListingInput(draft, price, quantity, visible);
    try {
      // Повторная проверка ловит смену аккаунта и уже отправленную заявку,
      // если предыдущая попытка потеряла ответ после успешного запроса.
      const latest = await readAccount();
      if (!latest.connected || !latest.profile?.verification || latest.profile.id !== accountId) {
        account = latest;
        error = "Аккаунт изменился или больше не подтверждён. Закройте это окно и проверьте подключение в разделе «Мои объявления».";
        return;
      }
      account = latest;
      if (existingBuyOrder(draft, latest.orders)) {
        error = "Заявка для этого варианта уже есть. Измените её в разделе «Мои объявления» → «Покупка».";
        return;
      }
      await invoke("account_create_listing", { input, confirmed: true });
      success = `Заявка на покупку размещена: ${quantity} шт. по ${money(price)}${visible ? " Её видят продавцы." : " Она пока скрыта от продавцов."}`;
      dialog.close();
      // Ошибка обновления родительского экрана не означает ошибку публикации.
      try { await onCreated?.(); } catch { /* Заявка уже успешно создана. */ }
    } catch (reason) {
      error = accountActionErrorMessage(String(reason));
    } finally { saving = false; }
  }
</script>

<button class="secondary buy-order-button" onclick={open}>Разместить заявку на покупку</button>
{#if success}<p class="creation-success" role="status">{success}</p>{/if}

<dialog class="buy-order-dialog" bind:this={dialog} aria-labelledby="buy-order-heading" oncancel={event => { if (saving) event.preventDefault(); else request += 1; }}>
  <div class="dialog-heading"><h2 id="buy-order-heading">Заявка на покупку</h2><button class="close-dialog" aria-label="Закрыть создание заявки" disabled={saving} onclick={close}>×</button></div>
  {#if draft}
    <div class="buy-item">{#if draft.imageUrl}<img src={draft.imageUrl} alt="" />{/if}<div><strong>{draft.displayName}</strong>{#if draft.displayNameEn && draft.displayNameEn !== draft.displayName}<span class="english" lang="en" translate="no">{draft.displayNameEn}</span>{/if}<span class="variant">{variantLabel(draft.recommendation.key)}</span></div></div>
  {/if}
  {#if loading}<p class="dialog-state" role="status">Проверяем подключение к Warframe Market…</p>
  {:else if loadError}<div class="dialog-state"><p class="error" role="alert">{loadError}</p><button class="secondary" onclick={loadAccount}>Проверить аккаунт снова</button></div>
  {:else if !account?.connected}<div class="dialog-state"><strong>Подключите аккаунт Warframe Market</strong><p>Откройте «Рынок» → «Мои объявления» и войдите в аккаунт. После этого можно размещать заявки из поиска.</p><button class="secondary" onclick={close}>Понятно</button></div>
  {:else if !account.profile?.verification}<div class="dialog-state"><strong>Подтвердите игровой аккаунт</strong><p>Warframe Market разрешает размещать заявки после подтверждения игрового имени в профиле на сайте. Затем проверьте подключение снова.</p><button class="secondary" onclick={loadAccount}>Проверить подтверждение</button></div>
  {:else if duplicate}<div class="dialog-state"><strong>Заявка на этот вариант уже размещена</strong><p>{duplicate.quantity} шт. по {money(duplicate.platinum)} · {duplicate.visible ? "видна продавцам" : "скрыта"}.</p><p>Изменить цену, количество и показ можно в «Мои объявления» → «Покупка».</p><button class="secondary" onclick={close}>Понятно</button></div>
  {:else}
    <form onsubmit={event => { event.preventDefault(); void save(); }}>
      <p class="dialog-intro">Продавцы увидят, какой предмет вы ищете и сколько готовы заплатить.</p>
      <div class="order-fields"><label>Цена за штуку, платина<input type="number" min="1" max="900000" step="1" required bind:value={price} disabled={saving} inputmode="numeric" /></label><label>Сколько хотите купить<input type="number" min="1" max="9999" step="1" required bind:value={quantity} disabled={saving} inputmode="numeric" /></label></div>
      <label class="visible-option"><input type="checkbox" bind:checked={visible} disabled={saving} />Показывать заявку продавцам</label>
      <div class="order-review" aria-live="polite"><div><span>Общая сумма</span><strong>{total === null ? "Укажите цену и количество" : money(total)}</strong></div><p>Аккаунт: <strong>{account.profile.ingameName}</strong> · {visible ? "Заявка будет видна" : "Заявка будет скрыта"}</p></div>
      <p class="no-payment">Размещение заявки не списывает платину. Покупку вы завершаете с продавцом в игре.</p>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      {#if validation && price !== undefined && quantity !== undefined}<p class="error" role="status">{validation}</p>{/if}
      <div class="dialog-actions"><button type="submit" disabled={!canCreate}>{saving ? "Размещаем заявку…" : "Разместить заявку"}</button><button type="button" class="secondary" disabled={saving} onclick={close}>Отмена</button></div>
    </form>
  {/if}
</dialog>

<style>
  .buy-order-button { font-size: .8125rem; }
  .creation-success { margin: .5rem 0; color: var(--success); font-size: .8125rem; line-height: 1.45; }
  .buy-order-dialog { width: min(31rem, calc(100vw - 2rem)); max-height: calc(100dvh - 2rem); overflow-y: auto; box-sizing: border-box; padding: 1.2rem; border: 1px solid var(--border-strong); border-radius: .85rem; color: var(--text); background: var(--surface-1); box-shadow: 0 1.5rem 5rem rgb(0 0 0 / .25); }
  .buy-order-dialog::backdrop { background: rgb(32 23 16 / .4); }
  .dialog-heading { display: flex; align-items: center; justify-content: space-between; gap: .7rem; }
  h2 { margin: 0; font-size: 1.15rem; }
  .close-dialog { width: 2rem; height: 2rem; min-height: 2rem; padding: 0; background: transparent; color: var(--text-muted); border: 1px solid transparent; box-shadow: none; font-size: 1.4rem; }
  .close-dialog:hover { border-color: var(--border); background: var(--surface-2); }
  .buy-item { display: flex; align-items: center; gap: .75rem; padding: .85rem 0; border-bottom: 1px solid var(--border); margin-bottom: .85rem; }
  .buy-item img { width: 2.8rem; height: 3.3rem; object-fit: contain; flex: 0 0 auto; }
  .buy-item strong { font-size: .95rem; overflow-wrap: anywhere; }
  .english, .variant { display: block; color: var(--text-muted); font-size: .75rem; margin-top: .15rem; }
  .dialog-intro, .dialog-state p { margin: .5rem 0 .85rem; color: var(--text-muted); font-size: .8125rem; line-height: 1.5; }
  .dialog-state { padding: .6rem 0; font-size: .85rem; }
  .order-fields { display: grid; grid-template-columns: 1fr 1fr; gap: .7rem; }
  .order-fields label { display: grid; gap: .35rem; min-width: 0; font-size: .75rem; color: var(--text-muted); font-weight: 650; }
  .order-fields input { width: 100%; min-width: 0; box-sizing: border-box; padding: .5rem .6rem; font-size: .95rem; border: 1px solid var(--border-strong); border-radius: .4rem; color: var(--text); background: var(--surface-1); }
  .visible-option { display: flex; gap: .55rem; align-items: center; margin: .85rem 0; font-size: .8rem; }
  .visible-option input { width: 1rem; height: 1rem; accent-color: var(--accent); margin: 0; }
  .order-review { padding: .75rem; border-radius: .5rem; background: var(--surface-2); }
  .order-review > div { display: flex; align-items: baseline; justify-content: space-between; gap: .7rem; }
  .order-review > div > span { font-size: .8rem; color: var(--text-muted); }
  .order-review > div > strong { font-size: 1rem; text-align: right; font-variant-numeric: tabular-nums; }
  .order-review p { margin: .45rem 0 0; color: var(--text-muted); font-size: .75rem; }
  .no-payment { color: var(--text-muted); font-size: .75rem; line-height: 1.45; margin: .65rem 0 1rem; }
  .error { color: var(--danger); font-size: .8rem; line-height: 1.45; }
  .dialog-actions { display: flex; flex-wrap: wrap; gap: .5rem; }
  .dialog-actions button { font-size: .8rem; }
  @media (max-width: 30rem) { .order-fields { grid-template-columns: 1fr; } .order-review > div { align-items: start; flex-direction: column; gap: .25rem; } }
</style>
