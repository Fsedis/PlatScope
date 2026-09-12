<script lang="ts">
  export let screen:
    | "world_activity"
    | "market"
    | "inventory"
    | "equipped_mods"
    | "squad"
    | "mission"
    | "insights"
    | "bounty_hunter"
    | "settings";
</script>

<svg class="nav-icon" viewBox="0 0 24 24" aria-hidden="true">
  {#if screen === "world_activity"}
    <circle cx="12" cy="12" r="9" /><path d="M12 6v6l4 2M3 12h2m14 0h2M12 3v2m0 14v2" />
  {:else if screen === "squad"}
    <circle cx="9" cy="7" r="3" /><path d="M3 21v-3a6 6 0 0 1 12 0v3M16 4a3 3 0 0 1 0 6m2 4a5 5 0 0 1 3 4v3" />
  {:else if screen === "mission"}
    <path d="m3 5 6-2 6 2 6-2v16l-6 2-6-2-6 2zM9 3v16m6-14v16" /><circle cx="12" cy="11" r="2" />
  {:else if screen === "market"}
    <path d="M4 17V9m5 8V5m5 12v-6m5 6V3M3 20h18" />
  {:else if screen === "inventory"}
    <path d="M4 7h16v13H4zM3 4h18v3H3zM9 11h6" />
  {:else if screen === "equipped_mods"}
    <path d="M8 3h8l4 4v13H4V7zM8 17h8M8 7h8" /><path d="m12 9 3 3-3 3-3-3z" />
  {:else if screen === "insights"}
    <path d="m11 3 1.8 4.2L17 9l-4.2 1.8L11 15l-1.8-4.2L5 9l4.2-1.8L11 3Z" />
    <path d="m18.5 13 .9 2.1 2.1.9-2.1.9-.9 2.1-.9-2.1-2.1-.9 2.1-.9.9-2.1Z" />
  {:else if screen === "bounty_hunter"}
    <circle cx="12" cy="12" r="7" />
    <circle cx="12" cy="12" r="2.5" />
    <path d="M12 2v3m0 14v3M2 12h3m14 0h3" />
  {:else}
    <circle cx="12" cy="12" r="3" />
    <path d="M10 3h4l.6 2.3 1.6.9 2.3-.6 2 3.4-1.7 1.7v2.6L20.5 15l-2 3.4-2.3-.6-1.6.9L14 21h-4l-.6-2.3-1.6-.9-2.3.6-2-3.4 1.7-1.7v-2.6L3.5 9l2-3.4 2.3.6 1.6-.9z" />
  {/if}
</svg>
