<script lang="ts">
  export let kind: "bell" | "refresh" | "arrow" | "chevron" | "close" | "clock";
</script>
<svg viewBox="0 0 32 32" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if kind === "bell"}
    <path d="M9 13a7 7 0 0 1 14 0c0 8 3 8 3 10H6c0-2 3-2 3-10ZM13 27h6M16 3v3" />
  {:else if kind === "refresh"}
    <path d="M26 13A10 10 0 0 0 8 8L4 12m0-8v8h8M6 19a10 10 0 0 0 18 5l4-4m0 8v-8h-8" />
  {:else if kind === "arrow"}
    <path d="M6 16h20m-8-8 8 8-8 8" />
  {:else if kind === "chevron"}
    <path d="m10 7 9 9-9 9" />
  {:else if kind === "close"}
    <path d="m8 8 16 16M24 8 8 24" />
  {:else}
    <circle cx="16" cy="16" r="12" /><path d="M16 8v9l6 3" />
  {/if}
</svg>
