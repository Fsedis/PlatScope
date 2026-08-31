<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import { useLocale } from "./i18n";
  import type { EquipmentKind, InventoryView } from "./inventory";
  import {
    buildEquippedModEntries,
    configLabel,
    filterEquippedModEntries,
    summarizeEquippedMods,
    type EquippedModEntry,
  } from "./equippedMods";

  export let onInventoryChange: (() => void) | undefined = undefined;

  const locale = useLocale();
  const copy = {
    ru: {
      loading: "Загружаем установленные моды…",
      error: "Не удалось открыть данные о сборках.",
      retry: "Повторить",
      notScanned: "Сборки ещё не считаны",
      notScannedBody: "Запустите Warframe и обновите инвентарь — PlatScope прочитает моды в конфигурациях A/B/C.",
      update: "Обновить из Warframe",
      updating: "Обновляем…",
      scanError: "Не удалось прочитать сборки. Запустите Warframe, войдите в игру и повторите.",
      summaryVariants: "Модов",
      summaryCopies: "Надето копий",
      summaryEquipment: "Предметов",
      summaryConfigurations: "Конфигураций",
      search: "Найти мод",
      searchPlaceholder: "Например, Поток Прайм",
      type: "Установлен на",
      allTypes: "Любой предмет",
      listTitle: "Надетые моды",
      shown: (count: number) => `Показано: ${count}`,
      empty: "Надетые моды не найдены",
      emptyBody: "В считанных конфигурациях нет модов из торгового инвентаря.",
      noFiltered: "Ничего не найдено",
      noFilteredBody: "Измените название мода или тип предмета.",
      clear: "Сбросить",
      rank: (rank: number) => `ранг ${rank}`,
      copies: (count: number) => `копий: ${count}`,
      configs: (count: number) => `конфигураций: ${count}`,
      selectPrompt: "Выберите мод слева",
      selectPromptBody: "Здесь появятся все предметы и конфигурации, где он установлен.",
      installedOn: "Где установлен",
      usage: (equipment: number, configs: number) => `Предметов: ${equipment} · Конфигураций: ${configs}`,
      free: (count: number) => `Свободно для продажи: ${count}`,
      noFree: "Свободных копий для продажи нет",
      config: (label: string) => `Конфигурация ${label}`,
      readOnly: "Чтобы снять мод, откройте указанный предмет и конфигурацию в Арсенале Warframe. PlatScope ничего не меняет в игре.",
    },
    en: {
      loading: "Loading equipped mods…",
      error: "Unable to open loadout data.",
      retry: "Try again",
      notScanned: "Loadouts have not been scanned",
      notScannedBody: "Start Warframe and update the inventory to read mods from configurations A/B/C.",
      update: "Update from Warframe",
      updating: "Updating…",
      scanError: "Unable to read loadouts. Start Warframe, sign in, and try again.",
      summaryVariants: "Mods",
      summaryCopies: "Equipped copies",
      summaryEquipment: "Items",
      summaryConfigurations: "Configurations",
      search: "Find a mod",
      searchPlaceholder: "For example, Primed Flow",
      type: "Installed on",
      allTypes: "Any item",
      listTitle: "Equipped mods",
      shown: (count: number) => `Shown: ${count}`,
      empty: "No equipped mods found",
      emptyBody: "The scanned configurations contain no mods from the market inventory.",
      noFiltered: "No matches",
      noFilteredBody: "Change the mod name or item type.",
      clear: "Reset",
      rank: (rank: number) => `rank ${rank}`,
      copies: (count: number) => `copies: ${count}`,
      configs: (count: number) => `configurations: ${count}`,
      selectPrompt: "Select a mod on the left",
      selectPromptBody: "Every item and configuration using it will appear here.",
      installedOn: "Installed on",
      usage: (equipment: number, configs: number) => `Items: ${equipment} · Configurations: ${configs}`,
      free: (count: number) => `Free to sell: ${count}`,
      noFree: "No free copies to sell",
      config: (label: string) => `Configuration ${label}`,
      readOnly: "To remove the mod, open the listed item and configuration in the Warframe Arsenal. PlatScope never changes the game.",
    },
  } as const;
  const kindCopy = {
    ru: { warframe: "Варфреймы", primary: "Основное", secondary: "Дополнительное", melee: "Ближний бой", companion: "Компаньоны", companion_weapon: "Оружие компаньона", archwing: "Арчвинг", archgun: "Арчган", archmelee: "Арчмили", necramech: "Некрамехи", amp: "Усилители", other: "Прочее" },
    en: { warframe: "Warframes", primary: "Primary", secondary: "Secondary", melee: "Melee", companion: "Companions", companion_weapon: "Companion weapons", archwing: "Archwings", archgun: "Archguns", archmelee: "Archmelee", necramech: "Necramechs", amp: "Amps", other: "Other" },
  } as const;
  $: c = copy[$locale];
  $: kindLabels = kindCopy[$locale];

  let inventory: InventoryView | null = null;
  let loading = true;
  let scanning = false;
  let errorMessage = "";
  let query = "";
  let kind: EquipmentKind | "all" = "all";
  let selectedIdentity = "";
  let detailPanel: HTMLElement | null = null;
  $: allEntries = buildEquippedModEntries(inventory?.items ?? []);
  $: entries = filterEquippedModEntries(allEntries, query, kind);
  $: summary = summarizeEquippedMods(allEntries);
  $: availableKinds = [...new Set(allEntries.flatMap((entry) => entry.kinds))]
    .sort((left, right) => kindLabels[left].localeCompare(kindLabels[right], $locale));
  $: if (entries.length === 0) selectedIdentity = "";
  $: if (entries.length > 0 && !entries.some((entry) => entry.identity === selectedIdentity)) {
    selectedIdentity = entries[0].identity;
  }
  $: selectedEntry = entries.find((entry) => entry.identity === selectedIdentity) ?? null;

  async function loadInventory(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      inventory = await invoke<InventoryView | null>("load_inventory");
    } catch {
      inventory = null;
      errorMessage = c.error;
    } finally {
      loading = false;
    }
  }

  async function scanWarframe(): Promise<void> {
    scanning = true;
    errorMessage = "";
    try {
      await invoke("scan_read_only_inventory");
      await loadInventory();
      onInventoryChange?.();
    } catch {
      errorMessage = c.scanError;
    } finally {
      scanning = false;
    }
  }

  async function selectEntry(entry: EquippedModEntry): Promise<void> {
    selectedIdentity = entry.identity;
    await tick();
    if (window.matchMedia("(max-width: 50rem)").matches) {
      detailPanel?.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  }

  onMount(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    void loadInventory();
    for (const event of ["inventory-updated", "game-metadata-updated"]) {
      void listen(event, () => void loadInventory()).then((cleanup) => {
        if (disposed) cleanup();
        else unlisteners.push(cleanup);
      });
    }
    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  });
