<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { AccountView } from "./account";
  import { isSaleTrade, planTradeReconciliation, type TradeEvent, type TradeSalesSummary } from "./tradeShift";
  import {
    filterTradeHistory, historyItems, tradeHistoryCsv, tradeHistoryKind, tradeReconciliationActions,
    tradeWasClosed, type TradeHistoryKind, type TradeHistoryPeriod,
  } from "./marketTradeHistory";

  export let events: TradeEvent[] = [];
  export let account: AccountView | null = null;
  export let unavailable = false;
  export let salesSummary: TradeSalesSummary | null = null;
  export let busy = false;
  export let onRetry: (event: TradeEvent) => void | Promise<void>;
  export let onIgnore: (event: TradeEvent) => void | Promise<void>;
  export let onRestore: (event: TradeEvent) => void | Promise<void>;
  export let onUndo: (event: TradeEvent) => void | Promise<void>;
  export let onReload: () => void | Promise<void>;

  let query = "";
  let kind: TradeHistoryKind = "all";
  let period: TradeHistoryPeriod = "all";
  let page = 1;
  let profileBusy: string | null = null;
  let profileMessage = "";
  let profileFailed = false;
  const pageSize = 25;
  const kindLabels = { sell: "Продажа", buy: "Покупка", exchange: "Обмен" };
  const dateFormatter = new Intl.DateTimeFormat("ru-RU", { day: "numeric", month: "short" });
  const timeFormatter = new Intl.DateTimeFormat("ru-RU", { hour: "2-digit", minute: "2-digit" });
  const platinum = new Intl.NumberFormat("ru-RU", { maximumFractionDigits: 1 });

  $: filtered = filterTradeHistory(events, query, kind, period, account);
  $: pageCount = Math.max(1, Math.ceil(filtered.length / pageSize));
  $: page = Math.min(page, pageCount);
  $: shown = filtered.slice((page - 1) * pageSize, page * pageSize);
  $: filterKey = `${query}\u0000${kind}\u0000${period}`;
  $: if (filterKey) page = 1;

  function eventCanApply(event: TradeEvent): boolean {
    if (!account) return false;
    const plan = planTradeReconciliation(event, account);
    return plan.actions.length > 0 && plan.unmatched.length === 0 && plan.unsafe.length === 0;
  }

  function status(event: TradeEvent): string {
    if (!isSaleTrade(event)) return "Записано";
    if (event.status === "ignored") return "Без изменения объявления";
    if (event.status === "pending") return eventCanApply(event) ? "Можно учесть продажу" : "Нужна проверка";
    if (tradeWasClosed(event)) return "Продажа учтена на рынке";
    return event.reconciliationJson ? "Объявление обновлено" : "Записано";
  }

  function clearFilters(): void { query = ""; kind = "all"; period = "all"; }

  async function openPartner(partner: string): Promise<void> {
    profileBusy = partner;
    profileMessage = "";
    profileFailed = false;
    try {
      await invoke("open_trade_partner_profile", { ingameName: partner });
      profileMessage = `Профиль ${partner} открыт в браузере. Отзыв можно оставить на странице игрока.`;
    } catch (error) {
      profileFailed = true;
      const reason = String(error);
      profileMessage = reason.includes("profile_not_found")
        ? `Не удалось найти профиль по игровому нику «${partner}». На Warframe Market он может отличаться.`
        : reason.includes("profile_name_mismatch")
          ? `Найденный профиль не совпадает с игровым ником «${partner}». Проверьте имя партнёра перед переходом.`
          : `Не удалось открыть профиль ${partner}. Попробуйте ещё раз через несколько секунд.`;
    } finally {
      profileBusy = null;
    }
  }

  function exportCsv(): void {
    const blob = new Blob([tradeHistoryCsv(filtered, account)], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `PlatScope-сделки-${new Date().toISOString().slice(0, 10)}.csv`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1_000);
  }
</script>

