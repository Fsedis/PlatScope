<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import { localeCode, useLocale, type UiLocale } from "./i18n";
  import { formatPlatinum } from "./market";
  import {
    compactNumber,
    visibleArcaneDecisions,
    type ArcaneConversionDecision,
    type ResourceConversionAction,
    type ResourceConversionRoute,
    type ResourceConverterView,
    type ResourceCurrency,
    type ResourceRouteStatus,
    type ResourceSource,
  } from "./resourceConverter";

  export let onOpenSettings: () => void;

  const locale = useLocale();
  const copy = {
    ru: {
      eyebrow: "Конвертер ресурсов",
      title: "Что превратить в платину сегодня",
      confirmed: "Можно получить примерно",
      confirmedHint: "Сюда входят доступные покупки и продажи со свежей ценой; слабые ценовые сигналы уменьшены с запасом.",
      expected: "ещё примерно",
      expectedHint: "математического ожидания через Восфор — это случайный результат, не гарантия",
      noConfirmed: "Подтверждённых обменов пока нет",
      noConfirmedHint: "Ни один доступный маршрут сейчас не имеет одновременно баланса, товара и свежей цены.",
      loading: "Сверяем валюты, продавцов и цены…",
      loadError: "Не удалось рассчитать конвертацию. Инвентарь и цены не изменились.",
      noData: "Сначала обновите инвентарь и данные предметов.",
      retry: "Повторить",
      settings: "Открыть настройки",
      market: "Открыть на рынке",
      opening: "Открываем…",
      opened: "Страница Warframe Market открыта.",
      openError: "Не удалось открыть Warframe Market.",
      conditional: "не входит в итог",
      balance: "Баланс",
      costs: "стоит",
      each: "за штуку",
      total: "итого",
      source: {
        syndicate: "Синдикаты",
        nightwave: "Ночная волна",
        void_trader: "Баро Ки’Тиир",
        steel_path: "Стальной путь",
      } satisfies Record<ResourceSource, string>,
      sourceHint: {
        syndicate: "Моды за репутацию",
        nightwave: "Товары за кредиты Норы",
        void_trader: "Покупки за дукаты",
        steel_path: "Недельная награда Тешина",
      } satisfies Record<ResourceSource, string>,
      status: {
        ready: "Можно сделать",
        conditional: "Сначала проверь",
        waiting: "Нужно подождать",
        unavailable: "Сейчас невыгодно",
        needs_data: "Нужны данные",
      } satisfies Record<ResourceRouteStatus, string>,
      currency: {
        standing: "репутации",
        nightwave_cred: "кредитов Норы",
        ducat: "дукатов",
        steel_essence: "стальной эссенции",
      } satisfies Record<ResourceCurrency, string>,
      arcanes: "Мистификаторы",
      arcanesHint: "Лишние копии после вашего резерва",
      bestPack: "Лучший набор Лойда сейчас",
      packExpected: "в среднем за набор",
      sell: "Продать",
      dissolve: "Растворить",
      hold: "Не трогать",
      rank: "ранг",
      marketValue: "рынок",
      vosforValue: "через Восфор",
      vosfor: "Восфора",
      showAll: "Показать весь список",
      showLess: "Свернуть список",
      noArcaneActions: "Лишних мистификаторов для сравнения не найдено.",
      methodology: "Что вошло в расчёт",
      methodologyBody: "Прямая продажа учитывается только при свежей цене и доступном количестве. Ассортимент Норы подтверждается во время обновления инвентаря из Warframe и учитывается до конца текущей ротации. Если получить его не удалось, вариант Ночной волны не входит в итог. Восфор считается по ожидаемой цене лучшего набора Лойда. Если для мистификатора есть только ордера продавцов без завершённых сделок, их цена уменьшается на 30%, чтобы не завышать результат.",
      inventoryDate: "Инвентарь",
      marketDate: "Рынок",
      worldstateDate: "Продавцы",
    },
    en: {
      eyebrow: "Resource converter",
      title: "What to convert into platinum today",
      confirmed: "Estimated direct value",
      confirmedHint: "Includes affordable purchases and sales with fresh prices; weaker price signals are discounted conservatively.",
      expected: "plus about",
      expectedHint: "in Vosfor expected value — random, not guaranteed",
      noConfirmed: "No confirmed conversion is available",
      noConfirmedHint: "No route currently has a balance, an available item, and a fresh price together.",
      loading: "Checking balances, vendors, and prices…",
      loadError: "Unable to calculate conversions. Inventory and prices were not changed.",
      noData: "Refresh inventory and item data first.",
      retry: "Try again",
      settings: "Open settings",
      market: "Open market",
      opening: "Opening…",
      opened: "Warframe Market page opened.",
      openError: "Unable to open Warframe Market.",
      conditional: "not included in total",
      balance: "Balance",
      costs: "cost",
      each: "each",
      total: "total",
      source: { syndicate: "Syndicates", nightwave: "Nightwave", void_trader: "Baro Ki’Teer", steel_path: "Steel Path" } satisfies Record<ResourceSource, string>,
      sourceHint: { syndicate: "Standing mods", nightwave: "Nora Cred offerings", void_trader: "Ducat purchases", steel_path: "Teshin weekly reward" } satisfies Record<ResourceSource, string>,
      status: { ready: "Ready", conditional: "Check first", waiting: "Waiting", unavailable: "No action", needs_data: "Data needed" } satisfies Record<ResourceRouteStatus, string>,
      currency: { standing: "standing", nightwave_cred: "Nora Cred", ducat: "ducats", steel_essence: "Steel Essence" } satisfies Record<ResourceCurrency, string>,
      arcanes: "Arcanes",
      arcanesHint: "Spare copies after your reserve",
      bestPack: "Best Loid pack now",
      packExpected: "expected per pack",
      sell: "Sell",
      dissolve: "Dissolve",
      hold: "Hold",
      rank: "rank",
      marketValue: "market",
      vosforValue: "via Vosfor",
      vosfor: "Vosfor",
      showAll: "Show the full list",
      showLess: "Collapse list",
      noArcaneActions: "No spare Arcanes were found for comparison.",
      methodology: "What is included",
      methodologyBody: "Direct sales are included only with a fresh price and available quantity. Nora’s stock is confirmed during a Warframe inventory refresh and remains valid until the current rotation ends. If it cannot be retrieved, Nightwave is excluded from the total. Vosfor is based on the expected value of Loid’s best pack. Sell-only Arcane prices without completed trades receive a 30% haircut so the result is not overstated.",
      inventoryDate: "Inventory",
      marketDate: "Market",
      worldstateDate: "Vendors",
    },
  } as const;
  $: c = copy[$locale];

  let view: ResourceConverterView | null = null;
  let loading = true;
  let errorMessage = "";
  let actionStatus = "";
  let openingSlug = "";
  let showAllArcanes = false;

  $: arcaneCount = view
    ? view.arcanes.sell.length + view.arcanes.dissolve.length + view.arcanes.hold.length
    : 0;

  function formatDate(value: string | null | undefined, language: UiLocale): string {
    if (!value) return "—";
    const date = new Date(value);
    return Number.isNaN(date.getTime())
      ? value
      : date.toLocaleString(localeCode(language), { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" });
  }

  function reasonText(route: ResourceConversionRoute): string {
    const ru: Record<string, string> = {
      confirmed: "Баланс, товар и цена подтверждены.",
      refresh_inventory: "Обновите инвентарь из Warframe — в старом снимке нет репутации.",
      refresh_inventory_for_credits: "Обновите инвентарь: для покупки у Баро нужен баланс кредитов.",
      refresh_item_data: "Обновите данные предметов в настройках.",
      no_accessible_priced_mod: "Нет доступного мода с подтверждённой ценой или не хватает 25 000 репутации.",
      nightwave_stock_confirmed: "Ассортимент Норы и рыночная цена подтверждены.",
      weekly_purchase_limit: "Учтён недельный лимит: одна покупка ротационной награды.",
      refresh_nightwave_stock: "Обновите инвентарь из Warframe, чтобы подтвердить ассортимент Норы.",
      no_priced_offer: "На доступный баланс не найден товар с подтверждённой ценой.",
      no_currency: "Текущих кредитов Ночной волны в инвентаре нет.",
      currency_not_resolved: "Не удалось определить валюту текущего сезона.",
      season_inactive: "Сезон сейчас не активен.",
      trader_not_arrived: "Баро ещё не прибыл.",
      trader_left: "Баро уже улетел; ждём следующую публикацию.",
      inventory_not_published: "Ассортимент Баро ещё не опубликован.",
      no_affordable_priced_offer: "В текущем ассортименте нет доступной покупки с подтверждённой ценой.",
      reward_not_tradeable: "Текущая награда не продаётся на обычном рынке.",
      reward_uses_auction_price: "Эта награда продаётся через аукцион, поэтому фиксированную цену не подставляем.",
      insufficient_balance: "На текущую недельную награду не хватает стальной эссенции.",
      no_reliable_price: "Для награды нет свежей рыночной цены.",
      rotation_inactive: "Данные недельной ротации ещё не обновились.",
      worldstate_unavailable: "Источник продавца временно недоступен.",
    };
    if ($locale === "ru") return ru[route.reason] ?? "Сейчас нет подтверждённого действия.";
    return route.reason.replaceAll("_", " ");
  }

  function instruction(route: ResourceConversionRoute, action: ResourceConversionAction): string {
    if ($locale === "en") {
      return route.source === "nightwave"
        ? route.status === "ready"
          ? `Buy ${action.quantity}× ${action.itemName} from Nora.`
          : `If available this week, buy ${action.quantity}× ${action.itemName}.`
        : `Buy ${action.quantity}× ${action.itemName} from ${action.vendorName}.`;
    }
    return route.source === "nightwave"
      ? route.status === "ready"
        ? `Купите ${action.quantity}× ${action.itemName} у Норы.`
        : `Если есть у Норы на этой неделе — купите ${action.quantity}× ${action.itemName}.`
      : `Купите ${action.quantity}× ${action.itemName} у ${action.vendorName}.`;
  }

  function arcaneReason(): string {
    if (view?.arcanes.available) return "";
    if ($locale === "en") return view?.arcanes.reason === "refresh_item_data" ? "Refresh item data." : "Pack prices are incomplete.";
    return view?.arcanes.reason === "refresh_item_data"
      ? "Обновите данные предметов: в старом снимке нет значений Восфора."
      : "Недостаточно цен для честного расчёта наборов Лойда.";
  }

  function comparison(row: ArcaneConversionDecision): string {
    const market = formatPlatinum(row.marketPriceEach ?? null, $locale);
    const vosfor = formatPlatinum(row.equivalentPlatinumEach, $locale);
    return row.decision === "sell"
      ? `${c.marketValue} ${market} · ${c.vosforValue} ≈${vosfor}`
      : `${c.vosforValue} ≈${vosfor} · ${c.marketValue} ${market}`;
  }

  async function loadConverter(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      view = await invoke<ResourceConverterView | null>("resource_converter");
    } catch {
      view = null;
      errorMessage = c.loadError;
    } finally {
      loading = false;
    }
  }

  async function openMarket(slug: string): Promise<void> {
    openingSlug = slug;
    actionStatus = "";
    try {
      await invoke<number>("open_market_items", { slugs: [slug] });
      actionStatus = c.opened;
    } catch {
      actionStatus = c.openError;
    } finally {
      openingSlug = "";
    }
  }

  onMount(() => {
    let disposed = false;
    const cleanups: UnlistenFn[] = [];
    void loadConverter();
    for (const event of ["game-metadata-updated", "market-data-updated", "inventory-updated"]) {
      void listen(event, () => void loadConverter()).then((cleanup) => {
        if (disposed) cleanup();
        else cleanups.push(cleanup);
      });
    }
    return () => {
      disposed = true;
      for (const cleanup of cleanups) cleanup();
    };
  });
</script>

<section class="converter" aria-labelledby="converter-title">
  <header class="converter__summary">
    <div class="converter__intro">
      <p class="eyebrow">{c.eyebrow}</p>
      <h2 id="converter-title">{c.title}</h2>
      {#if loading}
        <p class="summary-note" role="status">{c.loading}</p>
      {:else if view && view.confirmedPlatinum > 0}
        <p class="headline"><span>{c.confirmed}</span><strong>≈ {formatPlatinum(view.confirmedPlatinum, $locale)}</strong></p>
        <p class="summary-note">{c.confirmedHint}</p>
      {:else}
        <p class="headline headline--empty"><strong>{c.noConfirmed}</strong></p>
        <p class="summary-note">{c.noConfirmedHint}</p>
      {/if}
    </div>
    {#if view && view.expectedVosforPlatinum > 0}
      <div class="expected-value">
        <span>{c.expected}</span>
        <strong>≈ {formatPlatinum(view.expectedVosforPlatinum, $locale)}</strong>
        <small>{c.expectedHint}</small>
      </div>
    {/if}
  </header>

  {#if errorMessage}
    <div class="converter-message converter-message--error" role="alert">
      <p>{errorMessage}</p>
      <button type="button" onclick={loadConverter}>{c.retry}</button>
    </div>
  {:else if !loading && !view}
    <div class="converter-message">
      <p>{c.noData}</p>
      <button type="button" onclick={onOpenSettings}>{c.settings}</button>
    </div>
  {:else if view}
    <div class="route-grid">
      {#each view.routes as route (route.source)}
        <article class:route--ready={route.status === "ready"} class:route--conditional={route.status === "conditional"} class="route-card">
          <header>
            <div>
              <h3>{c.source[route.source]}</h3>
              <p>{c.sourceHint[route.source]}</p>
            </div>
            <span class={`status status--${route.status}`}>{c.status[route.status]}</span>
          </header>

          {#if route.actions.length > 0}
            <div class="route-actions">
              {#each route.actions as action (`${route.source}:${action.vendorName}:${action.itemSlug}`)}
                <div class="route-action">
                  <div class="item-identity">
                    {#if action.imageUrl}<img src={action.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                    <div><strong>{action.itemName}</strong><small>{instruction(route, action)}</small></div>
                  </div>
                  <dl>
                    <div><dt>{c.balance}</dt><dd>{compactNumber(action.balance, localeCode($locale))} <small>{c.currency[action.currency]}</small></dd></div>
                    <div><dt>{c.costs}</dt><dd>{compactNumber(action.cost, localeCode($locale))} <small>{c.currency[action.currency]}</small></dd></div>
                    <div><dt>{c.each}</dt><dd>{formatPlatinum(action.unitPrice, $locale)}</dd></div>
                  </dl>
                  <div class="route-result">
                    <span>{c.total}</span>
                    <strong>{route.status === "conditional" ? "до " : "≈ "}{formatPlatinum(action.estimatedPlatinum, $locale)}</strong>
                    {#if !action.includedInTotal}<small>{c.conditional}</small>{/if}
                  </div>
                  <button class="market-link" type="button" disabled={openingSlug === action.itemSlug} onclick={() => openMarket(action.itemSlug)}>
                    {openingSlug === action.itemSlug ? c.opening : c.market}
                  </button>
                </div>
              {/each}
            </div>
          {/if}
          <p class="route-reason">
            {reasonText(route)}{#if route.location} · {route.location}{/if}{#if route.source === "nightwave" && route.status === "ready" && route.availableUntil} · {$locale === "ru" ? "До" : "Until"} {formatDate(route.availableUntil, $locale)}{/if}
          </p>
          {#if route.status === "waiting" && route.availableAt}<p class="route-date">{formatDate(route.availableAt, $locale)}</p>{/if}
        </article>
      {/each}
    </div>

    <article class="arcane-card">
      <header class="arcane-card__header">
        <div>
          <h3>{c.arcanes}</h3>
          <p>{c.arcanesHint}</p>
        </div>
        {#if view.arcanes.available && view.arcanes.bestPackName}
          <div class="pack-summary">
            <span>{c.bestPack}</span>
            <strong>{view.arcanes.bestPackName}</strong>
            <small>≈ {formatPlatinum(view.arcanes.packExpectedPlatinum ?? null, $locale)} {c.packExpected}</small>
          </div>
        {/if}
      </header>

      {#if !view.arcanes.available}
        <p class="arcane-empty">{arcaneReason()}</p>
      {:else if arcaneCount === 0}
        <p class="arcane-empty">{c.noArcaneActions}</p>
      {:else}
        <div class="arcane-columns">
          <section class="arcane-lane arcane-lane--sell" aria-labelledby="arcane-sell-title">
            <header><h4 id="arcane-sell-title">{c.sell}</h4><strong>{view.arcanes.sell.length}</strong></header>
            <div class="arcane-list">
              {#each visibleArcaneDecisions(view.arcanes.sell, showAllArcanes) as row (`sell:${row.slug}:${row.rank}`)}
                <div class="arcane-row">
                  {#if row.imageUrl}<img src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                  <div><strong>{row.displayName}</strong><small>{c.rank} {row.rank} · ×{row.quantity}</small><small>{comparison(row)}</small></div>
                  <strong class="arcane-value">≈ {formatPlatinum(row.estimatedPlatinum, $locale)}</strong>
                </div>
              {:else}
                <p class="lane-empty">—</p>
              {/each}
            </div>
          </section>

          <section class="arcane-lane arcane-lane--dissolve" aria-labelledby="arcane-dissolve-title">
            <header><h4 id="arcane-dissolve-title">{c.dissolve}</h4><strong>{view.arcanes.dissolve.length}</strong></header>
            <div class="arcane-list">
              {#each visibleArcaneDecisions(view.arcanes.dissolve, showAllArcanes) as row (`dissolve:${row.slug}:${row.rank}`)}
                <div class="arcane-row">
                  {#if row.imageUrl}<img src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                  <div><strong>{row.displayName}</strong><small>{c.rank} {row.rank} · ×{row.quantity} · {compactNumber(row.vosforTotal, localeCode($locale))} {c.vosfor}</small><small>{comparison(row)}</small></div>
                  <strong class="arcane-value">≈ {formatPlatinum(row.estimatedPlatinum, $locale)}</strong>
                </div>
              {:else}
                <p class="lane-empty">—</p>
              {/each}
            </div>
          </section>
        </div>
        {#if view.arcanes.hold.length > 0}
          <p class="hold-note"><strong>{c.hold}: {view.arcanes.hold.length}.</strong> {$locale === "ru" ? "Разница меньше 10% — цена слишком близка к ожидаемой ценности Восфора." : "The difference is under 10%, so the values are too close to act."}</p>
        {/if}
        {#if view.arcanes.sell.length > 4 || view.arcanes.dissolve.length > 4}
          <button class="show-all" type="button" onclick={() => (showAllArcanes = !showAllArcanes)}>
            {showAllArcanes ? c.showLess : c.showAll}
          </button>
        {/if}
      {/if}
    </article>

    <div class="action-status" role="status" aria-live="polite">{actionStatus}</div>
    <details class="converter-method">
      <summary>{c.methodology}</summary>
      <p>{c.methodologyBody}</p>
      <dl>
        <div><dt>{c.inventoryDate}</dt><dd>{formatDate(view.inventoryObservedAt, $locale)}</dd></div>
        <div><dt>{c.marketDate}</dt><dd>{view.marketSourceDate ?? "—"}</dd></div>
        <div><dt>{c.worldstateDate}</dt><dd>{formatDate(view.fetchedAt, $locale)}</dd></div>
      </dl>
    </details>
  {/if}
</section>

<style>
  .converter { min-width: 0; overflow: hidden; border: 1px solid var(--border); border-radius: .9rem; background: var(--surface-1); box-shadow: var(--shadow-sm); }
  .converter__summary { display: flex; align-items: end; justify-content: space-between; gap: 1.25rem; padding: 1rem; background: linear-gradient(115deg, var(--accent-soft), var(--surface-2) 62%); }
  .converter__intro { min-width: 0; }
  .eyebrow { margin: 0 0 .15rem; color: var(--accent-strong); font-size: .68rem; font-weight: 800; letter-spacing: .07em; text-transform: uppercase; }
  h2, h3, h4, p { margin: 0; }
  h2 { font-size: 1.1rem; line-height: 1.25; }
  .headline { display: flex; flex-wrap: wrap; align-items: baseline; gap: .35rem .55rem; margin-block-start: .5rem; }
  .headline span { color: var(--text-muted); font-size: .78rem; }
  .headline strong { color: var(--accent-strong); font-size: 1.65rem; font-variant-numeric: tabular-nums; letter-spacing: -.025em; }
  .headline--empty strong { color: var(--text); font-size: .95rem; letter-spacing: 0; }
  .summary-note { max-width: 62ch; margin-block-start: .15rem; color: var(--text-muted); font-size: .7rem; line-height: 1.35; }
  .expected-value { display: grid; flex: 0 0 min(22rem, 38%); gap: .08rem; border-inline-start: 1px solid var(--border-strong); padding-inline-start: 1rem; }
  .expected-value span, .expected-value small { color: var(--text-muted); font-size: .68rem; }
  .expected-value strong { color: var(--accent-strong); font-size: 1.2rem; font-variant-numeric: tabular-nums; }
  .expected-value small { line-height: 1.3; }
  .converter-message { margin: .75rem; border-radius: .65rem; padding: .75rem; background: var(--surface-2); }
  .converter-message--error { background: var(--danger-soft); box-shadow: inset .2rem 0 0 var(--danger); }
  .converter-message button { margin-block-start: .55rem; }
  .route-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 1px; background: var(--border); border-block-start: 1px solid var(--border); }
  .route-card { min-width: 0; padding: .75rem; background: var(--surface-1); }
  .route-card > header { display: flex; align-items: start; justify-content: space-between; gap: .75rem; }
  .route-card h3 { font-size: .9rem; }
  .route-card header p { margin-block-start: .08rem; color: var(--text-muted); font-size: .66rem; }
  .status { flex: none; border: 1px solid var(--border); border-radius: 999px; padding: .18rem .42rem; color: var(--text-muted); background: var(--surface-2); font-size: .62rem; font-weight: 750; white-space: nowrap; }
  .status--ready { border-color: oklch(0.68 0.08 145); background: var(--success-soft); color: oklch(0.34 0.08 145); }
  .status--conditional { border-color: var(--accent); background: var(--accent-soft); color: var(--accent-strong); }
  .route-actions { display: grid; gap: .45rem; margin-block-start: .55rem; }
  .route-action { display: grid; grid-template-columns: minmax(10rem, 1.15fr) minmax(14rem, 1fr) auto auto; align-items: center; gap: .55rem; min-width: 0; border-radius: .55rem; padding: .5rem; background: var(--surface-2); }
  .item-identity { display: flex; align-items: center; min-width: 0; gap: .5rem; }
  .item-identity img { flex: none; width: 2.6rem; height: 2.6rem; border-radius: .35rem; object-fit: contain; outline: 1px solid oklch(0 0 0 / .1); outline-offset: -1px; }
  .item-identity strong, .item-identity small { display: block; }
  .item-identity strong { overflow: hidden; font-size: .78rem; line-height: 1.25; text-overflow: ellipsis; }
  .item-identity small { margin-block-start: .12rem; color: var(--text-muted); font-size: .62rem; line-height: 1.3; }
  .route-action dl { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: .35rem; margin: 0; }
  .route-action dt, .route-action dd { margin: 0; }
  .route-action dt { color: var(--text-muted); font-size: .58rem; }
  .route-action dd { margin-block-start: .08rem; font-size: .72rem; font-weight: 750; font-variant-numeric: tabular-nums; }
  .route-action dd small { display: block; color: var(--text-muted); font-size: .55rem; font-weight: 600; }
  .route-result { display: grid; justify-items: end; min-width: 4.6rem; }
  .route-result span, .route-result small { color: var(--text-muted); font-size: .56rem; }
  .route-result strong { color: var(--accent-strong); font-size: 1rem; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .market-link { min-height: 1.85rem; padding: .28rem .5rem; font-size: .65rem; white-space: nowrap; }
  .route-reason { margin-block-start: .45rem; color: var(--text-muted); font-size: .65rem; line-height: 1.35; }
  .route-date { margin-block-start: .15rem; color: var(--accent-strong); font-size: .68rem; font-weight: 700; }
  .arcane-card { border-block-start: 1px solid var(--border); padding: .75rem; background: var(--surface-1); }
  .arcane-card__header { display: flex; align-items: start; justify-content: space-between; gap: 1rem; }
  .arcane-card h3 { font-size: .9rem; }
  .arcane-card__header p { margin-block-start: .08rem; color: var(--text-muted); font-size: .66rem; }
  .pack-summary { display: grid; justify-items: end; gap: .02rem; text-align: end; }
  .pack-summary span, .pack-summary small { color: var(--text-muted); font-size: .6rem; }
  .pack-summary strong { color: var(--accent-strong); font-size: .78rem; }
  .arcane-columns { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .55rem; margin-block-start: .6rem; }
  .arcane-lane { min-width: 0; overflow: hidden; border: 1px solid var(--border); border-radius: .6rem; }
  .arcane-lane > header { display: flex; align-items: center; justify-content: space-between; padding: .42rem .55rem; background: var(--surface-2); }
  .arcane-lane h4 { font-size: .75rem; }
  .arcane-lane > header > strong { display: grid; place-items: center; min-width: 1.35rem; height: 1.35rem; border-radius: 999px; background: var(--surface-1); color: var(--text-muted); font-size: .62rem; }
  .arcane-lane--sell { box-shadow: inset .18rem 0 0 oklch(0.58 0.09 145); }
  .arcane-lane--dissolve { box-shadow: inset .18rem 0 0 var(--accent); }
  .arcane-list { display: grid; }
  .arcane-row { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: .45rem; min-width: 0; padding: .42rem .5rem; border-block-start: 1px solid var(--border); }
  .arcane-row img { width: 2.15rem; height: 2.15rem; border-radius: .3rem; object-fit: contain; outline: 1px solid oklch(0 0 0 / .1); outline-offset: -1px; }
  .arcane-row > div { min-width: 0; }
  .arcane-row > div > strong, .arcane-row small { display: block; }
  .arcane-row > div > strong { overflow: hidden; font-size: .72rem; text-overflow: ellipsis; white-space: nowrap; }
  .arcane-row small { margin-block-start: .06rem; color: var(--text-muted); font-size: .58rem; }
  .arcane-value { color: var(--accent-strong); font-size: .78rem; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .lane-empty, .arcane-empty { padding: .65rem; color: var(--text-muted); font-size: .7rem; }
  .hold-note { margin-block-start: .5rem; color: var(--text-muted); font-size: .65rem; }
  .hold-note strong { color: var(--text); }
  .show-all { margin-block-start: .55rem; min-height: 1.9rem; padding: .3rem .55rem; font-size: .66rem; }
  .action-status { min-height: 1rem; padding-inline: .75rem; color: var(--success); font-size: .65rem; font-weight: 700; }
  .converter-method { border-block-start: 1px solid var(--border); padding-inline: .75rem; }
  .converter-method summary { min-height: 2rem; padding-block: .4rem; color: var(--accent-strong); cursor: pointer; font-size: .68rem; font-weight: 700; }
  .converter-method > p { max-width: 90ch; margin-block-end: .5rem; color: var(--text-muted); font-size: .65rem; line-height: 1.4; }
  .converter-method dl { display: flex; flex-wrap: wrap; gap: .35rem 1rem; margin: 0 0 .6rem; }
  .converter-method dl div { display: flex; gap: .3rem; }
  .converter-method dt, .converter-method dd { margin: 0; font-size: .62rem; }
  .converter-method dt { color: var(--text-muted); }
  .converter-method dd { font-weight: 700; font-variant-numeric: tabular-nums; }
  @media (max-width: 72rem) {
    .route-grid { grid-template-columns: minmax(0, 1fr); }
  }
  @media (max-width: 54rem) {
    .converter__summary, .arcane-card__header { align-items: stretch; flex-direction: column; }
    .expected-value { flex-basis: auto; border-inline-start: 0; border-block-start: 1px solid var(--border-strong); padding: .65rem 0 0; }
    .route-action { grid-template-columns: minmax(0, 1fr) auto; }
    .route-action dl { grid-column: 1 / -1; grid-row: 2; }
    .route-result { grid-column: 2; grid-row: 1; }
    .market-link { grid-column: 2; grid-row: 2; }
    .pack-summary { justify-items: start; text-align: start; }
  }
  @media (max-width: 40rem) {
    .arcane-columns { grid-template-columns: minmax(0, 1fr); }
    .route-action { grid-template-columns: minmax(0, 1fr) auto; }
    .route-action dl { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .market-link { grid-column: 1 / -1; grid-row: auto; justify-self: start; }
  }
  @media (forced-colors: active) {
    .converter, .route-card, .arcane-lane, .status { outline: 1px solid CanvasText; }
  }
</style>