</script>

<div class="equipped-status" role="status" aria-live="polite">{loading ? c.loading : scanning ? c.updating : ""}</div>

{#if errorMessage}
  <div class="error-block" role="alert"><p>{errorMessage}</p><button type="button" onclick={loadInventory}>{c.retry}</button></div>
{/if}

{#if !loading && (!inventory || !inventory.modUsageScanned)}
  <section class="empty-panel equipped-empty" aria-labelledby="equipped-not-scanned">
    <h2 id="equipped-not-scanned">{c.notScanned}</h2>
    <p>{c.notScannedBody}</p>
    <button type="button" onclick={scanWarframe} disabled={scanning}>{scanning ? c.updating : c.update}</button>
  </section>
{:else if inventory}
  <section class="equipped-overview" aria-label={c.summaryVariants}>
    <dl>
      <div><dt>{c.summaryVariants}</dt><dd>{summary.modVariants}</dd></div>
      <div><dt>{c.summaryCopies}</dt><dd>{summary.modCopies}</dd></div>
      <div><dt>{c.summaryEquipment}</dt><dd>{summary.equipmentCount}</dd></div>
      <div><dt>{c.summaryConfigurations}</dt><dd>{summary.configCount}</dd></div>
    </dl>
    <button type="button" onclick={scanWarframe} disabled={scanning}>{scanning ? c.updating : c.update}</button>
  </section>

  <section class="equipped-toolbar" aria-label={c.search}>
    <div class="search-field">
      <label for="equipped-search">{c.search}</label>
      <input id="equipped-search" type="search" bind:value={query} maxlength="80" autocomplete="off" placeholder={c.searchPlaceholder} />
    </div>
    <div class="filter-field">
      <label for="equipped-kind">{c.type}</label>
      <select id="equipped-kind" bind:value={kind}>
        <option value="all">{c.allTypes}</option>
        {#each availableKinds as availableKind}<option value={availableKind}>{kindLabels[availableKind]}</option>{/each}
      </select>
    </div>
  </section>

  {#if allEntries.length === 0}
    <section class="empty-panel equipped-empty"><h2>{c.empty}</h2><p>{c.emptyBody}</p></section>
  {:else if entries.length === 0}
    <section class="empty-panel equipped-empty"><h2>{c.noFiltered}</h2><p>{c.noFilteredBody}</p><button type="button" onclick={() => { query = ""; kind = "all"; }}>{c.clear}</button></section>
  {:else}
    <div class="equipped-workspace">
      <section class="equipped-master" aria-labelledby="equipped-list-title">
        <header><h2 id="equipped-list-title">{c.listTitle}</h2><span>{c.shown(entries.length)}</span></header>
        <ul>
          {#each entries as entry (entry.identity)}
            <li>
              <button
                type="button"
                class:active={entry.identity === selectedIdentity}
                aria-pressed={entry.identity === selectedIdentity}
                aria-controls="equipped-detail"
                onclick={() => selectEntry(entry)}
              >
                <span class="equipped-mod-thumb">
                  {#if entry.imageUrl}<img src={entry.imageUrl} alt="" loading="lazy" decoding="async" />{:else}<span aria-hidden="true">◇</span>{/if}
                </span>
                <span class="equipped-mod-copy">
                  <strong>{entry.displayName}</strong>
                  <small>{entry.rank === null ? c.copies(entry.equippedQuantity) : `${c.rank(entry.rank)} · ${c.copies(entry.equippedQuantity)}`}</small>
                </span>
                <span class="equipped-mod-count">{c.configs(entry.configCount)}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>

      <section id="equipped-detail" class="equipped-detail" bind:this={detailPanel} aria-live="polite">
        {#if selectedEntry}
          <header class="equipped-detail__header">
            <span class="equipped-detail__image">
              {#if selectedEntry.imageUrl}<img src={selectedEntry.imageUrl} alt="" decoding="async" />{:else}<span aria-hidden="true">◇</span>{/if}
            </span>
            <div>
              {#if selectedEntry.rank !== null}<span>{c.rank(selectedEntry.rank)}</span>{/if}
              <h2>{selectedEntry.displayName}</h2>
              <p>{c.usage(selectedEntry.equipmentCount, selectedEntry.configCount)}</p>
            </div>
            <strong class:unavailable={selectedEntry.sellableQuantity === 0}>
              {selectedEntry.sellableQuantity > 0 ? c.free(selectedEntry.sellableQuantity) : c.noFree}
            </strong>
          </header>
          <div class="equipped-detail__body">
            <h3>{c.installedOn}</h3>
            <ul class="equipped-locations">
              {#each selectedEntry.locations as location (location.instanceKey)}
                <li>
                  <span class="equipped-location__image">
                    {#if location.imageUrl}<img src={location.imageUrl} alt="" loading="lazy" decoding="async" />{:else}<span aria-hidden="true">◇</span>{/if}
                  </span>
                  <div>
                    <small>{kindLabels[location.kind]}</small>
                    <strong>{location.displayName}</strong>
                  </div>
                  <div class="equipped-config-tags" aria-label={c.configs(location.configIndexes.length)}>
                    {#each location.configIndexes as configIndex}
                      <span>{c.config(configLabel(configIndex))}</span>
                    {/each}
                  </div>
                </li>
              {/each}
            </ul>
            <p class="equipped-readonly">{c.readOnly}</p>
          </div>
        {:else}
          <div class="equipped-detail__empty"><h2>{c.selectPrompt}</h2><p>{c.selectPromptBody}</p></div>
        {/if}
      </section>
    </div>
  {/if}
{/if}
