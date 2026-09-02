<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  import {
    BOUNTY_AUTO_RETRY_DELAY_MS,
    bestBountyJob,
    bountyAutomaticRefreshAt,
    bountyRotationAt,
    visibleBountyRegions,
    type BountyHunterView,
    type BountyJobView,
    type BountyRewardView,
  } from "./bountyHunter";
  import { localeCode, useLocale } from "./i18n";

  export let onOpenSettings: () => void;

  const LOAD_DEADLINE_MS = 20_000;
  const locale = useLocale();
  let view: BountyHunterView | null = null;
  let loading = false;
  let loadingStartedAt: number | null = null;
  let error = "";
  let region = "all";
  let onlyPriced = true;
  let marketMessage = "";
  let nowMs = Date.now();
  let retryAt: number | null = null;

  $: regions = visibleBountyRegions(view, region, onlyPriced);
  $: bestJob = bestBountyJob(view);
  $: rotationAt = bountyRotationAt(view);
  $: automaticRefreshAt = retryAt ?? bountyAutomaticRefreshAt(view);
  $: pricedJobs = view?.regions
    .flatMap((item) => item.jobs)
    .filter((job) => job.pricedRewardCount > 0).length ?? 0;

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

  function pricedRewardLabel(value: number): string {
    const mod10 = value % 10;
    const mod100 = value % 100;
    const noun = mod10 === 1 && mod100 !== 11
      ? "награду"
      : mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)
        ? "награды"
        : "наград";
    return `${value} ${noun} можно продать`;
  }

  function expiryLabel(value: string, currentTime: number): string {
    const date = new Date(value);
    if (!Number.isFinite(date.getTime())) return "Время ротации неизвестно";
    if (date.getTime() <= currentTime) return "Получаем новую ротацию";
    return `Смена через ${countdownLabel(date.getTime(), currentTime)}`;
  }

  function countdownLabel(target: number | null, currentTime: number): string {
    if (target === null || !Number.isFinite(target)) return "время неизвестно";
    const totalSeconds = Math.max(0, Math.ceil((target - currentTime) / 1000));
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;
    if (hours > 0) {
      return `${hours} ч ${minutes.toString().padStart(2, "0")} мин ${seconds.toString().padStart(2, "0")} с`;
    }
    if (minutes > 0) {
      return `${minutes} мин ${seconds.toString().padStart(2, "0")} с`;
    }
    return `${seconds} с`;
  }

  function updatedAtLabel(value: string): string {
    const date = new Date(value);
    if (!Number.isFinite(date.getTime())) return "Время получения данных неизвестно";
    return `Данные получены в ${date.toLocaleTimeString(localeCode($locale), {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    })}`;
  }

  function rotationLabel(target: number | null, currentTime: number): string {
    if (target === null) return "Время смены заказов неизвестно";
    if (target <= currentTime) return "Получаем новую ротацию";
    return `Смена заказов через ${countdownLabel(target, currentTime)}`;
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

  function isBest(job: BountyJobView): boolean {
    return bestJob === job && (bestJob?.expectedPlatinum ?? 0) > 0;
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
      const nextDueAt = bountyAutomaticRefreshAt(nextView);
      retryAt = nextDueAt !== null && nextDueAt <= Date.now()
        ? Date.now() + BOUNTY_AUTO_RETRY_DELAY_MS
        : null;
    } catch {
      error = view
        ? "Не удалось обновить заказы. Показываем последние полученные данные."
        : "Не удалось получить активные заказы. Автоповтор через 30 секунд.";
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
      marketMessage = "Не удалось открыть Warframe Market. Повторите действие.";
    }
  }

  function tick(): void {
    nowMs = Date.now();
    if (!loading && automaticRefreshAt !== null && nowMs >= automaticRefreshAt) {
      retryAt = nowMs + BOUNTY_AUTO_RETRY_DELAY_MS;
      void load(true);
    }
  }

  onMount(() => {
    const timer = window.setInterval(tick, 1000);
    const handleVisibility = () => {
      if (!document.hidden) tick();
    };
    document.addEventListener("visibilitychange", handleVisibility);
    void load();
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  });
</script>

