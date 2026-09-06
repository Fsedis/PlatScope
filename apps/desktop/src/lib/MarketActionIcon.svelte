<script lang="ts">
  export let name: "show" | "hide" | "edit" | "external" | "chart" | "check" | "close";
</script>

<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if name === "show" || name === "hide"}<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>{#if name === "hide"}<path d="m3 3 18 18"/>{/if}
  {:else if name === "edit"}<path d="m15 4 5 5M4 20l5-1L20 8a2.1 2.1 0 0 0-5-5L4 14Z"/>
  {:else if name === "external"}<path d="M14 3h7v7m0-7L10 14M10 4H4v16h16v-6"/>
  {:else if name === "chart"}<path d="M3 3v18h18M7 15l4-5 4 3 5-7"/>
  {:else if name === "check"}<path d="m5 12 4 4L19 6"/>
  {:else}<path d="m6 6 12 12M6 18 18 6"/>{/if}
</svg>
