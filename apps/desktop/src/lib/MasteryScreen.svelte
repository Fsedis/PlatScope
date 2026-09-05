<script lang="ts">
  import { tick } from "svelte";
  import { localeCode, useLocale } from "./i18n";
  import { filterMasteryItems, masteryCategoryLabel, masteryExplanation, masteryStore, type MasteryStatus } from "./mastery";
  import MasteryStatusMark from "./MasteryStatus.svelte";
  export let scanning = false;
  export let onScan: () => void;
  const locale = useLocale();
  let query = "";
  let category = "all";
  let status: MasteryStatus | "all" = "all";
  let page = 1;
  let resultsElement: HTMLDivElement | null = null;
  const pageSize = 30;
  $: t = $locale === "ru" ? (ru: string, _en: string) => ru : (_ru: string, en: string) => en;
  $: view = $masteryStore.view;
  $: categories = [...new Set((view?.items ?? []).map(item => item.category))]
    .sort((a, b) => masteryCategoryLabel(a, $locale).localeCompare(masteryCategoryLabel(b, $locale), $locale));
  $: filtered = filterMasteryItems(view?.items ?? [], query, category, status, $locale);
  $: pageCount = Math.max(1, Math.ceil(filtered.length / pageSize));
  $: currentPage = Math.min(page, pageCount);
  $: visible = filtered.slice((currentPage - 1) * pageSize, currentPage * pageSize);
  $: observed = view?.observedAt ? new Date(view.observedAt).toLocaleString(localeCode($locale)) : null;
  $: stale = $masteryStore.error || view?.refreshFailed;
  function resetFilters() { query = ""; category = "all"; status = "all"; page = 1; }
  async function changePage(next: number) {
    page = next;
    await tick();
    resultsElement?.scrollIntoView({ block: "start" });
    resultsElement?.focus({ preventScroll: true });
  }
</script>