<section class="market-history" aria-label="История сделок">
  <div class="history-heading">
    <div><p>Обмены из игры. Нажмите на имя игрока, чтобы открыть его профиль на Warframe Market.</p></div>
    <button class="secondary export-button" disabled={!filtered.length || unavailable} onclick={exportCsv} title="Скачать выбранные сделки в таблицу">Скачать CSV</button>
  </div>

  {#if salesSummary}<p class="history-total">Получено за все записанные продажи: <strong>{platinum.format(salesSummary.platinumReceived)} пл.</strong> · продаж: {salesSummary.saleCount}</p>{/if}
  <div class="history-filters">
    <label class="search">Найти сделку<input type="search" bind:value={query} placeholder="Предмет или имя игрока" /></label>
    <label>Тип сделки<select bind:value={kind}><option value="all">Все сделки</option><option value="sell">Продажи</option><option value="buy">Покупки</option><option value="exchange">Обмены</option></select></label>
    <label>Период<select bind:value={period}><option value="all">За всё время</option><option value="7">Последние 7 дней</option><option value="30">Последние 30 дней</option></select></label>
    <span class="history-count" aria-live="polite">Найдено: {filtered.length}</span>
  </div>

  {#if profileMessage}<p class:failed={profileFailed} class="profile-message" role={profileFailed ? "alert" : "status"}>{profileMessage}</p>{/if}
  {#if unavailable}
    <div class="history-empty" role="alert"><strong>История временно недоступна</strong><p>Сохранённые сделки появятся после успешной загрузки.</p><button class="secondary" disabled={busy} onclick={onReload}>Загрузить историю снова</button></div>
  {:else if !filtered.length}
    <div class="history-empty"><strong>{events.length ? "Таких сделок нет" : "Здесь появятся ваши сделки"}</strong><p>{events.length ? "Измените имя, предмет или период поиска." : "PlatScope запишет обмены, которые вы завершите в игре при запущенном приложении."}</p>{#if events.length}<button class="secondary" onclick={clearFilters}>Сбросить фильтры</button>{/if}</div>
  {:else}
    <div class="history-table-wrap">
      <table class="history-table">
        <thead><tr><th>Когда</th><th>Сделка</th><th>Предметы</th><th>Игрок</th><th>Учёт на рынке</th></tr></thead>
        <tbody>
          {#each shown as event (event.id)}
            {@const eventKind = tradeHistoryKind(event)}
            {@const items = historyItems(event, account)}
            {@const pending = isSaleTrade(event) && event.status === "pending"}
            <tr class:needs-review={pending}>
              <td class="event-date" title={new Date(event.occurredAt).toLocaleString("ru-RU")}><time datetime={event.occurredAt}>{dateFormatter.format(new Date(event.occurredAt))}<span>{timeFormatter.format(new Date(event.occurredAt))}</span></time></td>
              <td class="event-money"><span class="event-kind">{kindLabels[eventKind]}</span>{#if event.platinumReceived > 0}<strong class="received">+{platinum.format(event.platinumReceived)} пл.</strong>{/if}{#if event.platinumGiven > 0}<strong>−{platinum.format(event.platinumGiven)} пл.</strong>{/if}{#if !event.platinumReceived && !event.platinumGiven}<span class="no-platinum">Без платины</span>{/if}</td>
              <td class="event-items">
                {#each items.slice(0, 2) as item}
                  <div class="history-item"><span>{#if eventKind === "exchange"}<span class="direction">{item.direction === "given" ? "Отдано" : "Получено"}: </span>{/if}<strong>{item.name}</strong><span class="item-quantity"> ×{item.quantity}</span></span>{#if item.english}<small lang="en">{item.english}</small>{/if}</div>
                {/each}
                {#if items.length > 2}<details class="more-items"><summary>Ещё предметов: {items.length - 2}</summary>{#each items.slice(2) as item}<div class="history-item"><span>{#if eventKind === "exchange"}<span class="direction">{item.direction === "given" ? "Отдано" : "Получено"}: </span>{/if}<strong>{item.name}</strong> ×{item.quantity}</span>{#if item.english}<small lang="en">{item.english}</small>{/if}</div>{/each}</details>{/if}
                {#if !items.length}<span class="muted">Передача платины</span>{/if}
              </td>
              <td class="event-partner">{#if event.partner}<button class="partner-link" disabled={profileBusy !== null} onclick={() => event.partner && openPartner(event.partner)} aria-label={`Открыть профиль ${event.partner} на Warframe Market`} title="Открыть профиль на Warframe Market"><span>{event.partner}</span><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3H3v10h10v-3M8 3h5v5M13 3 7 9" /></svg></button>{#if profileBusy === event.partner}<small aria-live="polite">Проверяем профиль…</small>{/if}{:else}<span class="muted">Имя не записано</span>{/if}</td>
              <td class="event-actions"><span class:pending class:done={!pending && event.status === "reconciled"} class="event-status">{status(event)}</span>
                {#if pending}<div class="pending-actions"><button class="secondary compact-button" disabled={busy || !account?.profile?.verification} onclick={() => onRetry(event)}>{eventCanApply(event) ? "Учесть продажу" : "Проверить"}</button><button class="text-button" disabled={busy} onclick={() => onIgnore(event)}>Пропустить</button></div>{/if}
                {#if (isSaleTrade(event) && event.status === "ignored") || (event.status === "reconciled" && tradeReconciliationActions(event).length && !tradeWasClosed(event)) || pending}
                  <details class="event-details"><summary>Подробности</summary>{#if pending}<p>Продажа записана в игре, но ещё не учтена в объявлении. {eventCanApply(event) ? "Подходящее объявление найдено." : "Нужно проверить предмет, его вариант и количество в объявлениях."}</p>{:else if event.status === "ignored"}<p>Эта сделка сохранена без изменения объявления.</p><button class="text-button" disabled={busy} onclick={() => onRestore(event)}>Вернуть к проверке</button>{:else}<p>Количество в объявлении изменено после этой сделки.</p><button class="text-button" disabled={busy} onclick={() => onUndo(event)}>Отменить изменение</button>{/if}</details>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <footer class="history-footer"><p>{events.length >= 1000 ? "Показаны последние 1 000 сделок из игры." : "Источник — обмены, записанные PlatScope в игре."} Цены в этой истории — суммы ваших сделок.</p>{#if pageCount > 1}<nav aria-label="Страницы истории сделок"><button class="secondary" disabled={page === 1} onclick={() => page -= 1}>Назад</button><span>{page} / {pageCount}</span><button class="secondary" disabled={page === pageCount} onclick={() => page += 1}>Далее</button></nav>{/if}</footer>
  {/if}
</section>

<style>
  .history-total { margin:0 0 .6rem; font-size:.8125rem; color:var(--text-muted); } .history-total strong { color:var(--text); }
  .market-history { min-width: 0; }
  .history-heading { display: flex; align-items: center; justify-content: space-between; gap: 1rem; margin-bottom: .85rem; }
  .history-heading p { margin: .25rem 0 0; color: var(--text-muted); font-size: .8rem; line-height: 1.45; }
  .export-button { flex: 0 0 auto; }
  .history-filters { display: grid; grid-template-columns: minmax(13rem, 1fr) minmax(9rem, .3fr) minmax(11rem, .3fr) auto; align-items: end; gap: .65rem; padding: .75rem; border: 1px solid var(--border); border-radius: .7rem; background: var(--surface-1); margin-bottom: .6rem; }
  .history-filters label { display: grid; gap: .25rem; color: var(--text-muted); font-size: .75rem; font-weight: 650; min-width: 0; }
  .history-filters input, .history-filters select { width: 100%; min-height: 2.1rem; font-size: .8rem; }
  .history-count { padding: .55rem .25rem; font-size: .8rem; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .history-table-wrap { border: 1px solid var(--border); border-radius: .7rem; background: var(--surface-1); overflow-x: auto; }
  .history-table { width: 100%; border-collapse: collapse; text-align: left; table-layout: fixed; }
  th { padding: .5rem .75rem; background: var(--surface-2); font-size: .75rem; color: var(--text-muted); font-weight: 650; }
  th:first-child { width: 5.5rem; }
  th:nth-child(2) { width: 7.2rem; }
  th:nth-child(4) { width: 12rem; }
  th:nth-child(5) { width: 15rem; }
  td { padding: .65rem .75rem; vertical-align: top; border-top: 1px solid var(--border); font-size: .8rem; }
  tr:hover td { background: var(--surface-2); }
  tr.needs-review td:first-child { box-shadow: inset 3px 0 var(--accent); }
  .event-date { color: var(--text-muted); font-variant-numeric: tabular-nums; font-size: .75rem; }
  .event-date span { display: block; margin-top: .2rem; }
  .event-kind { display: block; color: var(--text-muted); font-size: .75rem; margin-bottom: .15rem; }
  .event-money strong { display: block; font-variant-numeric: tabular-nums; font-size: .9rem; white-space: nowrap; }
  .event-money strong.received { color: var(--success); }
  .no-platinum, .muted { color: var(--text-muted); font-size: .75rem; }
  .history-item + .history-item { margin-top: .35rem; }
  .history-item { overflow-wrap: anywhere; line-height: 1.35; }
  .history-item strong { font-weight: 650; }
  .history-item small { display: block; color: var(--text-muted); font-size: .75rem; margin-top: .1rem; }
  .item-quantity { white-space: nowrap; font-variant-numeric: tabular-nums; }
  .direction { color: var(--text-muted); }
  .partner-link { display: inline-flex; max-width: 100%; align-items: start; gap: .3rem; padding: 0; border: 0; border-radius: .2rem; background: transparent; color: var(--accent-strong); text-align: left; font-size: .8rem; font-weight: 650; box-shadow: none; min-height: 1.5rem; line-height: 1.4; }
  .partner-link span { overflow-wrap: anywhere; }
  .partner-link:hover { text-decoration: underline; }
  .partner-link svg { flex: 0 0 .9rem; width: .9rem; height: .9rem; margin-top: .1rem; fill: none; stroke: currentColor; stroke-width: 1.5; }
  .event-partner small { display: block; font-size: .75rem; color: var(--text-muted); }
  .event-status { display: block; font-size: .75rem; color: var(--text-muted); line-height: 1.4; }
  .event-status.pending { color: var(--accent-strong); font-weight: 650; }
  .event-status.done { color: var(--success); }
  .pending-actions { display: flex; gap: .5rem; align-items: center; flex-wrap: wrap; margin-top: .35rem; }
  .compact-button { padding: .25rem .5rem; font-size: .75rem; min-height: 1.8rem; }
  .text-button { padding: .2rem 0; border: 0; border-radius: .2rem; background: transparent; color: var(--accent-strong); text-decoration: underline; font-size: .75rem; box-shadow: none; }
  .event-details, .more-items { margin-top: .3rem; font-size: .75rem; }
  .event-details summary, .more-items summary { cursor: pointer; color: var(--text-muted); width: fit-content; }
  .event-details p { font-size: .75rem; color: var(--text-muted); line-height: 1.4; margin: .35rem 0; }
  .more-items .history-item { margin-top: .35rem; }
  .profile-message { padding: .65rem .8rem; border-radius: .5rem; background: var(--success-soft); color: var(--success); font-size: .8rem; }
  .profile-message.failed { background: var(--danger-soft); color: var(--danger); }
  .history-empty { padding: 2.5rem 1rem; border: 1px solid var(--border); border-radius: .7rem; background: var(--surface-1); text-align: center; }
  .history-empty p { color: var(--text-muted); font-size: .85rem; }
  .history-footer { display: flex; align-items: center; justify-content: space-between; gap: 1rem; margin-top: .65rem; }
  .history-footer p { font-size: .75rem; color: var(--text-muted); max-width: 42rem; margin: 0; line-height: 1.4; }
  .history-footer nav { display: flex; align-items: center; gap: .6rem; flex: 0 0 auto; font-size: .8rem; }
  .history-footer button { padding: .35rem .65rem; }
  @media (max-width: 85rem) { th:nth-child(4) { width: 9rem; } th:nth-child(5) { width: 12rem; } }
  @media (max-width: 60rem) {
    .history-filters { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
    .history-filters .search { grid-column: 1 / -1; }
    .history-count { grid-column: 1 / -1; padding: .15rem 0; }
    .history-table { table-layout: auto; min-width: 42rem; }
    th:first-child { width: 4.5rem; } th:nth-child(2) { width: 6rem; } th:nth-child(4) { width: 8rem; } th:nth-child(5) { width: 11rem; }
    td, th { padding-left: .5rem; padding-right: .5rem; }
    .event-items { min-width: 11rem; }
    .history-footer { flex-wrap: wrap; }
  }
</style>
