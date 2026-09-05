<script lang="ts">
  import { useLocale } from "./i18n";
  import { masteryStore, masteryStatusLabel } from "./mastery";
  export let gameRef: string;
  export let showName = false;
  const locale = useLocale();
  $: item = $masteryStore.byGameRef.get(gameRef);
  $: stale = $masteryStore.error || $masteryStore.view?.refreshFailed;
  $: name = item ? ($locale === "ru" ? item.displayName : item.displayNameEn) : "";
</script>

{#if item && item.status === "mastered"}
  <span class="mastery-badge" class:mastered={item.status === "mastered"}
    title={stale ? ($locale === "ru" ? "Последняя сохранённая история; обновить её не удалось." : "Last saved history; refresh failed.") : ($locale === "ru" ? "Готовый предмет уже освоен на аккаунте. Продажа и применение Формы не стирают эту отметку." : "The built item is already mastered on this account. Selling and Forma do not remove this record.")}>
    {showName ? `${name} · ` : ""}{masteryStatusLabel(item.status, $locale)}{#if stale}{` · ${$locale === "ru" ? "сохранено" : "saved"}`}{/if}
  </span>
{/if}

<style>
  .mastery-badge {display:block;width:fit-content;max-width:100%;margin-top:.4rem;font-size:.74rem;line-height:1.4;font-weight:500;color:var(--text-muted);overflow-wrap:anywhere}
  .mastery-badge.mastered {color:var(--positive, #32603b)}
</style>
