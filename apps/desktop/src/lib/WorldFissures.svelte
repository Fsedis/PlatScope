<script lang="ts">
  import { countdown, FISSURE_MISSION_TYPES, FISSURE_TIERS, fissureFilterSummary, periodState, sectionStale,
    type FissureFilterRule, type WorldActivityView, type WorldFissure } from "./worldActivity";
  import { saveWorldPreferences, worldPreferences } from "./worldActivityStore";
  import { prepareFissureRelics, selectFissureMatches } from "./worldFissures";
  import { refinementLabel, type InsightsView } from "./insights";
  import type { PersonalGoalsView } from "./personalGoals";

  export let view: WorldActivityView;
  export let insights: InsightsView | null;
  export let goals: PersonalGoalsView | null;
  export let loadingPersonal: boolean;
  export let personalError: boolean;
  export let now: number;
  export let onOpenRelic: (slug: string) => void;
  export let onOpenSettings: () => void;
  export let onRetryPersonal: () => void;

  const defaultRule: FissureFilterRule = { id: "default", tier: "any", missionType: "any", mode: "any" };
  const modes = [
    { key: "any", name: "Любой" },
    { key: "normal", name: "Обычный" },
    { key: "steel", name: "Стальной путь" },
    { key: "storm", name: "Буря Бездны" },
  ] as const;
  const initialLimit = 8;
  let visibleLimit = initialLimit;
  let preferenceError = "";

  $: rules = $worldPreferences.fissureRules;
  $: missionTypes = missionOptions(view.fissures ?? [], rules);
  $: tiers = tierOptions(view.fissures ?? [], rules);
  $: preparedRelics = prepareFissureRelics(insights, goals);
  $: matches = selectFissureMatches(view, insights, goals, rules, now, preparedRelics);
  $: visibleMatches = matches.slice(0, visibleLimit);
  $: liveCount = (view.fissures ?? []).filter(fissure => periodState(fissure, now) === "active").length;
  $: hasOwnedRelics = (insights?.relics ?? []).some(relic => relic.ownedQuantity > 0);
  $: unavailable = view.unavailableSections.includes("fissures");
  $: delayed = sectionStale(view, "fissures", now);

  function missionOptions(fissures: readonly WorldFissure[], currentRules: readonly FissureFilterRule[]) {
    const options: { key: string; name: string }[] = [...FISSURE_MISSION_TYPES];
    const known = new Set(options.map(option => option.key));
    for (const fissure of fissures) {
      if (!fissure.missionTypeKey || known.has(fissure.missionTypeKey)) continue;
      options.push({ key: fissure.missionTypeKey, name: fissure.missionType || fissure.missionTypeKey });
      known.add(fissure.missionTypeKey);
    }
    for (const rule of currentRules) {
      if (rule.missionType === "any" || known.has(rule.missionType)) continue;
      options.push({ key: rule.missionType, name: rule.missionType });
      known.add(rule.missionType);
    }
    return options;
  }

  function tierOptions(fissures: readonly WorldFissure[], currentRules: readonly FissureFilterRule[]) {
    const options: { key: string; name: string }[] = [...FISSURE_TIERS];
    const known = new Set(options.map(option => option.key));
    for (const tier of [...fissures.map(fissure => fissure.tier), ...currentRules.map(rule => rule.tier)]) {
      if (!tier || tier === "any" || known.has(tier)) continue;
      options.push({ key: tier, name: tier });
      known.add(tier);
    }
    return options;
  }

  function saveRules(next: FissureFilterRule[]): boolean {
    const saved = saveWorldPreferences({ ...$worldPreferences, fissureRules: next });
    preferenceError = saved ? "" : "Не удалось сохранить условия. Попробуйте ещё раз.";
    if (saved) visibleLimit = initialLimit;
    return saved;
  }

  function addRule(): void {
    if (rules.length >= 30) return;
    saveRules([...rules, { id: crypto.randomUUID(), tier: "any", missionType: "any", mode: "any" }]);
  }

  function removeRule(id: string): void {
    saveRules(rules.filter(rule => rule.id !== id));
  }

  function modeNames(fissure: WorldFissure): string[] {
    const names: string[] = [];
    if (fissure.isStorm) names.push("Буря Бездны");
    if (fissure.isHard) names.push("Стальной путь");
    if (!names.length) names.push("Обычный");
    return names;
  }

  function tierName(tier: string): string {
    return tiers.find(option => option.key === tier)?.name ?? tier;
  }

  function missionName(fissure: WorldFissure): string {
    return missionTypes.find(option => option.key === fissure.missionTypeKey)?.name
      ?? fissure.missionType ?? "Тип миссии неизвестен";
  }

  function percent(value: number): string {
    return `${new Intl.NumberFormat("ru-RU", { maximumFractionDigits: 1 }).format(value)}%`;
  }

  function platinum(value: number): string {
    return `${new Intl.NumberFormat("ru-RU", { maximumFractionDigits: 1 }).format(value)} пл.`;
  }

  function relicEnglishName(slug: string, russianName: string): string | null {
    const englishName = insights?.relics.find(relic => relic.definition.relicSlug === slug)?.definition.displayNameEn;
    return englishName && englishName !== russianName ? englishName : null;
  }
