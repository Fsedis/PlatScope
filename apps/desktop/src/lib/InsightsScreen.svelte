<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { localeCode, useLocale } from "./i18n";

  import { formatPlatinum } from "./market";
  import {
    coverageLabel,
    formatPercent,
    formatRatio,
    refinementLabel,
    relicReasonMessages,
    rivenCategoryLabel,
    setModeLabel,
    setReasonMessages,
    vaultLabel,
    type GameMetadataRefreshOutcome,
    type InsightsView,
  } from "./insights";

  type InsightTab = "sets" | "relics" | "ducats" | "rivens";

  const locale = useLocale();
  const copy = {
    ru: {
      loadError: (r: string) => `Не удалось прочитать локальную аналитику. Сохранённые цены и инвентарь не изменены. Техническая причина: ${r}`, refreshError: (r: string) => `Метаданные не обновлены. Последний корректный снимок сохранён. Сначала убедитесь, что рыночный каталог загружен. Техническая причина: ${r}`,
      kicker: "Локальная аналитика", heading: "Комплекты, реликвии, дукаты и Riven", intro: "WFCD-метаданные соединяются с точными вариантами инвентаря и credible fair price. Riven disposition показывается отдельно от оценки уникальных модов.", refreshing: "Обновляем метаданные…", refresh: "Обновить метаданные", reading: "Читаем последний корректный снимок метаданных…", validating: "Загружаем bounded-набор документов WFCD и проверяем схему…", lkg: "Новый снимок не прошёл проверку. Используется предыдущий LKG.", updated: (d: string) => `Метаданные обновлены: ${d}.`, ready: (d: string) => `Локальный снимок от ${d} готов.`, retry: "Повторить чтение",
      noSnapshot: "Снимка метаданных ещё нет", loadOnce: "Загрузите определения один раз", loadBody: "После проверки они сохранятся локально. Сбой источника не затронет рыночные цены.", load: "Загрузить метаданные", relics: "Реликвии", importInventory: "Импортируйте инвентарь: метаданные уже доступны, но персональные расчёты пока пусты.", tabs: "Вид аналитики", sets: "Комплекты", ducats: "Дукаты", rivens: "Riven",
      buildable: "Можно собрать", partsSum: "Сумма деталей", part: "Деталь", needed: "Нужно", sellable: "Для продажи", why: "Почему такой вывод?", noParts: "Нет деталей для сравнения", noPartsBody: "Раздел появится, когда в инвентаре будут распознаны Prime parts из одного комплекта.", owned: "В инвентаре", chanceCovered: "Покрыто шансом", reward: "Награда", chance: "Шанс", ev: "Как рассчитан EV?", noRelics: "Реликвии не найдены", noRelicsBody: "Нужен распознанный exact subtype: Intact, Exceptional, Flawless или Radiant.",
      ducatNote: "Plat/ducat — сравнительный показатель, а не автоматическая команда сдавать предмет Baro.", status: "Статус", insufficient: "Недостаточно данных", noDucats: "В инвентаре нет распознанных Prime parts с ducat metadata.", vaultDisclaimer: "Vault status — только дополнительный контекст. PlatScope не предполагает, что vaulted-предмет обязательно подорожает.",
      fairSet: "Справедливая цена комплекта", setPremium: "Премия комплекта", fair: "Справедливая", relicFair: "Справедливая цена реликвии", pricedEv: "Оценённый EV", credibleFair: "Надёжная справедливая цена", primeSetCount: "Prime-комплекты", primePartCount: "Prime-детали", itemDefinitionCount: "Предметы с MR", primePart: "Prime-деталь", platinumPerDucat: "Платина / дукат", credible: "Надёжно", rivenWeaponCount: "Оружие с disposition", rivenSearch: "Найти оружие", rivenSearchPlaceholder: "Например, Soma", weapon: "Оружие", category: "Категория", disposition: "Disposition", multiplier: "Множитель", averageMultiplier: "Средний множитель", multiplierRange: "Диапазон", rivenNote: "Disposition — общий коэффициент оружия из WFCD. Он не учитывает характеристики, прокачку, полярность и комбинацию положительных или отрицательных свойств конкретного Riven и не является ценой.", noRivens: "Оружие не найдено.", limitedRivens: (shown: number, total: number) => `Показаны первые ${shown} из ${total}. Уточните поиск, чтобы сузить список.`,
    },
    en: {
      loadError: (r: string) => `Unable to read local insights. Saved prices and inventory were not changed. Technical reason: ${r}`, refreshError: (r: string) => `Metadata was not refreshed. The latest valid snapshot was preserved. Make sure the market catalog is loaded first. Technical reason: ${r}`,
      kicker: "Local insights", heading: "Sets, relics, ducats, and Riven", intro: "WFCD metadata is combined with exact inventory variants and credible fair prices. Riven disposition stays separate from unique mod valuation.", refreshing: "Refreshing metadata…", refresh: "Refresh metadata", reading: "Reading the latest valid metadata snapshot…", validating: "Downloading a bounded WFCD document set and validating its schema…", lkg: "The new snapshot failed validation. Using the previous LKG.", updated: (d: string) => `Metadata refreshed: ${d}.`, ready: (d: string) => `Local snapshot from ${d} is ready.`, retry: "Read again",
      noSnapshot: "No metadata snapshot yet", loadOnce: "Load definitions once", loadBody: "After validation, they are stored locally. A provider failure will not affect market prices.", load: "Load metadata", relics: "Relics", importInventory: "Import inventory: metadata is ready, but personalized calculations are empty.", tabs: "Insight view", sets: "Sets", ducats: "Ducats", rivens: "Riven",
      buildable: "Buildable", partsSum: "Parts total", part: "Part", needed: "Required", sellable: "Sellable", why: "Why this result?", noParts: "No parts to compare", noPartsBody: "This section appears when the inventory contains recognized Prime parts from one set.", owned: "Owned", chanceCovered: "Chance covered", reward: "Reward", chance: "Chance", ev: "How is EV calculated?", noRelics: "No relics found", noRelicsBody: "A recognized exact subtype is required: Intact, Exceptional, Flawless, or Radiant.",
      ducatNote: "Plat/ducat is a comparison metric, not an automatic instruction to trade an item to Baro.", status: "Status", insufficient: "Insufficient data", noDucats: "Inventory has no recognized Prime parts with ducat metadata.", vaultDisclaimer: "Vault status is additional context only. PlatScope does not assume that a vaulted item must rise in price.",
      fairSet: "Fair set", setPremium: "Set premium", fair: "Fair", relicFair: "Relic fair", pricedEv: "Priced EV", credibleFair: "Credible fair", primeSetCount: "Prime sets", primePartCount: "Prime parts", itemDefinitionCount: "Items with MR", primePart: "Prime part", platinumPerDucat: "Plat / ducat", credible: "Credible", rivenWeaponCount: "Weapons with disposition", rivenSearch: "Find a weapon", rivenSearchPlaceholder: "For example, Soma", weapon: "Weapon", category: "Category", disposition: "Disposition", multiplier: "Multiplier", averageMultiplier: "Average multiplier", multiplierRange: "Range", rivenNote: "Disposition is a weapon-wide coefficient from WFCD. It does not account for the stats, rank, polarity, or positive and negative property mix of a specific Riven and is not a price.", noRivens: "No weapons found.", limitedRivens: (shown: number, total: number) => `Showing the first ${shown} of ${total}. Refine the search to narrow the list.`,
    },
  } as const;
  $: c = copy[$locale];

  let view: InsightsView | null = null;
  let activeTab: InsightTab = "sets";
  let loading = true;
  let refreshing = false;
  let errorMessage = "";
  let refreshOutcome: GameMetadataRefreshOutcome | null = null;
  let rivenQuery = "";
  $: normalizedRivenQuery = rivenQuery.trim().toLocaleLowerCase(localeCode($locale));
  $: filteredRivens = (view?.rivenDispositions ?? []).filter((definition) =>
    definition.weaponNameEn.toLocaleLowerCase(localeCode($locale)).includes(normalizedRivenQuery)
  );
  $: visibleRivens = filteredRivens.slice(0, 200);
  $: rivenAverage = view?.rivenDispositions.length
    ? view.rivenDispositions.reduce((sum, definition) => sum + definition.multiplier, 0) / view.rivenDispositions.length
    : null;
  $: rivenMinimum = view?.rivenDispositions.length
    ? Math.min(...view.rivenDispositions.map((definition) => definition.multiplier))
    : null;
  $: rivenMaximum = view?.rivenDispositions.length
    ? Math.max(...view.rivenDispositions.map((definition) => definition.multiplier))
    : null;

  async function loadInsights(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      view = await invoke<InsightsView | null>("insights");
    } catch (error) {
      view = null;
      errorMessage = c.loadError(String(error));
    } finally {
      loading = false;
    }
  }

  async function refreshMetadata(): Promise<void> {
    refreshing = true;
    errorMessage = "";
    try {
      refreshOutcome = await invoke<GameMetadataRefreshOutcome>("refresh_game_metadata");
      await loadInsights();
    } catch (error) {
      errorMessage = c.refreshError(String(error));
    } finally {
      refreshing = false;
    }
  }

  onMount(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void loadInsights();
    void listen("game-metadata-updated", () => void loadInsights()).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });
