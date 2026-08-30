<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { useLocale } from "./i18n";
  import type { EquipmentKind, InventoryView } from "./inventory";
  import {
    buildEquippedEquipmentGroups,
    configLabel,
    summarizeEquippedMods,
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
      summaryMods: "Надето модов",
      summaryEquipment: "Предметов со сборками",
      summaryConfigs: "Конфигураций",
      search: "Найти мод или предмет",
      searchPlaceholder: "Например, Поток Прайм или Вольт",
      type: "Тип предмета",
      allTypes: "Все типы",
      empty: "Надетых модов не найдено",
      emptyBody: "В считанных конфигурациях нет модов из торгового инвентаря.",
      noFiltered: "Ничего не найдено",
      noFilteredBody: "Измените запрос или тип предмета.",
      clear: "Сбросить",
      config: (label: string) => `Конфигурация ${label}`,
      rank: (rank: number) => `ранг ${rank}`,
      free: (count: number) => `свободно для продажи: ${count}`,
      protected: "эта копия защищена от продажи",
      readOnly: "Снять мод можно в Арсенале Warframe. PlatScope только показывает сборки и не меняет игру.",
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
      summaryMods: "Equipped mods",
      summaryEquipment: "Items with loadouts",
      summaryConfigs: "Configurations",
      search: "Find a mod or item",
      searchPlaceholder: "For example, Primed Flow or Volt",
      type: "Item type",
      allTypes: "All types",
      empty: "No equipped mods found",
      emptyBody: "The scanned configurations contain no mods from the market inventory.",
      noFiltered: "No matches",
      noFilteredBody: "Change the search or item type.",
      clear: "Reset",
      config: (label: string) => `Configuration ${label}`,
      rank: (rank: number) => `rank ${rank}`,
      free: (count: number) => `free to sell: ${count}`,
      protected: "this copy is protected from sale",
      readOnly: "Remove mods in the Warframe Arsenal. PlatScope only displays loadouts and never changes the game.",
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
  $: allGroups = buildEquippedEquipmentGroups(inventory?.items ?? []);
  $: groups = buildEquippedEquipmentGroups(inventory?.items ?? [], query, kind);
  $: summary = summarizeEquippedMods(inventory?.items ?? [], allGroups);
  $: availableKinds = [...new Set(allGroups.map((group) => group.kind))];

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

  onMount(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void loadInventory();
    void listen("inventory-updated", () => void loadInventory()).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });
    return () => {
      disposed = true;
      unlisten?.();
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
  <section class="equipped-overview" aria-label={c.summaryMods}>
    <dl>
      <div><dt>{c.summaryMods}</dt><dd>{summary.modCopies}</dd></div>
      <div><dt>{c.summaryEquipment}</dt><dd>{summary.equipmentCount}</dd></div>
      <div><dt>{c.summaryConfigs}</dt><dd>{summary.configCount}</dd></div>
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

  {#if allGroups.length === 0}
    <section class="empty-panel equipped-empty"><h2>{c.empty}</h2><p>{c.emptyBody}</p></section>
  {:else if groups.length === 0}
    <section class="empty-panel equipped-empty"><h2>{c.noFiltered}</h2><p>{c.noFilteredBody}</p><button type="button" onclick={() => { query = ""; kind = "all"; }}>{c.clear}</button></section>
  {:else}
    <div class="equipped-grid">
      {#each groups as group (group.instanceKey)}
        <article class="equipment-card">
          <header class="equipment-card__header">
            {#if group.imageUrl}<img src={group.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
            <div><span>{kindLabels[group.kind]}</span><h2>{group.displayName}</h2></div>
          </header>
          <div class="equipment-configs">
            {#each group.configs as config (config.index)}
              <section class="equipment-config" aria-label={c.config(configLabel(config.index))}>
                <h3>{c.config(configLabel(config.index))}</h3>
                <ul>
                  {#each config.mods as mod (mod.identity)}
                    <li>
                      {#if mod.imageUrl}<img src={mod.imageUrl} alt="" loading="lazy" decoding="async" />{/if}
                      <div><strong>{mod.displayName}</strong><span>{mod.rank === null ? c.protected : `${c.rank(mod.rank)} · ${c.protected}`}</span><small>{c.free(mod.freeQuantity)}</small></div>
                    </li>
                  {/each}
                </ul>
              </section>
            {/each}
          </div>
        </article>
      {/each}
    </div>
    <p class="equipped-readonly">{c.readOnly}</p>
  {/if}
{/if}
