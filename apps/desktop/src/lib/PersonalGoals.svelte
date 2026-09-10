<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { refinementLabel } from "./insights";
  import { goalProgress, partRelics, personalAcquisition, PERSONAL_GOALS_VIEW_EVENT, type PersonalGoalsView } from "./personalGoals";
  import PersonalGoalImage from "./PersonalGoalImage.svelte";

  export let onOpenSettings: () => void;
  export let onChanged: () => void;
  export let onOpenRelic: (slug: string) => void;
  export let listedParts = new Set<string>();
  export let onOpenMarketSales: () => void;
  let view: PersonalGoalsView | null = null;
  let query = "";
  let selectedSlug = "";
  let adding = false;
  let busy = "";
  let loading = true;
  let error = "";
  let announcement = "";
  let request = 0;
  let goalDetail: HTMLElement | undefined;
  const name = (value: string) => value.replace(/:\s*комплект\s*$/i, "").replace(/ Set$/i, "");
  const normalize = (value: string) => value.toLocaleLowerCase("ru").replaceAll("ё", "е").trim();
  $: selected = view?.goals.find(goal => goal.setSlug === selectedSlug) ?? view?.goals.find(goal => !goal.completedAt) ?? view?.goals[0];
  $: results = (view?.catalog ?? []).filter(set => !view?.goals.some(goal => goal.setSlug === set.setSlug)
    && normalize(set.displayName + " " + set.displayNameEn).includes(normalize(query)));
  $: progress = selected ? goalProgress(selected) : null;
  $: route = selected && view?.inventoryAvailable ? personalAcquisition(selected, view.relics) : null;
  $: next = route?.steps[0];
  $: groups = [
    {title:"Собираю", goals:(view?.goals ?? []).filter(goal => !goal.completedAt)},
    {title:"Выполнено", goals:(view?.goals ?? []).filter(goal => goal.completedAt)},
  ];

  async function selectGoal(slug: string) {
    selectedSlug = slug;
    await tick();
    goalDetail?.focus({preventScroll:true});
    if (goalDetail && goalDetail.clientWidth > 0 && window.innerWidth <= 900) goalDetail.scrollIntoView({block:"start"});
  }

  async function refresh() {
    const current = ++request;
    loading = !view;
    try {
      const result = await invoke<PersonalGoalsView>("personal_goals");
      if (current === request) {
        view = result; error = "";
        window.dispatchEvent(new CustomEvent(PERSONAL_GOALS_VIEW_EVENT, {detail:result}));
      }
    } catch {
      if (current === request) error = "Не удалось обновить цели. Повторите загрузку, чтобы увидеть актуальный прогресс.";
    } finally { if (current === request) loading = false; }
  }
  async function change(setSlug: string, enabled: boolean) {
    if (busy) return;
    busy = setSlug; error = "";
    try {
      await invoke("set_personal_goal", {setSlug,enabled});
      selectedSlug = enabled ? setSlug : "";
      if (enabled) { adding = false; query = ""; }
      announcement = enabled ? "Цель сохранена. Имеющиеся детали защищены от продажи и обмена на дукаты." : "Цель удалена. Общий запас копий и другие цели сохранены.";
      await refresh();
      if (selected) await selectGoal(selected.setSlug);
      onChanged();
    } catch { error = "Не удалось сохранить изменение. Повторите действие."; }
    finally { busy = ""; }
  }
  onMount(() => {
    let disposed = false;
    const cleanups: UnlistenFn[] = [];
    void refresh();
    for (const event of ["inventory-updated", "game-metadata-updated", "market-data-updated"]) {
      void listen(event, () => { if (!busy) void refresh(); }).then(cleanup => {
        if (disposed) cleanup(); else cleanups.push(cleanup);
      });
    }
    return () => { disposed = true; ++request; cleanups.forEach(cleanup => cleanup()); };
  });
</script>