</script>

<section class="insights-shell" aria-labelledby="insights-heading">
  <div class="insights-toolbar">
    <div>
      <p class="section-kicker">{c.kicker}</p><h2 id="insights-heading">{c.heading}</h2><p>{c.intro}</p>
    </div>
    <button type="button" onclick={refreshMetadata} disabled={refreshing}>
      {refreshing ? c.refreshing : c.refresh}
    </button>
  </div>

  <div class="insights-status" role="status" aria-live="polite">
    {#if loading}
      {c.reading}
    {:else if refreshing}
      {c.validating}
    {:else if refreshOutcome?.usedLkg}
      {c.lkg}
    {:else if refreshOutcome}
      {c.updated(refreshOutcome.metadata.fetchedAt.slice(0, 10))}
    {:else if view}
      {c.ready(view.metadata.fetchedAt.slice(0, 10))}
    {/if}
  </div>

  {#if errorMessage}
    <div class="insights-error" role="alert">
      <p>{errorMessage}</p>
      <button type="button" onclick={loadInsights}>{c.retry}</button>
    </div>
  {/if}

  {#if !loading && !view}
    <div class="insights-empty">
      <p class="section-kicker">{c.noSnapshot}</p><h3>{c.loadOnce}</h3><p>{c.loadBody}</p><button type="button" onclick={refreshMetadata} disabled={refreshing}>{c.load}</button>
    </div>
  {:else if view}
    <dl class="insights-summary">
      <div><dt>{c.primeSetCount}</dt><dd>{view.metadata.setCount.toLocaleString(localeCode($locale))}</dd></div><div><dt>{c.relics}</dt><dd>{view.metadata.relicCount.toLocaleString(localeCode($locale))}</dd></div><div><dt>{c.primePartCount}</dt><dd>{view.metadata.primePartCount.toLocaleString(localeCode($locale))}</dd></div><div><dt>{c.itemDefinitionCount}</dt><dd>{view.metadata.itemDefinitionCount.toLocaleString(localeCode($locale))}</dd></div><div><dt>{c.rivenWeaponCount}</dt><dd>{view.metadata.rivenDispositionCount.toLocaleString(localeCode($locale))}</dd></div>
    </dl>

    {#if !view.inventoryAvailable}
      <div class="insights-note" role="note">
        {c.importInventory}
      </div>
    {/if}

    <div class="insight-tabs" role="group" aria-label={c.tabs}>
      <button type="button" aria-pressed={activeTab === "sets"} onclick={() => (activeTab = "sets")}>
        {c.sets} <span>{view.sets.length}</span>
      </button>
      <button type="button" aria-pressed={activeTab === "relics"} onclick={() => (activeTab = "relics")}>
        {c.relics} <span>{view.relics.length}</span>
      </button>
      <button type="button" aria-pressed={activeTab === "ducats"} onclick={() => (activeTab = "ducats")}>
        {c.ducats} <span>{view.ducats.length}</span>
      </button>
      <button type="button" aria-pressed={activeTab === "rivens"} onclick={() => (activeTab = "rivens")}>
        {c.rivens} <span>{view.rivenDispositions.length}</span>
      </button>
    </div>

    {#if activeTab === "sets"}
      <div class="insight-list">
        {#each view.sets as row (row.definition.setSlug)}
          <article class="insight-card">
            <header>
              {#if row.imageUrl}<img class="insight-thumb" src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
              <div>
                <p class={`vault vault--${row.definition.vaultStatus}`}>{vaultLabel(row.definition.vaultStatus, $locale)}</p>
                <h3>{row.definition.displayNameEn}</h3>
              </div>
              <strong class={`decision decision--${row.comparison.recommendedMode}`}>
                {setModeLabel(row.comparison.recommendedMode, $locale)}
              </strong>
            </header>
            <dl class="metric-grid">
              <div><dt>{c.buildable}</dt><dd>{row.comparison.completeSets}</dd></div><div><dt>{c.fairSet}</dt><dd>{formatPlatinum(row.comparison.setFairValue, $locale)}</dd></div><div><dt>{c.partsSum}</dt><dd>{formatPlatinum(row.comparison.partsFairValue, $locale)}</dd></div><div><dt>{c.setPremium}</dt><dd>{formatPercent(row.comparison.setPremiumPercent, $locale)}</dd></div>
            </dl>
            <div class="table-scroll">
              <table>
                <thead><tr><th>{c.part}</th><th>{c.needed}</th><th>{c.sellable}</th><th>{c.fair}</th></tr></thead>
                <tbody>
                  {#each row.components as component (component.definition.slug)}
                    <tr>
                      <th scope="row"><span class="insight-item-name">{#if component.imageUrl}<img src={component.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{component.definition.slug.replaceAll("_", " ")}</span></th>
                      <td>{component.definition.requiredQuantity}</td>
                      <td>{component.ownedQuantity}</td>
                      <td>{formatPlatinum(component.recommendation?.fairPrice ?? null, $locale)}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
            <details>
              <summary>{c.why}</summary>
              <ul>{#each setReasonMessages(row, $locale) as reason}<li>{reason}</li>{/each}</ul>
            </details>
          </article>
        {:else}
          <div class="insights-empty compact">
            <h3>{c.noParts}</h3><p>{c.noPartsBody}</p>
          </div>
        {/each}
      </div>
    {:else if activeTab === "relics"}
      <div class="insight-list">
        {#each view.relics as row (`${row.definition.relicSlug}:${row.definition.refinement}`)}
          <article class="insight-card">
            <header>
              {#if row.imageUrl}<img class="insight-thumb" src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
              <div>
                <p class={`vault vault--${row.definition.vaultStatus}`}>{vaultLabel(row.definition.vaultStatus, $locale)}</p><h3>{row.definition.displayNameEn} · {refinementLabel(row.definition.refinement, $locale)}</h3>
              </div>
              <strong class={`decision decision--${row.expectedValue.coverage}`}>
                {coverageLabel(row.expectedValue.coverage, $locale)}
              </strong>
            </header>
            <dl class="metric-grid">
              <div><dt>{c.owned}</dt><dd>{row.ownedQuantity}</dd></div><div><dt>{c.relicFair}</dt><dd>{formatPlatinum(row.relicRecommendation?.fairPrice ?? null, $locale)}</dd></div><div><dt>{c.pricedEv}</dt><dd>{formatPlatinum(row.expectedValue.pricedExpectedValue, $locale)}</dd></div><div><dt>{c.chanceCovered}</dt><dd>{row.expectedValue.pricedChancePercent.toLocaleString(localeCode($locale), { maximumFractionDigits: 1 })}%</dd></div>
            </dl>
            <div class="table-scroll">
              <table>
                <thead><tr><th>{c.reward}</th><th>{c.chance}</th><th>{c.credibleFair}</th></tr></thead>
                <tbody>
                  {#each row.rewards as reward (reward.definition.rewardGameRef)}
                    <tr>
                      <th scope="row"><span class="insight-item-name">{#if reward.imageUrl}<img src={reward.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{reward.definition.displayNameEn}</span></th>
                      <td>{reward.definition.chancePercent.toLocaleString(localeCode($locale), { maximumFractionDigits: 2 })}%</td><td>{formatPlatinum(reward.recommendation?.fairPrice ?? null, $locale)}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
            <details>
              <summary>{c.ev}</summary>
              <ul>{#each relicReasonMessages(row, $locale) as reason}<li>{reason}</li>{/each}</ul>
            </details>
          </article>
        {:else}
          <div class="insights-empty compact">
            <h3>{c.noRelics}</h3><p>{c.noRelicsBody}</p>
          </div>
        {/each}
      </div>
    {:else if activeTab === "ducats"}
      <div class="insight-card ducat-card">
        <div class="insights-note" role="note">
          {c.ducatNote}
        </div>
        <div class="table-scroll">
          <table>
            <thead><tr><th>{c.primePart}</th><th>{c.sellable}</th><th>{c.fair}</th><th>{c.ducats}</th><th>{c.platinumPerDucat}</th><th>{c.status}</th></tr></thead>
            <tbody>
              {#each view.ducats as row (row.metadata.slug)}
                <tr>
                  <th scope="row"><span class="insight-item-name">{#if row.imageUrl}<img src={row.imageUrl} alt="" loading="lazy" decoding="async" />{/if}{row.displayName}</span></th>
                  <td>{row.sellableQuantity}</td>
                  <td>{formatPlatinum(row.efficiency.fairPrice, $locale)}</td>
                  <td>{row.efficiency.ducats}</td>
                  <td>{formatRatio(row.efficiency.platinumPerDucat, $locale)}</td><td>{row.efficiency.credible ? c.credible : c.insufficient}</td>
                </tr>
              {:else}
                <tr><td colspan="6">{c.noDucats}</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {:else}
      <div class="insight-card riven-card">
        <div class="insights-note" role="note">{c.rivenNote}</div>
        <dl class="metric-grid riven-metrics">
          <div><dt>{c.rivenWeaponCount}</dt><dd>{view.rivenDispositions.length.toLocaleString(localeCode($locale))}</dd></div>
          <div><dt>{c.averageMultiplier}</dt><dd>{formatRatio(rivenAverage, $locale)}</dd></div>
          <div><dt>{c.multiplierRange}</dt><dd>{formatRatio(rivenMinimum, $locale)}–{formatRatio(rivenMaximum, $locale)}</dd></div>
        </dl>
        <label class="riven-search">
          <span>{c.rivenSearch}</span>
          <input bind:value={rivenQuery} type="search" placeholder={c.rivenSearchPlaceholder} />
        </label>
        <div class="table-scroll">
          <table>
            <thead><tr><th>{c.weapon}</th><th>{c.category}</th><th>{c.disposition}</th><th>{c.multiplier}</th></tr></thead>
            <tbody>
              {#each visibleRivens as definition (definition.weaponGameRef)}
                <tr>
                  <th scope="row">{definition.weaponNameEn}</th>
                  <td>{rivenCategoryLabel(definition.category, $locale)}</td>
                  <td>{definition.disposition} / 5</td>
                  <td>{formatRatio(definition.multiplier, $locale)}×</td>
                </tr>
              {:else}
                <tr><td colspan="4">{c.noRivens}</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#if filteredRivens.length > visibleRivens.length}
          <p class="result-limit" role="status">{c.limitedRivens(visibleRivens.length, filteredRivens.length)}</p>
        {/if}
      </div>
    {/if}

    <p class="vault-disclaimer">
      {c.vaultDisclaimer}
    </p>
  {/if}
</section>

<style>
  .insights-shell { display: grid; gap: 1rem; }
  .insights-toolbar { display: flex; align-items: start; justify-content: space-between; gap: 1.25rem; padding: 1.1rem; border: 1px solid #283752; border-radius: .75rem; background: #111b2f; box-shadow: 0 .75rem 2rem rgb(0 0 0 / 14%); }
  .insights-toolbar h2 { margin-block-end: .35rem; font-size: 1.25rem; }
  .insights-toolbar p { margin-block-end: 0; color: #9ba9bd; max-width: 65ch; }
  .section-kicker { margin-block-end: .3rem !important; color: #72a7ff !important; font-size: .78rem; font-weight: 750; letter-spacing: .08em; text-transform: uppercase; }
  .insights-status { min-height: 1.5rem; color: #9ba9bd; }
  .insights-error, .insights-empty, .insights-note { border: 1px solid #34496b; border-radius: .7rem; padding: 1rem; background: #0c1526; }
  .insights-error { border-color: #9c5555; background: #2b1719; }
  .insights-error p, .insights-empty p { color: #c2d1d6; }
  .insights-error p, .insights-empty p:last-of-type { margin-block-end: .8rem; }
  .insights-empty.compact { text-align: center; }
  .insights-summary, .metric-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: .65rem; margin: 0; }
  .insights-summary div, .metric-grid div { min-width: 0; border: 1px solid #283752; border-radius: .65rem; padding: .85rem; background: #172238; }
  dt { color: #91aab3; font-size: .8rem; }
  dd { margin: .2rem 0 0; font-variant-numeric: tabular-nums; font-size: 1.2rem; font-weight: 760; }
  .insight-tabs { display: flex; gap: .5rem; overflow-x: auto; margin: -.4rem; padding: .4rem; }
  .insight-tabs button { flex: 0 0 auto; }
  .insight-tabs button[aria-pressed="true"] { border-color: #72a7ff; background: #20385f; }
  .insight-tabs span { margin-inline-start: .35rem; color: #b9d7df; font-variant-numeric: tabular-nums; }
  .insight-list { display: grid; gap: .85rem; }
  .insight-card { min-width: 0; border: 1px solid #283752; border-radius: .75rem; padding: 1rem; background: #111b2f; box-shadow: 0 .75rem 2rem rgb(0 0 0 / 14%); }
  .insight-card > header { display: flex; align-items: start; justify-content: space-between; gap: 1rem; margin-block-end: .85rem; }
  .insight-thumb { flex: none; width: 4rem; height: 4rem; object-fit: contain; outline: 1px solid rgb(255 255 255 / 10%); outline-offset: -1px; }
  .insight-item-name { display: inline-flex; align-items: center; gap: .55rem; }
  .insight-item-name img { flex: none; width: 2rem; height: 2rem; object-fit: contain; outline: 1px solid rgb(255 255 255 / 10%); outline-offset: -1px; }
  .insight-card h3 { margin-block-end: 0; font-size: 1.05rem; }
  .vault { margin-block-end: .25rem; font-size: .78rem; font-weight: 720; }
  .vault--available { color: #75d3ad; }
  .vault--vaulted { color: #e7bd72; }
  .vault--unknown { color: #9fb0b6; }
  .decision { max-width: 18rem; border-radius: 999px; padding: .35rem .65rem; background: #193642; color: #bfe8f1; font-size: .78rem; text-align: center; }
  .decision--set, .decision--complete { background: #183c31; color: #a7e8ca; }
  .decision--parts, .decision--partial { background: #44351d; color: #f0d59c; }
  .decision--insufficient, .decision--insufficient_pricing { background: #412528; color: #f0b9be; }
  .metric-grid { grid-template-columns: repeat(4, minmax(0, 1fr)); margin-block-end: .85rem; }
  .metric-grid dd { font-size: 1rem; }
  .table-scroll { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-size: .88rem; }
  th, td { border-block-end: 1px solid #283752; padding: .65rem .55rem; text-align: start; font-variant-numeric: tabular-nums; }
  thead th { color: #9db3bb; font-size: .76rem; text-transform: uppercase; letter-spacing: .04em; }
  tbody th { font-weight: 650; }
  details { margin-block-start: .75rem; }
  summary { min-height: 2.5rem; padding-block: .55rem; color: #b9dce4; cursor: pointer; font-weight: 650; }
  details ul { margin-block-end: 0; padding-inline-start: 1.25rem; color: #b4c5ca; }
  details li + li { margin-block-start: .35rem; }
  .ducat-card { padding: 0; overflow: hidden; }
  .ducat-card .insights-note { border-width: 0 0 1px; border-radius: 0; }
  .ducat-card table { min-width: 46rem; }
  .riven-card { padding: 0; overflow: hidden; }
  .riven-card .insights-note { border-width: 0 0 1px; border-radius: 0; }
  .riven-metrics { grid-template-columns: repeat(3, minmax(0, 1fr)); padding: 1rem; }
  .riven-search { display: grid; gap: .4rem; padding: 0 1rem 1rem; color: #b9dce4; font-weight: 650; }
  .riven-search input { min-height: 2.75rem; width: min(100%, 32rem); }
  .riven-card table { min-width: 38rem; }
  .result-limit { margin: 0; padding: .8rem 1rem; color: #a9bec6; }
  .vault-disclaimer { margin: 0; color: #9eb0b6; font-size: .86rem; }
  @media (max-width: 48rem) {
    .insights-toolbar, .insight-card > header { align-items: stretch; flex-direction: column; }
    .insights-toolbar button { width: 100%; }
    .metric-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }
  @media (max-width: 30rem) {
    .insights-summary { grid-template-columns: 1fr; }
    .metric-grid { grid-template-columns: 1fr 1fr; }
    .insight-card { padding: .8rem; }
  }
</style>