</script>

<section class="fissures" aria-labelledby="fissures-heading">
  <header class="fissures-heading">
    <div>
      <h2 id="fissures-heading">Разломы для моих реликвий</h2>
      <p>Текущие миссии, куда можно взять реликвию из вашего инвентаря</p>
    </div>
    {#if !loadingPersonal && !delayed && insights?.inventoryAvailable && hasOwnedRelics}
      <span class="result-count">Найдено: {matches.length}</span>
    {/if}
  </header>

  <details class="fissure-settings">
    <summary>Настроить разломы <span>{fissureFilterSummary(rules)}</span></summary>
    <div class="settings-body">
      <p>Разлом подходит, если совпадает хотя бы одно условие. Внутри условия учитываются все выбранные поля.</p>
      {#if !rules.length}<p class="hidden-note">Условий нет. Все разломы скрыты.</p>{/if}
      <div class="rule-list">
        {#each rules as rule, index (rule.id)}
          <div class="rule-row">
            <span class="rule-number">Условие {index + 1}</span>
            <label>Эра
              <select value={rule.tier} onchange={event => {
                const select = event.currentTarget;
                if (!saveRules(rules.map(existing => existing.id === rule.id ? { ...existing, tier: select.value } : existing))) select.value = rule.tier;
              }}>
                <option value="any">Любая</option>
                {#each tiers as tier (tier.key)}<option value={tier.key}>{tier.name}</option>{/each}
              </select>
            </label>
            <label>Тип миссии
              <select value={rule.missionType} onchange={event => {
                const select = event.currentTarget;
                if (!saveRules(rules.map(existing => existing.id === rule.id ? { ...existing, missionType: select.value } : existing))) select.value = rule.missionType;
              }}>
                <option value="any">Любой</option>
                {#each missionTypes as mission (mission.key)}<option value={mission.key}>{mission.name}</option>{/each}
              </select>
            </label>
            <label>Режим
              <select value={rule.mode} onchange={event => {
                const select = event.currentTarget;
                if (!saveRules(rules.map(existing => existing.id === rule.id ? { ...existing, mode: select.value as FissureFilterRule["mode"] } : existing))) select.value = rule.mode;
              }}>
                {#each modes as mode (mode.key)}<option value={mode.key}>{mode.name}</option>{/each}
              </select>
            </label>
            <button type="button" class="remove-rule"
              aria-label={`Удалить условие ${index + 1}`} onclick={() => removeRule(rule.id)}>Удалить</button>
          </div>
        {/each}
      </div>
      <div class="settings-actions">
        <button type="button" class="secondary" disabled={rules.length >= 30} onclick={addRule}>Добавить условие</button>
        <button type="button" class="text-button" disabled={rules.length === 1 && rules[0].tier === "any" && rules[0].missionType === "any" && rules[0].mode === "any"}
          onclick={() => saveRules([defaultRule])}>Сбросить условия</button>
        <span>Сохраняются автоматически</span>
      </div>
      {#if preferenceError}<p class="save-error" role="alert">{preferenceError}</p>{/if}
    </div>
  </details>

  {#if loadingPersonal}
    <div class="empty-state" role="status" aria-busy="true"><h3>Сверяем разломы с вашими реликвиями…</h3><p>Загружаем инвентарь, цели и оценку наград.</p></div>
  {:else if personalError && !insights}
    <div class="empty-state" role="status"><h3>Не удалось загрузить мои реликвии</h3><p>Проверьте подключение и повторите загрузку.</p><button type="button" onclick={onRetryPersonal}>Повторить загрузку</button></div>
  {:else if !insights}
    <div class="empty-state"><h3>Нужны данные предметов и инвентаря</h3><p>Загрузите игровые данные и обновите инвентарь, чтобы сопоставить реликвии с разломами.</p><button type="button" onclick={onOpenSettings}>Открыть настройки данных</button></div>
  {:else if !insights.inventoryAvailable}
    <div class="empty-state"><h3>Сначала загрузите инвентарь</h3><p>После чтения инвентаря покажем только разломы для ваших реликвий.</p><button type="button" onclick={onOpenSettings}>Открыть настройки данных</button></div>
  {:else if !hasOwnedRelics}
    <div class="empty-state"><h3>Реликвий в инвентаре пока нет</h3><p>Обновите инвентарь после получения реликвий.</p><button type="button" onclick={onOpenSettings}>Обновить данные инвентаря</button></div>
  {:else if delayed}
    <div class="empty-state" role="status"><h3>Не удалось подтвердить текущие разломы</h3><p>{unavailable
      ? "Источник пока не передал свежий список миссий. Проверим его автоматически."
      : "Данные о разломах задерживаются. Проверим их автоматически, прежде чем предложить миссию."}</p></div>
  {:else if !rules.length}
    <div class="empty-state"><h3>Все разломы скрыты</h3><p>Добавьте условие отбора, чтобы увидеть подходящие миссии.</p><button type="button" onclick={() => saveRules([defaultRule])}>Показывать все</button></div>
  {:else if !liveCount}
    <div class="empty-state"><h3>Активных разломов сейчас нет</h3><p>Проверим список автоматически, когда источник обновится.</p></div>
  {:else if !matches.length}
    <div class="empty-state"><h3>Подходящих разломов сейчас нет</h3><p>Среди действующих миссий нет совпадений с вашими реликвиями и условиями отбора.</p></div>
  {:else}
    <div class="match-list">
      {#each visibleMatches as match (match.fissure.id)}
        {@const englishName = relicEnglishName(match.relic.relicSlug, match.relic.displayName)}
        <article class="match-row">
          <div class="mission">
            <div class="mission-heading"><h3>{tierName(match.fissure.tier)} · {missionName(match.fissure)}</h3><span title={`Закончится ${new Date(match.fissure.expiry).toLocaleString("ru-RU")}`}>{countdown(match.fissure.expiry, now)}</span></div>
            <p>{match.fissure.node}</p>
            <div class="mode-tags">{#each modeNames(match.fissure) as mode}<span>{mode}</span>{/each}</div>
          </div>
          <div class="recommendation">
            <span class="recommendation-label">{match.goalChancePercent > 0 ? "Для личной цели" : match.expectedPlatinum !== null ? "По оценке наград" : "Подходящая реликвия"}</span>
            <strong>{match.relic.displayName}</strong>
            {#if englishName}
              <small lang="en" class="relic-english">{englishName}</small>
            {/if}
            <p>Есть {match.relic.totalOwnedQuantity} · {match.relic.traceCost > 0
                ? `улучшить до ${refinementLabel(match.relic.recommendedRefinement, "ru")} за ${match.relic.traceCost} следов`
                : "можно открыть без улучшения"}</p>
            {#if match.goalChancePercent > 0}
              <p class="goal-detail">{match.goalNames.join(", ")} · шанс нужной детали {percent(match.goalChancePercent)}</p>
            {/if}
            <p class="value-detail">{match.expectedPlatinum === null
                ? "Оценка платины недоступна"
                : `Оценка выгоды открытия ≈ ${platinum(match.expectedPlatinum)}`}</p>
            <button type="button" class="relic-link" onclick={() => onOpenRelic(match.relic.relicSlug)}>Посмотреть реликвию</button>
          </div>
        </article>
      {/each}
    </div>
    {#if matches.length > visibleLimit}<button type="button" class="show-more secondary" onclick={() => visibleLimit += initialLimit}>Показать ещё {Math.min(initialLimit, matches.length - visibleLimit)}</button>{/if}
    <p class="estimate-note">Шанс и платина — оценки за одно открытие, а не гарантированная награда. Выбор миссии и реликвии выполняется в игре.</p>
  {/if}
  {#if personalError && insights && !loadingPersonal}
    <p class="data-warning" role="status">Личные цели сейчас недоступны; реликвию выбираем по оценке платины. <button type="button" class="retry-personal" onclick={onRetryPersonal}>Повторить загрузку</button></p>
  {/if}
</section>

<style>
  .fissures { display:grid; gap:.75rem; min-width:0; border:1px solid var(--border); border-radius:.85rem; padding:1.1rem; background:var(--surface-1); box-shadow:var(--shadow-sm); }
  h2,h3,p { margin:0; }
  .fissures-heading { display:flex; align-items:start; justify-content:space-between; flex-wrap:wrap; gap:.45rem 1rem; }
  .fissures-heading h2 { font-size:1.15rem; line-height:1.35; }
  .fissures-heading p { margin-top:.15rem; color:var(--text-muted); font-size:.8rem; line-height:1.45; }
  .result-count { flex:none; border-radius:999px; padding:.25rem .55rem; background:var(--accent-soft); color:var(--accent-strong); font-size:.75rem; font-weight:700; }
  .fissure-settings { border:1px solid var(--border); border-radius:.58rem; background:var(--surface-2); }
  .fissure-settings summary { display:flex; align-items:center; flex-wrap:wrap; gap:.2rem .55rem; min-height:2.35rem; padding:.45rem .7rem; color:var(--accent-strong); font-size:.8rem; font-weight:700; cursor:pointer; }
  .fissure-settings summary::marker { content:""; }
  .fissure-settings summary::-webkit-details-marker { display:none; }
  .fissure-settings summary::before { content:""; flex:none; width:.42rem; height:.42rem; border-right:1.5px solid currentColor; border-bottom:1.5px solid currentColor; transform:rotate(-45deg); transition:transform .15s ease; }
  .fissure-settings[open] summary::before { transform:rotate(45deg); }
  .fissure-settings summary span { color:var(--text-muted); font-size:.75rem; font-weight:400; }
  .settings-body { display:grid; gap:.7rem; border-top:1px solid var(--border); padding:.7rem; }
  .settings-body > p { color:var(--text-muted); font-size:.78rem; line-height:1.45; }
  .settings-body > .hidden-note { color:var(--accent-strong); }
  .rule-list { display:grid; gap:.5rem; }
  .rule-row { display:grid; grid-template-columns:auto repeat(3,minmax(0,1fr)) auto; align-items:end; gap:.5rem; border:1px solid var(--border); border-radius:.5rem; padding:.55rem; background:var(--surface-1); }
  .rule-number { align-self:center; min-width:5.4rem; font-size:.76rem; font-weight:700; }
  .rule-row label { display:grid; gap:.22rem; min-width:0; color:var(--text-muted); font-size:.75rem; }
  .rule-row select { min-width:0; width:100%; min-height:2.2rem; border:1px solid var(--border-strong); border-radius:.4rem; padding:.35rem .45rem; background:var(--surface-1); color:var(--text); font:inherit; font-size:.8rem; }
  .rule-row button,.settings-actions button { min-height:2.2rem; padding:.35rem .55rem; font-size:.75rem; }
  .remove-rule { background:var(--surface-2); color:var(--text-muted); border-color:var(--border); }
  .settings-actions { display:flex; align-items:center; flex-wrap:wrap; gap:.45rem; }
  .settings-actions span { color:var(--text-subtle); font-size:.75rem; }
  .settings-actions .text-button { border:0; background:transparent; color:var(--accent-strong); }
  .settings-actions .text-button:disabled { color:var(--text-subtle); }
  .save-error { color:var(--danger) !important; }
  .data-warning { border:1px solid var(--border); border-radius:.5rem; padding:.55rem .7rem; background:var(--accent-soft); color:var(--accent-strong); font-size:.76rem; line-height:1.45; }
  .retry-personal { margin-left:.4rem; border:0; background:transparent; color:inherit; padding:0; text-decoration:underline; font-size:inherit; }
  .empty-state { display:grid; justify-items:start; gap:.35rem; border:1px solid var(--border); border-radius:.58rem; padding:1rem; background:var(--surface-2); }
  .empty-state h3 { font-size:.9rem; }
  .empty-state p { color:var(--text-muted); font-size:.78rem; line-height:1.5; }
  .empty-state button { margin-top:.3rem; font-size:.78rem; }
  .match-list { display:grid; gap:.5rem; }
  .match-row { display:grid; grid-template-columns:minmax(0,1fr) minmax(0,1.15fr); gap:.8rem; border:1px solid var(--border); border-radius:.58rem; padding:.7rem .8rem; background:var(--surface-2); }
  .mission,.recommendation { min-width:0; }
  .mission-heading { display:flex; align-items:baseline; justify-content:space-between; gap:.3rem .7rem; }
  .mission-heading h3 { font-size:.88rem; line-height:1.35; overflow-wrap:anywhere; }
  .mission-heading > span { flex:none; color:var(--accent-strong); font-size:.85rem; font-weight:700; font-variant-numeric:tabular-nums; white-space:nowrap; }
  .mission p { margin-top:.25rem; color:var(--text-muted); font-size:.78rem; overflow-wrap:anywhere; }
  .mode-tags { display:flex; flex-wrap:wrap; gap:.25rem; margin-top:.48rem; }
  .mode-tags span { border-radius:.3rem; padding:.15rem .4rem; background:var(--surface-3); color:var(--text-muted); font-size:.75rem; }
  .recommendation { border-left:1px solid var(--border); padding-left:.8rem; }
  .recommendation-label { display:block; color:var(--accent-strong); font-size:.73rem; font-weight:700; }
  .recommendation strong { display:block; margin-top:.12rem; font-size:.86rem; line-height:1.35; overflow-wrap:anywhere; }
  .relic-english { display:block; margin-top:.1rem; color:var(--text-muted); font-size:.75rem; overflow-wrap:anywhere; }
  .recommendation p { margin-top:.2rem; color:var(--text-muted); font-size:.78rem; line-height:1.4; overflow-wrap:anywhere; }
  .recommendation .goal-detail { color:var(--success); }
  .recommendation .value-detail { color:var(--text); }
  .estimate-note { color:var(--text-muted); font-size:.75rem; line-height:1.45; }
  .show-more { justify-self:center; font-size:.78rem; }
  .relic-link { justify-self:start; min-height:2rem; margin-top:.2rem; padding:.25rem .2rem; border:0; background:transparent; color:var(--accent-strong); font-size:.78rem; font-weight:700; text-decoration:underline; text-underline-offset:.12rem; }
  .relic-link:hover { background:var(--accent-soft); }
  @container (max-width:54rem) { .rule-row { grid-template-columns:repeat(3,minmax(0,1fr)) auto; } .rule-number { grid-column:1/-1; } }
  @container (max-width:38rem) { .match-row { grid-template-columns:minmax(0,1fr); } .recommendation { border-left:0; border-top:1px solid var(--border); padding:.6rem 0 0; } .rule-row { grid-template-columns:repeat(2,minmax(0,1fr)); } .rule-number { grid-column:1/-1; } .remove-rule { justify-self:start; } }
  @container (max-width:26rem) { .rule-row { grid-template-columns:minmax(0,1fr); } .rule-row label { grid-column:1; } .mission-heading { flex-wrap:wrap; } }
</style>
