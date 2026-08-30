<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  import type { AccountView } from "./account";
  import { useLocale } from "./i18n";

  export let onOpenMarketSales: () => void;

  const locale = useLocale();
  const copy = {
    ru: {
      loading: "Проверяем подключение…",
      loadError: "Не удалось проверить подключение. Повторите попытку.",
      connecting: "Подключаем аккаунт…",
      connected: "Аккаунт подключён.",
      connectError: "Не удалось войти. Проверьте email и пароль Warframe Market.",
      disconnecting: "Отключаем аккаунт…",
      disconnected: "Аккаунт отключён от PlatScope.",
      localRemoved: "Данные входа удалены с этого компьютера. Warframe Market не подтвердил завершение сессии.",
      disconnectError: "Не удалось удалить данные входа. Повторите попытку.",
      retry: "Проверить ещё раз",
      connectHeading: "Подключить Warframe Market",
      connectBody: "Подключение нужно для публикации и управления ордерами из PlatScope.",
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
      verifyNote: "Warframe Market разрешает менять ордера только после подтверждения игрового аккаунта. Просматривать их можно уже сейчас.",
      sellOrders: "Ордеров на продажу",
      salesBody: "Ордера, цены и сделки находятся в разделе «Рынок» → «Мои продажи».",
      openSales: "Открыть мои продажи",
      refresh: "Обновить статус",
      disconnect: "Отключить аккаунт",
      technical: "Технические подробности",
    },
    en: {
      loading: "Checking connection…",
      loadError: "Unable to check the connection. Try again.",
      connecting: "Connecting account…",
      connected: "Account connected.",
      connectError: "Unable to sign in. Check your Warframe Market email and password.",
      disconnecting: "Disconnecting account…",
      disconnected: "Account disconnected from PlatScope.",
      localRemoved: "Sign-in data was removed from this computer. Warframe Market did not confirm that the remote session ended.",
      disconnectError: "Unable to remove sign-in data. Try again.",
      retry: "Check again",
      connectHeading: "Connect Warframe Market",
      connectBody: "Connect your account to publish and manage orders from PlatScope.",
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
      verifyNote: "Warframe Market allows order changes only after the game account is verified. You can still view orders now.",
      sellOrders: "Sell orders",
      salesBody: "Orders, prices, and trades are available under Market → My sales.",
      openSales: "Open my sales",
      refresh: "Refresh status",
      disconnect: "Disconnect account",
      technical: "Technical details",
    },
  } as const;
  $: c = copy[$locale];
  $: sellOrderCount = view?.orders.filter((order) => order.type === "sell").length ?? 0;

  let view: AccountView | null = null;
  let loading = true;
  let busy = false;
  let errorMessage = "";
  let errorDetail = "";
  let statusMessage = "";
  let email = "";
  let password = "";

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
      statusMessage = remotelyRevoked ? c.disconnected : c.localRemoved;
    } catch (error) {
      statusMessage = "";
      errorMessage = c.disconnectError;
      errorDetail = String(error);
    } finally {
      busy = false;
    }
  }

  onMount(() => void loadAccount());
</script>