<section class="personal-goals" aria-label="Личные цели сборки">
  <header class="goal-heading">
    <div><h2>Собрать для себя</h2><p>Выберите комплект. Нужные детали останутся у вас.</p></div>
    {#if view?.goals.length}<button type="button" onclick={() => { adding = !adding; query = ""; }} aria-expanded={adding}>{adding ? "Закрыть поиск" : "Добавить цель"}</button>{/if}
  </header>
  <div class="sr-only" role="status">{announcement}</div>
  {#if error}<div class="goal-error" role="alert"><p>{error}</p><button type="button" onclick={refresh} disabled={!!busy}>Повторить загрузку</button></div>{/if}
  {#if loading}<p aria-busy="true">Загружаем цели и инвентарь…</p>{/if}
  {#if view}
    {#if adding || !view.goals.length}
      <section class="goal-picker" aria-label="Выбор комплекта">
        <label for="personal-set-search">Какой комплект хотите собрать?</label>
        <input id="personal-set-search" type="search" bind:value={query} placeholder="Например, Эш Прайм или Ash Prime" />
        {#if !view.catalog.length}<p>Обновите данные предметов, чтобы выбрать комплект.</p><button type="button" onclick={onOpenSettings}>Открыть настройки</button>
        {:else if !results.length}<p>Новых комплектов с таким названием нет. Попробуйте другое название.</p>
        {:else}<ul class="search-results">{#each results.slice(0,8) as set (set.setSlug)}
          <li><div class="item-identity"><PersonalGoalImage src={set.imageUrl} /><span><strong>{name(set.displayName)}</strong>{#if name(set.displayName) !== name(set.displayNameEn)}<small>{name(set.displayNameEn)}</small>{/if}</span></div>
            <button type="button" onclick={() => change(set.setSlug,true)} disabled={!!busy} aria-label={`Собирать ${name(set.displayName)} для себя`}>{busy === set.setSlug ? "Сохраняем…" : "Собирать для себя"}</button></li>
        {/each}</ul>{#if results.length > 8}<p class="muted">Найдено {results.length} комплектов. Уточните название.</p>{/if}{/if}
      </section>
    {/if}
    {#if !view.inventoryAvailable}<div class="goal-note"><p>Цели сохранены. Прочитайте инвентарь, чтобы увидеть имеющиеся детали и реликвии.</p><button type="button" onclick={onOpenSettings}>Открыть настройки</button></div>{/if}
    {#if selected && progress}
      <div class="goal-workspace">
        <nav class="goal-list" aria-label="Мои цели">
          {#each groups as group}
          {#if group.goals.length}<section class="goal-group" aria-label={group.title}><h3>{group.title} <span>{group.goals.length}</span></h3>
          {#each group.goals as goal (goal.setSlug)}{@const count = goalProgress(goal)}
            <button type="button" class:chosen={selected.setSlug === goal.setSlug} aria-pressed={selected.setSlug === goal.setSlug} onclick={() => selectGoal(goal.setSlug)}>
              <div class="item-identity"><PersonalGoalImage src={goal.imageUrl} /><div><strong>{name(goal.displayName)}</strong>{#if name(goal.displayName) !== name(goal.displayNameEn)}<small>{name(goal.displayNameEn)}</small>{/if}</div></div>
              <span class="goal-state">{goal.completedAt ? "Комплект собран" : view.inventoryAvailable ? `${count.owned} из ${count.required} деталей` : "Нужен инвентарь"}</span>
            </button>
          {/each}
          </section>{/if}{/each}
        </nav>
        <article class="goal-detail" bind:this={goalDetail} tabindex="-1" aria-label={`Цель: ${name(selected.displayName)}`}>
          <header class="item-identity"><PersonalGoalImage src={selected.imageUrl} size="large" /><div><h3>{name(selected.displayName)}</h3>{#if name(selected.displayName) !== name(selected.displayNameEn)}<small>{name(selected.displayNameEn)}</small>{/if}<p class="goal-purpose">Один комплект деталей</p></div></header>
          {#if listedParts.has(selected.setSlug) || selected.parts.some(part => listedParts.has(part.slug))}
            <div class="goal-note order-note"><p>Этот комплект или его детали есть в ваших ордерах на продажу. Цель сохраняет копии в приложении; существующие ордера нужно проверить на рынке.</p><button type="button" onclick={onOpenMarketSales}>Открыть мои ордера</button></div>
          {/if}
          {#if selected.completedAt}
            <section class="completed-state"><h4>Выполнено · {new Date(selected.completedAt).toLocaleDateString("ru-RU")}</h4><p>Полный комплект деталей собран. Один комплект остаётся защищённым до удаления цели; лишние копии доступны с учётом остальных правил.</p></section>
          {/if}
          {#if view.inventoryAvailable}
            <p class="progress-count"><strong>{progress.owned}<span> / {progress.required}</span></strong> {selected.completedAt ? "деталей сохранено сейчас" : "деталей уже есть"}</p>
            <progress max={progress.required || 1} value={progress.owned} aria-label="Детали в наличии для цели"></progress>
            {#if !selected.completedAt}
            <section class="next-step" aria-label="Следующий шаг">
              {#if next}<h4>Откройте: {next.source.displayName}</h4>{#if next.source.displayName !== next.source.definition.displayNameEn}<small>{next.source.definition.displayNameEn}</small>{/if}<p>{refinementLabel(next.target, "ru")} · есть {next.source.ownedQuantity} шт.</p>
                <p>{next.source.rewards.filter(reward => selected!.parts.some(part => part.slug === reward.definition.rewardSlug && part.allocatedQuantity < part.requiredQuantity)).map(reward => `${selected!.parts.find(part => part.slug === reward.definition.rewardSlug)!.displayName} — ${reward.definition.chancePercent}%`).join("; ")}</p>
                <button class="primary" type="button" onclick={() => onOpenRelic(next.source.definition.relicSlug)}>Посмотреть реликвию</button>
                <small>Шанс за одно одиночное открытие, без гарантии выпадения.</small>
              {:else if !view.metadataAvailable}<h4>Обновите данные реликвий</h4><p>Сохранённого каталога реликвий нет. Цель и резерв деталей продолжают действовать.</p><button type="button" onclick={onOpenSettings}>Открыть настройки</button>
              {:else}<h4>Найдите реликвии для недостающих деталей</h4><p>Подходящих реликвий в инвентаре нет. Раскройте нужную деталь ниже, чтобы посмотреть варианты.</p>{/if}
            </section>
            {:else if !progress.complete}<p class="muted">Состав инвентаря изменился. Цель остаётся выполненной; приложение не предлагает собирать её заново.</p>{/if}
          {/if}
          <h4 class="parts-title">Детали комплекта</h4>
          <div class="goal-parts">
            {#each selected.parts as part, index (`${part.slug}:${index}`)}
              {@const missing = Math.max(0,part.requiredQuantity-part.allocatedQuantity)}
              {@const options = partRelics(part,view.relics)}
              {#if missing && view.inventoryAvailable && !selected.completedAt}
                <details><summary><span class="item-identity"><PersonalGoalImage src={part.imageUrl} /><span><strong>{part.displayName}</strong>{#if part.displayName !== part.displayNameEn}<small>{part.displayNameEn}</small>{/if}</span></span><span class="part-count">Есть {part.allocatedQuantity} из {part.requiredQuantity}<small>Получить ещё {missing}</small></span></summary>
                  <div class="part-sources"><h5>Реликвии с этой деталью</h5>
                    {#if !options.length}<p>В сохранённых данных реликвии не найдены. Обновите данные предметов или найдите деталь на рынке.</p>
                    {:else}<ul>{#each options.slice(0,6) as relic}<li><div><strong>{relic.displayName}</strong>{#if relic.displayName !== relic.definition.displayNameEn}<small>{relic.definition.displayNameEn}</small>{/if}<small>{refinementLabel(relic.definition.refinement,"ru")} · шанс {relic.chance}%</small></div><span>{relic.ownedQuantity > 0 ? `Есть ${relic.ownedQuantity}` : relic.definition.vaultStatus === "vaulted" ? "В хранилище" : relic.definition.vaultStatus === "available" ? "Можно добыть" : "Доступность неизвестна"}</span></li>{/each}</ul><p class="muted">Вероятность за одно одиночное открытие. Реликвии из хранилища можно открыть, если они у вас есть, или получить у других игроков.</p>{/if}
                  </div>
                </details>
              {:else}<div class="owned-part"><span class="item-identity"><PersonalGoalImage src={part.imageUrl} /><span><strong>{part.displayName}</strong>{#if part.displayName !== part.displayNameEn}<small>{part.displayNameEn}</small>{/if}</span></span><span class="part-count">{view.inventoryAvailable ? `Есть ${part.allocatedQuantity} из ${part.requiredQuantity}` : `На комплект ${part.requiredQuantity}`}<small>{view.inventoryAvailable ? part.allocatedQuantity > 0 ? "Сохранено для цели" : "Для цели нет копий" : ""}</small></span></div>{/if}
            {/each}
          </div>
          <footer><p class="muted">{view.observedAt ? `Инвентарь от ${new Date(view.observedAt).toLocaleString("ru-RU")}. ` : ""}Прогресс обновляется при чтении инвентаря. Общие детали сначала выделяются ранее добавленной цели.</p>
            <button type="button" onclick={() => change(selected!.setSlug,false)} disabled={!!busy}>{busy === selected.setSlug ? "Сохраняем…" : "Удалить цель"}</button></footer>
        </article>
      </div>
    {/if}
  {/if}
</section>

<style>
  .personal-goals { container-type:inline-size; }
  .goal-heading { display:flex; align-items:center; justify-content:space-between; flex-wrap:wrap; gap:1rem; margin:1rem 0 1.25rem; }
  h2,h3,h4,h5,p { margin:0; } h2 { font-size:1.4rem; } h3 { font-size:1.3rem; } h4 { font-size:1rem; } h5 { font-size:.875rem; }
  .goal-heading p { margin-top:.4rem; color:var(--text-muted); }
  button { border:1px solid var(--border); border-radius:.5rem; background:var(--surface-2); color:var(--text); padding:.65rem .9rem; font:inherit; cursor:pointer; }
  button:disabled { cursor:wait; opacity:.6; } button:hover:not(:disabled) { border-color:var(--accent); } button:focus-visible,input:focus-visible,summary:focus-visible { outline:2px solid var(--accent); outline-offset:3px; }
  .primary { background:var(--accent); color:var(--accent-contrast,#fff); }
  small { display:block; color:var(--text-muted); font-size:.8rem; line-height:1.45; font-weight:400; }
  .goal-picker,.goal-detail { border:1px solid var(--border); border-radius:.8rem; background:var(--surface-1); padding:1.25rem; }
  .goal-picker { margin-bottom:1.2rem; } label { display:block; font-weight:600; margin-bottom:.6rem; }
  input { width:100%; box-sizing:border-box; color:var(--text); background:var(--surface-2); border:1px solid var(--border); border-radius:.5rem; padding:.75rem; font:inherit; }
  ul { list-style:none; padding:0; margin:.8rem 0 0; } .search-results li,.part-sources li { display:flex; align-items:center; justify-content:space-between; gap:1rem; border-top:1px solid var(--border); padding:.8rem 0; } .search-results button { flex-shrink:0; }
  .goal-workspace { display:grid; grid-template-columns:minmax(13rem, .7fr) minmax(0,1.8fr); gap:1.2rem; align-items:start; }
  .goal-list,.goal-group { display:grid; gap:.6rem; } .goal-list button { width:100%; text-align:left; background:var(--surface-1); padding:.85rem; overflow-wrap:anywhere; }
  .goal-group h3 { font-size:.875rem; margin:.3rem 0; color:var(--text-muted); } .goal-group h3 span { margin-left:.4rem; font-weight:400; }
  .goal-list .chosen { border-color:var(--accent); background:var(--surface-2); } .goal-state { display:block; margin-top:.7rem; font-size:.8rem; color:var(--accent-strong); }
  .item-identity { display:flex; align-items:center; gap:.8rem; min-width:0; } .item-identity>span,.item-identity>div { min-width:0; overflow-wrap:anywhere; }
  .goal-purpose { font-size:.8rem; color:var(--text-muted); margin-top:.6rem; }
  .completed-state { margin-top:1rem; padding:1rem; background:var(--surface-2); border-left:3px solid var(--accent); border-radius:.5rem; }
  .completed-state h4 { color:var(--accent-strong); } .completed-state p { margin-top:.5rem; font-size:.875rem; line-height:1.5; }
  .progress-count { margin-top:1.3rem; font-size:.875rem; } .progress-count strong { font-size:1.6rem; margin-right:.5rem; } .progress-count strong span { color:var(--text-muted); font-weight:400; }
  progress { width:100%; height:.5rem; accent-color:var(--accent); margin:.7rem 0 1rem; }
  .next-step,.goal-note { background:var(--surface-2); padding:1rem; border-radius:.5rem; } .next-step p { margin:.5rem 0; font-size:.875rem; line-height:1.5; } .next-step>small { margin-top:.6rem; } .goal-note { margin-bottom:1rem; } .goal-note button { margin-top:.6rem; }
  .parts-title { margin:1.3rem 0 .7rem; } .goal-parts>details,.owned-part { border-top:1px solid var(--border); }
  summary,.owned-part { display:flex; align-items:center; justify-content:space-between; gap:1rem; padding:.85rem 0; font-size:.875rem; }
  summary { cursor:pointer; } summary>span:first-child::before { content:"+ "; color:var(--accent-strong); } details[open]>summary>span:first-child::before { content:"− "; }
  .part-count { flex-shrink:0; text-align:right; font-weight:600; } .part-sources { padding:.8rem; background:var(--surface-2); border-radius:.5rem; margin-bottom:.8rem; font-size:.8rem; } .part-sources li { align-items:start; } .part-sources li>span { flex-shrink:0; }
  footer { display:flex; align-items:end; justify-content:space-between; gap:1rem; margin-top:1rem; } footer button { flex-shrink:0; }
  .muted { color:var(--text-muted); font-size:.8rem; line-height:1.5; margin-top:.6rem; }
  .order-note { margin-top:1rem; font-size:.875rem; line-height:1.5; }
  .goal-error { padding:1rem; border:1px solid var(--danger); border-radius:.5rem; margin-bottom:1rem; } .goal-error button { margin-top:.6rem; }
  @container (max-width:850px) { .goal-workspace { grid-template-columns:1fr; } .goal-group { grid-template-columns:repeat(auto-fit,minmax(12rem,1fr)); } .goal-group h3 { grid-column:1/-1; } }
  @container (max-width:530px) { .search-results li { flex-wrap:wrap; } .search-results button { width:100%; } .goal-detail,.goal-picker { padding:.9rem; } footer { flex-wrap:wrap; } .part-sources li { flex-wrap:wrap; gap:.3rem; } }
</style>
