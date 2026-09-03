<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  import {
    BOUNTY_AUTO_RETRY_DELAY_MS,
    bountyAutomaticRefreshAt,
    bountyRotationAt,
    rankedBountyJobs,
    type BountyHunterView,
    type BountyJobView,
    type BountyRewardView,
    type BountySortKey,
  } from "./bountyHunter";
  import {
    publishBountyView,
    publishBountyWatchlistChange,
    requestBountyNotificationPermission,
  } from "./bountyAlerts";
  import {
    BOUNTY_VIEW_REFRESHED_EVENT,
    DEFAULT_BOUNTY_WATCH_PREFERENCES,
    loadBountyWatchPreferences,
    saveBountyWatchPreferences,
    watchedReward,
    type BountyWatchPreferences,
  } from "./bountyWatchlist";
  import { localeCode, useLocale } from "./i18n";
  import type { LivePricingResult } from "./market";

  export let onOpenSettings: () => void;

  const LOAD_DEADLINE_MS = 20_000;
  const locale = useLocale();
  let view: BountyHunterView | null = null;
  let loading = false;
  let loadingStartedAt: number | null = null;
  let error = "";
  let region = "all";
  let onlyPriced = true;
  let query = "";
  let sort: BountySortKey = "platinum";
  let marketMessage = "";
  let nowMs = Date.now();
  let retryAt: number | null = null;
  let livePrices = new Map<string, number>();
  let liveBusyJobId = "";
  let liveMessages = new Map<string, string>();
  let expandedJobIds = new Set<string>();
  let watchPreferences: BountyWatchPreferences = DEFAULT_BOUNTY_WATCH_PREFERENCES;
  let watchMessage = "";

  $: jobs = rankedBountyJobs(view, { region, onlyPriced, query, sort });
  $: watchedRewardKeys = new Set(watchPreferences.rewards.map((reward) => reward.key));
  $: bestRow = rankedBountyJobs(view, {
    region: "all",
    onlyPriced: true,
    query: "",
    sort: "platinum",
  }).find((row) => row.job.priceCoveragePercent >= 80) ?? null;
  $: rotationAt = bountyRotationAt(view);
  $: automaticRefreshAt = retryAt ?? bountyAutomaticRefreshAt(view);

  function platinum(value: number | null | undefined): string {
    if (value == null) return "—";
    return `${value.toLocaleString(localeCode($locale), {
      minimumFractionDigits: value > 0 && value < 1 ? 1 : 0,
      maximumFractionDigits: 1,
    })}p`;
  }

  function percent(value: number): string {
    return `${value.toLocaleString(localeCode($locale), { maximumFractionDigits: 1 })}%`;
  }

  function number(value: number): string {
    return value.toLocaleString(localeCode($locale), { maximumFractionDigits: 0 });
  }

  function countdownLabel(target: number | null, currentTime: number): string {
    if (target === null || !Number.isFinite(target)) return "неизвестно";
    const totalSeconds = Math.max(0, Math.ceil((target - currentTime) / 1000));
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;
    if (hours > 0) return `${hours}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
    return `${minutes}:${seconds.toString().padStart(2, "0")}`;
  }

  function timeLabel(value: string): string {
    const date = new Date(value);
    if (!Number.isFinite(date.getTime())) return "время неизвестно";
    return date.toLocaleTimeString(localeCode($locale), { hour: "2-digit", minute: "2-digit" });
  }

  function withDeadline<T>(request: Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const timeout = window.setTimeout(
        () => reject(new Error("bounty request deadline exceeded")),
        LOAD_DEADLINE_MS,
      );
      request.then(
        (value) => {
          window.clearTimeout(timeout);
          resolve(value);
        },
        (reason: unknown) => {
          window.clearTimeout(timeout);
          reject(reason);
        },
      );
    });
  }

  async function load(forceRefresh = false): Promise<void> {
    if (loading) return;
    loading = true;
    loadingStartedAt = Date.now();
    nowMs = loadingStartedAt;
    if (!view) error = "";
    marketMessage = "";
    try {
      const nextView = await withDeadline(
        invoke<BountyHunterView | null>("bounty_hunter", { forceRefresh }),
      );
      view = nextView;
      error = "";
      livePrices = new Map();
      liveMessages = new Map();
      const nextDueAt = bountyAutomaticRefreshAt(nextView);
      retryAt = nextDueAt !== null && nextDueAt <= Date.now()
        ? Date.now() + BOUNTY_AUTO_RETRY_DELAY_MS
        : null;
      if (nextView) publishBountyView(nextView);
    } catch {
      error = view
        ? "Не удалось обновить заказы. Пока показаны последние данные."
        : "Не удалось получить активные заказы. Повторим автоматически.";
      retryAt = Date.now() + BOUNTY_AUTO_RETRY_DELAY_MS;
    } finally {
      loading = false;
      loadingStartedAt = null;
      nowMs = Date.now();
    }
  }

  async function openMarket(reward: BountyRewardView): Promise<void> {
    if (!reward.slug) return;
    marketMessage = "";
    try {
      await invoke<number>("open_market_items", { slugs: [reward.slug] });
      marketMessage = `Открыта страница «${reward.displayName}» на Warframe Market.`;
    } catch {
      marketMessage = "Не удалось открыть Warframe Market.";
    }
  }

  function rewardPrice(reward: BountyRewardView): number | null {
    if (reward.slug && livePrices.has(reward.slug)) return livePrices.get(reward.slug) ?? null;
    return reward.unitPrice ?? null;
  }

  function rewardContribution(reward: BountyRewardView): number | null {
    if (reward.slug && livePrices.has(reward.slug)) {
      const livePrice = livePrices.get(reward.slug) ?? null;
      return livePrice == null ? null : livePrice * reward.expectedQuantity;
    }
    return reward.expectedPlatinum ?? null;
  }

  function jobPlatinum(job: BountyJobView): number {
    return job.rewards.reduce((total, reward) => total + (rewardContribution(reward) ?? 0), 0);
  }

  function topReward(job: BountyJobView): BountyRewardView | null {
    return job.rewards.find((reward) => rewardPrice(reward) != null) ?? job.rewards[0] ?? null;
  }

  function coverageLabel(job: BountyJobView): string {
    if (job.marketRewardCount === 0) return "Нет наград для продажи";
    return `Учтено цен: ${job.pricedRewardCount} из ${job.marketRewardCount}`;
  }

  function jobIdentity(regionKey: string, job: BountyJobView): string {
    return `${regionKey}|${job.id}`;
  }

  function handleJobToggle(event: Event, regionKey: string, job: BountyJobView): void {
    const details = event.currentTarget as HTMLDetailsElement;
    const identity = jobIdentity(regionKey, job);
    const next = new Set(expandedJobIds);
    if (details.open) next.add(identity);
    else next.delete(identity);
    expandedJobIds = next;
  }

  async function toggleWatchedReward(reward: BountyRewardView): Promise<void> {
    const adding = !watchedRewardKeys.has(reward.trackingKey);
    const rewards = adding
      ? [...watchPreferences.rewards, watchedReward(reward)]
      : watchPreferences.rewards.filter((item) => item.key !== reward.trackingKey);
    watchPreferences = { ...watchPreferences, rewards };
    if (!saveBountyWatchPreferences(watchPreferences)) {
      watchMessage = "Не удалось сохранить отмеченные награды.";
      return;
    }
    publishBountyWatchlistChange(view, adding ? [reward.trackingKey] : []);
    watchMessage = adding
      ? `«${reward.displayName}» отмечена.`
      : `«${reward.displayName}» больше не отслеживается.`;

    if (adding && watchPreferences.enabled) {
      const granted = await requestBountyNotificationPermission();
      if (!granted) {
        watchPreferences = { ...watchPreferences, enabled: false };
        saveBountyWatchPreferences(watchPreferences);
        publishBountyWatchlistChange(view);
        watchMessage = "Награда отмечена, но системные уведомления не разрешены.";
      }
    }
  }

  function removeWatchedReward(key: string): void {
    const reward = watchPreferences.rewards.find((item) => item.key === key);
    watchPreferences = {
      ...watchPreferences,
      rewards: watchPreferences.rewards.filter((item) => item.key !== key),
    };
    saveBountyWatchPreferences(watchPreferences);
    publishBountyWatchlistChange(view);
    watchMessage = reward ? `«${reward.displayName}» больше не отслеживается.` : "";
  }

  async function setWatchNotifications(enabled: boolean): Promise<void> {
    if (enabled && !(await requestBountyNotificationPermission())) {
      watchPreferences = { ...watchPreferences, enabled: false };
      saveBountyWatchPreferences(watchPreferences);
      publishBountyWatchlistChange(view);
      watchMessage = "Разрешите уведомления для PlatScope в настройках Windows.";
      return;
    }
    watchPreferences = { ...watchPreferences, enabled };
    saveBountyWatchPreferences(watchPreferences);
    publishBountyWatchlistChange(
      view,
      enabled ? watchPreferences.rewards.map((reward) => reward.key) : [],
    );
    watchMessage = enabled ? "Уведомления включены." : "Уведомления выключены.";
  }

  async function checkJobPrices(job: BountyJobView): Promise<void> {
    if (liveBusyJobId) return;
    const rewards = job.rewards.filter((reward) => reward.slug && reward.marketKey);
    if (rewards.length === 0) {
      liveMessages = new Map(liveMessages).set(job.id, "У этого заказа нет наград на Warframe Market.");
      return;
    }
    liveBusyJobId = job.id;
    liveMessages = new Map(liveMessages).set(job.id, "Проверяем цены игроков, которые сейчас в игре…");
    const results = await Promise.allSettled(rewards.map(async (reward) => {
      const result = await invoke<LivePricingResult | null>("live_price_current_variant", {
        key: reward.marketKey,
        itemKind: "standard",
      });
      const price = result?.recommendation.listPrice ?? result?.recommendation.fairPrice ?? null;
      return { slug: reward.slug!, price };
    }));
    const nextPrices = new Map(livePrices);
    let updated = 0;
    for (const result of results) {
      if (result.status === "fulfilled" && result.value.price != null) {
        nextPrices.set(result.value.slug, result.value.price);
        updated += 1;
      }
    }
    livePrices = nextPrices;
    liveMessages = new Map(liveMessages).set(
      job.id,
      updated > 0
        ? `Актуальные цены получены: ${updated} из ${rewards.length}.`
        : "Сейчас нет подходящих ордеров игроков в игре.",
    );
    liveBusyJobId = "";
  }

  function tick(): void {
    nowMs = Date.now();
    if (!loading && automaticRefreshAt !== null && nowMs >= automaticRefreshAt) {
      retryAt = nowMs + BOUNTY_AUTO_RETRY_DELAY_MS;
      void load(true);
    }
  }

  onMount(() => {
    watchPreferences = loadBountyWatchPreferences();
    const timer = window.setInterval(tick, 1000);
    const handleVisibility = () => {
      if (!document.hidden) tick();
    };
    const handleRefreshedView = (event: Event): void => {
      const nextView = (event as CustomEvent<BountyHunterView>).detail;
      if (!nextView) return;
      view = nextView;
      error = "";
      retryAt = null;
      livePrices = new Map();
      liveMessages = new Map();
      nowMs = Date.now();
    };
    document.addEventListener("visibilitychange", handleVisibility);
    window.addEventListener(BOUNTY_VIEW_REFRESHED_EVENT, handleRefreshedView);
    void load();
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", handleVisibility);
      window.removeEventListener(BOUNTY_VIEW_REFRESHED_EVENT, handleRefreshedView);
    };
  });
</script>

<div class="bounty-screen">
  <div class="live-region" role="status" aria-live="polite">
    {#if loading}Обновляем активные заказы…{:else if marketMessage}{marketMessage}{/if}
  </div>

  {#if error && !view}
    <section class="error-block" role="alert">
      <p>{error}</p>
      <span>Повтор через {countdownLabel(retryAt, nowMs)}</span>
      <button type="button" onclick={() => load(true)}>Проверить сейчас</button>
    </section>
  {:else if loading && !view}
    <section class="bounty-loading" aria-label="Загрузка активных заказов">
      <div class="bounty-loading__status">
        <strong>Получаем ротацию заказов</strong>
        <span>{loadingStartedAt === null ? "Подключаемся…" : `${Math.floor((nowMs - loadingStartedAt) / 1000)} с`}</span>
      </div>
      <div class="bounty-loading__row" aria-hidden="true"></div>
      <div class="bounty-loading__row" aria-hidden="true"></div>
    </section>
  {:else if !view}
    <section class="empty-panel">
      <p class="empty-panel__label">Нужны данные рынка</p>
      <h2>Сначала загрузите цены предметов</h2>
      <p>После этого PlatScope сопоставит награды заказов с Warframe Market.</p>
      <button type="button" onclick={onOpenSettings}>Открыть настройки</button>
    </section>
  {:else}
    <section class="rotation-bar" aria-label="Обновление заказов">
      <div>
        <span>Текущая ротация</span>
        <strong>Смена через {countdownLabel(rotationAt, nowMs)}</strong>
      </div>
      <div>
        <span>Обновлено в {timeLabel(view.fetchedAt)}</span>
        {#if loading}<strong>Обновляем сейчас</strong>{/if}
      </div>
      <button type="button" class="secondary" disabled={loading} onclick={() => load(true)}>
        {loading ? "Обновляем…" : "Обновить"}
      </button>
    </section>

    {#if error}
      <section class="refresh-warning" role="status">
        <span>{error} Повтор через {countdownLabel(retryAt, nowMs)}.</span>
        <button type="button" class="secondary" disabled={loading} onclick={() => load(true)}>Повторить</button>
      </section>
    {/if}

    {#if bestRow}
      <section class="best-card" aria-label="Лучший заказ сейчас">
        <div class="best-card__main">
          <span class="eyebrow">Лучший сейчас</span>
          <h2>{bestRow.job.title}</h2>
          <p>{bestRow.regionName} · ур. {bestRow.job.minLevel}–{bestRow.job.maxLevel} · {bestRow.job.stageCount} этапов</p>
        </div>
        <div class="best-card__value">
          <span>В среднем за заказ</span>
          <strong>≈{platinum(jobPlatinum(bestRow.job))}</strong>
          <small>{coverageLabel(bestRow.job)}</small>
        </div>
        <div class="best-card__reward">
          <span>Главная рыночная награда</span>
          <strong>{topReward(bestRow.job)?.displayName ?? "Нет"}</strong>
          <small>{topReward(bestRow.job) ? `шанс ${percent(topReward(bestRow.job)!.chancePercent)}` : ""}</small>
        </div>
      </section>
    {/if}

    <section class="bounty-toolbar" aria-label="Поиск и сортировка заказов">
      <label class="search-field">
        <span>Найти награду или заказ</span>
        <input bind:value={query} type="search" placeholder="Например, Айя или Гаруда" />
      </label>
      <label>
        <span>Регион</span>
        <select bind:value={region}>
          <option value="all">Все регионы</option>
          {#each view.regions as item (item.key)}
            <option value={item.key}>{item.displayName}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>Сначала</span>
        <select bind:value={sort}>
          <option value="platinum">Больше платины</option>
          <option value="reward_chance">Выше шанс награды</option>
          <option value="level">Ниже уровень</option>
          <option value="rotation">Скорее сменится</option>
        </select>
      </label>
      <label class="priced-toggle">
        <input bind:checked={onlyPriced} type="checkbox" />
        <span>Только с платиной</span>
      </label>
    </section>

    <details class="watchlist">
      <summary>
        <span class="watchlist__mark" aria-hidden="true">{watchPreferences.rewards.length > 0 ? "★" : "☆"}</span>
        <strong>Отмеченные награды</strong>
        <span>{watchPreferences.rewards.length > 0 ? `${watchPreferences.rewards.length} отслеживается` : "Список пуст"}</span>
      </summary>
      <div class="watchlist__body">
        <div class="watchlist__heading">
          <p>PlatScope сообщит о новой ротации с этой наградой, пока приложение запущено.</p>
          <label class="watchlist__toggle">
            <input
              type="checkbox"
              checked={watchPreferences.enabled}
              disabled={watchPreferences.rewards.length === 0}
              onchange={(event) => void setWatchNotifications((event.currentTarget as HTMLInputElement).checked)}
            />
            <span>Уведомлять</span>
          </label>
        </div>
        {#if watchPreferences.rewards.length === 0}
          <p class="watchlist__empty">Раскройте заказ и нажмите ☆ рядом с нужной наградой.</p>
        {:else}
          <ul class="watchlist__items">
            {#each watchPreferences.rewards as reward (reward.key)}
              <li>
                <span class="watchlist__image">{#if reward.imageUrl}<img src={reward.imageUrl} alt="" loading="lazy" decoding="async" />{:else}★{/if}</span>
                <strong>{reward.displayName}</strong>
                <button type="button" class="text-action" onclick={() => removeWatchedReward(reward.key)}>Убрать</button>
              </li>
            {/each}
          </ul>
        {/if}
        <p class="watchlist__message" aria-live="polite">{watchMessage}</p>
      </div>
    </details>

    {#if jobs.length === 0}
      <section class="empty-panel">
        <p class="empty-panel__label">Ничего не найдено</p>
        <h2>Измените поиск или фильтры</h2>
        <button type="button" onclick={() => { query = ""; region = "all"; onlyPriced = false; }}>Сбросить фильтры</button>
      </section>
    {:else}
      <section class="ranking" aria-label="Рейтинг активных заказов">
        <header class="ranking__header">
          <span>№</span><span>Заказ</span><span>Уровень</span><span>Главная награда</span><span>За заказ</span><span>Смена</span>
        </header>
        {#each jobs as row, index (`${row.regionKey}-${row.job.id}`)}
          {@const reward = topReward(row.job)}
          {@const identity = jobIdentity(row.regionKey, row.job)}
          <details class="bounty-row" ontoggle={(event) => handleJobToggle(event, row.regionKey, row.job)}>
            <summary class="bounty-row__summary">
              <strong class="rank">{index + 1}</strong>
              <div class="job-name">
                <strong>{row.job.title}</strong>
                <span>{row.regionName} · {row.job.stageCount} этапов · {number(row.job.totalStanding)} репутации</span>
              </div>
              <span class="level">{row.job.minLevel}–{row.job.maxLevel}</span>
              <div class="top-reward">
                <span class="top-reward__image">{#if reward?.imageUrl}<img src={reward.imageUrl} alt="" loading="lazy" decoding="async" />{:else}◇{/if}</span>
                <span><strong>{reward?.displayName ?? "Нет рыночной награды"}</strong>{#if reward}<small>шанс {percent(reward.chancePercent)}</small>{/if}</span>
              </div>
              <div class="job-price">
                <strong>{row.job.pricedRewardCount > 0 ? `≈${platinum(jobPlatinum(row.job))}` : "—"}</strong>
                <small class:incomplete={row.job.pricedRewardCount < row.job.marketRewardCount}>{coverageLabel(row.job)}</small>
              </div>
              <span class="rotation">{countdownLabel(new Date(row.expiry).getTime(), nowMs)}</span>
            </summary>

            {#if expandedJobIds.has(identity)}
              <div class="job-details">
                <header>
                  <div>
                    <strong>Награды за полный заказ</strong>
                    <span>Шанс учитывает все этапы; платина — среднее за много прохождений.</span>
                  </div>
                  <button type="button" class="secondary" disabled={Boolean(liveBusyJobId)} onclick={() => checkJobPrices(row.job)}>
                    {liveBusyJobId === row.job.id ? "Проверяем…" : "Проверить цены сейчас"}
                  </button>
                </header>
                {#if liveMessages.has(row.job.id)}
                  <p class="live-message">{liveMessages.get(row.job.id)}</p>
                {/if}
                <div class="reward-table">
                  <div class="reward-table__head"><span>Награда</span><span>Шанс</span><span>Цена</span><span>В среднем</span><span>В инвентаре</span><span></span></div>
                  {#each row.job.rewards as item (`${row.job.id}-${item.displayName}`)}
                    <article class:unpriced={item.marketKey && rewardPrice(item) == null} class:untradeable={!item.marketKey}>
                      <div class="reward-name">
                        <span class="reward-image">{#if item.imageUrl}<img src={item.imageUrl} alt="" loading="lazy" decoding="async" />{:else}◇{/if}</span>
                        <span class="reward-name__copy"><strong>{item.displayName}</strong><small>{item.rarity}</small></span>
                        <button
                          type="button"
                          class="watch-button"
                          class:active={watchedRewardKeys.has(item.trackingKey)}
                          aria-pressed={watchedRewardKeys.has(item.trackingKey)}
                          aria-label={watchedRewardKeys.has(item.trackingKey) ? `Не отслеживать ${item.displayName}` : `Отслеживать ${item.displayName}`}
                          title={watchedRewardKeys.has(item.trackingKey) ? "Не отслеживать" : "Сообщить при появлении"}
                          onclick={() => void toggleWatchedReward(item)}
                        >{watchedRewardKeys.has(item.trackingKey) ? "★" : "☆"}</button>
                      </div>
                      <strong>{percent(item.chancePercent)}</strong>
                      <span>{item.marketKey ? (rewardPrice(item) == null ? "Нет свежей цены" : platinum(rewardPrice(item))) : "Не продаётся"}</span>
                      <span>{rewardContribution(item) == null ? "—" : `≈${platinum(rewardContribution(item))}`}</span>
                      <span>{item.ownedQuantity == null ? "—" : `${item.ownedQuantity} шт.`}</span>
                      <span class="reward-actions">
                        {#if item.slug}<button type="button" class="text-action" onclick={() => openMarket(item)}>На рынок ↗</button>{/if}
                      </span>
                    </article>
                  {/each}
                </div>
              </div>
            {/if}
          </details>
        {/each}
      </section>
    {/if}

    <p class="bounty-limit">Показаны активные заказы Цетуса, Фортуны и Некралиска. Названия взяты из русской локализации Warframe.</p>
  {/if}
</div>

<style>
  .bounty-screen { display: grid; gap: 0.65rem; }
  .live-region { min-height: 1rem; color: var(--text-muted); font-size: 0.72rem; }

  .rotation-bar {
    display: grid;
    grid-template-columns: minmax(13rem, 1fr) minmax(15rem, 1fr) auto;
    align-items: center;
    gap: 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.7rem;
    padding: 0.55rem 0.7rem;
    background: var(--surface-1);
    box-shadow: var(--shadow-sm);
  }
  .rotation-bar > div { display: grid; gap: 0.08rem; }
  .rotation-bar span { color: var(--text-muted); font-size: 0.68rem; }
  .rotation-bar strong { font-size: 0.82rem; font-variant-numeric: tabular-nums; }

  .refresh-warning {
    display: flex; align-items: center; justify-content: space-between; gap: 0.7rem;
    border: 1px solid var(--gold); border-radius: 0.6rem; padding: 0.5rem 0.65rem;
    background: var(--accent-soft); font-size: 0.75rem;
  }

  .best-card {
    display: grid;
    grid-template-columns: minmax(15rem, 1.5fr) minmax(9rem, 0.55fr) minmax(13rem, 0.9fr);
    overflow: hidden;
    border: 1px solid color-mix(in oklch, var(--success) 50%, var(--border));
    border-radius: 0.75rem;
    background: color-mix(in oklch, var(--success-soft) 32%, var(--surface-1));
    box-shadow: var(--shadow-sm);
  }
  .best-card > div { display: grid; align-content: center; gap: 0.1rem; min-width: 0; padding: 0.65rem 0.8rem; }
  .best-card > div + div { border-inline-start: 1px solid var(--border); }
  .best-card h2, .best-card p { margin: 0; }
  .best-card h2 { font-size: 1rem; }
  .best-card p, .best-card span, .best-card small { color: var(--text-muted); font-size: 0.68rem; }
  .best-card .eyebrow { color: var(--success); font-weight: 850; letter-spacing: 0.06em; text-transform: uppercase; }
  .best-card__value strong { color: var(--accent-strong); font-size: 1.45rem; font-variant-numeric: tabular-nums; }
  .best-card__reward strong { overflow-wrap: anywhere; font-size: 0.85rem; }

  .bounty-toolbar {
    display: grid;
    grid-template-columns: minmax(15rem, 1.6fr) minmax(9rem, 0.65fr) minmax(11rem, 0.8fr) auto;
    align-items: end;
    gap: 0.55rem;
    border: 1px solid var(--border);
    border-radius: 0.7rem;
    padding: 0.55rem 0.65rem;
    background: var(--surface-1);
  }
  .bounty-toolbar label { display: grid; gap: 0.22rem; }
  .bounty-toolbar label > span { color: var(--text-muted); font-size: 0.68rem; font-weight: 750; }
  .bounty-toolbar input[type="search"], .bounty-toolbar select {
    width: 100%; min-height: 2.05rem; border: 1px solid var(--border-strong); border-radius: 0.45rem;
    padding: 0.38rem 0.55rem; background: var(--surface-1); color: var(--text);
  }
  .bounty-toolbar .priced-toggle { display: flex; align-items: center; gap: 0.38rem; min-height: 2.05rem; white-space: nowrap; }
  .priced-toggle input { width: 1rem; height: 1rem; accent-color: var(--accent); }
  .bounty-toolbar .priced-toggle > span { color: var(--text); }

  .watchlist {
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 0.7rem;
    background: var(--surface-1);
  }
  .watchlist > summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.55rem 0.7rem;
    cursor: pointer;
    list-style: none;
  }
  .watchlist > summary::-webkit-details-marker { display: none; }
  .watchlist__mark { color: var(--accent-strong); font-size: 1rem; }
  .watchlist > summary strong { margin-inline-end: auto; font-size: 0.8rem; }
  .watchlist > summary span { color: var(--text-muted); font-size: 0.68rem; }
  .watchlist__body { border-block-start: 1px solid var(--border); padding: 0.6rem 0.7rem; }
  .watchlist__heading { display: flex; align-items: center; justify-content: space-between; gap: 0.75rem; }
  .watchlist__heading p, .watchlist__empty, .watchlist__message { margin: 0; color: var(--text-muted); font-size: 0.68rem; }
  .watchlist__toggle { display: flex; align-items: center; gap: 0.35rem; font-size: 0.72rem; font-weight: 750; white-space: nowrap; }
  .watchlist__toggle input { width: 1rem; height: 1rem; accent-color: var(--accent); }
  .watchlist__items { display: grid; gap: 1px; margin: 0.55rem 0 0; padding: 0; overflow: hidden; border: 1px solid var(--border); border-radius: 0.5rem; background: var(--border); list-style: none; }
  .watchlist__items li { display: grid; grid-template-columns: 2rem minmax(0, 1fr) auto; align-items: center; gap: 0.5rem; padding: 0.35rem 0.5rem; background: var(--surface-1); }
  .watchlist__items strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 0.74rem; }
  .watchlist__image { display: grid; width: 2rem; height: 2rem; place-items: center; overflow: hidden; border-radius: 0.35rem; background: var(--surface-2); color: var(--accent-strong); }
  .watchlist__image img { width: 100%; height: 100%; object-fit: contain; }
  .watchlist__empty { margin-block-start: 0.45rem; }
  .watchlist__message { min-height: 1rem; margin-block-start: 0.35rem; color: var(--accent-strong); }

  .ranking {
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 0.75rem;
    background: var(--surface-1);
    box-shadow: var(--shadow-sm);
  }
  .ranking__header, .bounty-row__summary {
    display: grid;
    grid-template-columns: 2rem minmax(12rem, 1.25fr) 4.5rem minmax(12rem, 1fr) 7.5rem 5.5rem;
    align-items: center;
    gap: 0.55rem;
  }
  .ranking__header {
    padding: 0.48rem 0.7rem;
    background: var(--surface-2);
    color: var(--text-muted);
    font-size: 0.64rem;
    font-weight: 850;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .bounty-row + .bounty-row { border-block-start: 1px solid var(--border); }
  .bounty-row[open] { background: color-mix(in oklch, var(--accent-soft) 18%, var(--surface-1)); }
  .bounty-row__summary { padding: 0.52rem 0.7rem; cursor: pointer; list-style: none; }
  .bounty-row__summary::-webkit-details-marker { display: none; }
  .bounty-row__summary:hover { background: var(--surface-hover); }
  .rank { color: var(--accent-strong); text-align: center; font-variant-numeric: tabular-nums; }
  .job-name, .top-reward, .job-price { display: grid; gap: 0.08rem; min-width: 0; }
  .job-name > strong, .top-reward strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 0.78rem; }
  .job-name span, .top-reward small, .job-price small { color: var(--text-muted); font-size: 0.64rem; }
  .level, .rotation { font-size: 0.75rem; font-variant-numeric: tabular-nums; }
  .top-reward { grid-template-columns: 1.8rem minmax(0, 1fr); align-items: center; }
  .top-reward > span { display: grid; min-width: 0; }
  .top-reward__image { display: grid; width: 1.8rem; height: 1.8rem; place-items: center; color: var(--text-muted); border-radius: 0.3rem; background: var(--surface-2); }
  .top-reward img { width: 100%; height: 100%; object-fit: contain; }
  .job-price strong { color: var(--accent-strong); font-size: 0.95rem; font-variant-numeric: tabular-nums; }
  .job-price small.incomplete { color: var(--warning, var(--accent-strong)); }

  .job-details { border-block-start: 1px solid var(--border); padding: 0.65rem 0.75rem 0.75rem 3.25rem; }
  .job-details > header { display: flex; align-items: center; justify-content: space-between; gap: 0.75rem; margin-block-end: 0.45rem; }
  .job-details > header > div { display: grid; gap: 0.08rem; }
  .job-details header strong { font-size: 0.8rem; }
  .job-details header span, .live-message { color: var(--text-muted); font-size: 0.66rem; }
  .live-message { margin: 0 0 0.4rem; }

  .reward-table { overflow: hidden; border: 1px solid var(--border); border-radius: 0.55rem; }
  .reward-table__head, .reward-table article {
    display: grid;
    grid-template-columns: minmax(13rem, 1.5fr) 5rem 8rem 7rem 6rem 5.5rem;
    align-items: center;
    gap: 0.5rem;
    padding: 0.38rem 0.55rem;
  }
  .reward-table__head { background: var(--surface-2); color: var(--text-muted); font-size: 0.61rem; font-weight: 850; text-transform: uppercase; }
  .reward-table article + article { border-block-start: 1px solid var(--border); }
  .reward-table article { font-size: 0.72rem; }
  .reward-table article.unpriced { background: color-mix(in oklch, var(--accent-soft) 24%, var(--surface-1)); }
  .reward-table article.untradeable { color: var(--text-muted); }
  .reward-name { display: grid; grid-template-columns: 2rem minmax(0, 1fr) auto; align-items: center; gap: 0.45rem; min-width: 0; }
  .reward-name__copy { display: grid; min-width: 0; }
  .reward-name strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 0.74rem; }
  .reward-name small { color: var(--text-muted); font-size: 0.62rem; }
  .reward-image { display: grid; width: 2rem; height: 2rem; place-items: center; overflow: hidden; border-radius: 0.35rem; background: var(--surface-2); }
  .reward-image img { width: 100%; height: 100%; object-fit: contain; }
  .text-action { border: 0; padding: 0.15rem; background: transparent; color: var(--accent-strong); font-size: 0.68rem; font-weight: 800; white-space: nowrap; }
  .text-action:hover { text-decoration: underline; }
  .reward-actions { display: flex; align-items: center; justify-content: flex-end; gap: 0.35rem; }
  .watch-button { display: grid; width: 1.85rem; height: 1.85rem; place-items: center; border: 1px solid var(--border-strong); border-radius: 0.4rem; padding: 0; background: var(--surface-1); color: var(--text-muted); font-size: 1rem; }
  .watch-button:hover, .watch-button.active { border-color: var(--accent); background: var(--accent-soft); color: var(--accent-strong); }

  .bounty-limit { margin: 0; color: var(--text-muted); font-size: 0.68rem; }
  .bounty-loading { display: grid; gap: 0.55rem; }
  .bounty-loading__status { display: flex; justify-content: space-between; border: 1px solid var(--border); border-radius: 0.7rem; padding: 0.65rem; background: var(--surface-1); }
  .bounty-loading__status span { color: var(--text-muted); font-size: 0.72rem; }
  .bounty-loading__row { height: 4.5rem; border-radius: 0.7rem; background: var(--surface-2); }

  @media (max-width: 75rem) {
    .ranking__header, .bounty-row__summary { grid-template-columns: 1.8rem minmax(11rem, 1.2fr) 4rem minmax(10rem, 1fr) 6rem; }
    .ranking__header > :last-child, .bounty-row__summary > .rotation { display: none; }
    .reward-table__head, .reward-table article { grid-template-columns: minmax(12rem, 1.4fr) 4.5rem 7rem 6rem 5rem; }
    .reward-table__head > :last-child, .reward-table article > :last-child { grid-column: 1 / -1; justify-self: end; }
  }

  @media (max-width: 55rem) {
    .rotation-bar, .best-card, .bounty-toolbar { grid-template-columns: 1fr 1fr; }
    .rotation-bar button { justify-self: start; }
    .best-card > div:nth-child(3) { grid-column: 1 / -1; border-inline-start: 0; border-block-start: 1px solid var(--border); }
    .search-field { grid-column: 1 / -1; }
    .ranking__header { display: none; }
    .bounty-row__summary { grid-template-columns: 1.7rem minmax(0, 1fr) auto; }
    .bounty-row__summary > .level { justify-self: end; }
    .bounty-row__summary > .top-reward, .bounty-row__summary > .job-price { grid-column: 2 / -1; }
    .bounty-row__summary > .rotation { display: none; }
    .job-details { padding-inline-start: 0.75rem; }
    .reward-table { overflow-x: auto; }
    .reward-table__head, .reward-table article { min-width: 46rem; }
  }

  @media (max-width: 38rem) {
    .rotation-bar, .best-card, .bounty-toolbar { grid-template-columns: 1fr; }
    .best-card > div + div { border-inline-start: 0; border-block-start: 1px solid var(--border); }
    .best-card > div:nth-child(3), .search-field { grid-column: auto; }
    .job-details > header, .refresh-warning { align-items: flex-start; flex-direction: column; }
    .watchlist__heading { align-items: flex-start; flex-direction: column; }
  }
</style>
