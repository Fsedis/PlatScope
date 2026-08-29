<script lang="ts">
  export let screen:
    | "market"
    | "inventory"
    | "insights"
    | "account"
    | "diagnostics"
    | "settings";
</script>

<svg class="nav-icon" viewBox="0 0 24 24" aria-hidden="true">
  {#if screen === "market"}
    <path d="M4 17V9m5 8V5m5 12v-6m5 6V3M3 20h18" />
  {:else if screen === "inventory"}
    <path d="M4 7h16v13H4zM3 4h18v3H3zM9 11h6" />
  {:else if screen === "insights"}
    <path d="M4 19V9m5 10V5m5 14v-7m5 7V3M3 21h18" />
    <path d="m4 8 5-4 5 7 5-9" />
  {:else if screen === "account"}
    <circle cx="12" cy="8" r="4" />
    <path d="M4.5 21a7.5 7.5 0 0 1 15 0" />
  {:else if screen === "diagnostics"}
    <path d="M4 5h16v14H4zM7 9h3m-3 4h6m-6 4h9" />
    <circle cx="17" cy="9" r="1" />
  {:else}
    <circle cx="12" cy="12" r="3" />
    <path d="M12 3v3m0 12v3M3 12h3m12 0h3M5.6 5.6l2.1 2.1m8.6 8.6 2.1 2.1m0-12.8-2.1 2.1m-8.6 8.6-2.1 2.1" />
  {/if}
</svg>
