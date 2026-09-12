<script context="module" lang="ts">
  export type AppScreen = "world_activity" | "market" | "inventory" | "equipped_mods" | "squad" | "mission" | "insights" | "bounty_hunter" | "settings";
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import AppNavIcon from "./AppNavIcon.svelte";
  import brandIcon from "../../src-tauri/icons/128x128.png";
  import { localeCode, type UiLocale } from "./i18n";
  import type { MarketSnapshotSummary } from "./foundation";

  export let activeScreen: AppScreen;
  export let labels: Record<AppScreen, string>;
  export let locale: UiLocale;
  export let snapshot: MarketSnapshotSummary | null;
  export let pending: boolean;
  export let unavailable: boolean;
  export let onNavigate: (screen: AppScreen) => void;
  export let compact = false;

  const preferenceKey = "platscope.sidebar.compact.v1";
  const groups: { ru: string; en: string; screens: AppScreen[] }[] = [
    { ru: "Игра", en: "Play", screens: ["world_activity", "bounty_hunter", "mission", "squad"] },
    { ru: "Коллекция", en: "Collection", screens: ["inventory", "equipped_mods"] },
    { ru: "Торговля", en: "Trading", screens: ["market", "insights"] },
  ];
  $: copy = locale === "ru" ? {
    brandTagline: "Помощник в Warframe",
    nav: "Разделы приложения", collapse: "Свернуть меню", expand: "Развернуть меню",
    market: "Данные рынка", loading: "Загружаем данные", missing: "Нет сохранённых цен",
    unavailable: "Данные недоступны", settingsHint: "Проверьте настройки", loadHint: "Загрузите в настройках",
    datePrefix: "Цены за", saved: "Цены сохранены", waiting: "Проверяем сохранённые цены",
  } : {
    brandTagline: "Warframe companion",
    nav: "Application sections", collapse: "Collapse menu", expand: "Expand menu",
    market: "Market data", loading: "Loading data", missing: "No saved prices",
    unavailable: "Data unavailable", settingsHint: "Check settings", loadHint: "Load in settings",
    datePrefix: "Prices for", saved: "Prices saved", waiting: "Checking saved prices",
  };
  $: sourceDate = snapshot ? new Date(`${snapshot.sourceDate}T00:00:00Z`) : null;
  $: dateLabel = sourceDate && Number.isFinite(sourceDate.getTime())
    ? sourceDate.toLocaleDateString(localeCode(locale), { day: "numeric", month: "long", timeZone: "UTC" }) : "";
  $: statusTitle = pending && !snapshot ? copy.loading : unavailable ? copy.unavailable : snapshot ? copy.market : copy.missing;
  $: statusDetail = pending && !snapshot ? copy.waiting : unavailable ? copy.settingsHint : snapshot
    ? dateLabel ? `${copy.datePrefix} ${dateLabel}` : copy.saved : copy.loadHint;
  onMount(() => {
    try { compact = localStorage.getItem(preferenceKey) === "true"; } catch { /* Меню работает без сохранения настройки. */ }
  });
  function toggleCompact() {
    compact = !compact;
    try { localStorage.setItem(preferenceKey, String(compact)); } catch { /* Выбранная ширина действует до закрытия приложения. */ }
  }
</script>