<section class="mastery-screen" aria-labelledby="mastery-title">
  <header class="mastery-header">
    <div><h2 id="mastery-title">{t("Освоение аккаунта", "Account mastery")}</h2><p>{t("История снаряжения, включая предметы, которых уже нет в инвентаре.", "Equipment history, including items no longer in your inventory.")}</p></div>
    <button type="button" onclick={onScan} disabled={scanning}>{scanning ? t("Обновляем…", "Updating…") : t("Обновить из Warframe", "Update from Warframe")}</button>
    {#if view && view.catalogAvailable}
      <div class="history-status">
        <p>{observed ? t(`История от ${observed}`, `History from ${observed}`) : t("История ещё не получена. Запустите Warframe и обновите данные.", "History has not been received yet. Start Warframe and update the data.")}</p>
        <details><summary>{t("Что означают отметки", "About these labels")}</summary><p>{t("«Освоено» с игровым венком — достигнут максимальный ранг. «Не освоено» — по истории максимальный ранг ещё не достигнут; рядом указан прогресс. «Нет данных» — нет записи либо правило пока не поддерживается. Продажа, применение Формы и настройка «Оставлять копий» не сбрасывают освоение. Для модульного снаряжения учитывается определяющая деталь, например призма усилителя, а не каждая сборка.", "Mastered with the in-game laurel means the maximum rank was reached. Not mastered means history is below the maximum rank; progress is shown alongside it. Unknown means a missing entry or unsupported rule. Selling, Forma and Keep copies do not reset mastery. Modular equipment is tracked by its mastery-bearing part, such as an amp prism, not each build.")}</p></details>
      </div>
    {/if}
  </header>

  {#if stale}
    <div class="notice" role="alert"><p>{view?.observedAt ? t("Не удалось обновить историю освоения. Показаны последние сохранённые данные.", "Mastery history could not be refreshed. Showing the last saved data.") : t("Не удалось получить историю освоения.", "Mastery history could not be loaded.")}</p>{#if $masteryStore.error}<button class="secondary" type="button" disabled={$masteryStore.loading} onclick={() => masteryStore.refresh()}>{t("Повторить загрузку", "Retry loading")}</button>{/if}</div>
  {/if}
  {#if $masteryStore.loading && !view}
    <p class="empty" role="status">{t("Загружаем историю освоения…", "Loading mastery history…")}</p>
  {:else if view && !view.catalogAvailable}
    <div class="empty"><h3>{t("Каталог снаряжения ещё не загружен", "Equipment catalog is not loaded yet")}</h3><p>{t("Обновите данные предметов в настройках. История аккаунта сохранится независимо от каталога.", "Update item data in Settings. Account history is retained independently of the catalog.")}</p></div>
  {:else if view}
    <div class="mastery-filters">
      <label class="search">{t("Найти снаряжение", "Find equipment")}<input type="search" bind:value={query} oninput={() => page = 1} placeholder={t("Например, Никс Прайм или Nyx Prime", "For example, Nyx Prime")} /></label>
      <label>{t("Тип снаряжения", "Equipment type")}<select bind:value={category} onchange={() => page = 1}><option value="all">{t("Все типы", "All types")}</option>{#each categories as value}<option value={value}>{masteryCategoryLabel(value, $locale)}</option>{/each}</select></label>
      <label>{t("Освоение", "Mastery")}<select bind:value={status} onchange={() => page = 1}><option value="all">{t("Все состояния", "All statuses")}</option><option value="mastered">{t("Освоено", "Mastered")}</option><option value="progress">{t("Не освоено", "Not mastered")}</option><option value="unknown">{t("Нет данных", "Unknown")}</option></select></label>
    </div>
    <div class="mastery-result-count" role="status" tabindex="-1" bind:this={resultsElement}>{t(`Найдено: ${filtered.length} из ${view.items.length}`, `Found: ${filtered.length} of ${view.items.length}`)}{#if $masteryStore.loading || scanning} · {t("обновляем…", "updating…")}{/if}</div>
    <div class="mastery-list">
      {#each visible as item (item.gameRef)}
        <article class="mastery-item">
          <div class="item-main">
            {#if item.imageUrl}<img src={item.imageUrl} alt="" loading="lazy" />{:else}<span class="image-placeholder" aria-hidden="true">◇</span>{/if}
            <div class="item-name"><h3>{$locale === "ru" ? item.displayName : item.displayNameEn}</h3><p>{masteryCategoryLabel(item.category, $locale)}{#if $locale === "ru" && item.displayName !== item.displayNameEn}{` · ${item.displayNameEn}`}{/if}</p></div>
            <span class="item-status"><MasteryStatusMark {item} historyAvailable={Boolean(observed)} /></span>
          </div>
          <details class="item-details"><summary>{t("Почему такой статус", "Why this status")}</summary><p>{masteryExplanation(item, $locale)}</p>{#if item.xp !== null}<p>{t("Накопленный опыт в истории аккаунта", "Accumulated XP in account history")}: {item.xp.toLocaleString(localeCode($locale))}</p>{/if}{#if view.source && observed}<p>{t("Источник: данные аккаунта Warframe", "Source: Warframe account data")} · {observed}</p>{/if}</details>
        </article>
      {:else}
        <div class="empty"><h3>{t("Ничего не найдено", "No matches")}</h3><p>{t("Измените название, тип снаряжения или состояние.", "Change the name, equipment type or status.")}</p><button type="button" class="secondary" onclick={resetFilters}>{t("Сбросить фильтры", "Reset filters")}</button></div>
      {/each}
    </div>
    {#if pageCount > 1}<nav class="pagination" aria-label={t("Страницы снаряжения", "Equipment pages")}><button type="button" class="secondary" disabled={currentPage === 1} onclick={() => changePage(currentPage - 1)}>{t("Назад", "Previous")}</button><span>{t(`Страница ${currentPage} из ${pageCount}`, `Page ${currentPage} of ${pageCount}`)}</span><button type="button" class="secondary" disabled={currentPage === pageCount} onclick={() => changePage(currentPage + 1)}>{t("Дальше", "Next")}</button></nav>{/if}
  {/if}
</section>

<style>
  .mastery-screen{display:grid;gap:.8rem;min-width:0}.mastery-header,.mastery-filters,.mastery-list{border:1px solid var(--border);background:var(--surface-1);border-radius:.8rem}.mastery-header{padding:1rem;display:grid;grid-template-columns:minmax(0,1fr) auto;gap:.7rem 1rem;align-items:center}.mastery-header button{flex-shrink:0}h2,h3,p{margin:0}h2{font-size:1.15rem}h3{font-size:.95rem}.mastery-header p,.history-status p,.notice p,.empty p{font-size:.85rem;color:var(--text-muted);line-height:1.5;margin-top:.4rem}.history-status{grid-column:1/-1;display:flex;gap:.6rem 1.5rem;align-items:start;justify-content:space-between}.history-status>p{margin:0;flex:1;font-size:.78rem}.history-status details{max-width:36rem;flex:1}.history-status summary{width:fit-content;margin-left:auto}.history-status details[open] summary{margin-left:0}summary{font-size:.8rem;cursor:pointer;font-weight:600}.mastery-filters{padding:.8rem 1rem;display:flex;gap:1rem;align-items:end;flex-wrap:wrap}label{display:grid;gap:.4rem;font-size:.8rem}label.search{flex:1;min-width:16rem}input,select{width:100%;min-width:0;background:var(--surface-1);border:1px solid var(--border);border-radius:.45rem;color:var(--text);font:inherit;padding:.6rem}.mastery-result-count{color:var(--text-muted);font-size:.8rem}.mastery-list{overflow:hidden}.mastery-item{padding:.8rem 1rem;border-bottom:1px solid var(--border)}.mastery-item:last-child{border:0}.item-main{display:flex;align-items:center;gap:.8rem}.item-main img,.image-placeholder{width:42px;height:42px;object-fit:contain;flex-shrink:0}.image-placeholder{display:grid;place-items:center;font-size:1.5rem;color:var(--text-muted);border:1px solid var(--border);border-radius:.4rem}.item-name{flex:1;min-width:0}.item-name h3{overflow-wrap:anywhere}.item-name p{margin-top:.25rem;color:var(--text-muted);font-size:.76rem;line-height:1.4}.item-status{font-size:.8rem;color:var(--text-muted);text-align:right;max-width:15rem;flex-shrink:0}.item-details{margin:.4rem 0 0 3.4rem}.item-details summary{color:var(--text-muted);font-weight:400;width:fit-content}.item-details p{font-size:.8rem;line-height:1.5;margin-top:.5rem;max-width:60rem;color:var(--text-muted)}.empty{padding:1.5rem}.empty button,.notice button{margin-top:.6rem}.notice{padding:.8rem 1rem;border:1px solid var(--border);border-radius:.6rem;background:var(--surface-2)}.pagination{display:flex;align-items:center;justify-content:center;gap:1rem}.pagination span{font-size:.8rem;color:var(--text-muted)}
  @media(max-width:800px){.mastery-header{grid-template-columns:minmax(0,1fr)}.mastery-header button{justify-self:start}.history-status{flex-wrap:wrap}.history-status details{min-width:100%}.history-status summary{margin-left:0}.item-status{max-width:11rem}.mastery-filters>label{flex:1;min-width:12rem}}
</style>
