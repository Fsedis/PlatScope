<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick as nextTick } from "svelte";
  import { checkedLivePrice, priceCheckSummary } from "./livePriceCheck";
  import {
    BOUNTY_AUTO_RETRY_DELAY_MS, activeBountyView, bountyAutomaticRefreshAt, bountyRotationAt,
    bountyEstimate, bountyFeaturedReward, bountyJobIdentity, rankedBountyJobs, withBountyLivePrices,
    type BountyHunterView, type BountyRewardView, type BountySortKey, type RankedBountyJob,
  } from "./bountyHunter";
  import { publishBountyView, publishBountyWatchlistChange, requestBountyNotificationPermission } from "./bountyAlerts";
  import {
    BOUNTY_VIEW_REFRESHED_EVENT, DEFAULT_BOUNTY_WATCH_PREFERENCES, loadBountyWatchPreferences,
    saveBountyWatchPreferences, watchedReward, type BountyWatchPreferences,
  } from "./bountyWatchlist";
  import type { LivePricingResult } from "./market";
  import type { WorldArtworkKind } from "./worldActivityArtwork";
  import WorldActivityArtwork from "./WorldActivityArtwork.svelte";
  import BountyJobDetail from "./BountyJobDetail.svelte";

  export let onOpenSettings: () => void;
  export let initialRegion = "all";
  const art: Record<string, WorldArtworkKind> = { cetus: "cetus", fortuna: "vallis", necralisk: "cambion" };
  let view: BountyHunterView | null = null;
  let loading = true;
  let error = "";
  let region = initialRegion;
  let onlyPriced = false;
  let query = "";
  let targetKey = "";
  let sort: BountySortKey = "platinum";
  let nowMs = Date.now();
  let retryAt: number | null = null;
  let livePrices = new Map<string, number>();
  let liveBusyJobId = "";
  let liveMessages = new Map<string, string>();
  let selectedId = "";
  let showAll = false;
  let watchPreferences: BountyWatchPreferences = DEFAULT_BOUNTY_WATCH_PREFERENCES;
  let watchMessage = "";
  let marketMessage = "";
  let notificationBusy = false;
  let disposed = false;
  let revision = 0;
  let detailPanel: HTMLElement;
  let listHeading: HTMLHeadingElement;
  let layout: HTMLDivElement;
  let searchInput: HTMLInputElement;

  $: pricedView = withBountyLivePrices(view, livePrices);
  $: activeView = activeBountyView(pricedView, nowMs);
  $: allJobs = rankedBountyJobs(activeView, { region: "all", onlyPriced: false, query: "", sort: "platinum" });
  $: jobs = rankedBountyJobs(activeView, { region, onlyPriced, query, sort, targetKey });
  $: selected = jobs.find(row => bountyJobIdentity(row) === selectedId) ?? jobs[0] ?? null;
  $: displayedJobs = showAll || !!query ? jobs : jobs.slice(0, 8);
  $: watchedKeys = new Set(watchPreferences.rewards.map(reward => reward.key));
  $: rotationAt = bountyRotationAt(activeView);
  $: automaticRefreshAt = retryAt ?? bountyAutomaticRefreshAt(view);
  $: expiredCount = (view?.regions.length ?? 0) - (activeView?.regions.length ?? 0);
  const num = (value: number) => value.toLocaleString("ru-RU", { maximumFractionDigits: 1 });
  const time = (value: string) => new Date(value).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });

  function countdown(target: number | null, currentTime: number): string {
    if (target == null || !Number.isFinite(target)) return "—";
    const seconds = Math.max(0, Math.ceil((target - currentTime) / 1000));
    const minutes = Math.floor(seconds / 60);
    return (minutes >= 60 ? Math.floor(minutes / 60) + ":" + String(minutes % 60).padStart(2, "0") : String(minutes))
      + ":" + String(seconds % 60).padStart(2, "0");
  }

  function withDeadline<T>(request: Promise<T>): Promise<T> {
    return new Promise((resolve, reject) => {
      const timeout = window.setTimeout(() => reject(new Error("Истекло время ожидания")), 20_000);
      request.then(value => { window.clearTimeout(timeout); resolve(value); }, reason => { window.clearTimeout(timeout); reject(reason); });
    });
  }

  function acceptView(nextView: BountyHunterView | null): void {
    revision += 1;
    view = nextView;
    error = "";
    livePrices = new Map();
    liveMessages = new Map();
    liveBusyJobId = "";
    marketMessage = "";
    nowMs = Date.now();
    const due = bountyAutomaticRefreshAt(nextView);
    retryAt = nextView && (due === null || due <= nowMs) ? nowMs + BOUNTY_AUTO_RETRY_DELAY_MS : null;
  }

  async function load(forceRefresh = false): Promise<void> {
    loading = true;
    const requestedRevision = revision;
    if (!view) error = "";
    try {
      const nextView = await withDeadline(invoke<BountyHunterView | null>("bounty_hunter", { forceRefresh }));
      if (disposed || revision !== requestedRevision) return;
      acceptView(nextView);
      if (nextView) publishBountyView(nextView);
    } catch {
      if (disposed || revision !== requestedRevision) return;
      error = view ? "Не удалось обновить заказы. Показываем только те, у которых ещё не истёк срок."
        : "Не удалось загрузить заказы. Проверьте соединение с интернетом.";
      retryAt = Date.now() + BOUNTY_AUTO_RETRY_DELAY_MS;
    } finally {
      if (!disposed) { loading = false; nowMs = Date.now(); }
    }
  }

  function resetSelection(): void { selectedId = ""; showAll = false; marketMessage = ""; }
  function resetFilters(): void { query = ""; targetKey = ""; region = "all"; onlyPriced = false; sort = "platinum"; resetSelection(); }
  function search(value: string): void {
    if (!query.trim() && value.trim()) sort = "reward_chance";
    if (!value.trim()) sort = "platinum";
    query = value; targetKey = ""; resetSelection();
  }
  async function findReward(reward: { key: string; displayName: string }): Promise<void> {
    query = reward.displayName; targetKey = reward.key; onlyPriced = false; region = "all"; sort = "reward_chance"; resetSelection();
    await nextTick(); searchInput?.scrollIntoView({ block: "center" }); searchInput?.focus({ preventScroll: true });
  }
  async function selectJob(row: RankedBountyJob): Promise<void> {
    selectedId = bountyJobIdentity(row); marketMessage = "";
    await nextTick();
    if (layout?.getBoundingClientRect().width <= 1088) {
      detailPanel?.scrollIntoView({ block: "start" }); detailPanel?.focus({ preventScroll: true });
    }
  }
  function backToList(): void { listHeading?.scrollIntoView({ block: "start" }); listHeading?.focus({ preventScroll: true }); }

  async function openMarket(reward: BountyRewardView): Promise<void> {
    if (!reward.slug) return;
    const currentRevision = revision;
    try {
      await invoke<number>("open_market_items", { slugs: [reward.slug] });
      if (!disposed && revision === currentRevision) marketMessage = "Открыта страница «" + reward.displayName + "» на Warframe Market.";
    } catch { if (!disposed) marketMessage = "Не удалось открыть Warframe Market. Попробуйте ещё раз."; }
  }

  function saveWatch(next: BountyWatchPreferences, suppressed: string[] = []): boolean {
    if (!saveBountyWatchPreferences(next)) {
      watchMessage = "Не удалось сохранить изменения. Попробуйте ещё раз.";
      return false;
    }
    watchPreferences = next;
    publishBountyWatchlistChange(view, suppressed);
    return true;
  }
  async function toggleWatchedReward(reward: BountyRewardView): Promise<void> {
    const adding = !watchedKeys.has(reward.trackingKey);
    const rewards = adding ? [...watchPreferences.rewards, watchedReward(reward)] : watchPreferences.rewards.filter(item => item.key !== reward.trackingKey);
    if (!saveWatch({ ...watchPreferences, rewards }, adding ? [reward.trackingKey] : [])) return;
    watchMessage = adding ? "«" + reward.displayName + "» добавлена в отслеживаемые награды." : "«" + reward.displayName + "» больше не отслеживается.";
    if (adding && watchPreferences.enabled && !(await requestBountyNotificationPermission()) && !disposed) {
      saveWatch({ ...watchPreferences, enabled: false });
      watchMessage = "Награда сохранена. Для уведомлений нужно разрешение в настройках Windows.";
    }
  }
  function removeWatchedReward(key: string): void {
    if (saveWatch({ ...watchPreferences, rewards: watchPreferences.rewards.filter(item => item.key !== key) })) watchMessage = "Награда удалена из отслеживаемых.";
  }
  async function setWatchNotifications(enabled: boolean): Promise<void> {
    if (notificationBusy) return;
    notificationBusy = true;
    const granted = !enabled || await requestBountyNotificationPermission();
    notificationBusy = false;
    if (disposed) return;
    if (saveWatch({ ...watchPreferences, enabled: enabled && granted }, enabled ? watchPreferences.rewards.map(reward => reward.key) : [])) {
      watchMessage = !granted ? "Разрешите уведомления для PlatScope в настройках Windows." : enabled ? "Уведомления включены." : "Уведомления выключены.";
    }
  }

  async function checkJobPrices(row: RankedBountyJob): Promise<void> {
    if (liveBusyJobId) return;
    const identity = bountyJobIdentity(row);
    selectedId = identity;
    const currentRevision = revision;
    const rewards = [...new Map(row.job.rewards.filter(reward => reward.slug && reward.marketKey).map(reward => [reward.slug!, reward])).values()];
    if (!rewards.length) return;
    liveBusyJobId = identity;
    liveMessages = new Map(liveMessages).set(identity, "Проверяем предложения игроков, которые сейчас в игре…");
    const results = await Promise.allSettled(rewards.map(async reward => {
      const result = await withDeadline(invoke<LivePricingResult | null>("live_price_current_variant", { key: reward.marketKey, itemKind: "standard" }));
      return { slug: reward.slug!, ...checkedLivePrice(result) };
    }));
    if (disposed || revision !== currentRevision) return;
    const nextPrices = new Map(livePrices);
    let updated = 0, empty = 0, failed = 0;
    for (const result of results) {
      if (result.status === "rejected" || result.value.state === "failed") failed += 1;
      else if (result.value.state === "priced") { nextPrices.set(result.value.slug, result.value.price); updated += 1; }
      else { empty += 1; nextPrices.delete(result.value.slug); }
    }
    livePrices = nextPrices;
    liveMessages = new Map(liveMessages).set(identity, priceCheckSummary(updated, empty, failed));
    liveBusyJobId = "";
  }

  function tick(): void {
    nowMs = Date.now();
    if (!loading && automaticRefreshAt !== null && nowMs >= automaticRefreshAt) {
      retryAt = nowMs + BOUNTY_AUTO_RETRY_DELAY_MS; void load(true);
    }
  }
  onMount(() => {
    watchPreferences = loadBountyWatchPreferences();
    const timer = window.setInterval(tick, 1000);
    const onVisibility = () => { if (!document.hidden) tick(); };
    const onRefreshed = (event: Event) => {
      const next = (event as CustomEvent<BountyHunterView>).detail;
      if (next) acceptView(next);
    };
    document.addEventListener("visibilitychange", onVisibility);
    window.addEventListener(BOUNTY_VIEW_REFRESHED_EVENT, onRefreshed);
    void load();
    return () => {
      disposed = true;
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisibility);
      window.removeEventListener(BOUNTY_VIEW_REFRESHED_EVENT, onRefreshed);
    };
  });