<section class="account-shell" aria-busy={loading || busy}>
  <div class="account-status" role="status" aria-live="polite">{loading ? c.loading : statusMessage}</div>

  {#if errorMessage}
    <div class="account-error" role="alert">
      <p>{errorMessage}</p>
      {#if errorDetail}<details><summary>{c.technical}</summary><code>{errorDetail}</code></details>{/if}
      <button type="button" onclick={loadAccount}>{c.retry}</button>
    </div>
  {/if}

  {#if !loading && !view?.connected}
    <form class="connect-card" onsubmit={connectAccount}>
      <div><h2>{c.connectHeading}</h2><p>{c.connectBody}</p></div>
      <div class="connect-fields">
        <div class="field-group"><label for="wfm-email">{c.email}</label><input id="wfm-email" name="username" type="email" autocomplete="username" spellcheck="false" bind:value={email} required maxlength="128" placeholder="name@example.com" /></div>
        <div class="field-group"><label for="wfm-password">{c.password}</label><input id="wfm-password" name="password" type="password" autocomplete="current-password" bind:value={password} required maxlength="128" /></div>
      </div>
      <button type="submit" disabled={busy}>{busy ? c.connectBusy : c.connect}</button>
      <details class="security-details">
        <summary>{c.securitySummary}</summary><p>{c.securityBody}</p>
        <details><summary>{c.connectionDetails}</summary><p>{c.legacyNote}</p></details>
      </details>
    </form>
  {:else if view?.connected && view.profile}
    <section class="profile-card" aria-labelledby="profile-heading">
      <div>
        <p class="section-kicker">{c.connectedLabel}</p>
        <h2 id="profile-heading"><span translate="no">{view.profile.ingameName}</span></h2>
        <div class="profile-state"><span>{view.profile.platform.toUpperCase()}</span><span>{view.profile.crossplay ? c.crossplayOn : c.crossplayOff}</span><span class:warning={!view.profile.verification}>{view.profile.verification ? c.verified : c.unverified}</span></div>
      </div>
      <button type="button" class="danger-secondary compact" onclick={disconnectAccount} disabled={busy}>{c.disconnect}</button>
    </section>

    {#if !view.profile.verification}<div class="account-note" role="note">{c.verifyNote}</div>{/if}

    <section class="sales-link" aria-labelledby="sales-link-heading">
      <div><span>{c.sellOrders}</span><strong>{sellOrderCount}</strong></div>
      <p id="sales-link-heading">{c.salesBody}</p>
      <div class="sales-link__actions"><button type="button" onclick={onOpenMarketSales}>{c.openSales}</button><button class="secondary" type="button" onclick={loadAccount} disabled={busy}>{c.refresh}</button></div>
    </section>
  {/if}
</section>

<style>
  .account-shell { display: grid; gap: .7rem; max-width: 58rem; }
  .account-status { min-height: 1rem; color: var(--text-muted); font-size: .72rem; }
  .account-status:empty { display: none; }
  .profile-card, .connect-card, .sales-link { border: 1px solid var(--border); border-radius: .7rem; padding: .8rem; background: var(--surface-1); box-shadow: var(--shadow-sm); }
  .profile-card { display: flex; align-items: flex-start; justify-content: space-between; gap: .8rem; }
  .profile-card h2, .connect-card h2 { margin-block-end: .25rem; font-size: 1.1rem; }
  .connect-card { display: grid; justify-items: start; gap: .7rem; }
  .connect-card p, .sales-link p { max-width: 68ch; margin: 0; color: var(--text-muted); font-size: .78rem; line-height: 1.45; }
  .connect-fields { display: grid; grid-template-columns: repeat(2, minmax(12rem, 1fr)); gap: .6rem; width: min(100%, 38rem); }
  .field-group { display: grid; gap: .25rem; }
  .field-group label { color: var(--text-muted); font-size: .72rem; font-weight: 650; }
  input { min-width: 0; min-height: 2.25rem; border: 1px solid var(--border); border-radius: .5rem; padding: .4rem .6rem; background: oklch(0.995 0.004 84); color: var(--text); }
  .security-details { width: min(100%, 38rem); border-radius: .5rem; padding: .55rem; background: var(--surface-2); color: var(--text-muted); font-size: .75rem; }
  .security-details summary { cursor: pointer; font-weight: 700; }
  .security-details p { margin: .5rem 0 0; }
  .section-kicker { margin: 0 0 .12rem; color: var(--accent); font-size: .65rem; font-weight: 800; letter-spacing: .08em; text-transform: uppercase; }
  .profile-state { display: flex; flex-wrap: wrap; gap: .35rem; }
  .profile-state span { border-radius: 999px; padding: .16rem .42rem; background: var(--success-soft); color: var(--success); font-size: .67rem; font-weight: 700; }
  .profile-state span.warning { background: var(--danger-soft); color: var(--danger); }
  .account-error, .account-note { border: 1px solid var(--border); border-radius: .55rem; padding: .7rem; background: var(--surface-2); font-size: .78rem; }
  .account-error { border-color: var(--danger); background: var(--danger-soft); }
  .account-error p { margin-bottom: .5rem; }
  .account-error code { display: block; margin-block: .4rem; overflow-wrap: anywhere; }
  .sales-link { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: .8rem; }
  .sales-link > div:first-child { display: grid; min-width: 6.5rem; }
  .sales-link > div:first-child span { color: var(--text-muted); font-size: .68rem; }
  .sales-link > div:first-child strong { font-size: 1.25rem; font-variant-numeric: tabular-nums; }
  .sales-link__actions { display: flex; gap: .4rem; }
  button.compact { min-height: 1.8rem; padding: .22rem .5rem; font-size: .75rem; }
  @media (max-width: 46rem) { .profile-card, .sales-link { align-items: stretch; grid-template-columns: minmax(0, 1fr); flex-direction: column; } .connect-fields { grid-template-columns: minmax(0, 1fr); } .sales-link__actions { flex-wrap: wrap; } }
</style>
