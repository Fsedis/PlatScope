<script lang="ts">
  export let src: string | null;
  export let size: "small" | "large" = "small";
  let failed = false;
  $: { src; failed = false; }
</script>

<span class="artwork" class:large={size === "large"} aria-hidden="true">
  {#if src && !failed}
    <img {src} alt="" loading="lazy" onerror={() => failed = true} />
  {:else}
    <svg viewBox="0 0 48 48" fill="none"><path d="m24 7 15 9v16l-15 9-15-9V16l15-9Z M9 16l15 9 15-9 M24 25v16" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" /></svg>
  {/if}
</span>

<style>
  .artwork { display:flex; align-items:center; justify-content:center; flex:0 0 auto; width:3rem; height:3rem; border-radius:.6rem; background:var(--surface-2); overflow:hidden; color:var(--text-muted); }
  .large { width:7rem; height:7rem; }
  img { width:100%; height:100%; object-fit:contain; }
  svg { width:55%; height:55%; opacity:.5; }
</style>
