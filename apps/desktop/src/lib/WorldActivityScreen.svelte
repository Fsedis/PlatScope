<script lang="ts">
  import { onMount, tick } from "svelte";
  import MasteryBadge from "./MasteryBadge.svelte";
  import PrimeResurgence from "./PrimeResurgence.svelte";
  import WorldActivityIcon from "./WorldActivityIcon.svelte";
  import { ALERT_NAMES, CYCLES, alertStates, countdown, nextReset, nextState, offerCost, periodState,
    sectionStale, stateName, steelReward, traderLocation,
    type ActivityCycle, type AlertKey, type CycleKey, type WorldAlertRule } from "./worldActivity";
  import { requestWorldNotificationPermission, retainWorldActivityScreen, saveWorldPreferences,
    worldActivityStore, worldNow, worldPreferences } from "./worldActivityStore";
  import type { InsightsViewMode } from "./viewPreferences";

  export let onOpenBounties: (region: string) => void;
  export let onOpenInsights: (mode: InsightsViewMode) => void;
  export let onOpenSettings: () => void;

  const cycleKeys = Object.keys(CYCLES) as CycleKey[];
  const alertKeys = Object.keys(ALERT_NAMES) as AlertKey[];
  let showNotifications = false;
  let notificationPanel: HTMLDivElement;
  let notificationTrigger: HTMLButtonElement | undefined;
  let notificationEvent: HTMLSelectElement;
  let editingId = "";
  let alertKey: AlertKey = "cetus";
  let alertState = "night";
  let leadMinutes: 0 | 5 = 0;
  let repeat = false;
  let saving = false;
  let message = "";
  let preferenceMessage = "";
  let baroQuery = "";
  let baroSearch: HTMLInputElement;
  $: view = $worldActivityStore.view;
  $: now = $worldNow;
  $: activeRules = $worldPreferences.rules.filter(rule => rule.enabled);
  $: baroState = periodState(view?.baro, now);
  $: resurgenceState = periodState(view?.resurgence, now);
  $: baroOffers = view?.baroOffers.filter(offer => matches(offer.displayName, offer.displayNameEn, baroQuery)) ?? [];
  $: dailyReset = nextReset(now);
  $: weeklyReset = nextReset(now, true);
  $: liveEvents = view?.events.filter(event => periodState(event, now) === "active") ?? [];
  $: sourceDelayed = !!view && ($worldActivityStore.error || view.unavailableSections.length > 0 || sectionStale(view, "", now));
  $: refreshWait = Math.max(0, Math.ceil(($worldActivityStore.manualRefreshAt - now) / 1000));

  function cycleTone(cycle: ActivityCycle | undefined, active: boolean): string {
    if (!active || !cycle) return "muted";
    if (["night", "cold", "vome"].includes(cycle.state)) return "cool";
    if (cycle.key === "duviri") return "violet";
    if (cycle.key === "zariman") return "sage";
    return cycle.key === "cambion" ? "ember" : "gold";
  }

  function matches(ru: string, en: string, query: string): boolean {
    const normalize = (value: string) => value.toLocaleLowerCase("ru").replaceAll("ё", "е");
    const name = normalize(`${ru} ${en}`);
    return normalize(query).trim().split(/\s+/).every(word => name.includes(word));
  }
  function dateLabel(value: string | number): string {
    return new Date(value).toLocaleString("ru-RU", { day: "numeric", month: "long", hour: "2-digit", minute: "2-digit" });
  }
  function ruleSummary(rule: WorldAlertRule): string {
    const what = rule.key in CYCLES || rule.key === "baro" ? `${stateName(rule.state)} · ` : "";
    return `${what}${rule.leadMinutes ? "за 5 минут" : "в момент события"} · ${rule.repeat ? "каждый раз" : "один раз"}`;
  }
  function changeKey(key: AlertKey): void {
    alertKey = key;
    alertState = key === "cetus" ? "night" : alertStates(key)[0];
  }
  async function configure(key?: AlertKey, trigger?: HTMLButtonElement): Promise<void> {
    notificationTrigger = trigger;
    if (key) {
      const existing = $worldPreferences.rules.find(rule => rule.key === key);
      if (existing) editRule(existing);
      else { editingId = ""; changeKey(key); leadMinutes = 0; repeat = false; }
    }
    message = "";
    showNotifications = true;
    await tick();
    notificationPanel?.focus({ preventScroll: true });
    notificationPanel?.scrollIntoView({ block: "nearest" });
  }
  async function closeNotifications(): Promise<void> {
    showNotifications = false;
    message = "";
    await tick();
    notificationTrigger?.focus();
  }
  function editRule(rule: WorldAlertRule): void {
    editingId = rule.id; alertKey = rule.key; alertState = rule.state;
    leadMinutes = rule.leadMinutes; repeat = rule.repeat;
    message = "";
  }
  async function saveRule(): Promise<void> {
    if (saving) return;
    saving = true;
    message = "";
    try {
      if (!await requestWorldNotificationPermission()) {
        message = "Windows не разрешила уведомления. Разрешите их для PlatScope в настройках системы.";
        return;
      }
      const duplicate = $worldPreferences.rules.find(rule => rule.key === alertKey && rule.state === alertState);
      const rule: WorldAlertRule = { id: duplicate?.id ?? (editingId || crypto.randomUUID()), key: alertKey,
        state: alertState, leadMinutes: Number(leadMinutes) as 0 | 5, repeat, enabled: true, createdAt: Date.now() };
      const rules = $worldPreferences.rules.filter(old => old.id !== rule.id && old.id !== editingId);
      if (rules.length >= 30) { message = "Можно сохранить до 30 напоминаний. Удалите ненужное."; return; }
      if (!saveWorldPreferences({ ...$worldPreferences, rules: [...rules, rule] })) {
        message = "Не удалось сохранить напоминание. Попробуйте ещё раз.";
        return;
      }
      message = editingId ? "Изменения сохранены." : "Напоминание добавлено.";
      editingId = "";
    } finally { saving = false; }
  }
  function removeRule(id: string): void {
    message = "";
    if (!saveWorldPreferences({ ...$worldPreferences, rules: $worldPreferences.rules.filter(rule => rule.id !== id) })) {
      message = "Не удалось удалить напоминание.";
      return;
    }
    if (editingId === id) editingId = "";
    message = "Напоминание удалено.";
    notificationEvent?.focus({ preventScroll: true });
  }
  async function toggleRule(rule: WorldAlertRule): Promise<void> {
    message = "";
    if (!rule.enabled && !await requestWorldNotificationPermission()) { message = "Windows не разрешила уведомления."; return; }
    if (!saveWorldPreferences({ ...$worldPreferences, rules: $worldPreferences.rules.map(old => old.id === rule.id
      ? { ...old, enabled: !old.enabled, createdAt: Date.now() } : old) })) message = "Не удалось сохранить настройку.";
    else message = rule.enabled ? "Напоминание выключено." : "Напоминание включено.";
  }
  function setStartHere(value: boolean): boolean {
    const saved = saveWorldPreferences({ ...$worldPreferences, startHere: value });
    preferenceMessage = saved ? "" : "Не удалось сохранить стартовый экран.";
    return saved;
  }
  onMount(retainWorldActivityScreen);
