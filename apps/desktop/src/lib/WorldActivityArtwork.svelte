<script lang="ts">
  import { worldArtwork, type WorldArtworkKind } from "./worldActivityArtwork";
  export let kind: WorldArtworkKind;
  $: artwork = worldArtwork[kind];
</script>

{#if artwork.mask}
  <span class="game-art mask" style:--game-art={`url("${artwork.src}")`} aria-hidden="true"></span>
{:else}
  <img class="game-art" src={artwork.src} alt="" aria-hidden="true" draggable="false" />
{/if}

<style>
  .game-art { display:block; width:100%; height:100%; object-fit:contain; }
  .mask { background:currentColor; mask:var(--game-art) center/contain no-repeat; -webkit-mask:var(--game-art) center/contain no-repeat; }
</style>
