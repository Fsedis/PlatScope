<script lang="ts">
  import type { MissionObjectKind } from "./missionResearch";
  import { MISSION_MARKER_GLYPHS } from "./missionMapMarkers";

  export let kind: MissionObjectKind;
  export let color = "currentColor";
  export let size = 18;

  $: glyph = MISSION_MARKER_GLYPHS[kind];
</script>

<svg
  viewBox="0 0 24 24"
  width={size}
  height={size}
  fill="none"
  stroke={color}
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  <path d={glyph.outline} fill={color} fill-opacity={kind === "avatar" ? 0.18 : 0.08} />
  {#if glyph.detail}<path d={glyph.detail} />{/if}
</svg>