<aside class="sidebar" class:compact aria-label="PlatScope">
  <div class="brand" title={compact ? "PlatScope" : undefined}>
    <img class="brand-mark" src={brandIcon} alt="" width="48" height="48" draggable="false" />
    <span class="brand-copy"><strong translate="no">PlatScope</strong><small>{copy.brandTagline}</small></span>
  </div>

  <nav id="app-navigation" class="navigation" aria-label={copy.nav}>
    {#each groups as group}
      <div class="nav-group" role="group" aria-label={locale === "ru" ? group.ru : group.en}>
        <p class="group-label" aria-hidden="true">{locale === "ru" ? group.ru : group.en}</p>
        {#each group.screens as screen}
          <button class="nav-button" type="button" class:active={activeScreen === screen}
            aria-label={labels[screen]} aria-current={activeScreen === screen ? "page" : undefined}
            title={compact ? labels[screen] : undefined} onclick={() => onNavigate(screen)}>
            <span class="icon-box"><AppNavIcon {screen} /></span><span class="nav-label">{labels[screen]}</span>
          </button>
        {/each}
      </div>
    {/each}
  </nav>

  <div class="sidebar-footer">
    <button class="nav-button settings-button" type="button" class:active={activeScreen === "settings"}
      aria-current={activeScreen === "settings" ? "page" : undefined} aria-label={labels.settings}
      title={compact ? labels.settings : undefined} onclick={() => onNavigate("settings")}>
      <span class="icon-box"><AppNavIcon screen="settings" /></span><span class="nav-label">{labels.settings}</span>
    </button>
    <div class="market-status" class:ready={!!snapshot && !unavailable} class:failed={unavailable}
      class:loading={pending && !snapshot} title={`${statusTitle}. ${statusDetail}`}>
      <span class="status-indicator" aria-hidden="true">{#if snapshot && !unavailable}<svg viewBox="0 0 16 16"><path d="m4 8 2.5 2.5L12 5" /></svg>{:else}<span></span>{/if}</span>
      <span class="status-copy"><strong>{statusTitle}</strong><small>{statusDetail}</small></span>
    </div>
    <button class="collapse-button" type="button" aria-label={compact ? copy.expand : copy.collapse}
      aria-expanded={!compact} aria-controls="app-navigation" title={compact ? copy.expand : undefined} onclick={toggleCompact}>
      <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="3" /><path d="M9 4v16" /><path d={compact ? "m13 9 3 3-3 3" : "m16 9-3 3 3 3"} /></svg>
      <span class="nav-label">{copy.collapse}</span>
    </button>
  </div>
</aside>

<style>
  .sidebar { position:sticky; top:0; display:flex; flex-direction:column; gap:1.1rem; height:100dvh; min-height:0; padding:1.25rem .75rem .8rem; border-right:1px solid var(--border); background:linear-gradient(160deg, var(--surface-3), var(--sidebar-bg) 48%, var(--surface-3)); overflow-y:auto; scrollbar-width:thin; }
  .brand { display:flex; align-items:center; gap:.65rem; padding:.15rem .25rem .75rem; flex:none; }
  .brand-mark { display:block; width:3rem; height:3rem; flex:none; object-fit:contain; }
  .brand-copy { min-width:0; }
  .brand-copy strong { display:block; color:var(--text); font-size:1.5rem; font-weight:750; line-height:1.1; letter-spacing:-.045em; }
  .brand-copy small { display:block; margin-top:.3rem; font-size:.75rem; line-height:1.35; color:var(--text-muted); }
  .navigation { display:grid; gap:1.1rem; }
  .nav-group { display:grid; gap:.25rem; }
  .group-label { margin:0 0 .2rem .75rem; color:var(--text-subtle); font-size:.75rem; font-weight:600; }
  .nav-button { position:relative; display:flex; align-items:center; gap:.55rem; width:100%; min-height:2.85rem; padding:.45rem .65rem; border:1px solid transparent; border-radius:.65rem; background:transparent; color:var(--text-muted); font-size:.875rem; font-weight:500; text-align:left; line-height:1.35; box-shadow:none; }
  .nav-label { min-width:0; overflow-wrap:anywhere; }
  .icon-box { display:grid; place-items:center; width:1.55rem; height:1.6rem; flex:none; }
  .icon-box :global(.nav-icon) { width:1.15rem; height:1.15rem; }
  .nav-button:hover { background:oklch(.99 .01 80 / .58); color:var(--text); border-color:transparent; }
  .nav-button.active { color:var(--accent-strong); background:var(--accent-soft); font-weight:650; }
  .nav-button.active::before { content:""; position:absolute; left:0; top:.65rem; bottom:.65rem; width:3px; border-radius:0 3px 3px 0; background:var(--accent); }
  .sidebar button:focus-visible { outline:2px solid var(--accent); outline-offset:-2px; }
  .sidebar button:active { scale:1; }
  .sidebar-footer { display:grid; gap:.45rem; margin-top:auto; padding-top:.75rem; border-top:1px solid var(--border); }
  .market-status { display:flex; align-items:center; gap:.6rem; margin:.45rem .3rem; padding:.65rem .4rem; line-height:1.35; }
  .status-indicator { display:grid; place-items:center; width:1.35rem; height:1.35rem; border-radius:50%; flex:none; background:var(--surface-1); color:var(--text-subtle); }
  .status-indicator > span { width:.35rem; height:.35rem; border-radius:50%; background:currentColor; }
  .status-indicator svg { width:1rem; height:1rem; fill:none; stroke:currentColor; stroke-width:1.7; stroke-linecap:round; stroke-linejoin:round; }
  .ready .status-indicator { background:var(--success-soft); color:var(--success); }
  .failed .status-indicator { background:var(--danger-soft); color:var(--danger); }
  .status-copy { min-width:0; }
  .status-copy strong { display:block; font-size:.8125rem; font-weight:600; color:var(--text-muted); }
  .status-copy small { display:block; margin-top:.2rem; font-size:.75rem; color:var(--text-subtle); }
  .collapse-button { display:flex; align-items:center; gap:.7rem; padding:.55rem .85rem; border:0; border-radius:.5rem; color:var(--text-subtle); background:transparent; text-align:left; font-size:.75rem; font-weight:500; }
  .collapse-button:hover { background:var(--surface-2); color:var(--text); }
  .collapse-button svg { width:1rem; height:1rem; flex:none; stroke:currentColor; stroke-width:1.5; fill:none; stroke-linejoin:round; stroke-linecap:round; }
  @media (min-width:68.001rem) {
    .compact { padding-inline:.6rem; }
    .compact .brand { padding-inline:0; justify-content:center; }
    .compact .brand-copy,.compact .nav-label { display:none; }
    .compact .status-copy { position:absolute; width:1px; height:1px; overflow:hidden; clip-path:inset(50%); }
    .compact .group-label { height:1px; font-size:0; background:var(--border); margin:.2rem .7rem .6rem; }
    .compact .nav-button { justify-content:center; padding-inline:0; min-height:2.85rem; }
    .compact .market-status,.compact .collapse-button { justify-content:center; padding-inline:0; margin-inline:0; }
    .compact .navigation { gap:.65rem; }
  }
  @media (max-width:68rem) {
    .sidebar { position:static; height:auto; display:grid; grid-template-columns:1fr auto; gap:.7rem 1rem; padding:.8rem; border-right:0; border-bottom:1px solid var(--border); overflow:visible; }
    .brand { padding:0 .3rem; }
    .brand-mark { width:2.5rem; height:2.5rem; }
    .brand-copy strong { font-size:1.35rem; }
    .brand-copy small { margin-top:.2rem; }
    .navigation { grid-column:1 / -1; grid-row:2; grid-template-columns:repeat(3,minmax(0,1fr)); gap:.35rem; }
    .nav-group { display:contents; }
    .group-label,.collapse-button { display:none; }
    .nav-button { min-height:2.6rem; }
    .sidebar-footer { display:flex; align-items:center; gap:1rem; grid-column:2; grid-row:1; padding:0; margin:0; border:0; }
    .settings-button { order:2; width:auto; }
    .market-status { margin:0; padding:0; }
    .status-copy small { margin-top:.1rem; }
  }
  @media (max-width:38rem) {
    .sidebar { gap:.75rem .4rem; }
    .market-status { display:none; }
    .navigation { grid-template-columns:repeat(2,minmax(0,1fr)); }
    .nav-button { font-size:.8125rem; }
    .settings-button .nav-label { display:none; }
  }
  @media (max-height:46rem) and (min-width:68.001rem) {
    .sidebar { gap:.65rem; padding-top:.75rem; }
    .navigation { gap:.65rem; }
    .nav-button { min-height:2.5rem; }
    .market-status { margin-block:0; }
  }
</style>