</script>

<div class="bounty-screen">
  <header class="intro">
    <div><h2>Выберите, за чем отправиться</h2><p>Найдите нужную награду или сравните заказы по стоимости добычи.</p></div>
    {#if view}<div class="rotation"><span class="auto-label"><i></i>Обновляется автоматически</span><strong>{loading ? "Получаем заказы…" : rotationAt ? "Ближайшая смена через " + countdown(rotationAt, nowMs) : "Ожидаем новые заказы"}</strong>{#if view}<small>Проверено в {time(view.fetchedAt)}</small>{/if}</div>{/if}
  </header>

  {#if error}
    <section class="notice error" role="alert"><div><strong>{error}</strong><p>Повторим автоматически{retryAt ? " через " + countdown(retryAt, nowMs) : ""}.</p></div><button class="secondary" disabled={loading} onclick={() => load(true)}>{loading ? "Проверяем…" : "Повторить загрузку"}</button></section>
  {/if}
  {#if loading && !view}
    <section class="loading" role="status" aria-label="Загрузка заказов"><strong>Получаем активные заказы и награды…</strong><p>Сверяем текущую ротацию и сохранённые цены.</p><div></div><div></div><div></div></section>
  {:else if !view && !error}
    <section class="empty"><h3>Нужны данные о предметах</h3><p>Загрузите справочник предметов в настройках, чтобы сопоставить награды и цены.</p><button onclick={onOpenSettings}>Открыть настройки данных</button></section>
  {/if}

  {#if view}
    {#if expiredCount > 0 && !error}
      <section class="notice"><div><strong>Ротация сменилась — ждём новые заказы.</strong><p>Заказы с истёкшим сроком скрыты. {loading ? "Обновляем список…" : "Следующая проверка через " + countdown(retryAt, nowMs) + "."}</p></div></section>
    {/if}
    <div class="region-choices" role="group" aria-label="Регион заказов">
      <button class:chosen={region === "all"} aria-pressed={region === "all"} onclick={() => { region = "all"; resetSelection(); }}><span><strong>Все регионы</strong><small>Сравнить все заказы</small></span><b>{allJobs.length}</b></button>
      {#each view.regions as place (place.key)}
        <button class:chosen={region === place.key} aria-pressed={region === place.key} onclick={() => { region = place.key; resetSelection(); }}>
          {#if art[place.key]}<span class="region-art"><WorldActivityArtwork kind={art[place.key]} /></span>{/if}
          <span><strong>{place.displayName}</strong><small>{new Date(place.expiry).getTime() > nowMs ? "Смена через " + countdown(new Date(place.expiry).getTime(), nowMs) : "Ждём новые заказы"}</small></span><b>{allJobs.filter(row => row.regionKey === place.key).length}</b>
        </button>
      {/each}
    </div>
    <section class="filters" aria-label="Поиск заказов">
      <label class="search-label">Что хотите получить?<span class="search-field"><input aria-label="Что хотите получить?" bind:this={searchInput} value={query} oninput={event => search(event.currentTarget.value)} placeholder="Например, Айя. Можно искать и по названию заказа." />{#if query}<button class="secondary" aria-label="Очистить поиск" onclick={() => search("")}>Сбросить</button>{/if}</span></label>
      <label>Показывать сначала<select bind:value={sort} onchange={resetSelection}><option value="platinum">Дороже награды</option><option value="reward_chance">Выше шанс награды</option><option value="level">Ниже уровень врагов</option><option value="rotation">Скорее сменятся</option></select></label>
      <label class="priced-filter"><input type="checkbox" bind:checked={onlyPriced} onchange={resetSelection} /> Только с оценкой в платине</label>
    </section>
    <details class="watchlist">
      <summary><span>Отслеживаемые награды <b>{watchPreferences.rewards.length}</b></span><small>{watchPreferences.rewards.length ? "Открыть список" : "Сохраняйте нужное и узнавайте о появлении"}</small></summary>
      <div class="watchlist-body">
        <div class="watch-intro"><p>Отслеживайте награды в подробностях заказа. Уведомления о появлении приходят, пока PlatScope работает.</p><label><input type="checkbox" checked={watchPreferences.enabled} disabled={notificationBusy} onchange={event => setWatchNotifications(event.currentTarget.checked)} /> Уведомлять о появлении</label></div>
        {#each watchPreferences.rewards as reward (reward.key)}
          {@const available = allJobs.filter(row => row.job.rewards.some(item => item.trackingKey === reward.key)).length}
          <div class="watch-row">{#if reward.imageUrl}<img src={reward.imageUrl} alt="" loading="lazy" onerror={event => { (event.currentTarget as HTMLImageElement).style.display = "none"; }} onload={event => { (event.currentTarget as HTMLImageElement).style.display = ""; }} />{/if}<div><strong>{reward.displayName}</strong><span>{available ? "Сейчас в заказах: " + available : "Нет в активных заказах"}</span></div><button class="secondary" onclick={() => findReward(reward)} aria-label={"Найти заказы: " + reward.displayName}>Найти заказы</button><button class="text-button" aria-label={"Удалить из отслеживаемых: " + reward.displayName} onclick={() => removeWatchedReward(reward.key)}>Удалить</button></div>
        {:else}<p class="watch-empty">Пока ничего не отслеживается. Откройте заказ и нажмите «Отслеживать» рядом с нужной наградой.</p>{/each}
      </div>
    </details>
    {#if watchMessage}<p class="feedback" role="status">{watchMessage}</p>{/if}

    <div class="hunter-layout" bind:this={layout}>
      <section class="job-list" aria-label="Подходящие заказы">
        <div class="list-title"><h2 bind:this={listHeading} tabindex="-1">Подходящие заказы <span>{jobs.length}</span></h2>{#if region !== "all" || query || onlyPriced}<button class="text-button" onclick={resetFilters}>Сбросить фильтры</button>{/if}</div>
        <p class="list-help">{query ? "Найденные заказы и награды. Для поиска по награде сравниваем шанс её получения." : sort === "reward_chance" ? "Сначала заказы с наибольшим шансом одной из наград. Уточните награду в поиске." : "Сравните добычу и сложность. Выберите заказ, чтобы увидеть все награды."}</p>
        <div class="list-columns"><span>Заказ и награда</span><span>Стоимость добычи</span></div>
        {#each displayedJobs as row (bountyJobIdentity(row))}
          {@const reward = bountyFeaturedReward(row.job, query, targetKey, sort === "reward_chance")}
          {@const estimate = bountyEstimate(row.job)}
          <button class="job-row" class:selected={selected && bountyJobIdentity(selected) === bountyJobIdentity(row)} aria-pressed={!!selected && bountyJobIdentity(selected) === bountyJobIdentity(row)} aria-controls="bounty-detail" onclick={() => selectJob(row)}>
            <span class="job-main"><strong class="job-title">{row.job.title}</strong><span class="job-meta">{row.regionName} · Ур. {row.job.minLevel}–{row.job.maxLevel} · Этапов: {row.job.stageCount}</span>
              {#if reward}<span class="featured">{#if reward.imageUrl}<img src={reward.imageUrl} alt="" loading="lazy" onerror={event => { (event.currentTarget as HTMLImageElement).style.display = "none"; }} onload={event => { (event.currentTarget as HTMLImageElement).style.display = ""; }} />{/if}<span>{reward.displayName}<small>Шанс {num(reward.chancePercent)}%</small></span></span>{:else}<span class="no-rewards">Нет списка наград</span>{/if}
            </span>
            <span class="job-value"><strong>{estimate.value === null ? (!row.job.rewards.length ? "Нет данных" : estimate.total ? "Нет оценки" : "Не для продажи") : "≈ " + num(estimate.value) + " пл."}</strong><small>{estimate.value !== null ? (row.job.title.toLocaleLowerCase("ru").includes("бесконечн") ? "за цикл этапов" : "за весь заказ") : (estimate.total ? "Проверьте цены" : "Полезно в игре")}{#if estimate.total && estimate.priced < estimate.total}<span class="partial">Цены: {estimate.priced} из {estimate.total}</span>{/if}</small><span class="row-action">{selected && bountyJobIdentity(selected) === bountyJobIdentity(row) ? "Выбран" : "Подробнее"} →</span></span>
          </button>
        {:else}
          <section class="empty"><h3>{allJobs.length ? (onlyPriced ? "Нет подходящих заказов с оценкой" : "Подходящих заказов нет") : "Ждём актуальные заказы"}</h3><p>{allJobs.length ? (onlyPriced ? "Снимите фильтр «Только с оценкой в платине», чтобы увидеть и награды без известной цены." : "Измените награду или регион. Возможно, нужной награды нет в этой ротации.") : "Список появится автоматически, когда источник обновится."}</p>{#if allJobs.length}<button class="secondary" onclick={resetFilters}>Показать все заказы</button>{/if}</section>
        {/each}
        {#if jobs.length > displayedJobs.length}<button class="secondary show-more" onclick={() => showAll = true}>Показать остальные заказы · {jobs.length - displayedJobs.length}</button>{/if}
        {#if jobs.length}<p class="list-footnote">≈ пл. — средняя стоимость выпавших предметов при продаже. Это не гарантированная выплата за заказ.</p>{/if}
      </section>

      {#if selected}
        <section class="job-detail" id="bounty-detail" bind:this={detailPanel} tabindex="-1" aria-label={"Подробности заказа: " + selected.job.title}>
          <BountyJobDetail row={selected} {query} {targetKey} {watchedKeys} remaining={countdown(new Date(selected.expiry).getTime(), nowMs)}
            priceDate={view.marketSourceDate ?? null} busy={liveBusyJobId === bountyJobIdentity(selected)} anyBusy={!!liveBusyJobId}
            message={liveMessages.get(bountyJobIdentity(selected)) ?? ""} onCheck={() => selected && checkJobPrices(selected)}
            onMarket={openMarket} onWatch={toggleWatchedReward} onFind={reward => findReward({ key: reward.trackingKey, displayName: reward.displayName })} onBack={backToList} />
          {#if marketMessage}<p class="feedback" role="status">{marketMessage}</p>{/if}
        </section>
      {/if}
    </div>
  {/if}
</div>

<style>
  .bounty-screen { container:bounty / inline-size; min-width:0; }
  h2,h3,p { margin:0; } h2 { font-size:1.125rem; } h3 { font-size:1rem; }
  p { font-size:.875rem; line-height:1.5; color:var(--text-muted); }
  .intro { display:flex; justify-content:space-between; gap:1.5rem; margin-bottom:1.4rem; align-items:flex-start; }
  .intro h2 { font-size:1.2rem; margin-bottom:.4rem; }
  .rotation { display:grid; gap:.25rem; text-align:right; flex:none; font-variant-numeric:tabular-nums; }
  .rotation strong { font-size:.8125rem; font-weight:600; }
  .rotation small { font-size:.75rem; color:var(--text-muted); }
  .auto-label { color:var(--text-muted); font-size:.75rem; }
  .auto-label i { display:inline-block; width:.4rem; height:.4rem; background:var(--success); border-radius:50%; margin-right:.4rem; }
  .region-choices { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:.65rem; }
  .region-choices button { display:flex; align-items:center; gap:.75rem; text-align:left; border:1px solid var(--border); border-radius:.75rem; background:var(--surface-1); color:var(--text); padding:.85rem 1rem; min-width:0; }
  .region-choices button:hover { background:var(--surface-hover); }
  .region-choices button.chosen { border-color:var(--accent); background:var(--accent-soft); box-shadow:inset 0 0 0 1px var(--accent); }
  .region-choices strong { display:block; font-size:.875rem; }
  .region-choices small { display:block; font-size:.75rem; color:var(--text-muted); font-weight:400; margin-top:.25rem; }
  .region-choices b { margin-left:auto; font-size:.875rem; font-variant-numeric:tabular-nums; }
  .region-art { display:block; flex:none; width:2rem; height:2rem; color:var(--accent); }
  .filters { display:grid; grid-template-columns:minmax(0,1fr) 14rem auto; align-items:end; gap:1rem; margin:1.25rem 0 1rem; }
  .filters label { display:grid; gap:.4rem; font-size:.8125rem; font-weight:600; }
  .filters input:not([type=checkbox]), select { width:100%; min-width:0; padding:.65rem .8rem; border:1px solid var(--border-strong); border-radius:.5rem; background:var(--surface-1); color:var(--text); font-size:.875rem; }
  .filters input::placeholder { color:var(--text-subtle); }
  .search-field { position:relative; display:flex; gap:.4rem; }
  .search-field input { flex:1; }
  .search-field button { flex:none; font-size:.75rem; }
  .filters .priced-filter { display:flex; align-items:center; gap:.5rem; font-size:.75rem; font-weight:400; min-height:2.7rem; padding-bottom:.1rem; }
  input[type=checkbox] { accent-color:var(--accent); width:1rem; height:1rem; margin:0; flex:none; }
  .watchlist { border:1px solid var(--border); border-radius:.6rem; background:var(--surface-1); margin-bottom:1.25rem; }
  .watchlist summary { cursor:pointer; padding:.8rem 1rem; font-size:.8125rem; }
  .watchlist summary > span { font-weight:650; }
  .watchlist summary b { margin-left:.4rem; font-size:.75rem; border-radius:1rem; padding:.1rem .4rem; background:var(--surface-3); }
  .watchlist summary small { float:right; font-size:.75rem; color:var(--text-muted); margin-top:.1rem; }
  .watchlist-body { padding:0 1rem 1rem; }
  .watch-intro { display:flex; gap:2rem; align-items:center; justify-content:space-between; border-top:1px solid var(--border); padding-top:.8rem; }
  .watch-intro p { font-size:.8125rem; max-width:44rem; }
  .watch-intro label { display:flex; align-items:center; gap:.5rem; font-size:.8125rem; flex:none; }
  .watch-empty { margin-top:.8rem; }
  .watch-row { display:flex; align-items:center; gap:.75rem; padding:.75rem 0; border-bottom:1px solid var(--border); }
  .watch-row:last-child { border-bottom:0; padding-bottom:0; }
  .watch-row img { width:2rem; height:2.5rem; object-fit:contain; }
  .watch-row div { display:grid; gap:.2rem; flex:1; }
  .watch-row strong { font-size:.875rem; } .watch-row span { font-size:.75rem; color:var(--text-muted); }
  .hunter-layout { display:grid; grid-template-columns:minmax(0,1.25fr) minmax(24rem,1fr); align-items:start; gap:1.25rem; }
  .job-list { min-width:0; }
  .list-title { display:flex; align-items:center; justify-content:space-between; gap:1rem; min-height:2rem; }
  .list-title h2 { scroll-margin-top:1.5rem; }
  .list-title span { color:var(--text-subtle); margin-left:.4rem; font-size:.875rem; font-weight:400; }
  .list-title button { font-size:.75rem; padding:0; border:0; }
  .list-help { font-size:.8125rem; margin:.4rem 0 1rem; min-height:2.5em; }
  .list-columns { display:flex; justify-content:space-between; padding:0 1rem .6rem; color:var(--text-subtle); font-size:.75rem; }
  .job-row { display:flex; width:100%; align-items:stretch; justify-content:space-between; gap:1rem; text-align:left; color:var(--text); background:var(--surface-1); border:1px solid var(--border); border-radius:.65rem; padding:1rem; margin-bottom:.55rem; font-weight:400; }
  .job-row:hover { background:var(--surface-hover); border-color:var(--border-strong); }
  .job-row.selected { border-color:var(--accent); box-shadow:inset .2rem 0 var(--accent); background:var(--accent-soft); }
  .job-main { min-width:0; display:block; }
  .job-title { display:block; font-size:.9375rem; line-height:1.35; overflow-wrap:anywhere; }
  .job-meta { display:block; margin-top:.3rem; font-size:.75rem; color:var(--text-muted); line-height:1.4; }
  .featured { display:flex; align-items:center; gap:.55rem; margin-top:.7rem; font-size:.8125rem; line-height:1.3; }
  .featured img { width:1.75rem; height:2.15rem; object-fit:contain; flex:none; }
  .featured small { display:block; font-size:.75rem; color:var(--text-muted); margin-top:.2rem; }
  .no-rewards { display:block; font-size:.8125rem; margin-top:.7rem; }
  .job-value { display:flex; flex-direction:column; align-items:flex-end; gap:.35rem; text-align:right; min-width:7.75rem; flex:none; }
  .job-value strong { color:var(--accent); font-size:1.125rem; font-variant-numeric:tabular-nums; }
  .job-value small { font-size:.75rem; color:var(--text-muted); line-height:1.4; }
  .partial { display:block; color:var(--accent); }
  .row-action { font-size:.75rem; font-weight:600; color:var(--accent); margin-top:auto; padding-top:.6rem; }
  .show-more { width:100%; margin-top:.35rem; min-height:2.75rem; }
  .list-footnote { font-size:.75rem; margin:1rem .2rem; }
  .job-detail { border:1px solid var(--border); border-radius:.85rem; padding:1.35rem; background:var(--surface-1); box-shadow:var(--shadow-sm); position:sticky; top:1rem; max-height:calc(100dvh - 2rem); overflow:auto; scrollbar-width:thin; scroll-margin-top:1rem; }
  .job-detail:focus-visible,.list-title h2:focus-visible { outline:2px solid var(--accent); outline-offset:3px; }
  .notice { display:flex; align-items:center; justify-content:space-between; gap:1rem; border:1px solid var(--border); border-radius:.6rem; background:var(--surface-2); padding:1rem; margin-bottom:1rem; font-size:.875rem; }
  .notice p { font-size:.8125rem; margin-top:.35rem; } .notice button { flex:none; }
  .notice.error { border-color:var(--danger); }
  .empty,.loading { padding:2rem; border:1px solid var(--border); border-radius:.75rem; background:var(--surface-1); }
  .empty p,.loading p { margin:.65rem 0 1rem; max-width:36rem; }
  .loading div { height:4rem; background:var(--surface-2); border-radius:.5rem; margin-top:.75rem; }
  .feedback { padding:.7rem 1rem; border-radius:.5rem; background:var(--surface-2); border:1px solid var(--border); font-size:.8125rem; margin:0 0 1rem; color:var(--text); }
  @container bounty (max-width:78rem) {
    .filters { grid-template-columns:minmax(0,1fr) 14rem; gap:.75rem; }
    .filters .priced-filter { grid-column:1 / -1; min-height:1.5rem; }
  }
  @container bounty (max-width:68rem) {
    .hunter-layout { grid-template-columns:minmax(0,1fr); }
    .job-detail { position:static; max-height:none; overflow:visible; }
    .region-choices { grid-template-columns:repeat(2,minmax(0,1fr)); }
  }
  @container bounty (max-width:40rem) {
    .intro { flex-direction:column; gap:.8rem; }
    .rotation { text-align:left; }
    .filters { grid-template-columns:minmax(0,1fr); }
    .watchlist summary small { display:none; }
    .watch-intro { flex-direction:column; align-items:flex-start; gap:.75rem; }
    .watch-row { flex-wrap:wrap; }
    .watch-row div { min-width:55%; }
    .region-choices button { padding:.75rem; }
    .region-art { width:1.5rem; height:1.5rem; }
    .job-detail { padding:1rem; }
    .notice { flex-direction:column; align-items:flex-start; }
  }
</style>
