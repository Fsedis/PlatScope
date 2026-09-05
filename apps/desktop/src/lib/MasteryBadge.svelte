<script lang="ts">
  import { masteryStore } from "./mastery";
  import MasteryStatus from "./MasteryStatus.svelte";
  export let gameRef: string;
  export let showName = false;
  $: item = $masteryStore.byGameRef.get(gameRef);
  $: stale = $masteryStore.error || $masteryStore.view?.refreshFailed;
</script>

<span class="mastery-badge">
  <MasteryStatus {item} {showName} stale={Boolean(stale)} loading={$masteryStore.loading}
    error={$masteryStore.error} historyAvailable={Boolean($masteryStore.view?.observedAt)} />
</span>

<style>
  .mastery-badge {display:block;max-width:100%;margin-top:.3rem}
</style>
