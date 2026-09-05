<script lang="ts">
  import { onMount, tick } from "svelte";
  import MasteryBadge from "./MasteryBadge.svelte";
  import WorldActivityIcon from "./WorldActivityIcon.svelte";
  import { ALERT_NAMES, CYCLES, alertStates, countdown, nextReset, nextState, offerCost, periodState,
    sectionStale, stateName, steelReward, traderLocation,
    type AlertKey, type CycleKey, type WorldAlertRule } from "./worldActivity";
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
  let editingId = "";
  let alertKey: AlertKey = "cetus";
  let alertState = "night";
  let leadMinutes: 0 | 5 = 0;
  let repeat = false;
  let saving = false;
  let message = "";
  let baroQuery = "";
  let resurgenceQuery = "";
  $: view = $worldActivityStore.view;
  $: now = $worldNow;
  $: activeRules = $worldPreferences.rules.filter(rule => rule.enabled);
  $: baroState = periodState(view?.baro, now);
  $: resurgenceState = periodState(view?.resurgence, now);
  $: baroOffers = view?.baroOffers.filter(offer => matches(offer.displayName, offer.displayNameEn, baroQuery)) ?? [];
  $: resurgenceOffers = view?.resurgenceOffers.filter(offer => matches(offer.displayName, offer.displayNameEn, resurgenceQuery)) ?? [];
  $: equipment = view?.resurgenceOffers.filter(offer => offer.kind === "equipment") ?? [];
  $: dailyReset = nextReset(now);
  $: weeklyReset = nextReset(now, true);

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
  async function configure(key?: AlertKey): Promise<void> {
    if (key) {
      const existing = $worldPreferences.rules.find(rule => rule.key === key);
      if (existing) editRule(existing);
      else { editingId = ""; changeKey(key); leadMinutes = 0; repeat = false; }
    }
    message = "";
    showNotifications = true;
    await tick();
    notificationPanel?.focus({ preventScroll: true });
    notificationPanel?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }
  function editRule(rule: WorldAlertRule): void {
    editingId = rule.id; alertKey = rule.key; alertState = rule.state;
    leadMinutes = rule.leadMinutes; repeat = rule.repeat;
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
      editingId = "";
      message = "Напоминание включено. PlatScope должен оставаться запущенным — можно свернуть окно.";
    } finally { saving = false; }
  }
  function removeRule(id: string): void {
    if (!saveWorldPreferences({ ...$worldPreferences, rules: $worldPreferences.rules.filter(rule => rule.id !== id) })) {
      message = "Не удалось удалить напоминание.";
    }
    if (editingId === id) editingId = "";
  }
  async function toggleRule(rule: WorldAlertRule): Promise<void> {
    if (!rule.enabled && !await requestWorldNotificationPermission()) { message = "Windows не разрешила уведомления."; return; }
    if (!saveWorldPreferences({ ...$worldPreferences, rules: $worldPreferences.rules.map(old => old.id === rule.id
      ? { ...old, enabled: !old.enabled, createdAt: Date.now() } : old) })) message = "Не удалось сохранить настройку.";
  }
  function setStartHere(value: boolean): void {
    if (!saveWorldPreferences({ ...$worldPreferences, startHere: value })) message = "Не удалось сохранить стартовый экран.";
  }
  onMount(retainWorldActivityScreen);
</script>

