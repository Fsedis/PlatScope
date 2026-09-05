<script lang="ts">
  export let kind: string;
</script>
<svg viewBox="0 0 32 32" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if kind === "bell"}
    <path d="M9 13a7 7 0 0 1 14 0c0 8 3 8 3 10H6c0-2 3-2 3-10ZM13 27h6M16 3v3" />
  {:else if kind === "cetus"}
    <circle cx="16" cy="16" r="6" /><path d="M16 3v3m0 20v3M3 16h3m20 0h3M7 7l2 2m14 14 2 2M7 25l2-2M23 9l2-2" />
  {:else if kind === "vallis"}
    <path d="M16 3v26M5 9l22 14M5 23 27 9M12 5l4 4 4-4M12 27l4-4 4 4M5 14l6-1-1-6M22 25l-1-6 6-1M5 18l6 1-1 6M22 7l-1 6 6 1" />
  {:else if kind === "cambion"}
    <path d="M6 22C-1 8 18-3 25 10c8 14-11 25-18 12Z" /><path d="M8 22c10 4 2-19 15-12M6 15h2m16 3h2" />
  {:else if kind === "zariman"}
    <path d="m16 3 11 10-11 16L5 13 11-10ZM5 13h22M16 3v26" />
  {:else if kind === "duviri"}
    <path d="M6 5c6 4 14 4 20 0v12c0 7-10 12-10 12S6 24 6 17V5Z" /><path d="m10 14 3 2m6 0 3-2M12 22c2-3 6-3 8 0" />
  {:else if kind === "baro"}
    <path d="m16 3 11 10-5 15H10L5 13 16 3ZM16 3v12m-11-2 11 2 11-2M10 28l6-13 6 13" />
  {:else if kind === "resurgence"}
    <path d="M9 26C0 19 5 7 12 5m11 21c9-7 4-19-3-21M7 17l-4-3m6 10-5-1M25 17l4-3m-6 10 5-1M16 3l6 12-6 10-6-10L16 3Z" />
  {:else}
    <circle cx="16" cy="16" r="12" /><path d="M16 8v9l6 3" />
  {/if}
</svg>
