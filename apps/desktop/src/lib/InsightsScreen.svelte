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
  import { localeCode, useLocale } from "./i18n";
  import {
    filterAndSortOpportunitySets,
    formatPercent,
    formatRatio,
    refinementLabel,
    setOpportunity,
    setRelicSupport,
    vaultLabel,
    type InsightsView,
    type SetInsightRow,
    type SetOpportunityMode,
  } from "./insights";
  import { formatPlatinum } from "./market";

  export let onOpenSettings: () => void;
  export let onOpenAccount: () => void;

  type OpportunityMode = SetOpportunityMode | "ducats";

  const locale = useLocale();
  const copy = {
    ru: {
      region: "Возможности заработка",
      reading: "Считаем возможности по сохранённым данным…",
      ready: (date: string) => `Данные предметов от ${date}`,
      loadError: "Не удалось рассчитать возможности. Цены и инвентарь не изменились.",
      retry: "Повторить",
      noSnapshot: "Сначала загрузите данные предметов",
      noSnapshotBody: "Обновление рынка и игровых данных находится в настройках.",
      openSettings: "Открыть настройки",
      noInventory: "Инвентарь ещё не загружен",
      noInventoryBody: "Запустите чтение инвентаря в настройках — после этого появятся персональные варианты.",
      relicMode: "Из своих реликвий",
      buyMode: "Докупить",
      readyMode: "Продать сет",
      ducatMode: "На дукаты",
      filters: "Что сделать",
      search: "Найти сет",
      searchPlaceholder: "Например, Стран Прайм",
      clearSearch: "Очистить поиск",
      missing: "Не хватает",
      ownedRelics: "Подходящих копий",
      usefulChance: "Шанс нужной награды",
      chanceHint: "хотя бы одна из всех копий",
      buyFor: "Докупить примерно",
      setPrice: "Цена сета",
      readySets: "Готово сетов",
      partsPrice: "Детали отдельно",
      setPremium: "Премия сета",
      setPremiumHint: "к цене деталей",
      allPartsCovered: "Все недостающие виды деталей могут выпасть из ваших реликвий.",
      somePartsCovered: (covered: number, total: number) => `Из ваших реликвий выпадают ${covered} из ${total} недостающих видов деталей. Остальные можно докупить.`,
      buySummary: (cost: string, setPrice: string) => `Недостающие детали стоят около ${cost}, собранный сет — ${setPrice}.`,
      readySummary: (count: number) => `Можно выставить ${count} ${count === 1 ? "полный сет" : "полных сета"}.`,
      showRelics: (count: number) => `Показать реликвии (${count})`,
      hideRelics: "Скрыть реликвии",
      buyMissing: (count: number) => `Купить недостающие детали (${count})`,
      marketOpened: (count: number) => `Открыто страниц Warframe Market: ${count}.`,
      marketOpenError: "Не удалось открыть Warframe Market. Повторите действие.",
      relicPlan: "Подходящие реликвии",
      aggregateChance: "Шанс получить хотя бы одну нужную деталь из всех указанных копий",
      probabilityNote: "Это вероятность, а не гарантия. Расчёт предполагает одно открытие каждой копии и независимый результат.",
      owned: "Есть",
      perOpen: "За одно открытие",
      fromCopies: "Из всех копий",
      usefulDrops: "Нужные награды",
      needShort: "нужно",
      composition: "Состав сета",
      part: "Деталь",
      oneSetNeeds: "На один сет",
      ownedForSet: "Есть",
      missingForSet: "Не хватает",
      price: "Цена",
      sellSet: "Выставить сет",
      openOrders: "Открыть мои ордера",
      connectAccount: "Подключить Warframe Market",
      verifyAccount: "Подтвердить аккаунт",
      orderTitle: "Новый ордер на сет",
      orderPrice: "Цена, платина",
      orderQuantity: "Количество сетов",
      publishOrder: "Сразу опубликовать ордер",
      reviewOrder: "Проверить ордер",
      cancel: "Отменить",
      confirmTitle: "Подтвердите ордер",
      confirmSummary: (name: string, quantity: number, price: number) => `${name}: выставить ${quantity} шт. по ${price}p.`,
      confirmCheck: "Я проверил сет, цену и количество",
      createOrder: "Создать ордер",
      creatingOrder: "Создаём ордер…",
      orderCreated: "Сет выставлен на Warframe Market.",
      invalidOrder: "Укажите цену и количество не меньше 1.",
      setUnavailable: "Сет не найден в каталоге Warframe Market. Обновите данные рынка.",
      noResults: "Подходящих вариантов нет",
      noSearchResults: "Сеты с таким названием не найдены.",
      noRelicResults: (count: number) => count > 0
        ? `В инвентаре распознано реликвий: ${count}, но сейчас из них не выпадают недостающие детали ваших сетов.`
        : "В инвентаре нет распознанных реликвий. Запустите чтение инвентаря в настройках.",
      noBuyResults: "Нет сетов, которые выгодно дособрать покупкой недостающих деталей по надёжным ценам.",
      noReadyResults: "Готовых сетов для продажи сейчас нет.",
      ducatTitle: "Что дешевле обменять на дукаты",
      ducatBody: "Сначала показаны детали с наименьшей потерей платины за один дукат.",
      ducatWarning: "Проверьте количество перед обменом: действие в игре необратимо.",
      primePart: "Деталь Прайм",
      sellable: "Можно обменять",
      ducats: "Дукаты",
      platinumPerDucat: "Платина / дукат",
      noDucats: "Нет распознанных деталей Прайм с надёжной ценой и стоимостью в дукатах.",
    },
    en: {
      region: "Earning opportunities",
      reading: "Calculating opportunities from saved data…",
      ready: (date: string) => `Item data from ${date}`,
      loadError: "Unable to calculate opportunities. Prices and inventory were not changed.",
      retry: "Try again",
      noSnapshot: "Load item data first",
      noSnapshotBody: "Market and game-data updates are available in Settings.",
      openSettings: "Open settings",
      noInventory: "Inventory has not been loaded",
      noInventoryBody: "Run the inventory scan in Settings to see personal opportunities.",
      relicMode: "From owned relics",
      buyMode: "Buy missing parts",
      readyMode: "Sell sets",
      ducatMode: "For ducats",
      filters: "Choose an action",
      search: "Find a set",
      searchPlaceholder: "For example, Strun Prime",
      clearSearch: "Clear search",
      missing: "Missing",
      ownedRelics: "Matching copies",
      usefulChance: "Useful drop chance",
      chanceHint: "at least one across all copies",
      buyFor: "Buy missing for about",
      setPrice: "Set price",
      readySets: "Ready sets",
      partsPrice: "Parts separately",
      setPremium: "Set premium",
      setPremiumHint: "over the parts price",
      allPartsCovered: "Every missing part type can drop from your relics.",
      somePartsCovered: (covered: number, total: number) => `${covered} of ${total} missing part types can drop from your relics. Buy the rest if needed.`,
      buySummary: (cost: string, setPrice: string) => `Missing parts cost about ${cost}; the completed set is ${setPrice}.`,
      readySummary: (count: number) => `${count} complete ${count === 1 ? "set is" : "sets are"} ready to list.`,
      showRelics: (count: number) => `Show relics (${count})`,
      hideRelics: "Hide relics",
      buyMissing: (count: number) => `Buy missing parts (${count})`,
      marketOpened: (count: number) => `Opened ${count} Warframe Market pages.`,
      marketOpenError: "Unable to open Warframe Market. Try again.",
      relicPlan: "Matching relics",
      aggregateChance: "Chance of at least one useful drop across the listed copies",
      probabilityNote: "This is a probability, not a guarantee. It assumes one opening per copy and independent outcomes.",
      owned: "Owned",
      perOpen: "Per opening",
      fromCopies: "Across copies",
      usefulDrops: "Useful rewards",
      needShort: "need",
      composition: "Set components",
      part: "Part",
      oneSetNeeds: "One set needs",
      ownedForSet: "Owned",
      missingForSet: "Missing",
      price: "Price",
      sellSet: "List set",
      openOrders: "Open my orders",
      connectAccount: "Connect Warframe Market",
      verifyAccount: "Verify account",
      orderTitle: "New set order",
      orderPrice: "Price, platinum",
      orderQuantity: "Set quantity",
      publishOrder: "Publish the order immediately",
      reviewOrder: "Review order",
      cancel: "Cancel",
      confirmTitle: "Confirm order",
      confirmSummary: (name: string, quantity: number, price: number) => `${name}: list ${quantity} at ${price}p each.`,
      confirmCheck: "I checked the set, price, and quantity",
      createOrder: "Create order",
      creatingOrder: "Creating order…",
      orderCreated: "Set listed on Warframe Market.",
      invalidOrder: "Use a price and quantity of at least 1.",
      setUnavailable: "The set is missing from the Warframe Market catalog. Refresh market data.",
      noResults: "No matching opportunities",
      noSearchResults: "No sets match this name.",
      noRelicResults: (count: number) => count > 0
        ? `${count} relic variants were recognized, but none currently drop your missing set parts.`
        : "No relics were recognized. Run the inventory scan in Settings.",
      noBuyResults: "No sets are currently profitable to finish by buying missing parts at reliable prices.",
      noReadyResults: "No complete sets are ready to sell.",
      ducatTitle: "Lowest platinum cost per ducat",
      ducatBody: "Parts with the lowest platinum value per ducat are shown first.",
      ducatWarning: "Check the quantity before trading: the in-game action cannot be undone.",
      primePart: "Prime part",
      sellable: "Available",
      ducats: "Ducats",
      platinumPerDucat: "Platinum / ducat",
      noDucats: "No recognized Prime parts have both a reliable price and ducat value.",
    },
  } as const;
  $: c = copy[$locale];

  let view: InsightsView | null = null;
  let loading = true;
  let errorMessage = "";
  let activeMode: OpportunityMode = "relics";
  let setQuery = "";
  let expandedRelicSet = "";
  let marketStatus = "";
  let marketBusySlug = "";
  let accountView: AccountView | null = null;
  let listingSlug = "";
  let listingPrice = 1;
  let listingQuantity = 1;
  let listingVisible = true;
  let listingStage: "edit" | "confirm" = "edit";
  let listingConfirmed = false;
  let listingBusy = false;
  let listingError = "";

  $: relicRows = filterAndSortOpportunitySets(view?.sets ?? [], view?.relics ?? [], "relics", setQuery, $locale);
  $: buyRows = filterAndSortOpportunitySets(view?.sets ?? [], view?.relics ?? [], "buy", setQuery, $locale);
  $: readyRows = filterAndSortOpportunitySets(view?.sets ?? [], view?.relics ?? [], "ready", setQuery, $locale);
  $: relicOpportunityCount = filterAndSortOpportunitySets(view?.sets ?? [], view?.relics ?? [], "relics", "", $locale).length;
  $: buyOpportunityCount = filterAndSortOpportunitySets(view?.sets ?? [], view?.relics ?? [], "buy", "", $locale).length;
  $: readyOpportunityCount = filterAndSortOpportunitySets(view?.sets ?? [], view?.relics ?? [], "ready", "", $locale).length;
  $: setRows = activeMode === "relics" ? relicRows : activeMode === "buy" ? buyRows : activeMode === "ready" ? readyRows : [];
  $: ducatRows = (view?.ducats ?? []).filter((row) => row.sellableQuantity > 0 && row.efficiency.credible);
  $: ownedRelicCount = (view?.relics ?? []).reduce((sum, relic) => sum + relic.ownedQuantity, 0);
  $: listedSetItemIds = new Set(
    (accountView?.orders ?? []).filter((order) => order.type === "sell").map((order) => order.itemId),
  );
  $: listingRow = (view?.sets ?? []).find((row) => row.definition.setSlug === listingSlug) ?? null;

  function formatProbability(value: number): string {
    return `${value.toLocaleString(localeCode($locale), { maximumFractionDigits: 1 })}%`;
  }

  function selectMode(mode: OpportunityMode): void {
    activeMode = mode;
    expandedRelicSet = "";
    marketStatus = "";
  }

  async function loadInsights(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      view = await invoke<InsightsView | null>("insights");
    } catch {
      view = null;
      errorMessage = c.loadError;
    } finally {
      loading = false;
    }
  }

  async function loadAccount(): Promise<void> {
    try {
      accountView = await invoke<AccountView>("account_status");
    } catch {
      accountView = null;
    }
  }

  async function openMissingParts(row: SetInsightRow): Promise<void> {
    const slugs = setOpportunity(row).missingParts.map((part) => part.slug);
    if (slugs.length === 0) return;
    marketBusySlug = row.definition.setSlug;
    marketStatus = "";
    try {
      const opened = await invoke<number>("open_market_items", { slugs });
      marketStatus = c.marketOpened(opened);
    } catch {
      marketStatus = c.marketOpenError;
    } finally {
      marketBusySlug = "";
    }
  }

  function startListing(row: SetInsightRow): void {
    listingError = "";
    if (!accountView?.connected || !accountView.profile?.verification || (row.itemId && listedSetItemIds.has(row.itemId))) {
      onOpenAccount();
      return;
    }
    if (!row.itemId) {
      listingError = c.setUnavailable;
      listingSlug = row.definition.setSlug;
      return;
    }
    listingSlug = row.definition.setSlug;
    listingPrice = Math.max(1, Math.round(row.setRecommendation?.listPrice ?? row.comparison.setFairValue ?? 1));
    listingQuantity = Math.max(1, row.comparison.completeSets);
    listingVisible = true;
    listingStage = "edit";
    listingConfirmed = false;
  }

  function closeListing(): void {
    listingSlug = "";
    listingError = "";
    listingConfirmed = false;
    listingStage = "edit";
  }

  function reviewListing(event: SubmitEvent): void {
    event.preventDefault();
    listingError = "";
    if (!Number.isInteger(listingPrice) || listingPrice < 1 || !Number.isInteger(listingQuantity) || listingQuantity < 1) {
      listingError = c.invalidOrder;
      return;
    }
    listingStage = "confirm";
    listingConfirmed = false;
  }

  async function createSetListing(): Promise<void> {
    if (!listingRow?.itemId || !listingConfirmed) return;
    listingBusy = true;
    listingError = "";
    const input: CreateListingInput = {
      itemId: listingRow.itemId,
      type: "sell",
      platinum: listingPrice,
      quantity: listingQuantity,
      visible: listingVisible,
      perTrade: null,
      rank: null,
      charges: null,
      subtype: null,
      amberStars: null,
      cyanStars: null,
    };
    try {
      const order = await invoke<AccountOrder>("account_create_listing", { input, confirmed: true });
      if (accountView) accountView = { ...accountView, orders: [...accountView.orders, order] };
      marketStatus = c.orderCreated;
      await loadAccount();
      closeListing();
    } catch (error) {
      listingError = accountActionErrorMessage(String(error), $locale);
    } finally {
      listingBusy = false;
    }
  }

  onMount(() => {
    let disposed = false;
    const cleanups: UnlistenFn[] = [];
    void loadInsights();
    void loadAccount();
    for (const event of ["game-metadata-updated", "market-data-updated", "inventory-updated"]) {
      void listen(event, () => void loadInsights()).then((cleanup) => {
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

<section class="opportunities" aria-label={c.region}>
  <div class="data-status" role="status" aria-live="polite">
    {#if loading}{c.reading}{:else if view}{c.ready(view.metadata.fetchedAt.slice(0, 10))}{/if}
  </div>

  {#if errorMessage}
    <div class="message message--error" role="alert">
      <p>{errorMessage}</p>
      <button type="button" onclick={loadInsights}>{c.retry}</button>
    </div>
  {:else if !loading && !view}
    <div class="message">
      <h2>{c.noSnapshot}</h2>
      <p>{c.noSnapshotBody}</p>
      <button type="button" onclick={onOpenSettings}>{c.openSettings}</button>
    </div>
  {:else if view}
    {#if !view.inventoryAvailable}
      <div class="message message--action" role="note">
        <div><h2>{c.noInventory}</h2><p>{c.noInventoryBody}</p></div>
        <button type="button" onclick={onOpenSettings}>{c.openSettings}</button>
      </div>
    {/if}

    <div class="opportunity-toolbar">
      <div class="mode-switcher" role="group" aria-label={c.filters}>
        <button type="button" aria-pressed={activeMode === "relics"} onclick={() => selectMode("relics")}>
          {c.relicMode}<span>{relicOpportunityCount}</span>
        </button>
        <button type="button" aria-pressed={activeMode === "buy"} onclick={() => selectMode("buy")}>
          {c.buyMode}<span>{buyOpportunityCount}</span>
        </button>
        <button type="button" aria-pressed={activeMode === "ready"} onclick={() => selectMode("ready")}>
          {c.readyMode}<span>{readyOpportunityCount}</span>
        </button>
        <button type="button" aria-pressed={activeMode === "ducats"} onclick={() => selectMode("ducats")}>
          {c.ducatMode}<span>{ducatRows.length}</span>
        </button>
      </div>
      {#if activeMode !== "ducats"}
        <label class="set-search">
          <span>{c.search}</span>
          <input bind:value={setQuery} type="search" placeholder={c.searchPlaceholder} />
        </label>
      {/if}
    </div>

    <div class="action-status" role="status" aria-live="polite">{marketStatus}</div>

    {#if activeMode === "ducats"}
      <section class="ducat-panel" aria-labelledby="ducat-heading">
        <header>
          <div>
            <h2 id="ducat-heading">{c.ducatTitle}</h2>
            <p>{c.ducatBody}</p>
          </div>
          <p class="warning">{c.ducatWarning}</p>
        </header>
        <div class="table-scroll">
          <table>
            <thead><tr><th>{c.primePart}</th><th>{c.sellable}</th><th>{c.price}</th><th>{c.ducats}</th><th>{c.platinumPerDucat}</th></tr></thead>
            <tbody>
              {#each ducatRows as row (row.metadata.slug)}
                <tr>
                  <th scope="row"><span class="item-name">{#if row.imageUrl}<img src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{row.displayName}</span></th>
                  <td>{row.sellableQuantity}</td>
                  <td>{formatPlatinum(row.efficiency.fairPrice, $locale)}</td>
                  <td>{row.efficiency.ducats}</td>
                  <td>{formatRatio(row.efficiency.platinumPerDucat, $locale)}</td>
                </tr>
              {:else}
                <tr><td colspan="5">{c.noDucats}</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>
    {:else}
      <div class="set-list">
        {#each setRows as row (row.definition.setSlug)}
          {@const opportunity = setOpportunity(row)}
          {@const relicSupport = setRelicSupport(row, view.relics)}
          <article class="set-card">
            <header class="set-card__header">
              <div class="set-identity">
                {#if row.imageUrl}<img class="set-image" src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                <div>
                  <p class="set-context">{vaultLabel(row.definition.vaultStatus, $locale)}</p>
                  <h2>{row.displayName}</h2>
                </div>
              </div>
              <span class="route-badge">
                {activeMode === "relics" ? c.relicMode : activeMode === "buy" ? c.buyMode : c.readyMode}
              </span>
            </header>

            <dl class="set-metrics">
              {#if activeMode === "relics"}
                <div><dt>{c.missing}</dt><dd>{opportunity.missingQuantity}</dd></div>
                <div><dt>{c.ownedRelics}</dt><dd>{relicSupport.ownedRelicCount}</dd></div>
                <div><dt>{c.usefulChance}</dt><dd>{formatProbability(relicSupport.aggregateChancePercent)}<small>{c.chanceHint}</small></dd></div>
              {:else if activeMode === "buy"}
                <div><dt>{c.missing}</dt><dd>{opportunity.missingQuantity}</dd></div>
                <div><dt>{c.buyFor}</dt><dd>{formatPlatinum(opportunity.completionCost, $locale)}</dd></div>
                <div><dt>{c.setPrice}</dt><dd>{formatPlatinum(opportunity.setFairValue, $locale)}</dd></div>
              {:else}
                <div><dt>{c.readySets}</dt><dd>{opportunity.completeSets}</dd></div>
                <div><dt>{c.setPrice}</dt><dd>{formatPlatinum(opportunity.setFairValue, $locale)}</dd></div>
                <div><dt>{c.partsPrice}</dt><dd>{formatPlatinum(opportunity.partsFairValue, $locale)}</dd></div>
              {/if}
              <div class:positive={(opportunity.setPremiumValue ?? 0) > 0}>
                <dt>{c.setPremium}</dt>
                <dd>{formatPlatinum(opportunity.setPremiumValue, $locale)}<small>{formatPercent(opportunity.setPremiumPercent, $locale)} · {c.setPremiumHint}</small></dd>
              </div>
            </dl>

            <p class="decision-copy">
              {#if activeMode === "relics"}
                {relicSupport.allMissingPartsCovered ? c.allPartsCovered : c.somePartsCovered(relicSupport.coveredPartCount, relicSupport.missingPartCount)}
              {:else if activeMode === "buy"}
                {c.buySummary(formatPlatinum(opportunity.completionCost, $locale), formatPlatinum(opportunity.setFairValue, $locale))}
              {:else}
                {c.readySummary(opportunity.completeSets)}
              {/if}
            </p>

            {#if opportunity.missingParts.length > 0}
              <div class="missing-parts" aria-label={c.missing}>
                {#each opportunity.missingParts as part (part.slug)}
                  <span>{part.displayName} ×{part.quantity}<strong>{formatPlatinum(part.estimatedCost, $locale)}</strong></span>
                {/each}
              </div>
            {/if}

            <div class="card-actions">
              {#if activeMode === "relics"}
                <button type="button" onclick={() => (expandedRelicSet = expandedRelicSet === row.definition.setSlug ? "" : row.definition.setSlug)}>
                  {expandedRelicSet === row.definition.setSlug ? c.hideRelics : c.showRelics(relicSupport.matches.length)}
                </button>
                <button type="button" class="secondary" disabled={marketBusySlug === row.definition.setSlug} onclick={() => openMissingParts(row)}>
                  {c.buyMissing(opportunity.missingParts.length)}
                </button>
              {:else if activeMode === "buy"}
                <button type="button" disabled={marketBusySlug === row.definition.setSlug} onclick={() => openMissingParts(row)}>
                  {c.buyMissing(opportunity.missingParts.length)}
                </button>
              {:else}
                <button type="button" onclick={() => row.itemId && listedSetItemIds.has(row.itemId) ? onOpenAccount() : startListing(row)}>
                  {#if row.itemId && listedSetItemIds.has(row.itemId)}{c.openOrders}{:else if !accountView?.connected}{c.connectAccount}{:else if !accountView.profile?.verification}{c.verifyAccount}{:else}{c.sellSet}{/if}
                </button>
              {/if}
            </div>

            {#if expandedRelicSet === row.definition.setSlug && activeMode === "relics"}
              <section class="relic-plan" aria-label={c.relicPlan}>
                <div class="relic-plan__summary">
                  <span>{c.aggregateChance}</span>
                  <strong>{formatProbability(relicSupport.aggregateChancePercent)}</strong>
                  <small>{c.probabilityNote}</small>
                </div>
                <div class="relic-list">
                  {#each relicSupport.matches as match (`${match.relic.definition.relicSlug}:${match.relic.definition.refinement}`)}
                    <article class="relic-row">
                      <div class="relic-identity">
                        {#if match.relic.imageUrl}<img src={match.relic.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                        <div><h3>{match.relic.displayName}</h3><p>{refinementLabel(match.relic.definition.refinement, $locale)}</p></div>
                      </div>
                      <dl>
                        <div><dt>{c.owned}</dt><dd>×{match.relic.ownedQuantity}</dd></div>
                        <div><dt>{c.perOpen}</dt><dd>{formatProbability(match.chancePerRelicPercent)}</dd></div>
                        <div><dt>{c.fromCopies}</dt><dd>{formatProbability(match.chanceFromOwnedPercent)}</dd></div>
                      </dl>
                      <div class="useful-rewards" aria-label={c.usefulDrops}>
                        {#each match.usefulRewards as reward (reward.slug)}
                          <span>{#if reward.imageUrl}<img src={reward.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{reward.displayName}<small>{formatProbability(reward.chancePercent)} · {c.needShort} ×{reward.quantityNeeded}</small></span>
                        {/each}
                      </div>
                    </article>
                  {/each}
                </div>
              </section>
            {/if}

            {#if listingSlug === row.definition.setSlug}
              <section class="order-panel" aria-labelledby={`set-order-${row.definition.setSlug}`}>
                <h3 id={`set-order-${row.definition.setSlug}`}>{listingStage === "edit" ? c.orderTitle : c.confirmTitle}</h3>
                {#if listingError}<p class="inline-error" role="alert">{listingError}</p>{/if}
                {#if listingStage === "edit"}
                  <form onsubmit={reviewListing}>
                    <div class="order-fields">
                      <label><span>{c.orderPrice}</span><input bind:value={listingPrice} type="number" min="1" step="1" inputmode="numeric" /></label>
                      <label><span>{c.orderQuantity}</span><input bind:value={listingQuantity} type="number" min="1" max={opportunity.completeSets} step="1" inputmode="numeric" /></label>
                    </div>
                    <label class="order-visible"><input bind:checked={listingVisible} type="checkbox" /><span>{c.publishOrder}</span></label>
                    <div class="order-actions"><button type="submit">{c.reviewOrder}</button><button type="button" class="secondary" onclick={closeListing}>{c.cancel}</button></div>
                  </form>
                {:else}
                  <p>{c.confirmSummary(row.displayName, listingQuantity, listingPrice)}</p>
                  <label class="order-visible"><input bind:checked={listingConfirmed} type="checkbox" /><span>{c.confirmCheck}</span></label>
                  <div class="order-actions"><button type="button" disabled={listingBusy || !listingConfirmed} onclick={createSetListing}>{listingBusy ? c.creatingOrder : c.createOrder}</button><button type="button" class="secondary" disabled={listingBusy} onclick={closeListing}>{c.cancel}</button></div>
                {/if}
              </section>
            {/if}

            <details class="set-composition">
              <summary>{c.composition}</summary>
              <div class="table-scroll">
                <table>
                  <thead><tr><th>{c.part}</th><th>{c.oneSetNeeds}</th><th>{c.ownedForSet}</th><th>{c.missingForSet}</th><th>{c.price}</th></tr></thead>
                  <tbody>
                    {#each row.components as component (component.definition.slug)}
                      <tr>
                        <th scope="row"><span class="item-name">{#if component.imageUrl}<img src={component.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{component.displayName}</span></th>
                        <td>{component.definition.requiredQuantity}</td>
                        <td>{component.ownedQuantity}</td>
                        <td>{Math.max(0, component.definition.requiredQuantity * (opportunity.completeSets + 1) - component.ownedQuantity)}</td>
                        <td>{formatPlatinum(component.recommendation?.fairPrice ?? null, $locale)}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            </details>
          </article>
        {:else}
          <div class="message empty-result">
            <h2>{c.noResults}</h2>
            <p>{setQuery ? c.noSearchResults : activeMode === "relics" ? c.noRelicResults(ownedRelicCount) : activeMode === "buy" ? c.noBuyResults : c.noReadyResults}</p>
            {#if setQuery}<button type="button" onclick={() => (setQuery = "")}>{c.clearSearch}</button>{/if}
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</section>

<style>
  .opportunities { display: grid; gap: .75rem; }
  .data-status { min-height: 1.15rem; color: var(--text-muted); font-size: .75rem; }
  .message { border-radius: .75rem; padding: 1rem; background: var(--surface-2); box-shadow: var(--shadow-sm); }
  .message h2, .message p { margin: 0; }
  .message h2 { font-size: 1rem; }
  .message p { max-width: 68ch; margin-block-start: .3rem; color: var(--text-muted); }
  .message button { margin-block-start: .75rem; }
  .message--error { box-shadow: 0 0 0 1px var(--danger); background: var(--danger-soft); }
  .message--action { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
  .message--action button { flex: none; margin: 0; }
  .opportunity-toolbar { display: flex; align-items: end; justify-content: space-between; gap: 1rem; }
  .mode-switcher { display: flex; flex-wrap: wrap; gap: .4rem; }
  .mode-switcher button { border-color: var(--border); background: var(--surface-1); color: var(--text); }
  .mode-switcher button:hover { border-color: var(--border-strong); background: var(--surface-2); }
  .mode-switcher button[aria-pressed="true"] { border-color: var(--accent); background: var(--accent); color: oklch(0.985 0.009 84); }
  .mode-switcher span { margin-inline-start: .35rem; font-variant-numeric: tabular-nums; opacity: .78; }
  .set-search { display: grid; flex: 0 1 20rem; gap: .25rem; color: var(--text); font-size: .75rem; font-weight: 700; }
  .set-search input { min-height: 2.25rem; width: 100%; border: 1px solid var(--border); border-radius: .5rem; padding-inline: .65rem; background: oklch(0.995 0.004 84); color: var(--text); }
  .action-status { min-height: 1.15rem; color: var(--success); font-size: .75rem; font-weight: 700; }
  .set-list { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 32rem), 1fr)); align-items: start; gap: .7rem; }
  .set-card, .ducat-panel { min-width: 0; border-radius: .8rem; background: var(--surface-1); box-shadow: var(--shadow-sm); }
  .set-card { padding: .75rem; }
  .set-card__header { display: flex; align-items: start; justify-content: space-between; gap: .75rem; margin-block-end: .65rem; }
  .set-identity { display: flex; align-items: center; min-width: 0; gap: .65rem; }
  .set-image { flex: none; width: 3.25rem; height: 3.25rem; border-radius: .45rem; object-fit: contain; outline: 1px solid oklch(0 0 0 / .1); outline-offset: -1px; }
  .set-context { margin: 0 0 .1rem; color: var(--text-muted); font-size: .7rem; }
  .set-identity h2 { margin: 0; font-size: 1.05rem; line-height: 1.25; }
  .route-badge { flex: none; border-radius: 999px; padding: .22rem .5rem; background: var(--success-soft); color: oklch(0.37 0.08 145); font-size: .68rem; font-weight: 750; }
  .set-metrics { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: .4rem; margin: 0; }
  .set-metrics > div { min-width: 0; border-radius: .5rem; padding: .5rem; background: var(--surface-2); }
  dt { color: var(--text-muted); font-size: .7rem; }
  dd { margin: .15rem 0 0; font-size: 1rem; font-weight: 780; font-variant-numeric: tabular-nums; }
  dd small { display: block; margin-block-start: .08rem; color: var(--text-muted); font-size: .62rem; font-weight: 650; line-height: 1.25; }
  .positive dd { color: oklch(0.37 0.08 145); }
  .decision-copy { margin: .6rem 0 0; color: var(--text-muted); font-size: .78rem; }
  .missing-parts { display: flex; flex-wrap: wrap; gap: .35rem; margin-block-start: .55rem; }
  .missing-parts span { border-radius: 999px; padding: .22rem .5rem; background: var(--accent-soft); color: var(--accent-strong); font-size: .7rem; }
  .missing-parts strong { margin-inline-start: .35rem; font-variant-numeric: tabular-nums; }
  .card-actions, .order-actions { display: flex; flex-wrap: wrap; gap: .45rem; margin-block-start: .65rem; }
  .card-actions button, .order-actions button { flex: 0 1 auto; transition-property: scale, background-color, border-color; transition-duration: 120ms; transition-timing-function: ease-out; }
  .card-actions button:active, .order-actions button:active { scale: .96; }
  .relic-plan { margin-block-start: .75rem; border-radius: .65rem; padding: .65rem; background: var(--surface-2); }
  .relic-plan__summary { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: baseline; gap: .2rem .75rem; }
  .relic-plan__summary span { font-size: .76rem; font-weight: 700; }
  .relic-plan__summary strong { color: var(--accent-strong); font-size: 1.1rem; font-variant-numeric: tabular-nums; }
  .relic-plan__summary small { grid-column: 1 / -1; color: var(--text-muted); font-size: .68rem; }
  .relic-list { display: grid; gap: .5rem; margin-block-start: .65rem; }
  .relic-row { display: grid; grid-template-columns: minmax(11rem, 1.2fr) minmax(12rem, 1fr); gap: .55rem .75rem; border-radius: .55rem; padding: .55rem; background: var(--surface-1); box-shadow: 0 0 0 1px oklch(0 0 0 / .06); }
  .relic-identity { display: flex; align-items: center; min-width: 0; gap: .55rem; }
  .relic-identity img { flex: none; width: 2.75rem; height: 2.75rem; border-radius: .35rem; object-fit: contain; outline: 1px solid oklch(0 0 0 / .1); outline-offset: -1px; }
  .relic-identity h3, .relic-identity p { margin: 0; }
  .relic-identity h3 { font-size: .86rem; }
  .relic-identity p { margin-block-start: .12rem; color: var(--text-muted); font-size: .68rem; }
  .relic-row dl { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: .3rem; margin: 0; }
  .relic-row dl div { min-width: 0; border-radius: .4rem; padding: .35rem; background: var(--surface-2); }
  .relic-row dd { font-size: .82rem; }
  .useful-rewards { grid-column: 1 / -1; display: flex; flex-wrap: wrap; gap: .35rem; }
  .useful-rewards > span { display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: 0 .4rem; border-radius: .45rem; padding: .3rem .45rem; background: var(--success-soft); color: oklch(0.32 0.065 145); font-size: .72rem; font-weight: 700; }
  .useful-rewards img { grid-row: 1 / 3; width: 1.75rem; height: 1.75rem; object-fit: contain; outline: 1px solid oklch(0 0 0 / .1); outline-offset: -1px; }
  .useful-rewards small { color: oklch(0.43 0.05 145); font-size: .62rem; font-weight: 650; }
  .order-panel { margin-block-start: .7rem; border-radius: .65rem; padding: .65rem; background: var(--surface-2); box-shadow: 0 0 0 1px var(--border); }
  .order-panel h3, .order-panel p { margin: 0; }
  .order-panel h3 { font-size: .9rem; }
  .order-panel p { margin-block-start: .35rem; color: var(--text-muted); }
  .order-panel form { display: grid; gap: .5rem; margin-block-start: .5rem; }
  .order-fields { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .45rem; }
  .order-fields label { display: grid; gap: .25rem; font-size: .75rem; font-weight: 700; }
  .order-fields input { min-width: 0; min-height: 2.25rem; width: 100%; border: 1px solid var(--border); border-radius: .45rem; padding-inline: .55rem; background: oklch(0.995 0.004 84); color: var(--text); }
  .order-visible { display: flex; align-items: center; width: fit-content; min-height: 2.125rem; gap: .45rem; font-size: .75rem; font-weight: 700; cursor: pointer; }
  .order-visible input { width: 1.1rem; height: 1.1rem; accent-color: var(--accent); }
  .inline-error { color: var(--danger) !important; font-size: .75rem; }
  .set-composition { margin-block-start: .45rem; }
  summary { min-height: 2.125rem; padding-block: .4rem; color: var(--accent-strong); cursor: pointer; font-size: .78rem; font-weight: 700; }
  .table-scroll { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-size: .8rem; }
  th, td { border-block-end: 1px solid var(--border); padding: .45rem .5rem; text-align: start; font-variant-numeric: tabular-nums; }
  thead th { color: var(--text-muted); font-size: .68rem; text-transform: uppercase; letter-spacing: .035em; }
  tbody th { font-weight: 680; }
  .item-name { display: inline-flex; align-items: center; gap: .5rem; }
  .item-name img { flex: none; width: 2rem; height: 2rem; object-fit: contain; outline: 1px solid oklch(0 0 0 / .1); outline-offset: -1px; }
  .ducat-panel { overflow: hidden; }
  .ducat-panel > header { display: flex; align-items: start; justify-content: space-between; gap: 1rem; padding: .8rem; background: var(--surface-2); }
  .ducat-panel h2, .ducat-panel p { margin: 0; }
  .ducat-panel h2 { font-size: 1rem; }
  .ducat-panel header div p { margin-block-start: .25rem; color: var(--text-muted); font-size: .76rem; }
  .ducat-panel .warning { max-width: 28rem; color: var(--danger); font-size: .7rem; text-align: end; }
  .empty-result { text-align: center; }
  @media (max-width: 58rem) {
    .opportunity-toolbar, .message--action { align-items: stretch; flex-direction: column; }
    .set-search { flex-basis: auto; width: 100%; }
    .message--action button { align-self: start; }
    .relic-row { grid-template-columns: minmax(0, 1fr); }
    .useful-rewards { grid-column: 1; }
  }
  @media (max-width: 42rem) {
    .mode-switcher { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .set-card__header { align-items: stretch; flex-direction: column; }
    .route-badge { width: fit-content; }
    .set-metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .card-actions button { flex: 1 1 12rem; }
    .ducat-panel > header { flex-direction: column; }
    .ducat-panel .warning { text-align: start; }
  }
  @media (max-width: 28rem) {
    .mode-switcher, .order-fields, .relic-row dl { grid-template-columns: minmax(0, 1fr); }
    .set-card { padding: .65rem; }
  }
  @media (forced-colors: active) {
    .set-card, .relic-row, .order-panel, .mode-switcher button[aria-pressed="true"] { outline: 1px solid CanvasText; }
  }
</style>