<section class="world-activity" aria-label="Сводка событий Warframe">
  <div class="world-toolbar">
    <label class="start-preference"><input type="checkbox" checked={$worldPreferences.startHere}
      onchange={event => setStartHere(event.currentTarget.checked)} />Открывать при запуске</label>
    <div class="toolbar-actions">
      <button type="button" class="secondary" aria-expanded={showNotifications} onclick={() => showNotifications ? showNotifications = false : configure()}>
        <span class="small-icon"><WorldActivityIcon kind="bell" /></span>Напоминания{activeRules.length ? ` · ${activeRules.length}` : ""}
      </button>
      <button type="button" class="secondary" disabled={$worldActivityStore.loading || now < $worldActivityStore.manualRefreshAt}
        title={!$worldActivityStore.loading && now < $worldActivityStore.manualRefreshAt ? `Повторная проверка доступна через ${Math.ceil(($worldActivityStore.manualRefreshAt - now) / 1000)} с` : "Проверить события игры"}
        onclick={() => worldActivityStore.refresh(true)}>{$worldActivityStore.loading ? "Обновляем…" : "Обновить"}</button>
    </div>
  </div>

  {#if showNotifications}
    <div class="world-panel notification-panel" bind:this={notificationPanel} tabindex="-1">
      <header class="panel-title"><div><h2>О чём напомнить</h2><p>Работает, пока PlatScope запущен, в том числе в свёрнутом окне.</p></div>
        <button type="button" class="secondary" onclick={() => showNotifications = false}>Закрыть</button></header>
      <form onsubmit={event => { event.preventDefault(); void saveRule(); }}>
        <label>Событие<select value={alertKey} onchange={event => changeKey(event.currentTarget.value as AlertKey)}>
          {#each alertKeys as key}<option value={key}>{ALERT_NAMES[key]}</option>{/each}</select></label>
        {#if alertStates(alertKey).length > 1}<label>Когда<select bind:value={alertState}>
          {#each alertStates(alertKey) as state}<option value={state}>{stateName(state)}</option>{/each}</select></label>{/if}
        <label>Напомнить<select bind:value={leadMinutes}><option value={0}>В момент события</option><option value={5}>За 5 минут</option></select></label>
        <label>Повторять<select bind:value={repeat}><option value={false}>Один раз</option><option value={true}>Каждый раз</option></select></label>
        <button type="submit" disabled={saving}>{saving ? "Сохраняем…" : editingId ? "Сохранить напоминание" : "Добавить напоминание"}</button>
        {#if editingId}<button type="button" class="secondary" onclick={() => editingId = ""}>Отменить</button>{/if}
      </form>
      {#if $worldPreferences.rules.length}
        <ul class="notification-list">
          {#each $worldPreferences.rules as rule (rule.id)}
            <li><div><strong>{ALERT_NAMES[rule.key]}</strong><p>{ruleSummary(rule)}</p></div>
              <div class="rule-actions"><button type="button" class="secondary" aria-pressed={rule.enabled} onclick={() => toggleRule(rule)}>{rule.enabled ? "Включено" : "Выключено"}</button>
                <button type="button" class="secondary" onclick={() => editRule(rule)}>Изменить</button>
                <button type="button" class="secondary" aria-label={`Удалить напоминание: ${ALERT_NAMES[rule.key]}, ${stateName(rule.state)}`} onclick={() => removeRule(rule.id)}>Удалить</button></div></li>
          {/each}
        </ul>
      {:else}<p class="subtle">Напоминаний пока нет. Выберите событие выше или нажмите колокольчик рядом с нужным таймером.</p>{/if}
    </div>
  {/if}
  {#if message}<p class="world-message" role="status">{message}</p>{/if}

  {#if !view}
    <div class="world-panel initial-state" role="status">
      <span class="large-icon"><WorldActivityIcon kind="clock" /></span>
      <h2>{$worldActivityStore.error ? "Не удалось получить события" : "Получаем события игры…"}</h2>
      <p>{$worldActivityStore.error ? "Проверьте подключение. Приложение повторит попытку; инвентарь и рынок доступны как обычно." : "Сверяем циклы локаций и расписание торговцев."}</p>
    </div>
  {:else}
    {#if $worldActivityStore.error || view.refreshFailed}
      <p class="world-warning" role="status">Источник пока не отвечает. Показываем сохранённые данные; завершившиеся события не считаем активными.</p>
    {:else if view.unavailableSections.length || sectionStale(view, "", now)}
      <p class="world-warning" role="status">Часть данных задерживается. Такие разделы отмечены отдельно — остальные продолжают работать.</p>
    {/if}
    <div class="world-columns">
      <div class="world-events">
        <article class="world-panel vendor-card">
          <header class="panel-title">
            <div class="title-with-icon"><span class="large-icon gold"><WorldActivityIcon kind="baro" /></span>
              <div><h2>Баро Ки’Тиир</h2><p>{baroState === "active" ? "Сейчас в реле" : baroState === "upcoming" ? "Следующий визит" : "Ждём расписание"}</p></div></div>
            <button type="button" class="bell" class:enabled={activeRules.some(rule => rule.key === "baro")} aria-label="Напомнить о Баро" title="Напомнить о Баро" onclick={() => configure("baro")}><WorldActivityIcon kind="bell" /></button>
          </header>
          {#if view.baro && (baroState === "active" || baroState === "upcoming")}
            <div class="vendor-status"><strong>{traderLocation(view.baro.location)}</strong>
              <span title={dateLabel(baroState === "active" ? view.baro.expiry : view.baro.activation)}>{baroState === "active" ? "Улетит" : "Прибудет"} через <b>{countdown(baroState === "active" ? view.baro.expiry : view.baro.activation, now)}</b></span></div>
            {#if sectionStale(view, "baro", now)}<p class="stale-note">Сохранённое расписание · не удалось подтвердить обновление</p>{/if}
            {#if baroState === "active"}
              <div class="card-actions"><button type="button" class="secondary" onclick={() => onOpenInsights("resources")}>Оценить обмен ресурсов</button></div>
              <details class="offer-details"><summary>Товары Баро · {view.baroOffers.length}</summary>
                <label class="offer-search">Найти товар<input type="search" bind:value={baroQuery} placeholder="Например, Поток Прайм" /></label>
                {#if view.baro.inventoryIncomplete}<p class="stale-note">Источник передал неполный список товаров.</p>{/if}
                <ul class="offer-list">{#each baroOffers as offer}
                  <li><div><strong>{offer.displayName}</strong>{#if offer.masteryRef}<MasteryBadge gameRef={offer.masteryRef} />{/if}</div><span>{offerCost(offer, false)}</span></li>
                {:else}<li>{baroQuery ? "Ничего не найдено. Попробуйте другое название." : "Баро уже прибыл, но источник ещё не передал товары."}</li>{/each}</ul>
              </details>
            {:else}<p class="subtle">Ассортимент появится после прибытия.</p>{/if}
          {:else}<p class="subtle">{view.baro ? "Предыдущий визит закончился. Уточняем следующий." : "Источник пока не передал расписание Баро."}</p>{/if}
        </article>

        <article class="world-panel vendor-card">
          <header class="panel-title"><div class="title-with-icon"><span class="large-icon gold"><WorldActivityIcon kind="resurgence" /></span>
            <div><h2>Возрождение Прайм</h2><p>Варзия · Базар Мэру</p></div></div>
            <button type="button" class="bell" class:enabled={activeRules.some(rule => rule.key === "resurgence")} aria-label="Напомнить о смене Возрождения Прайм" title="Напомнить о смене ротации" onclick={() => configure("resurgence")}><WorldActivityIcon kind="bell" /></button></header>
          {#if view.resurgence && resurgenceState === "active"}
            <div class="vendor-status"><span>Ротация сменится <b>{new Date(view.resurgence.expiry).toLocaleDateString("ru-RU", { day: "numeric", month: "long" })}</b></span>
              <span title={dateLabel(view.resurgence.expiry)}>Через <b>{countdown(view.resurgence.expiry, now)}</b></span></div>
            {#if sectionStale(view, "resurgence", now)}<p class="stale-note">Сохранённая ротация · не удалось подтвердить обновление</p>{/if}
            {#if equipment.length}<ul class="prime-equipment">{#each equipment as offer}
              <li><strong>{offer.displayName}</strong>{#if offer.masteryRef}<MasteryBadge gameRef={offer.masteryRef} />{/if}</li>
            {/each}</ul>{/if}
            {#if !view.catalogAvailable}<p class="subtle">Для игровых названий и отметок освоения нужен справочник предметов. <button type="button" class="text-action" onclick={onOpenSettings}>Открыть настройки</button></p>{/if}
            <div class="card-actions"><button type="button" class="secondary" onclick={() => onOpenInsights("relics")}>Открыть мои реликвии</button>
              <button type="button" class="secondary" onclick={() => onOpenInsights("complete_sets")}>Найти, что дособрать</button></div>
            <details class="offer-details"><summary>Реликвии и товары Варзии · {view.resurgenceOffers.length}</summary>
              <p class="subtle">Реликвии — за Ая. Готовое снаряжение и украшения — за Королевскую Ая.</p>
              <label class="offer-search">Найти товар<input type="search" bind:value={resurgenceQuery} placeholder="Реликвия или предмет" /></label>
              {#if view.resurgence.inventoryIncomplete}<p class="stale-note">Источник передал неполный список товаров.</p>{/if}
              <ul class="offer-list">{#each resurgenceOffers as offer}<li><strong>{offer.displayName}</strong><span>{offerCost(offer, true)}</span></li>
                {:else}<li>{resurgenceQuery ? "Ничего не найдено. Попробуйте другое название." : "Источник ещё не передал товары этой ротации."}</li>{/each}</ul>
            </details>
          {:else}<p class="subtle">{resurgenceState === "upcoming" && view.resurgence ? `Начнётся ${dateLabel(view.resurgence.activation)}.` : !view.resurgence ? "Источник пока не передал текущую ротацию Варзии." : "Ротация обновляется. Покажем товары, когда источник подтвердит новый список."}</p>{/if}
        </article>

          <article class="world-panel teshin-card"><header class="panel-title"><div><h2>Тешин · товар недели</h2>
            <p>{view.steelPath && periodState(view.steelPath, now) === "active" ? `Смена через ${countdown(view.steelPath.expiry, now)}` : view.steelPath ? "Уточняем новую ротацию" : "Источник пока не передал товар недели"}</p></div></header>
            {#if view.steelPath && periodState(view.steelPath, now) === "active"}<div class="vendor-status"><strong>{steelReward(view.steelPath.reward)}</strong><span>{view.steelPath.cost} стальной эссенции</span></div>
              {#if sectionStale(view, "steel_path", now)}<p class="stale-note">Сохранённая ротация · не удалось подтвердить обновление</p>{/if}
            {/if}</article>
      </div>

      <aside class="world-panel cycles-panel" aria-label="Циклы локаций">
        <header class="panel-title"><div><h2>Куда отправиться</h2><p>Состояние локаций прямо сейчас</p></div></header>
        <div class="cycle-list">{#each cycleKeys as key}
          {@const cycle = view.cycles.find(cycle => cycle.key === key)}
          {@const active = periodState(cycle, now) === "active"}
          {@const next = cycle ? nextState(cycle) : null}
          <article class="cycle-row">
            <span class="cycle-icon" class:phase-night={cycle?.state === "night" || cycle?.state === "cold"}><WorldActivityIcon kind={key} /></span>
            <div class="cycle-copy"><h3>{CYCLES[key].name}</h3>
              <div class="phase-line"><strong class:unknown={!active}>{active && cycle ? stateName(cycle.state) : cycle ? "Уточняем смену…" : "Нет данных"}</strong>
                {#if active && cycle}<span class="cycle-time" title={dateLabel(cycle.expiry)}>{countdown(cycle.expiry, now)}</span>{/if}</div>
              {#if active && next}<p>Затем: {stateName(next).toLocaleLowerCase("ru")}</p>{/if}
              {#if sectionStale(view, key, now)}<p class="stale-note">Не удалось обновить</p>{/if}
              {#if CYCLES[key].region}<button type="button" class="text-action" onclick={() => onOpenBounties(CYCLES[key].region!)}>Посмотреть заказы</button>{/if}
            </div>
            <button type="button" class="bell" class:enabled={activeRules.some(rule => rule.key === key)} aria-label={`Напомнить: ${CYCLES[key].name}`} title="Настроить напоминание" onclick={() => configure(key)}><WorldActivityIcon kind="bell" /></button>
          </article>
        {/each}</div>
      </aside>
    </div>

    <section class="world-panel resets-panel" aria-label="Сбросы заданий">
      {#each [{ key: "daily" as const, title: "Ежедневный сброс", hint: "Лимиты и ежедневный вход", at: dailyReset },
        { key: "weekly" as const, title: "Еженедельный сброс", hint: "Недельные активности", at: weeklyReset },
        { key: "sortie" as const, title: "Новая вылазка", hint: "Смена трёх миссий", at: view.sortie ? Date.parse(view.sortie.expiry) : null }] as reset}
        <div class="reset-cell"><div><h3>{reset.title}</h3><strong title={reset.at ? dateLabel(reset.at) : undefined}>{countdown(reset.at, now)}</strong><p>{reset.hint}</p>
          {#if reset.key === "sortie" && sectionStale(view, "sortie", now)}<p class="stale-note">Не удалось обновить</p>{/if}</div>
          <button type="button" class="bell" class:enabled={activeRules.some(rule => rule.key === reset.key)} aria-label={`Напомнить: ${reset.title}`} title="Настроить напоминание" onclick={() => configure(reset.key)}><WorldActivityIcon kind="bell" /></button></div>
      {/each}
    </section>
    {#if view.events.some(event => periodState(event, now) === "active")}
      <details class="world-panel live-events"><summary>События игры · {view.events.filter(event => periodState(event, now) === "active").length}</summary>
        {#each view.events.filter(event => periodState(event, now) === "active") as event}<div><strong>{event.name}</strong><span>Закончится через {countdown(event.expiry, now)}</span></div>{/each}
        {#if sectionStale(view, "events", now)}<p class="stale-note">Сохранённый список событий · не удалось обновить</p>{/if}
      </details>
    {/if}
    <footer class="world-source" title={dateLabel(view.sourceAt)}>Данные игры · {new Date(view.sourceAt).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" })}</footer>
  {/if}
</section>

<style>
  .world-activity { container-type:inline-size; display:grid; gap:1rem; min-width:0; }
  .world-toolbar,.toolbar-actions,.panel-title,.title-with-icon,.vendor-status,.card-actions,.phase-line,.rule-actions { display:flex; align-items:center; gap:.75rem; }
  .world-toolbar,.panel-title,.vendor-status,.phase-line { justify-content:space-between; }
  .world-toolbar { flex-wrap:wrap; } .toolbar-actions { flex-wrap:wrap; }
  .start-preference { display:flex; align-items:center; gap:.5rem; font-size:.8rem; color:var(--text-muted); }
  .start-preference input { width:1rem; height:1rem; accent-color:var(--accent); }
  .toolbar-actions button { display:flex; align-items:center; gap:.45rem; }
  .small-icon { display:inline-flex; width:1.05rem; height:1.05rem; }
  .large-icon { display:inline-flex; flex-shrink:0; width:2.6rem; height:2.6rem; }
  .gold { color:var(--gold); }
  .world-columns { display:grid; grid-template-columns:minmax(0,1.3fr) minmax(22rem,.9fr); gap:1rem; align-items:start; }
  .world-events { display:grid; gap:1rem; min-width:0; }
  .world-panel { background:var(--surface-1); border:1px solid var(--border); border-radius:.85rem; box-shadow:var(--shadow-sm); min-width:0; }
  .vendor-card,.teshin-card,.notification-panel { padding:1.1rem 1.25rem; }
  .panel-title { gap:1rem; margin-bottom:1rem; align-items:flex-start; }
  h2 { font-size:1.04rem; line-height:1.35; margin:0; } h3 { font-size:.86rem; line-height:1.4; margin:0; }
  p { font-size:.78rem; line-height:1.5; color:var(--text-muted); margin:.25rem 0 0; }
  .title-with-icon { min-width:0; }
  .vendor-status { align-items:baseline; flex-wrap:wrap; font-size:.88rem; row-gap:.35rem; }
  .vendor-status span { color:var(--text-muted); font-size:.8rem; }
  .vendor-status b { color:var(--text); font-variant-numeric:tabular-nums; }
  .card-actions { flex-wrap:wrap; margin-top:1rem; gap:.5rem; }
  button { font-size:.78rem; } .card-actions button { padding:.5rem .7rem; }
  .bell { display:inline-flex; align-items:center; justify-content:center; padding:.4rem; width:2.1rem; height:2.1rem; flex-shrink:0;
    background:transparent; color:var(--text-muted); border:1px solid transparent; border-radius:.55rem; }
  .bell:hover { background:var(--surface-3); border-color:var(--border); color:var(--accent); }
  .bell.enabled { color:var(--accent); background:var(--accent-soft); border-color:var(--border); }
  .bell :global(svg) { width:1.35rem; height:1.35rem; }
  .offer-details { margin-top:1rem; border-top:1px solid var(--border); padding-top:.7rem; }
  summary { cursor:pointer; font-size:.8rem; font-weight:700; color:var(--accent-strong); }
  .offer-search { display:grid; gap:.3rem; margin:.8rem 0; font-size:.75rem; color:var(--text-muted); }
  input[type="search"],select { min-width:0; width:100%; background:var(--surface-1); border:1px solid var(--border-strong); border-radius:.45rem; padding:.5rem .6rem; color:var(--text); }
  .offer-list { padding:0; margin:.6rem 0 0; list-style:none; max-height:24rem; overflow:auto; overscroll-behavior:contain; }
  .offer-list li { display:flex; justify-content:space-between; align-items:baseline; gap:1rem; padding:.7rem .2rem; font-size:.78rem; border-bottom:1px solid var(--border); }
  .offer-list li > div,.offer-list li > strong { min-width:0; overflow-wrap:anywhere; }
  .offer-list li > span { font-size:.72rem; color:var(--text-muted); text-align:right; flex:0 0 40%; }
  .prime-equipment { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:.7rem; list-style:none; margin:1rem 0 0; padding:0; }
  .prime-equipment li { background:var(--surface-2); padding:.65rem .75rem; border-radius:.5rem; font-size:.85rem; overflow-wrap:anywhere; }
  .cycles-panel { overflow:hidden; } .cycles-panel > header { margin:0; padding:1.1rem 1.2rem .9rem; }
  .cycle-row { display:flex; align-items:flex-start; gap:.8rem; padding:1rem 1.1rem; border-top:1px solid var(--border); }
  .cycle-icon { width:2.4rem; height:2.4rem; padding:.4rem; flex-shrink:0; color:var(--gold); background:var(--surface-2); border-radius:.6rem; }
  .phase-night { color:var(--text-muted); }
  .cycle-copy { flex:1; min-width:0; }
  .phase-line { flex-wrap:wrap; gap:.25rem .8rem; margin-top:.35rem; font-size:1.02rem; }
  .phase-line strong { color:var(--success); } .phase-line strong.unknown { color:var(--text-muted); font-size:.85rem; }
  .cycle-time { font-size:.94rem; font-weight:700; font-variant-numeric:tabular-nums; }
  .cycle-copy p { font-size:.74rem; }
  .text-action { background:none; border:0; color:var(--accent); padding:0; margin-top:.5rem; font-size:.75rem; text-decoration:underline; text-underline-offset:3px; box-shadow:none; }
  .text-action:hover { background:none; color:var(--accent-strong); }
  .resets-panel { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); }
  .reset-cell { display:flex; align-items:flex-start; justify-content:space-between; gap:.5rem; padding:1rem 1.1rem; }
  .reset-cell + .reset-cell { border-left:1px solid var(--border); }
  .reset-cell strong { display:block; font-size:1.08rem; margin-top:.4rem; font-variant-numeric:tabular-nums; }
  .reset-cell p { font-size:.7rem; }
  .notification-panel { border-color:var(--border-strong); }
  .notification-panel:focus { outline:none; }
  .notification-panel form { display:flex; align-items:end; flex-wrap:wrap; gap:.65rem; }
  .notification-panel form label { display:grid; gap:.3rem; font-size:.75rem; min-width:11rem; flex:1; }
  .notification-panel form label:first-child { flex:1.5; min-width:14rem; }
  .notification-panel form button { min-height:2.1rem; }
  .notification-list { list-style:none; margin:1rem 0 0; padding:0; }
  .notification-list li { display:flex; justify-content:space-between; gap:1rem; align-items:center; padding:.8rem 0; border-top:1px solid var(--border); font-size:.85rem; }
  .rule-actions { gap:.35rem; flex-wrap:wrap; }
  .rule-actions button { padding:.4rem .55rem; }
  .rule-actions button[aria-pressed="true"] { color:var(--success); background:var(--success-soft); }
  .subtle { margin-top:.7rem; }
  .stale-note { color:var(--accent-strong); font-size:.73rem; }
  .world-warning,.world-message { margin:0; padding:.8rem 1rem; border-radius:.6rem; border:1px solid var(--border); background:var(--surface-2); }
  .world-warning { background:var(--accent-soft); }
  .initial-state { text-align:center; padding:3rem 1.5rem; } .initial-state .large-icon { margin-bottom:1rem; color:var(--text-muted); }
  .live-events { padding:.9rem 1.2rem; } .live-events div { display:flex; flex-wrap:wrap; justify-content:space-between; gap:.5rem; font-size:.8rem; margin-top:1rem; }
  .live-events span { color:var(--text-muted); }
  .world-source { display:flex; flex-wrap:wrap; gap:.5rem 1rem; font-size:.7rem; color:var(--text-subtle); padding:0 .15rem; }
  @container (max-width: 52rem) {
    .world-columns { grid-template-columns:minmax(0,1fr); } .cycles-panel { grid-row:1; }
    .cycle-list { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); }
    .cycle-row:last-child { grid-column:1/-1; } .cycle-row:nth-child(even) { border-left:1px solid var(--border); }
    .notification-list li { flex-wrap:wrap; }
  }
  @container (max-width: 36rem) {
    .cycle-list { grid-template-columns:minmax(0,1fr); } .cycle-row:nth-child(even) { border-left:0; }
    .resets-panel { grid-template-columns:minmax(0,1fr); } .reset-cell + .reset-cell { border-left:0; border-top:1px solid var(--border); }
    .prime-equipment { grid-template-columns:minmax(0,1fr); }
  }
</style>
