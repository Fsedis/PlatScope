<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { masteryCategoryLabel } from "./mastery";
  import PersonalGoalImage from "./PersonalGoalImage.svelte";
  import { planStatusLabel, planNextStep, planDuration, matchesBlueprintSource, type BlueprintSourceFilter, type MasteryPlanItem, type MasteryPlanView, type MasteryPlanState } from "./masteryPlan";

  export let scanning = false;
  export let onScan: () => void;
  export let onOpenSettings: () => void;
  let view: MasteryPlanView | null = null;
  let loading = true;
  let error = "";
  let busy = false;
  let query = "";
  let category = "all";
  let blueprintSource: BlueprintSourceFilter = "all";
  let filter = "all";
  let selectedRef = "";
  let visibleCount = 24;
  let detail: HTMLElement | null = null;
  let revision = 0;
  let disposed = false;
  let refreshPending = false;
  let announcement = "";
  const number = (value: number) => value.toLocaleString("ru-RU");
  const filters = [{ key:"all",label:"Все варианты" },{ key:"owned",label:"Уже есть" },
    { key:"craft",label:"Можно изготовить" },{ key:"buy_blueprint",label:"Купить и изготовить" },
    { key:"one_short",label:"Не хватает одного" },{ key:"plan",label:"Мой план" }];
  $: categories = [...new Set(view?.candidates.map(item => item.category) ?? [])];
  $: search = query.toLocaleLowerCase("ru").replaceAll("ё","е").trim().split(/\s+/).filter(Boolean);
  const matchesFilter = (item: MasteryPlanItem, key: string, savedRefs: string[]) => key === "all"
    || (key === "plan" ? savedRefs.includes(item.gameRef) : item.state === key);
  $: scoped = (view?.candidates ?? []).filter(item => (category === "all" || item.category === category)
    && matchesBlueprintSource(item,blueprintSource)
    && search.every(term => `${item.displayName} ${item.displayNameEn}`.toLocaleLowerCase("ru").replaceAll("ё","е").includes(term)));
  $: filterCounts = Object.fromEntries(filters.map(({key}) => [key,
    scoped.filter(item => matchesFilter(item,key,view?.savedRefs ?? [])).length]));
  $: filtered = scoped.filter(item => matchesFilter(item,filter,view?.savedRefs ?? []));
  $: visible = filtered.slice(0,visibleCount);
  $: selected = filtered.find(item => item.gameRef === selectedRef)
    ?? (filter === "plan" && category === "all" && blueprintSource === "all" && !search.length
      ? view?.queue.find(item => item.gameRef === selectedRef) : null) ?? filtered[0] ?? null;
  $: inPlan = Boolean(selected && view?.savedRefs.includes(selected.gameRef));
  $: planPoints = view?.queue.reduce((sum,item) => sum + (item.remainingMasteryPoints ?? 0),0) ?? 0;
  $: nextInPlan = view?.queue.find(item => item.state !== "mastered") ?? null;
  const goodState = (state: MasteryPlanState) => ["owned","craft","buy_blueprint","mastered"].includes(state);

  async function load() {
    if (busy) { refreshPending = true; return; }
    const current = ++revision;
    loading = true;
    try {
      const next = await invoke<MasteryPlanView>("load_mastery_plan");
      if (disposed || current !== revision) return;
      view = next; error = "";
    } catch { if (!disposed && current === revision) error = "Не удалось загрузить план. Сохранённые предметы и порядок не изменились."; }
    finally { if (!disposed && current === revision) loading = false; }
  }
  async function save(refs: string[], message: string) {
    if (busy || !view?.inventoryChecksum) return;
    if (refs.length > 24) { error = "В план можно добавить до 24 предметов. Завершите или уберите один из выбранных."; return; }
    busy = true; ++revision;
    try {
      const next = await invoke<MasteryPlanView>("save_mastery_plan",{ gameRefs:refs,expectedInventoryChecksum:view.inventoryChecksum });
      if (!disposed) { view = next; error = ""; announcement = message; }
    } catch { if (!disposed) error = "Не удалось сохранить изменения. Предыдущий план сохранён; повторите попытку."; }
    finally { busy = false; loading = false; if (refreshPending && !disposed) { refreshPending = false; void load(); } }
  }
  function add(item: MasteryPlanItem) { if (view) void save([...view.savedRefs,item.gameRef],`${item.displayName} добавлен в план.`); }
  function remove(gameRef: string) { if (view) void save(view.savedRefs.filter(ref => ref !== gameRef),"Предмет убран из плана."); }
  function move(index: number, direction: number) {
    if (!view) return;
    const refs = [...view.savedRefs]; const other = index + direction;
    if (other < 0 || other >= refs.length) return;
    [refs[index],refs[other]] = [refs[other],refs[index]];
    void save(refs,"Порядок обновлён. Материалы пересчитаны.");
  }
  async function select(item: MasteryPlanItem, fromPlan = false) {
    if (fromPlan) { query = "";category = "all";blueprintSource = "all";filter = "plan";visibleCount = 24; }
    selectedRef = item.gameRef;
    await tick();
    if ((detail?.closest(".planner")?.clientWidth ?? window.innerWidth) <= 1050) { detail?.scrollIntoView({block:"start",behavior:"smooth"}); detail?.focus({preventScroll:true}); }
  }
  onMount(() => {
    const cleanups: UnlistenFn[] = [];
    for (const event of ["inventory-updated","game-metadata-updated"]) {
      void listen(event,() => void load()).then(cleanup => { if (disposed) cleanup(); else cleanups.push(cleanup); }).catch(() => undefined);
    }
    void load();
    return () => { disposed = true; ++revision; cleanups.forEach(cleanup => cleanup()); };
  });