<div class="bounty-screen">
  <div class="live-region" role="status" aria-live="polite">
    {#if loading}
      Обновляем активные заказы…
    {:else if marketMessage}
      {marketMessage}
    {/if}
  </div>

  {#if error && !view}
    <section class="error-block" role="alert">
      <p>{error}</p>
      <span>Следующая попытка через {countdownLabel(retryAt, nowMs)}</span>
      <button type="button" onclick={() => load(true)}>Проверить сейчас</button>
    </section>
  {:else if loading && !view}
    <section class="bounty-loading" aria-label="Загрузка активных заказов">
      <div class="bounty-loading__status">
        <strong>Получаем активные заказы</strong>
        <span>
          {loadingStartedAt === null
            ? "Подключаемся к источнику…"
            : `Ожидаем ответ · ${Math.floor((nowMs - loadingStartedAt) / 1000)} с`}
        </span>
      </div>
      <div class="bounty-loading__row" aria-hidden="true"></div>
      <div class="bounty-loading__row" aria-hidden="true"></div>
    </section>
  {:else if !view}
    <section class="empty-panel">
      <p class="empty-panel__label">Нужны данные предметов</p>
      <h2>Сначала загрузите рынок</h2>
      <p>Без каталога нельзя сопоставить награды заказов с товарами Warframe Market.</p>
      <button type="button" onclick={onOpenSettings}>Открыть настройки</button>
    </section>
  {:else}
    {#if error}
      <section class="refresh-warning" role="status">
        <span>{error} Повтор через {countdownLabel(retryAt, nowMs)}.</span>
        <button type="button" class="secondary" disabled={loading} onclick={() => load(true)}>
          {loading ? "Обновляем…" : "Проверить сейчас"}
        </button>
      </section>
    {/if}

    <section class="bounty-summary" aria-label="Краткий итог">
      <div>
        <small>Активные регионы</small>
        <strong>{view.regions.length}</strong>
      </div>
      <div>
        <small>Заказы с продажей наград</small>
        <strong>{pricedJobs}</strong>
      </div>
      <div class="bounty-summary__best">
        <small>Лучший заказ по платине</small>
        <strong>{bestJob ? `≈${platinum(bestJob.expectedPlatinum)}` : "—"}</strong>
        <span>{bestJob ? `${bestJob.title} · уровни ${bestJob.minLevel}–${bestJob.maxLevel}` : "Нет подтверждённых цен"}</span>
      </div>
      <div class="bounty-summary__action">
        <small>{updatedAtLabel(view.fetchedAt)}</small>
        <strong>
          {loading
            ? "Обновляем сейчас"
            : `Автопроверка через ${countdownLabel(automaticRefreshAt, nowMs)}`}
        </strong>
        <span>
          {rotationAt === null
            ? `Цены рынка от ${view.marketSourceDate ?? "—"}`
            : rotationLabel(rotationAt, nowMs)}
        </span>
        <button type="button" class="secondary" disabled={loading} onclick={() => load(true)}>
          {loading ? "Обновляем…" : "Обновить сейчас"}
        </button>
      </div>
    </section>

    <section class="bounty-toolbar" aria-label="Фильтры заказов">
      <label>
        <span>Регион</span>
        <select bind:value={region}>
          <option value="all">Все регионы</option>
          {#each view.regions as item (item.key)}
            <option value={item.key}>{item.displayName}</option>
          {/each}
        </select>
      </label>
      <div>
        <span>Показывать</span>
        <div class="segmented" role="group" aria-label="Какие заказы показывать">
          <button type="button" aria-pressed={onlyPriced} onclick={() => (onlyPriced = true)}>С платиной</button>
          <button type="button" aria-pressed={!onlyPriced} onclick={() => (onlyPriced = false)}>Все заказы</button>
        </div>
      </div>
    </section>

    <p class="bounty-method">
      Оценка за полный заказ: шанс награды на каждом этапе × ориентир продажи. Это среднее на серии прохождений, а не гарантированная платина.
    </p>

    {#if regions.length === 0}
      <section class="empty-panel">
        <p class="empty-panel__label">Подходящих заказов нет</p>
        <h2>В выбранном регионе нет наград с подтверждённой ценой</h2>
        <p>Покажите все заказы или выберите другой регион.</p>
        <button type="button" onclick={() => (onlyPriced = false)}>Показать все заказы</button>
      </section>
    {:else}
      <div class="bounty-regions">
        {#each regions as regionView (regionView.key)}
          <section class="bounty-region" aria-labelledby={`region-${regionView.key}`}>
            <header>
              <div>
                <p>Регион</p>
                <h2 id={`region-${regionView.key}`}>{regionView.displayName}</h2>
              </div>
              <span>{expiryLabel(regionView.expiry, nowMs)}</span>
            </header>

            <div class="bounty-jobs">
              {#each regionView.jobs as job, jobIndex (`${regionView.key}-${job.id}-${jobIndex}`)}
                <details class:best={isBest(job)} open={isBest(job)}>
                  <summary>
                    <div class="job-title">
                      <span class="job-level">Ур. {job.minLevel}–{job.maxLevel}</span>
                      <strong>{job.title}</strong>
                      <small>
                        {job.stageCount} этапов · {number(job.totalStanding)} репутации
                        {#if job.minMasteryRank > 0} · ранг мастерства {job.minMasteryRank}{/if}
                        {#if job.timeBound} · {job.timeBound}{/if}
                      </small>
                    </div>
                    <div class="job-value">
                      {#if isBest(job)}<span class="best-label">Лучший выбор</span>{/if}
                      <small>В среднем за заказ</small>
                      <strong>{job.pricedRewardCount > 0 ? `≈${platinum(job.expectedPlatinum)}` : "Нет цены"}</strong>
                      <span>{pricedRewardLabel(job.pricedRewardCount)}</span>
                    </div>
                  </summary>

                  <div class="reward-list">
                    {#each job.rewards as reward (`${job.id}-${reward.displayName}`)}
                      <article class:tradeable={reward.unitPrice != null}>
                        <div class="reward-image" aria-hidden="true">
                          {#if reward.imageUrl}
                            <img src={reward.imageUrl} alt="" />
                          {:else}
                            <span>◇</span>
                          {/if}
                        </div>
                        <div class="reward-name">
                          <strong>{reward.displayName}</strong>
                          <span>{reward.rarity} · шанс за заказ {percent(reward.chancePercent)}</span>
                        </div>
                        <div class="reward-price">
                          {#if reward.unitPrice != null}
                            <small>Ориентир продажи</small>
                            <strong>{platinum(reward.unitPrice)}</strong>
                            <span>вклад ≈{platinum(reward.expectedPlatinum)}</span>
                          {:else}
                            <small>Рынок</small>
                            <strong>Не продаётся</strong>
                          {/if}
                        </div>
                        {#if reward.slug}
                          <button type="button" class="secondary reward-market" onclick={() => openMarket(reward)}>
                            Открыть на рынке
                          </button>
                        {/if}
                      </article>
                    {/each}
                  </div>
                </details>
              {/each}
            </div>
          </section>
        {/each}
      </div>
    {/if}

    <p class="bounty-limit">
      Worldstate публикует заказы Цетуса, Фортуны и Некралиска. Заказы Заримана и Кавии источник сейчас не раскрывает, поэтому приложение их не выдумывает.
    </p>
  {/if}
</div>

<style>
  .bounty-screen {
    display: grid;
    gap: 0.75rem;
  }

  .live-region {
    min-height: 1rem;
    color: var(--text-muted);
    font-size: 0.75rem;
  }

  .bounty-summary {
    display: grid;
    grid-template-columns: minmax(8rem, 0.65fr) minmax(11rem, 0.85fr) minmax(16rem, 1.4fr) auto;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 0.75rem;
    background: var(--surface-1);
    box-shadow: var(--shadow-sm);
  }

  .bounty-summary > div {
    display: grid;
    align-content: center;
    gap: 0.15rem;
    min-width: 0;
    padding: 0.7rem 0.8rem;
  }

  .bounty-summary > div + div {
    border-inline-start: 1px solid var(--border);
  }

  .bounty-summary small,
  .bounty-summary span {
    color: var(--text-muted);
    font-size: 0.72rem;
  }

  .bounty-summary strong {
    color: var(--accent-strong);
    font-size: 1.15rem;
    font-variant-numeric: tabular-nums;
  }

  .bounty-summary__best strong {
    font-size: 1.35rem;
  }

  .bounty-summary__action {
    justify-items: end;
  }

  .bounty-summary__action strong {
    color: var(--text);
    font-size: 0.82rem;
    font-variant-numeric: tabular-nums;
  }

  .refresh-warning {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border: 1px solid var(--gold);
    border-radius: 0.65rem;
    padding: 0.55rem 0.7rem;
    background: var(--accent-soft);
    color: var(--text);
    font-size: 0.78rem;
  }

  .refresh-warning button {
    flex: 0 0 auto;
  }

  .bounty-toolbar {
    display: flex;
    align-items: end;
    gap: 0.9rem;
    border: 1px solid var(--border);
    border-radius: 0.65rem;
    padding: 0.65rem 0.75rem;
    background: var(--surface-1);
  }

  .bounty-toolbar label,
  .bounty-toolbar > div {
    display: grid;
    gap: 0.25rem;
  }

  .bounty-toolbar label {
    min-width: 12rem;
  }

  .bounty-toolbar span {
    color: var(--text-muted);
    font-size: 0.72rem;
    font-weight: 700;
  }

  .bounty-toolbar select {
    min-height: 2.125rem;
    border: 1px solid var(--border-strong);
    border-radius: 0.45rem;
    padding-inline: 0.55rem;
    background: var(--surface-1);
    color: var(--text);
  }

  .segmented {
    display: flex;
    gap: 0.2rem;
    border-radius: 0.55rem;
    padding: 0.18rem;
    background: var(--surface-3);
  }

  .segmented button {
    border-color: transparent;
    background: transparent;
    color: var(--text-muted);
  }

  .segmented button[aria-pressed="true"] {
    border-color: var(--border-strong);
    background: var(--surface-1);
    color: var(--accent-strong);
    box-shadow: var(--shadow-sm);
  }

  .bounty-method,
  .bounty-limit {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.75rem;
  }

  .bounty-regions {
    display: grid;
    gap: 0.8rem;
  }

  .bounty-region {
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 0.75rem;
    background: var(--surface-1);
    box-shadow: var(--shadow-sm);
  }

  .bounty-region > header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.65rem 0.8rem;
    background: var(--surface-2);
  }

  .bounty-region header p,
  .bounty-region header h2 {
    margin: 0;
  }

  .bounty-region header p {
    color: var(--text-muted);
    font-size: 0.66rem;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .bounty-region header h2 {
    font-size: 1rem;
  }

  .bounty-region header > span {
    color: var(--text-muted);
    font-size: 0.72rem;
  }

  .bounty-jobs {
    display: grid;
  }

  details + details {
    border-block-start: 1px solid var(--border);
  }

  details.best {
    background: color-mix(in oklch, var(--success-soft) 38%, var(--surface-1));
  }

  summary {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(11rem, auto);
    align-items: center;
    gap: 1rem;
    padding: 0.65rem 0.8rem;
    cursor: pointer;
    list-style: none;
  }

  summary::-webkit-details-marker {
    display: none;
  }

  summary:hover {
    background: var(--surface-hover);
  }

  .job-title,
  .job-value {
    display: grid;
    gap: 0.12rem;
    min-width: 0;
  }

  .job-title strong {
    overflow-wrap: anywhere;
    font-size: 0.9rem;
  }

  .job-title small,
  .job-value small,
  .job-value span {
    color: var(--text-muted);
    font-size: 0.7rem;
  }

  .job-level {
    width: fit-content;
    border-radius: 999px;
    padding: 0.12rem 0.38rem;
    background: var(--accent-soft);
    color: var(--accent-strong);
    font-size: 0.67rem;
    font-weight: 800;
  }

  .job-value {
    justify-items: end;
    text-align: end;
  }

  .job-value strong {
    color: var(--accent-strong);
    font-size: 1.2rem;
    font-variant-numeric: tabular-nums;
  }

  .job-value .best-label {
    border-radius: 999px;
    padding: 0.1rem 0.35rem;
    background: var(--success-soft);
    color: var(--success);
    font-weight: 800;
  }

  .reward-list {
    display: grid;
    gap: 0.35rem;
    padding: 0 0.8rem 0.75rem;
  }

  .reward-list article {
    display: grid;
    grid-template-columns: 2.2rem minmax(10rem, 1fr) minmax(8.5rem, auto) auto;
    align-items: center;
    gap: 0.55rem;
    min-width: 0;
    border-radius: 0.5rem;
    padding: 0.4rem;
    background: var(--surface-2);
  }

  .reward-list article.tradeable {
    background: color-mix(in oklch, var(--accent-soft) 35%, var(--surface-1));
  }

  .reward-image {
    display: grid;
    width: 2.2rem;
    height: 2.2rem;
    place-items: center;
    overflow: hidden;
    border-radius: 0.4rem;
    background: var(--surface-1);
    outline: 1px solid oklch(0 0 0 / 0.1);
  }

  .reward-image img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .reward-name,
  .reward-price {
    display: grid;
    gap: 0.08rem;
    min-width: 0;
  }

  .reward-name strong {
    overflow-wrap: anywhere;
    font-size: 0.78rem;
  }

  .reward-name span,
  .reward-price small,
  .reward-price span {
    color: var(--text-muted);
    font-size: 0.66rem;
  }

  .reward-price strong {
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
  }

  .reward-market {
    white-space: nowrap;
  }

  .bounty-loading {
    display: grid;
    gap: 0.6rem;
  }

  .bounty-loading__status {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    min-height: 3rem;
    border: 1px solid var(--border);
    border-radius: 0.75rem;
    padding: 0.7rem 0.8rem;
    background: var(--surface-1);
    box-shadow: var(--shadow-sm);
  }

  .bounty-loading__status strong {
    font-size: 0.88rem;
  }

  .bounty-loading__status span {
    color: var(--text-muted);
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
  }

  .bounty-loading__row {
    height: 5rem;
    border-radius: 0.75rem;
    background: var(--surface-2);
  }

  @media (max-width: 68rem) {
    .bounty-summary {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .bounty-summary > div:nth-child(3) {
      border-inline-start: 0;
      border-block-start: 1px solid var(--border);
    }

    .bounty-summary > div:nth-child(4) {
      border-block-start: 1px solid var(--border);
    }

    .reward-list article {
      grid-template-columns: 2.2rem minmax(0, 1fr) minmax(8rem, auto);
    }

    .reward-market {
      grid-column: 2 / -1;
      justify-self: start;
    }
  }

  @media (max-width: 45rem) {
    .bounty-summary {
      grid-template-columns: 1fr;
    }

    .bounty-summary > div + div {
      border-inline-start: 0;
      border-block-start: 1px solid var(--border);
    }

    .bounty-summary__action {
      justify-items: start;
    }

    .refresh-warning,
    .bounty-loading__status {
      align-items: flex-start;
      flex-direction: column;
    }

    .bounty-toolbar {
      align-items: stretch;
      flex-direction: column;
    }

    .bounty-toolbar label {
      min-width: 0;
    }

    summary {
      grid-template-columns: 1fr;
    }

    .job-value {
      justify-items: start;
      text-align: start;
    }

    .reward-list article {
      grid-template-columns: 2.2rem minmax(0, 1fr);
    }

    .reward-price,
    .reward-market {
      grid-column: 2;
    }
  }
</style>