</script>
<svelte:window onkeydown={event => { if (event.key === "Escape" && showNotifications) void closeNotifications(); }} />

<section class="world-activity" aria-label="Сводка событий Warframe">
  <div class="world-toolbar">
    <div class="source-status" class:delayed={sourceDelayed || $worldActivityStore.error} class:pending={!view && !$worldActivityStore.error}>
      <span class="status-dot" aria-hidden="true"></span>
      {#if view}<span title={dateLabel(view.sourceAt)}>{sourceDelayed ? "Есть задержка данных" : "Данные игры"}<span class="source-time"> · {new Date(view.sourceAt).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" })}</span></span>
      {:else}<span>{$worldActivityStore.error ? "Нет соединения с источником" : "Получаем данные игры"}</span>{/if}
    </div>
    <div class="toolbar-actions">
      <button type="button" class="secondary refresh-button" disabled={$worldActivityStore.loading || refreshWait > 0}
        title={!$worldActivityStore.loading && refreshWait > 0 ? `Повторная проверка доступна через ${refreshWait} с` : "Проверить события игры"}
        onclick={() => worldActivityStore.refresh(true)}><span class="small-icon" class:spinning={$worldActivityStore.loading}><WorldActivityIcon kind="refresh" /></span>{$worldActivityStore.loading ? "Обновляем…" : "Обновить"}</button>
      <button type="button" class="secondary reminders-button" aria-expanded={showNotifications} aria-controls="world-notifications"
        onclick={event => showNotifications ? closeNotifications() : configure(undefined, event.currentTarget)}>
        <span class="small-icon"><WorldActivityIcon kind="bell" /></span>Напоминания{#if activeRules.length}<span class="count-badge">{activeRules.length}</span>{/if}
      </button>
    </div>
  </div>

  {#if showNotifications}
    <div id="world-notifications" class="world-panel notification-panel" bind:this={notificationPanel} tabindex="-1" role="region" aria-labelledby="notification-heading">
      <header class="panel-title"><div><h2 id="notification-heading">{editingId ? "Изменить напоминание" : "О чём напомнить"}</h2><p>PlatScope должен быть запущен. Свёрнутое окно не мешает напоминаниям.</p></div>
        <button type="button" class="icon-button" aria-label="Закрыть напоминания" onclick={closeNotifications}><WorldActivityIcon kind="close" /></button></header>
      <form onsubmit={event => { event.preventDefault(); void saveRule(); }}>
        <div class="notification-fields">
          <label>Событие<select bind:this={notificationEvent} value={alertKey} onchange={event => changeKey(event.currentTarget.value as AlertKey)}>
            {#each alertKeys as key}<option value={key}>{ALERT_NAMES[key]}</option>{/each}</select></label>
          {#if alertStates(alertKey).length > 1}<label>Когда<select bind:value={alertState}>
            {#each alertStates(alertKey) as state}<option value={state}>{stateName(state)}</option>{/each}</select></label>{/if}
          <label>Напомнить<select bind:value={leadMinutes}><option value={0}>В момент события</option><option value={5}>За 5 минут</option></select></label>
          <label>Повторять<select bind:value={repeat}><option value={false}>Один раз</option><option value={true}>Каждый раз</option></select></label>
        </div>
        <div class="notification-submit"><button type="submit" disabled={saving}>{saving ? "Сохраняем…" : editingId ? "Сохранить напоминание" : "Добавить напоминание"}</button>
          {#if editingId}<button type="button" class="secondary" onclick={() => { editingId = ""; message = ""; }}>Отменить изменение</button>{/if}</div>
      </form>
      {#if message}<p class="world-message" role="status">{message}</p>{/if}
      {#if $worldPreferences.rules.length}
        <ul class="notification-list">
          {#each $worldPreferences.rules as rule (rule.id)}
            <li class:rule-disabled={!rule.enabled}><div><strong>{ALERT_NAMES[rule.key]}</strong><p>{ruleSummary(rule)}</p></div>
              <div class="rule-actions"><button type="button" class="secondary" aria-pressed={rule.enabled} aria-label={`Напоминание ${ALERT_NAMES[rule.key]}: ${rule.enabled ? "включено" : "выключено"}`} onclick={() => toggleRule(rule)}>{rule.enabled ? "Включено" : "Выключено"}</button>
                <button type="button" class="secondary" aria-label={`Изменить напоминание: ${ALERT_NAMES[rule.key]}`} onclick={() => { editRule(rule); notificationEvent?.focus(); }}>Изменить</button>
                <button type="button" class="secondary" aria-label={`Удалить напоминание: ${ALERT_NAMES[rule.key]}, ${stateName(rule.state)}`} onclick={() => removeRule(rule.id)}>Удалить</button></div></li>
          {/each}
        </ul>
      {:else}<p class="notification-empty">Напоминаний пока нет. Выберите событие выше или колокольчик рядом с таймером.</p>{/if}
    </div>
  {/if}

  {#if !view}
    <div class="world-panel initial-state" role="status" aria-busy={$worldActivityStore.loading}>
      <span class="initial-icon"><WorldActivityIcon kind={$worldActivityStore.error ? "refresh" : "clock"} /></span>
      <h2>{$worldActivityStore.error ? "Не удалось получить события" : "Получаем события игры…"}</h2>
      <p>{$worldActivityStore.error ? "Проверьте подключение к интернету. Мы повторим попытку автоматически." : "Сверяем циклы локаций, визиты торговцев и расписание ротаций."}</p>
    </div>
  {:else}
    {#if $worldActivityStore.error || view.refreshFailed}
      <p class="world-warning" role="status"><strong>Источник пока не отвечает.</strong> Показываем сохранённые данные. Завершившиеся события не считаем активными.</p>
    {:else if view.unavailableSections.length || sectionStale(view, "", now)}
      <p class="world-warning" role="status"><strong>Часть данных задерживается.</strong> Отметили разделы, которые не удалось обновить.</p>
    {/if}

    <section class="destinations" aria-labelledby="destinations-heading">
      <header class="section-heading"><div><h2 id="destinations-heading">Куда отправиться</h2><p>Текущие циклы и время до их смены</p></div></header>
      <div class="cycle-grid">{#each cycleKeys as key}
        {@const cycle = view.cycles.find(cycle => cycle.key === key)}
        {@const active = periodState(cycle, now) === "active"}
        {@const next = cycle ? nextState(cycle) : null}
        <article class="cycle-card" data-tone={cycleTone(cycle, active)} aria-label={CYCLES[key].name}>
          <header class="cycle-heading"><span class="cycle-icon"><WorldActivityIcon kind={key === "cetus" && cycle?.state === "night" ? "night" : key} /></span>
            <h3>{CYCLES[key].name}</h3>
            <button type="button" class="bell" class:enabled={activeRules.some(rule => rule.key === key)} aria-label={`Напомнить: ${CYCLES[key].name}`} title="Настроить напоминание" onclick={event => configure(key, event.currentTarget)}><WorldActivityIcon kind="bell" /></button>
          </header>
          <div class="cycle-body"><strong class="cycle-phase" class:unknown={!active}>{active && cycle ? stateName(cycle.state) : cycle ? "Уточняем смену…" : "Нет данных"}</strong>
            {#if active && cycle}<div class="cycle-countdown"><span class="cycle-time" title={dateLabel(cycle.expiry)}>{countdown(cycle.expiry, now)}</span>
              {#if next}<span class="next-phase">Затем: {stateName(next).toLocaleLowerCase("ru")}</span>{/if}</div>
            {:else}<p class="cycle-unavailable">{cycle ? "Ждём подтверждение нового цикла" : "Расписание пока недоступно"}</p>{/if}
          </div>
          <footer class="cycle-footer">
            {#if sectionStale(view, key, now)}<span class="stale-note">Не удалось обновить</span>{/if}
            {#if CYCLES[key].region}<button type="button" class="text-action" aria-label={`Посмотреть заказы: ${CYCLES[key].name}`} onclick={() => onOpenBounties(CYCLES[key].region!)}>Посмотреть заказы<span class="small-icon"><WorldActivityIcon kind="arrow" /></span></button>{/if}
          </footer>
        </article>
      {/each}</div>
    </section>

    <div class="world-columns">
      <article class="world-panel resurgence-panel">
        <header class="resurgence-title"><div class="title-with-icon"><span class="vendor-icon"><WorldActivityIcon kind="resurgence" /></span>
          <div><h2>Возрождение Прайм</h2><p>Варзия · Базар Мэру</p></div></div>
          <button type="button" class="bell" class:enabled={activeRules.some(rule => rule.key === "resurgence")} aria-label="Напомнить о смене Возрождения Прайм" title="Напомнить о смене ротации" onclick={event => configure("resurgence", event.currentTarget)}><WorldActivityIcon kind="bell" /></button></header>
        {#if view.resurgence && resurgenceState === "active"}
          <div class="rotation-period"><span>Текущая ротация <b>до {new Date(view.resurgence.expiry).toLocaleDateString("ru-RU", { day: "numeric", month: "long" })}</b></span>
            <span title={dateLabel(view.resurgence.expiry)}>Осталось <b>{countdown(view.resurgence.expiry, now)}</b></span></div>
          {#if sectionStale(view, "resurgence", now)}<p class="stale-note">Сохранённая ротация · не удалось подтвердить обновление</p>{/if}
          <PrimeResurgence offers={view.resurgenceOffers} catalogAvailable={view.catalogAvailable}
            incomplete={view.resurgence.inventoryIncomplete} {onOpenSettings} />
          <div class="resurgence-actions"><button type="button" onclick={() => onOpenInsights("relics")}>Открыть мои реликвии<span class="small-icon"><WorldActivityIcon kind="arrow" /></span></button>
            <button type="button" class="secondary" onclick={() => onOpenInsights("complete_sets")}>Найти, что дособрать</button></div>
        {:else}<p class="empty-copy">{resurgenceState === "upcoming" && view.resurgence ? `Начнётся ${dateLabel(view.resurgence.activation)}.` : !view.resurgence ? "Источник пока не передал текущую ротацию Варзии." : "Ротация обновляется. Покажем товары, когда источник подтвердит новый список."}</p>{/if}
      </article>

      <aside class="world-schedule" aria-label="Торговцы и расписание">
        <article class="world-panel baro-panel">
          <header class="panel-title"><div class="title-with-icon"><span class="vendor-icon"><WorldActivityIcon kind="baro" /></span>
            <div><h2>Баро Ки’Тиир</h2><p>{baroState === "active" ? "Торговец Бездны" : baroState === "upcoming" ? "Следующий визит" : "Ждём расписание"}</p></div></div>
            <button type="button" class="bell" class:enabled={activeRules.some(rule => rule.key === "baro")} aria-label="Напомнить о Баро" title="Напомнить о Баро" onclick={event => configure("baro", event.currentTarget)}><WorldActivityIcon kind="bell" /></button>
          </header>
          {#if view.baro && (baroState === "active" || baroState === "upcoming")}
            <div class="baro-location">{#if baroState === "active"}<span class="availability" class:unconfirmed={sectionStale(view, "baro", now)}>{sectionStale(view, "baro", now) ? "По сохранённым данным" : "Сейчас в реле"}</span>{/if}
              <strong>{traderLocation(view.baro.location)}</strong></div>
            <div class="baro-countdown"><span>{baroState === "active" ? "Улетит через" : "Прибудет через"}</span><strong title={dateLabel(baroState === "active" ? view.baro.expiry : view.baro.activation)}>{countdown(baroState === "active" ? view.baro.expiry : view.baro.activation, now)}</strong></div>
            {#if sectionStale(view, "baro", now)}<p class="stale-note">Сохранённое расписание · не удалось подтвердить обновление</p>{/if}
            {#if baroState === "active"}
              <details class="offer-details"><summary><span>Товары Баро <span class="count-badge">{view.baroOffers.length}</span></span><span class="disclosure-icon"><WorldActivityIcon kind="chevron" /></span></summary>
                <label class="offer-search">Найти товар<input type="search" bind:this={baroSearch} bind:value={baroQuery} placeholder="Например, Поток Прайм" /></label>
                {#if view.baro.inventoryIncomplete}<p class="stale-note">Источник передал неполный список товаров.</p>{/if}
                {#if baroQuery}<p class="search-result" role="status">Найдено: {baroOffers.length} из {view.baroOffers.length}</p>{/if}
                <ul class="offer-list">{#each baroOffers as offer}
                  <li><div><strong>{offer.displayName}</strong>{#if offer.masteryRef}<MasteryBadge gameRef={offer.masteryRef} />{/if}</div><span>{offerCost(offer, false)}</span></li>
                {:else}<li class="offer-empty"><p>{baroQuery ? "Ничего не найдено. Попробуйте другое название." : "Баро уже прибыл, но источник ещё не передал товары."}</p>
                  {#if baroQuery}<button type="button" class="text-action" onclick={() => { baroQuery = ""; baroSearch?.focus(); }}>Сбросить поиск</button>{/if}</li>{/each}</ul>
              </details>
              <div class="baro-action"><button type="button" class="secondary" onclick={() => onOpenInsights("resources")}>Оценить обмен ресурсов<span class="small-icon"><WorldActivityIcon kind="arrow" /></span></button></div>
            {:else}<p class="empty-copy">Ассортимент появится после прибытия.</p>{/if}
          {:else}<p class="empty-copy">{view.baro ? "Предыдущий визит закончился. Уточняем следующий." : "Источник пока не передал расписание Баро."}</p>{/if}
        </article>

        <article class="world-panel teshin-panel"><header class="panel-title"><div class="title-with-icon"><span class="vendor-icon teshin-icon"><WorldActivityIcon kind="teshin" /></span><div><h2>Тешин</h2><p>Товар недели · Стальной Путь</p></div></div></header>
          {#if view.steelPath && periodState(view.steelPath, now) === "active"}
            <strong class="teshin-reward">{steelReward(view.steelPath.reward)}</strong><p class="teshin-cost">{view.steelPath.cost} стальной эссенции</p>
            <p class="schedule-change" title={dateLabel(view.steelPath.expiry)}>Смена через <b>{countdown(view.steelPath.expiry, now)}</b></p>
            {#if sectionStale(view, "steel_path", now)}<p class="stale-note">Сохранённая ротация · не удалось подтвердить обновление</p>{/if}
          {:else}<p class="empty-copy">{view.steelPath ? "Уточняем новую ротацию" : "Источник пока не передал товар недели"}</p>{/if}
        </article>

        <section class="world-panel resets-panel" aria-labelledby="resets-heading">
          <header class="schedule-heading"><h2 id="resets-heading">Обновления и сбросы</h2></header>
          {#each [{ key: "daily" as const, title: "Ежедневный сброс", hint: "Лимиты и ежедневный вход", at: dailyReset },
            { key: "weekly" as const, title: "Еженедельный сброс", hint: "Недельные активности", at: weeklyReset },
            { key: "sortie" as const, title: "Новая вылазка", hint: "Смена трёх миссий", at: view.sortie ? Date.parse(view.sortie.expiry) : null }] as reset}
            <div class="reset-cell"><div><h3>{reset.title}</h3><p>{reset.hint}</p>
              {#if reset.key === "sortie" && sectionStale(view, "sortie", now)}<p class="stale-note">Не удалось обновить</p>{/if}</div>
              <div class="reset-time"><strong title={reset.at ? dateLabel(reset.at) : undefined}>{countdown(reset.at, now)}</strong>
                <button type="button" class="bell" class:enabled={activeRules.some(rule => rule.key === reset.key)} aria-label={`Напомнить: ${reset.title}`} title="Настроить напоминание" onclick={event => configure(reset.key, event.currentTarget)}><WorldActivityIcon kind="bell" /></button></div></div>
          {/each}
        </section>

        {#if liveEvents.length}
          <section class="world-panel live-events" aria-labelledby="events-heading"><header class="panel-title"><div class="title-with-icon"><span class="small-icon"><WorldActivityIcon kind="events" /></span><h2 id="events-heading">События игры</h2></div><span class="count-badge">{liveEvents.length}</span></header>
            {#each liveEvents as event}<div class="event-row"><strong>{event.name}</strong><span title={dateLabel(event.expiry)}>До конца {countdown(event.expiry, now)}</span></div>{/each}
            {#if sectionStale(view, "events", now)}<p class="stale-note">Сохранённый список событий · не удалось обновить</p>{/if}
          </section>
        {/if}
      </aside>
    </div>
  {/if}

  <footer class="world-footer"><label class="start-preference"><input type="checkbox" checked={$worldPreferences.startHere}
    onchange={event => { if (!setStartHere(event.currentTarget.checked)) event.currentTarget.checked = $worldPreferences.startHere; }} />Открывать «Сейчас в игре» при запуске</label><span>Таймеры обновляются автоматически</span>
    {#if preferenceMessage}<p class="preference-message" role="status">{preferenceMessage}</p>{/if}</footer>
</section>

<style>
  .world-activity { container-type:inline-size; display:grid; gap:1.35rem; min-width:0; }
  h2,h3,p { margin:0; }
  h2 { font-size:1.08rem; line-height:1.35; letter-spacing:-.015em; }
  h3 { font-size:.875rem; line-height:1.4; }
  p { font-size:.8125rem; line-height:1.5; color:var(--text-muted); }
  button { font-size:.8125rem; }
  .small-icon { display:inline-flex; flex-shrink:0; width:1.15rem; height:1.15rem; }
  .world-toolbar,.toolbar-actions,.panel-title,.title-with-icon,.resurgence-title { display:flex; align-items:center; gap:.75rem; }
  .world-toolbar,.panel-title,.resurgence-title { justify-content:space-between; }
  .world-toolbar { min-height:2.3rem; flex-wrap:wrap; gap:.6rem 1rem; }
  .source-status { display:flex; align-items:center; gap:.5rem; color:var(--text-muted); font-size:.8125rem; }
  .status-dot { flex:none; width:.4rem; height:.4rem; border-radius:50%; background:var(--success); }
  .delayed .status-dot { background:var(--accent); }
  .pending .status-dot { background:var(--text-subtle); }
  .source-time { color:var(--text-subtle); font-variant-numeric:tabular-nums; }
  .toolbar-actions { flex-wrap:wrap; gap:.5rem; }
  .toolbar-actions button,.resurgence-actions button,.baro-action button { display:inline-flex; align-items:center; justify-content:center; gap:.5rem; min-height:2.4rem; padding:.45rem .8rem; }
  .refresh-button { border-color:transparent; }
  .refresh-button:disabled { cursor:default; }
  .reminders-button { background:var(--surface-1); border-color:var(--border); }
  .reminders-button[aria-expanded="true"] { background:var(--accent-soft); border-color:var(--accent); }
  .count-badge { display:inline-flex; align-items:center; justify-content:center; min-width:1.4rem; min-height:1.4rem; padding:.05rem .4rem; border-radius:.4rem; background:var(--surface-3); color:var(--text-muted); font-size:.75rem; font-weight:650; font-variant-numeric:tabular-nums; }
  .section-heading { margin-bottom:.8rem; }
  .section-heading h2 { font-size:1.15rem; }
  .section-heading p { margin-top:.2rem; }
  .cycle-grid { display:grid; grid-template-columns:repeat(5,minmax(0,1fr)); gap:.7rem; }
  .cycle-card { --cycle-color:var(--text-muted); --cycle-tint:var(--surface-2); display:flex; flex-direction:column; min-width:0; border:1px solid var(--border); border-radius:.85rem; padding:1rem; background:linear-gradient(155deg,var(--cycle-tint),var(--surface-1) 75%); box-shadow:0 2px 3px oklch(.3 .02 60 / .025); }
  .cycle-card[data-tone="gold"] { --cycle-color:oklch(.44 .095 65); --cycle-tint:oklch(.946 .038 85); }
  .cycle-card[data-tone="cool"] { --cycle-color:oklch(.43 .062 230); --cycle-tint:oklch(.944 .023 220); }
  .cycle-card[data-tone="ember"] { --cycle-color:oklch(.46 .1 40); --cycle-tint:oklch(.945 .029 48); }
  .cycle-card[data-tone="sage"] { --cycle-color:oklch(.42 .056 160); --cycle-tint:oklch(.941 .025 145); }
  .cycle-card[data-tone="violet"] { --cycle-color:oklch(.45 .067 310); --cycle-tint:oklch(.945 .021 306); }
  .cycle-heading { display:grid; grid-template-columns:1.7rem minmax(0,1fr) auto; align-items:center; gap:.45rem; min-height:2.5rem; }
  .cycle-heading h3 { font-size:.8125rem; line-height:1.35; text-wrap:balance; }
  .cycle-icon { display:inline-flex; width:1.7rem; height:1.7rem; color:var(--cycle-color); }
  .cycle-body { padding-top:.95rem; }
  .cycle-phase { display:block; color:var(--cycle-color); font-size:1.02rem; font-weight:650; }
  .cycle-phase.unknown { color:var(--text-muted); font-size:.875rem; }
  .cycle-countdown { display:grid; gap:.22rem; margin-top:.4rem; }
  .cycle-time { font-size:clamp(1.4rem,1.65cqw,1.8rem); font-variant-numeric:tabular-nums; font-weight:650; line-height:1.25; letter-spacing:-.035em; white-space:nowrap; }
  .next-phase { font-size:.75rem; color:var(--text-muted); }
  .cycle-unavailable { min-height:3.15rem; margin-top:.5rem; font-size:.75rem; }
  .cycle-footer { display:flex; flex-wrap:wrap; align-items:center; gap:.3rem; min-height:2.25rem; margin-top:auto; padding-top:.75rem; }
  .cycle-footer .stale-note { flex-basis:100%; }
  .text-action { display:inline-flex; align-items:center; justify-content:space-between; gap:.4rem; padding:0; min-height:1.65rem; background:none; border:0; border-radius:.2rem; color:var(--accent-strong); font-size:.75rem; font-weight:650; text-align:left; box-shadow:none; }
  .text-action:hover { background:none; text-decoration:underline; text-underline-offset:3px; color:var(--accent); }
  .cycle-footer .text-action { width:100%; }
  .cycle-footer .small-icon { width:.95rem; height:.95rem; }
  .bell,.icon-button { display:inline-flex; align-items:center; justify-content:center; flex:none; width:2.15rem; height:2.15rem; min-height:2.15rem; padding:.4rem; border:1px solid transparent; border-radius:.5rem; color:var(--text-muted); background:transparent; }
  .bell :global(svg),.icon-button :global(svg) { width:1.2rem; height:1.2rem; }
  .cycle-heading .bell { margin-right:-.45rem; }
  .bell:hover,.icon-button:hover { background:var(--surface-3); border-color:var(--border); color:var(--accent-strong); }
  .bell.enabled { color:var(--accent-strong); background:var(--accent-soft); border-color:var(--border); }
  .world-columns { display:grid; grid-template-columns:minmax(0,1fr) minmax(20rem,.49fr); align-items:start; gap:1.1rem; }
  .world-panel { min-width:0; background:var(--surface-1); border:1px solid var(--border); border-radius:.85rem; box-shadow:var(--shadow-sm); }
  .resurgence-panel { padding:1.3rem; border-top:3px solid var(--gold); }
  .resurgence-title h2 { font-size:1.25rem; }
  .title-with-icon { min-width:0; gap:.7rem; }
  .title-with-icon > div { min-width:0; }
  .title-with-icon p { margin-top:.2rem; }
  .vendor-icon { display:inline-flex; flex:none; width:2.35rem; height:2.35rem; padding:.3rem; color:var(--accent-strong); background:var(--accent-soft); border-radius:.65rem; }
  .resurgence-title .vendor-icon { width:2.75rem; height:2.75rem; background:var(--surface-2); color:var(--gold); }
  .rotation-period { display:flex; align-items:baseline; flex-wrap:wrap; justify-content:space-between; gap:.3rem .75rem; margin-top:1rem; padding-bottom:1rem; border-bottom:1px solid var(--border); color:var(--text-muted); font-size:.8125rem; }
  .rotation-period b { font-weight:600; color:var(--text); font-variant-numeric:tabular-nums; }
  .resurgence-actions { display:flex; flex-wrap:wrap; gap:.6rem; border-top:1px solid var(--border); padding-top:1rem; margin-top:1.25rem; }
  .world-schedule { display:grid; align-content:start; gap:1rem; min-width:0; }
  .baro-panel,.teshin-panel,.live-events { padding:1.1rem 1.15rem; }
  .panel-title { align-items:flex-start; margin-bottom:1rem; }
  .baro-location { display:grid; justify-items:start; gap:.6rem; font-size:.875rem; }
  .availability { display:inline-flex; align-items:center; gap:.35rem; padding:.18rem .5rem; border-radius:.35rem; background:var(--success-soft); color:var(--success); font-size:.75rem; font-weight:650; }
  .availability::before { content:""; width:.3rem; height:.3rem; background:currentColor; border-radius:50%; }
  .availability.unconfirmed { color:var(--accent-strong); background:var(--accent-soft); }
  .baro-countdown { display:flex; flex-wrap:wrap; align-items:baseline; justify-content:space-between; gap:.3rem .75rem; margin-top:.8rem; }
  .baro-countdown > span { color:var(--text-muted); font-size:.8125rem; }
  .baro-countdown strong { font-size:1.35rem; font-weight:650; font-variant-numeric:tabular-nums; letter-spacing:-.025em; }
  .offer-details { margin-top:1rem; border-top:1px solid var(--border); border-bottom:1px solid var(--border); }
  summary { cursor:pointer; font-size:.8125rem; font-weight:650; color:var(--accent-strong); }
  .offer-details summary { display:flex; justify-content:space-between; align-items:center; gap:.5rem; min-height:2.9rem; padding:.5rem 0; list-style:none; }
  .offer-details summary::-webkit-details-marker { display:none; }
  .offer-details summary > span:first-child { display:flex; align-items:center; gap:.5rem; }
  .disclosure-icon { display:inline-flex; width:1rem; height:1rem; }
  .offer-details[open] .disclosure-icon { transform:rotate(90deg); }
  .offer-details summary:hover { color:var(--accent); }
  .offer-search { display:grid; gap:.35rem; margin:.35rem 0 .65rem; font-size:.75rem; color:var(--text-muted); }
  input[type="search"],select { min-width:0; width:100%; min-height:2.5rem; background:var(--surface-1); border:1px solid var(--border-strong); border-radius:.45rem; padding:.5rem .6rem; color:var(--text); font-size:.8125rem; }
  .search-result { font-size:.75rem; }
  .offer-list { padding:0; margin:.3rem 0 .6rem; list-style:none; max-height:23rem; overflow:auto; overscroll-behavior:contain; scrollbar-width:thin; scrollbar-gutter:stable; }
  .offer-list li { display:grid; gap:.3rem; padding:.7rem .3rem .7rem 0; font-size:.8125rem; border-bottom:1px solid var(--border); }
  .offer-list li:last-child { border-bottom:0; }
  .offer-list li > div { min-width:0; overflow-wrap:anywhere; }
  .offer-list li strong { font-weight:600; }
  .offer-list li > span { color:var(--text-muted); font-size:.75rem; }
  .offer-empty { justify-items:start; }
  .baro-action { margin-top:.9rem; }
  .baro-action button { width:100%; justify-content:space-between; }
  .teshin-icon { background:var(--surface-3); color:var(--text-muted); }
  .teshin-reward { display:block; font-size:1rem; font-weight:650; line-height:1.4; }
  .teshin-cost { margin-top:.25rem; }
  .schedule-change { display:flex; flex-wrap:wrap; justify-content:space-between; gap:.35rem .6rem; margin-top:.85rem; padding-top:.75rem; border-top:1px solid var(--border); }
  .schedule-change b { color:var(--text); font-weight:600; font-variant-numeric:tabular-nums; }
  .schedule-heading { padding:1.1rem 1.15rem .8rem; }
  .schedule-heading h2 { font-size:1rem; }
  .reset-cell { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:.35rem .5rem; padding:.8rem 1.15rem; border-top:1px solid var(--border); }
  .reset-cell h3 { font-size:.8125rem; font-weight:600; }
  .reset-cell p { font-size:.75rem; margin-top:.2rem; }
  .reset-time { display:flex; align-items:center; gap:.35rem; margin-left:auto; }
  .reset-time strong { font-size:.875rem; font-variant-numeric:tabular-nums; font-weight:650; }
  .reset-time .bell { margin-right:-.35rem; }
  .live-events .panel-title { margin-bottom:.2rem; }
  .live-events h2 { font-size:1rem; }
  .live-events .small-icon { color:var(--accent-strong); }
  .event-row { display:grid; gap:.35rem; padding-top:.8rem; font-size:.875rem; }
  .event-row + .event-row { border-top:1px solid var(--border); margin-top:.8rem; }
  .event-row span { color:var(--text-muted); font-size:.8125rem; }
  .empty-copy { margin-top:.7rem; }
  .stale-note { color:var(--accent-strong); font-size:.75rem; line-height:1.45; }
  p.stale-note { margin-top:.55rem; }
  .world-warning,.world-message { padding:.85rem 1rem; border-radius:.65rem; border:1px solid var(--border); background:var(--surface-2); font-size:.8125rem; line-height:1.5; }
  .world-warning { background:var(--accent-soft); color:var(--accent-strong); }
  .world-warning strong { font-weight:650; }
  .notification-panel { padding:1.2rem; border-color:var(--border-strong); }
  .notification-panel:focus { outline:2px solid var(--accent); outline-offset:3px; }
  .notification-fields { display:grid; grid-template-columns:1.4fr 1fr 1fr 1fr; gap:.75rem; }
  .notification-fields label { display:grid; align-content:start; gap:.4rem; min-width:0; color:var(--text-muted); font-size:.8125rem; }
  .notification-submit { display:flex; flex-wrap:wrap; gap:.5rem; margin-top:.85rem; }
  .notification-submit button { min-height:2.4rem; }
  .notification-panel .world-message { margin-top:.85rem; }
  .notification-list { list-style:none; margin:1rem 0 0; padding:0; }
  .notification-list li { display:flex; flex-wrap:wrap; justify-content:space-between; gap:.65rem 1rem; align-items:center; padding:.9rem 0; border-top:1px solid var(--border); font-size:.875rem; }
  .notification-list li:last-child { padding-bottom:0; }
  .notification-list p { margin-top:.2rem; }
  .rule-disabled > div:first-child { color:var(--text-muted); }
  .rule-actions { display:flex; gap:.35rem; flex-wrap:wrap; }
  .rule-actions button { padding:.4rem .65rem; }
  .rule-actions button[aria-pressed="true"] { color:var(--success); background:var(--success-soft); }
  .notification-empty { margin-top:1rem; padding-top:.8rem; border-top:1px solid var(--border); }
  .initial-state { display:grid; justify-items:center; gap:.8rem; text-align:center; padding:4.5rem 1.5rem; }
  .initial-icon { display:flex; width:3rem; height:3rem; color:var(--accent); padding:.4rem; background:var(--accent-soft); border-radius:.8rem; }
  .initial-state p { max-width:48ch; }
  .world-footer { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:.6rem 1rem; padding:.4rem .15rem; font-size:.75rem; color:var(--text-subtle); }
  .start-preference { display:flex; align-items:center; gap:.5rem; cursor:pointer; color:var(--text-muted); }
  .start-preference input { width:1rem; height:1rem; margin:0; accent-color:var(--accent); }
  .preference-message { flex-basis:100%; color:var(--danger); }
  @container (max-width:70rem) {
    .world-columns { grid-template-columns:minmax(0,1fr) minmax(18rem,.55fr); }
    .cycle-card { padding:.8rem; }
    .cycle-heading { grid-template-columns:1.4rem minmax(0,1fr); gap:.4rem; position:relative; padding-right:1.55rem; align-items:start; }
    .cycle-icon { width:1.4rem; height:1.4rem; }
    .cycle-heading .bell { position:absolute; right:-.35rem; top:-.4rem; margin:0; }
    .cycle-heading h3 { font-size:.8125rem; }
    .resurgence-panel { padding:1.1rem; }
    .baro-panel,.teshin-panel,.live-events { padding:1rem; }
    .reset-cell { padding:.75rem 1rem; }
    .reset-time { flex-basis:100%; justify-content:space-between; }
    .schedule-heading { padding:1rem 1rem .8rem; }
  }
  @container (max-width:54rem) {
    .cycle-grid { grid-template-columns:repeat(6,minmax(0,1fr)); }
    .cycle-card { grid-column:span 2; }
    .cycle-card:nth-child(n+4) { grid-column:span 3; }
    .cycle-heading { min-height:2.3rem; }
    .cycle-body { padding-top:.65rem; }
    .world-columns { grid-template-columns:minmax(0,1fr); }
    .world-schedule { grid-template-columns:repeat(2,minmax(0,1fr)); align-items:start; }
    .notification-fields { grid-template-columns:repeat(2,minmax(0,1fr)); }
  }
  @container (max-width:34rem) {
    .cycle-grid { grid-template-columns:repeat(2,minmax(0,1fr)); }
    .cycle-card,.cycle-card:nth-child(n+4) { grid-column:auto; }
    .cycle-card:last-child { grid-column:1/-1; }
    .world-schedule { grid-template-columns:minmax(0,1fr); }
    .notification-fields { grid-template-columns:minmax(0,1fr); }
    .world-toolbar { align-items:flex-start; }
    .toolbar-actions { width:100%; }
    .reset-time { flex-basis:auto; }
  }
  @media (prefers-reduced-motion:no-preference) {
    .spinning { animation:world-spin 1.5s linear infinite; }
    @keyframes world-spin { to { transform:rotate(360deg); } }
  }
</style>
