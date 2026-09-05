<script lang="ts">
  import { useLocale } from "./i18n";
  import { masteryAnnotation, type MasteryItemView } from "./mastery";
  import masteredIcon from "./assets/warframe-mastered.png";

  export let item: MasteryItemView | undefined = undefined;
  export let showName = false;
  export let loading = false;
  export let error = false;
  export let stale = false;
  export let historyAvailable = false;
  const locale = useLocale();
  $: annotation = masteryAnnotation(item, { loading, error, stale, historyAvailable }, $locale);
  $: name = item ? ($locale === "ru" ? item.displayName : item.displayNameEn) : "";
</script>

<span class="mastery-status" class:mastered={annotation.mastered}
  class:progress={annotation.tone === "progress"} title={annotation.title}>
  {#if annotation.mastered}
    <span class="mastered-icon" style:--mastered-icon={`url("${masteredIcon}")`} aria-hidden="true"></span>
  {/if}
  <span>{showName && name ? `${name} · ` : ""}{annotation.text}</span>
</span>

<style>
  .mastery-status{display:inline-flex;align-items:center;gap:.35rem;max-width:100%;font-size:.78rem;line-height:1.45;font-weight:500;color:var(--text-muted);overflow-wrap:anywhere}
  .mastery-status.mastered{color:var(--positive,#32603b)}
  .mastery-status.progress{color:var(--accent-strong)}
  .mastered-icon{display:block;width:1.25rem;height:1.25rem;flex:none;background:currentColor;mask:var(--mastered-icon) center/contain no-repeat;-webkit-mask:var(--mastered-icon) center/contain no-repeat}
</style>