</script>

<section class="planner" aria-labelledby="planner-title">
  <header class="planner-heading">
    <div class="heading-copy"><h2 id="planner-title">План прокачки</h2></div>
    <div class="heading-actions">{#if view?.historyAvailable && view.inventoryAvailable}<button type="button" class="secondary" onclick={onScan} disabled={scanning || busy}>{scanning ? "Обновляем…" : "Обновить из Warframe"}</button>{/if}<small>{view?.observedAt ? `Инвентарь от ${new Date(view.observedAt).toLocaleString("ru-RU",{day:"numeric",month:"short",hour:"2-digit",minute:"2-digit"})}` : "Нужны инвентарь и история освоения"}</small></div>
  </header>
  <span class="sr-only" role="status" aria-live="polite">{announcement}</span>
  {#if error}<div class="planner-notice error" role="alert"><p>{error}</p><button type="button" onclick={() => load()} disabled={busy || loading}>Повторить загрузку</button></div>{/if}
  {#if view?.refreshFailed}<p class="planner-notice">Историю освоения не удалось обновить. Показаны последние сохранённые данные аккаунта.</p>{/if}
  {#if loading && !view}<div class="planner-empty" role="status"><span class="empty-symbol">◇</span><h3>Подбираем снаряжение для освоения…</h3></div>
  {:else if view && (!view.historyAvailable || !view.inventoryAvailable)}
    <div class="planner-empty"><span class="empty-symbol">◇</span><h3>Инвентарь и история освоения ещё не загружены</h3><button type="button" class="primary" onclick={onScan} disabled={scanning}>{scanning ? "Обновляем…" : "Обновить из Warframe"}</button></div>
  {:else if view}
    {#if !view.recipesAvailable}<div class="planner-notice"><p>Рецепты ещё не загружены. Можно выбрать имеющееся снаряжение; для остальных вариантов обновите данные предметов.</p><button type="button" onclick={onOpenSettings}>Открыть настройки</button></div>{/if}
    <div class="planner-toolbar">
      <label class="search-field"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/></svg><input aria-label="Найти снаряжение для освоения" type="search" bind:value={query} oninput={() => visibleCount = 24} placeholder="Найти снаряжение" /></label>
      <select aria-label="Тип снаряжения" bind:value={category} onchange={() => visibleCount = 24}><option value="all">Все типы снаряжения</option>{#each categories as value}<option value={value}>{masteryCategoryLabel(value)}</option>{/each}</select>
      <select aria-label="Источник чертежа" bind:value={blueprintSource} onchange={() => visibleCount = 24}>
        <option value="all">Все источники чертежей</option><option value="market">Магазин за кредиты</option><option value="dojo">Додзё</option><option value="drops">Выпадение чертежа</option><option value="unknown">Источник неизвестен</option>
      </select>
    </div>
    <nav class="plan-filters" aria-label="Готовность снаряжения">{#each filters as item}<button type="button" class:active={filter === item.key} aria-pressed={filter === item.key} onclick={() => { filter = item.key; visibleCount = 24; }}><span>{item.label}</span><small>{number(filterCounts[item.key] ?? 0)}</small></button>{/each}</nav>
    <div class="planner-workspace">
      <section class="candidate-panel" aria-label="Рекомендации снаряжения">
        <header class="panel-heading"><div><h3>{filter === "plan" ? "Выбранное снаряжение" : "С чего начать"}</h3></div><span>{number(filtered.length)}</span></header>
        <div class="candidate-list">
          {#each visible as item (item.gameRef)}
            <button type="button" class="candidate" class:chosen={selected?.gameRef === item.gameRef} onclick={() => select(item)} aria-pressed={selected?.gameRef === item.gameRef}>
              <span class="candidate-art"><PersonalGoalImage src={item.imageUrl} /></span>
              <span class="candidate-copy"><strong>{item.displayName}</strong>{#if item.displayName !== item.displayNameEn}<small class="english">{item.displayNameEn}</small>{/if}<span class="candidate-state" class:positive={goodState(item.state)}><i></i>{planStatusLabel(item)}</span></span>
              <span class="candidate-tail">{#if view.savedRefs.includes(item.gameRef)}<span class="saved-mark">В плане</span>{/if}{#if item.remainingMasteryPoints !== null}<strong>+{number(item.remainingMasteryPoints)}</strong><small>освоения</small>{/if}<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 5 7 7-7 7"/></svg></span>
            </button>
          {:else}<div class="list-empty"><h4>Подходящих предметов нет</h4><button type="button" onclick={() => { query = "";category = "all";blueprintSource = "all";filter = "all"; }}>Все варианты</button></div>{/each}
        </div>
        {#if filtered.length > visibleCount}<button type="button" class="show-more" onclick={() => visibleCount += 24}>Показать ещё {Math.min(24,filtered.length-visibleCount)}</button>{/if}
      </section>
      <div class="detail-column">
        {#if selected}
          <article class="equipment-detail" bind:this={detail} tabindex="-1" aria-label={`План освоения: ${selected.displayName}`}>
            <div class="equipment-hero"><div class="equipment-identity"><span class="eyebrow">{masteryCategoryLabel(selected.category)}</span><h3>{selected.displayName}</h3>{#if selected.displayName !== selected.displayNameEn}<p class="english">{selected.displayNameEn}</p>{/if}<span class="status-pill" class:positive={goodState(selected.state)}>{planStatusLabel(selected)}</span></div><div class="hero-art"><PersonalGoalImage src={selected.imageUrl} size="large" /></div></div>
            <div class="equipment-facts"><div><small>До полного освоения</small><strong>{selected.remainingMasteryPoints === null ? "Нет данных" : `+${number(selected.remainingMasteryPoints)} очков`}</strong></div><div><small>Освоено рангов</small><strong>{selected.state === "unknown" ? "Нет данных" : `${selected.masteryRank ?? 0} / ${selected.maxRank ?? "?"}`}</strong></div>{#if selected.recipe && selected.ownedQuantity === 0}<div><small>Изготовление</small><strong>{planDuration(selected.recipe.buildTimeSeconds)}</strong></div>{/if}</div>
            <section class="next-step"><span class="step-number">01</span><div><h4>Следующий шаг</h4><p>{planNextStep(selected)}</p></div></section>
            {#if selected.recipe && selected.ownedQuantity === 0}
              <div class="blueprint-line"><span class="blueprint-icon">◇</span><div><strong>Чертёж</strong><p>{selected.blueprintOwned > 0 ? `У вас есть · ${number(selected.blueprintOwned)} шт.` : selected.recipe.blueprintSource === "market" ? "Игровой магазин · за кредиты" : selected.recipe.blueprintSource === "dojo" ? "Додзё · копия исследования" : "Источник покупки не подтверждён"}</p></div>{#if selected.blueprintOwned === 0 && selected.recipe.blueprintPrice !== null}<strong class="blueprint-price">{number(selected.recipe.blueprintPrice)}<small>кредитов</small></strong>{/if}</div>
              {#if selected.recipe.blueprintSource === "dojo" && selected.blueprintOwned === 0}<p class="condition-note">Нужен доступ к додзё с завершённым исследованием. Его состояние не прочитано.</p>{/if}
              {#if selected.recipe.masteryRequirement !== null && selected.recipe.masteryRequirement > 0}<p class="condition-note" class:warning={selected.rankBlocked}>Требуется ранг мастерства {selected.recipe.masteryRequirement}.{view.accountRank !== null ? ` Ваш ранг: ${view.accountRank}.` : " Проверьте доступность в игре."}</p>{/if}
              <details class="materials"><summary><span>Материалы для изготовления</span><strong>{selected.missingTypes ? `Не хватает: ${selected.missingTypes}` : "Все есть"}</strong></summary><ul>{#each selected.materials as material (material.definition.gameRef)}<li><PersonalGoalImage src={material.definition.imageUrl} /><div><strong>{material.definition.displayNameRu ?? material.definition.displayNameEn}</strong>{#if material.definition.displayNameRu && material.definition.displayNameRu !== material.definition.displayNameEn}<small>{material.definition.displayNameEn}</small>{/if}{#if material.protectedQuantity}<small class="warning">Защищено: {number(material.protectedQuantity)} · для личной цели или освоения</small>{/if}{#if material.definition.equipment}<small class="warning">Изготовление потратит это снаряжение</small>{/if}{#if material.missingQuantity && material.definition.drops.length}<details class="drop-sources"><summary>Где добыть</summary>{#each material.definition.drops as drop}<p>{drop.location} · {drop.chancePercent}%</p>{/each}</details>{/if}</div><span class="material-quantity" class:missing={material.missingQuantity > 0}>{number(Math.min(material.availableQuantity,material.definition.quantity))} / {number(material.definition.quantity)}<small>{material.missingQuantity ? `Получить ещё ${number(material.missingQuantity)}` : "Хватает"}</small></span></li>{/each}</ul></details>
              {#if selected.blueprintOwned === 0 && selected.recipe.blueprintDrops.length}<details class="blueprint-drops"><summary>Другие способы получить чертёж</summary>{#each selected.recipe.blueprintDrops as drop}<p>{drop.location} · шанс {drop.chancePercent}%</p>{/each}</details>{/if}
              <div class="credit-total"><div><span>Покупка и изготовление</span><small>{selected.blueprintOwned > 0 ? "Чертёж уже есть" : selected.recipe.blueprintPrice === null ? "Цена получения чертежа неизвестна" : `Чертёж ${number(selected.recipe.blueprintPrice)} + кузница ${number(selected.recipe.buildPrice)}`}</small></div><strong>{selected.totalCredits === null ? `От ${number(selected.recipe.buildPrice)}` : number(selected.totalCredits)}<small>кредитов</small></strong></div>
              {#if selected.missingCredits}<p class="condition-note warning">Нужно ещё {number(selected.missingCredits)} кредитов.</p>{:else if selected.missingCredits === null}<p class="condition-note">Полную стоимость или запас кредитов пока нельзя подтвердить.</p>{/if}
            {/if}
            <footer class="detail-actions">{#if inPlan}<button type="button" class="secondary" onclick={() => remove(selected!.gameRef)} disabled={busy}>Убрать из плана</button>{:else}<button type="button" class="primary" onclick={() => add(selected!)} disabled={busy || selected.state === "unknown" || selected.state === "mastered"}>{busy ? "Сохраняем…" : "Добавить в план"}<span aria-hidden="true">＋</span></button>{/if}</footer>
          </article>
        {/if}
        <section class="saved-plan" aria-labelledby="saved-plan-title"><header class="panel-heading"><div><h3 id="saved-plan-title">Мой план <small>{view.savedRefs.length} / 24</small></h3></div>{#if planPoints}<span class="plan-points">+{number(planPoints)}<small>очков освоения</small></span>{/if}</header>
          {#if view.queue.length}<ol class="queue-list">{#each view.queue as item,index (item.gameRef)}<li class:next={item.gameRef === nextInPlan?.gameRef}><span class="queue-position">{String(index+1).padStart(2,"0")}</span><button type="button" class="queue-item" onclick={() => select(item,true)}><strong>{item.displayName}</strong><small>{planStatusLabel(item)}</small></button><div class="queue-controls"><button type="button" title="Поднять в плане" aria-label={`Поднять ${item.displayName} в плане`} disabled={busy || index === 0} onclick={() => move(index,-1)}>↑</button><button type="button" title="Опустить в плане" aria-label={`Опустить ${item.displayName} в плане`} disabled={busy || index === view!.queue.length-1} onclick={() => move(index,1)}>↓</button><button type="button" title="Убрать из плана" aria-label={`Убрать ${item.displayName} из плана`} disabled={busy} onclick={() => remove(item.gameRef)}>×</button></div></li>{/each}</ol>
          {:else}<div class="plan-empty"><span class="empty-plan-mark">＋</span><div><h4>Выберите несколько предметов</h4></div></div>{/if}
        </section>
      </div>
    </div>
  {/if}
</section>

<style>
  button{color:var(--text);background:var(--surface-2);box-shadow:none;border:1px solid var(--border)}button.primary{color:var(--surface-1);background:var(--accent);border-color:var(--accent)}button.secondary{color:var(--text);background:var(--surface-1)}.planner-toolbar select{width:15rem;flex:0 0 15rem;min-width:0}.candidate-art :global(.artwork){width:3.1rem;height:3.1rem}.hero-art :global(.artwork){width:7.7rem;height:7.7rem}.materials li :global(.artwork){width:2rem;height:2rem}
  .planner{container-type:inline-size;display:grid;gap:1rem;min-width:0}.planner h2,.planner h3,.planner h4,.planner p{margin:0}.planner-heading{display:flex;justify-content:space-between;align-items:center;gap:1.5rem;padding:1.5rem 1.75rem;border:1px solid var(--border);border-radius:1rem;background:linear-gradient(110deg,var(--surface-1),var(--surface-2));position:relative;overflow:hidden}.planner-heading::before{content:"";position:absolute;left:0;top:1.4rem;bottom:1.4rem;width:3px;background:var(--accent)}.eyebrow{font-size:.65rem;letter-spacing:.13em;font-weight:700;color:var(--accent-strong)}.heading-copy h2{font-size:1.8rem;line-height:1.2;letter-spacing:-.04em;margin:.3rem 0 .5rem}.heading-actions{display:flex;align-items:flex-end;flex-direction:column;gap:.6rem;flex-shrink:0}.heading-actions small{font-size:.72rem;color:var(--text-muted)}.planner-toolbar{display:flex;gap:.75rem}.search-field{display:flex;align-items:center;gap:.65rem;flex:1;border:1px solid var(--border);background:var(--surface-1);border-radius:.65rem;padding:0 .9rem;min-width:0}.search-field svg{width:1.1rem;height:1.1rem;fill:none;stroke:var(--text-muted);stroke-width:1.7;flex:none}.search-field input{width:100%;border:0;background:transparent;outline-offset:2px;color:var(--text);font:inherit;font-size:.85rem;padding:.8rem 0;min-width:0}.planner-toolbar select{border:1px solid var(--border);background:var(--surface-1);border-radius:.65rem;padding:.65rem .9rem;color:var(--text);font:inherit;font-size:.8rem;max-width:100%}.plan-filters{display:flex;flex-wrap:wrap;gap:.4rem}.plan-filters button{display:flex;align-items:center;gap:.6rem;font-size:.77rem;padding:.5rem .75rem;min-height:2.15rem;border:1px solid transparent;box-shadow:none;background:transparent;color:var(--text-muted)}.plan-filters button small{font-size:.69rem;background:var(--surface-3);border-radius:.3rem;padding:.15rem .3rem;min-width:1.25rem;text-align:center}.plan-filters button.active{background:var(--accent-soft);border-color:color-mix(in oklab,var(--accent) 25%,transparent);color:var(--accent-strong)}.plan-filters button.active small{background:var(--surface-1)}.planner-workspace{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1.06fr);align-items:start;gap:1.2rem}.candidate-panel,.equipment-detail,.saved-plan{border:1px solid var(--border);border-radius:.9rem;background:var(--surface-1);overflow:hidden}.panel-heading{padding:1.15rem 1.25rem;display:flex;justify-content:space-between;align-items:center;gap:1rem;border-bottom:1px solid var(--border)}.panel-heading h3{font-size:1rem;letter-spacing:-.025em}.panel-heading>span{font-size:.8rem;color:var(--text-muted)}.candidate-list{max-height:48rem;overflow:auto}.candidate{width:100%;display:flex;align-items:center;gap:.9rem;border:0;border-bottom:1px solid var(--border);border-radius:0;padding:1rem 1.2rem;background:transparent;text-align:left;box-shadow:none;min-width:0}.candidate:last-child{border-bottom:0}.candidate:hover{background:var(--surface-2)}.candidate.chosen{background:var(--accent-soft);box-shadow:inset 3px 0 var(--accent)}.candidate:active{scale:1}.candidate-art{width:3.8rem;height:3.8rem;display:grid;place-items:center;border-radius:.7rem;background:var(--surface-2);flex:none}.candidate-art :global(img),.candidate-art :global(.image-fallback){width:3.1rem;height:3.1rem}.candidate-copy{display:flex;flex-direction:column;gap:.23rem;flex:1;min-width:0}.candidate-copy>strong{font-size:.9rem;line-height:1.35;overflow-wrap:anywhere}.english{font-size:.73rem;color:var(--text-muted);line-height:1.4}.candidate-state{display:flex;align-items:center;gap:.35rem;color:var(--text-muted);font-size:.71rem;margin-top:.27rem;line-height:1.4}.candidate-state i{width:.3rem;height:.3rem;border-radius:50%;background:currentColor;flex:none}.positive{color:var(--success)!important}.candidate-tail{display:grid;grid-template-columns:auto auto;justify-items:end;align-items:center;gap:.15rem .5rem;flex:none;max-width:7.5rem}.candidate-tail>strong{font-size:.82rem;font-variant-numeric:tabular-nums}.candidate-tail>small{grid-column:1;color:var(--text-muted);font-size:.65rem}.candidate-tail>svg{grid-column:2;grid-row:1/4;width:.85rem;height:.85rem;stroke:var(--accent);stroke-width:1.7;fill:none}.saved-mark{font-size:.62rem;color:var(--accent-strong);margin-bottom:.1rem}.detail-column{display:grid;gap:1.1rem;min-width:0}.equipment-detail:focus-visible{outline:2px solid var(--accent);outline-offset:3px}.equipment-hero{display:flex;justify-content:space-between;align-items:center;gap:1rem;padding:1.35rem 1.5rem;background:radial-gradient(ellipse at right,var(--surface-3),transparent 68%)}.equipment-identity{min-width:0}.equipment-identity h3{font-size:1.65rem;letter-spacing:-.035em;margin:.35rem 0 .2rem;overflow-wrap:anywhere}.equipment-identity .english{font-size:.82rem}.status-pill{display:inline-block;margin-top:.75rem;padding:.35rem .6rem;border:1px solid var(--border);border-radius:2rem;font-size:.7rem;background:var(--surface-1);line-height:1.4}.hero-art{display:grid;place-items:center;flex:none;width:8rem;height:8rem}.hero-art :global(img),.hero-art :global(.image-fallback){width:7.7rem;height:7.7rem;filter:drop-shadow(0 .5rem .6rem oklch(.3 .025 55 / .13))}.equipment-facts{display:flex;flex-wrap:wrap;gap:1rem 1.5rem;padding:0 1.5rem 1.2rem;border-bottom:1px solid var(--border)}.equipment-facts>div{display:grid;gap:.3rem}.equipment-facts small{color:var(--text-muted);font-size:.7rem}.equipment-facts strong{font-size:.85rem;font-variant-numeric:tabular-nums}.next-step{display:flex;align-items:flex-start;gap:.8rem;padding:1.2rem 1.5rem}.step-number{display:grid;place-items:center;flex:none;width:2rem;height:2rem;background:var(--accent-soft);color:var(--accent-strong);border-radius:.5rem;font-size:.72rem;font-weight:700}.next-step h4{font-size:.85rem;margin-bottom:.35rem}.next-step p{font-size:.85rem;line-height:1.55}.blueprint-line{display:flex;align-items:center;gap:.7rem;margin:0 1.5rem;padding:.85rem;border:1px solid var(--border);border-radius:.6rem;background:var(--surface-2)}.blueprint-icon{color:var(--accent);font-size:1.6rem}.blueprint-line>div{flex:1;min-width:0}.blueprint-line strong{font-size:.8rem}.blueprint-line p{font-size:.73rem;color:var(--text-muted);margin-top:.25rem;line-height:1.5}.blueprint-price{text-align:right;flex:none}.blueprint-price small,.credit-total>strong small{display:block;font-weight:400;color:var(--text-muted);font-size:.65rem;margin-top:.2rem}.condition-note{font-size:.73rem;color:var(--text-muted);padding:.5rem 1.5rem 0;line-height:1.5}.warning{color:var(--accent-strong)!important}.materials{margin:.85rem 1.5rem 0;border-top:1px solid var(--border);border-bottom:1px solid var(--border)}.materials>summary{display:flex;justify-content:space-between;gap:.7rem;padding:.85rem 0;font-size:.78rem;cursor:pointer}.materials>summary::after{content:"＋";color:var(--accent)}.materials[open]>summary::after{content:"−"}.materials>summary>strong{font-size:.72rem;color:var(--text-muted);font-weight:500;margin-left:auto}.materials ul{list-style:none;margin:0;padding:0}.materials li{display:flex;gap:.65rem;padding:.75rem 0;border-top:1px solid var(--border);align-items:flex-start}.materials li :global(img),.materials li :global(.image-fallback){width:2rem;height:2rem;flex:none}.materials li>div{flex:1;min-width:0}.materials li>div>strong{font-size:.76rem;overflow-wrap:anywhere}.materials li small{display:block;font-size:.66rem;color:var(--text-muted);margin-top:.25rem;line-height:1.4}.material-quantity{font-size:.75rem;text-align:right;white-space:nowrap;color:var(--success);font-variant-numeric:tabular-nums}.material-quantity.missing{color:var(--accent-strong)}.drop-sources{margin-top:.3rem}.drop-sources summary,.blueprint-drops summary{cursor:pointer;font-size:.72rem;color:var(--accent-strong)}.drop-sources p,.blueprint-drops p{font-size:.7rem;color:var(--text-muted);line-height:1.5;margin-top:.4rem}.blueprint-drops{padding:.8rem 1.5rem 0}.credit-total{padding:1rem 1.5rem .4rem;display:flex;align-items:center;justify-content:space-between;gap:1rem}.credit-total>div{display:grid;gap:.3rem}.credit-total span{font-size:.77rem}.credit-total>div small{font-size:.67rem;color:var(--text-muted);line-height:1.4}.credit-total>strong{font-size:1.05rem;text-align:right;flex:none;font-variant-numeric:tabular-nums}.detail-actions{display:flex;align-items:center;gap:1rem;padding:1.2rem 1.5rem;flex-wrap:wrap}.detail-actions button{display:flex;align-items:center;justify-content:space-between;gap:1.2rem;font-size:.85rem;min-height:2.7rem}.saved-plan .panel-heading h3{margin-top:.3rem}.saved-plan .panel-heading h3 small{font-size:.7rem;color:var(--text-muted);margin-left:.4rem;font-weight:400}.plan-points{font-size:1rem!important;color:var(--accent-strong)!important;text-align:right}.plan-points small{display:block;font-size:.62rem;color:var(--text-muted);margin-top:.2rem}.plan-empty{display:flex;gap:.8rem;padding:1.1rem 1.25rem;align-items:center}.empty-plan-mark{display:grid;place-items:center;flex:none;width:2.5rem;height:2.5rem;border:1px dashed var(--border-strong);border-radius:.6rem;color:var(--accent);font-size:1.1rem}.plan-empty h4{font-size:.8rem}.queue-list{margin:0;padding:0;list-style:none;max-height:18rem;overflow:auto}.queue-list li{display:flex;gap:.65rem;padding:.8rem 1.1rem;align-items:center;border-bottom:1px solid var(--border)}.queue-list li.next{background:var(--surface-2)}.queue-position{font-size:.75rem;font-weight:600;color:var(--text-subtle);flex:none}.queue-item{display:grid;gap:.2rem;text-align:left;flex:1;min-width:0;background:transparent;border:0;padding:.15rem;box-shadow:none}.queue-item strong{font-size:.76rem;overflow-wrap:anywhere}.queue-item small{font-size:.65rem;font-weight:400;color:var(--text-muted);line-height:1.4}.queue-controls{display:flex;gap:.15rem}.queue-controls button{padding:.25rem;width:1.7rem;height:1.7rem;min-height:0;font-size:.85rem;border-color:transparent;background:transparent;color:var(--text-muted);box-shadow:none}.queue-controls button:hover{background:var(--surface-3)}.show-more{width:100%;border:0;border-top:1px solid var(--border);border-radius:0;background:var(--surface-2);font-size:.78rem;padding:.8rem}.list-empty,.planner-empty{padding:2rem;display:grid;justify-items:start;gap:.7rem}.list-empty h4,.planner-empty h3{font-size:1rem}.planner-empty{border:1px solid var(--border);background:var(--surface-1);border-radius:1rem;padding:2.5rem}.empty-symbol{font-size:2rem;color:var(--accent)}.planner-notice{padding:.8rem 1rem;background:var(--surface-2);border:1px solid var(--border);border-radius:.6rem;display:flex;align-items:center;justify-content:space-between;gap:1rem;font-size:.8rem;line-height:1.5}.planner-notice button{font-size:.75rem;flex:none}.planner-notice.error{border-color:var(--danger);color:var(--danger)}
  @container(max-width:1050px){.planner-workspace{grid-template-columns:1fr}.candidate-list{max-height:30rem}.detail-column{grid-template-columns:minmax(0,1.2fr) minmax(0,1fr);align-items:start}.planner-heading{padding:1.2rem}.heading-copy h2{font-size:1.5rem}.candidate-tail{max-width:none}.hero-art{width:5rem;height:5rem}.hero-art :global(.artwork),.hero-art :global(img),.hero-art :global(.image-fallback){width:5rem;height:5rem}}
  @container(max-width:720px){.planner-heading{align-items:flex-start;flex-direction:column;gap:1rem}.heading-actions{align-items:flex-start;flex-shrink:1}.planner-toolbar{flex-direction:column}.planner-toolbar select{width:100%;flex:none}.detail-column{grid-template-columns:1fr}.plan-filters button{font-size:.72rem;padding:.45rem .6rem}.candidate{padding:.85rem;gap:.7rem}.candidate-art{width:3rem;height:3rem}.candidate-art :global(.artwork),.candidate-art :global(img){width:2.5rem;height:2.5rem}.candidate-copy>strong{font-size:.82rem}.candidate-tail>svg{display:none}.equipment-hero{padding:1.15rem}.equipment-identity h3{font-size:1.4rem}.equipment-facts,.next-step,.detail-actions{padding:1rem 1.15rem}.blueprint-line{margin:0 1.15rem}.materials{margin:.85rem 1.15rem 0}.credit-total{padding:1rem 1.15rem .3rem}.condition-note{padding:.5rem 1.15rem 0}.planner-notice{align-items:flex-start;flex-direction:column}.material-quantity{white-space:normal;max-width:7rem}}
</style>
