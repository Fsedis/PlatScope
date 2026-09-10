<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { PERSONAL_GOALS_VIEW_EVENT, unseenGoalCompletions, type PersonalGoalsView, type PersonalSetGoal } from "./personalGoals";

  let notices: PersonalSetGoal[] = [];
  $: names = notices.map(goal => goal.displayName.replace(/:\s*комплект\s*$/i, "").replace(/ Set$/i, ""));
  onMount(() => {
    let disposed = false;
    let revision = 0;
    const seen = new Set<string>();
    const cleanups: UnlistenFn[] = [];
    async function receive(view: PersonalGoalsView) {
      if (disposed) return;
      notices = notices.filter(notice => view.goals.some(goal => goal.setSlug === notice.setSlug && goal.completedAt === notice.completedAt));
      const fresh = unseenGoalCompletions(view.goals, seen);
      if (fresh.length) {
        notices = [...notices, ...fresh];
        fresh.forEach(goal => seen.add(`${goal.setSlug}:${goal.completedAt}`));
        await tick();
      }
      if (disposed) return;
      const completions = view.goals.filter(goal => goal.completedAt && goal.completionPending
        && seen.has(`${goal.setSlug}:${goal.completedAt}`))
        .map(goal => ({setSlug:goal.setSlug, completedAt:goal.completedAt!}));
      // После ошибки подтверждения повторяем запись при следующем обновлении, без повторного сообщения.
      if (completions.length) await invoke("acknowledge_personal_goal_completions", {completions}).catch(() => undefined);
    }
    async function refresh() {
      const current = ++revision;
      try {
        const view = await invoke<PersonalGoalsView>("personal_goals");
        if (current === revision) await receive(view);
      } catch { /* Ошибка чтения не создаёт уведомление о выполнении. */ }
    }
    const published = (event: Event) => {
      ++revision;
      void receive((event as CustomEvent<PersonalGoalsView>).detail);
    };
    window.addEventListener(PERSONAL_GOALS_VIEW_EVENT, published);
    for (const event of ["inventory-updated", "game-metadata-updated"]) {
      void listen(event, () => void refresh()).then(cleanup => {
        if (disposed) cleanup(); else cleanups.push(cleanup);
      }).catch(() => undefined);
    }
    void refresh();
    return () => { disposed = true; ++revision; cleanups.forEach(cleanup => cleanup()); window.removeEventListener(PERSONAL_GOALS_VIEW_EVENT, published); };
  });
</script>

{#if names.length}
  <div class="completion-notice" role="status" aria-live="polite">
    <div><strong>{names.length === 1 ? "Комплект собран" : "Комплекты собраны"}: {names.slice(0,3).join(", ")}{names.length > 3 ? ` и ещё ${names.length-3}` : ""}</strong><p>{names.length === 1 ? "Цель" : "Цели"} в списке «Выполнено». Детали защищены до удаления цели.</p></div>
    <button type="button" onclick={() => notices = []} aria-label="Скрыть уведомление о выполненной цели">×</button>
  </div>
{/if}

<style>
  .completion-notice { display:flex; align-items:start; justify-content:space-between; gap:1rem; padding:1rem 1.2rem; margin-bottom:1rem; border:1px solid var(--accent); background:var(--surface-2); border-radius:.7rem; font-size:.875rem; }
  p { margin:.4rem 0 0; color:var(--text-muted); }
  button { flex-shrink:0; color:var(--text-muted); background:transparent; border:0; cursor:pointer; font-size:1.3rem; padding:0 .4rem; }
</style>
