<script lang="ts">
  export let points: { sourceDate: string; closedMedian: number | null; closedVolume: number | null }[];
  export let metric: "price" | "volume" = "price";
  export let compact = false;
  export let label = "";
  const number = (v: number) => v.toLocaleString("ru-RU", { maximumFractionDigits: 1 });
  $: values = points.map(p => metric === "price" ? p.closedMedian : p.closedVolume);
  $: known = values.filter((v): v is number => v !== null && Number.isFinite(v));
  $: low = metric === "volume" ? 0 : Math.max(0, Math.min(...known) * .9);
  $: high = Math.max(low + 1, ...known.map(v => v * 1.1));
  $: width = compact ? 88 : 500;
  $: height = compact ? 28 : 150;
  $: top = compact ? 2 : 15;
  $: bottom = compact ? 26 : 125;
  $: left = compact ? 3 : 42;
  $: right = width - (compact ? 3 : 12);
  $: barWidth = compact ? 6 : Math.min(16,(right-left)/Math.max(1,values.length)*.7);
  $: dots = values.map((value, i) => ({
    x: left + i / Math.max(1, values.length - 1) * (right - left),
    y: value === null ? bottom : bottom - (value - low) / (high - low) * (bottom - top),
    value, date: points[i].sourceDate,
  }));
  $: path = dots.map((d, i) => d.value === null ? "" : `${i === 0 || dots[i - 1].value === null ? "M" : "L"}${d.x},${d.y}`).join(" ");
</script>

{#if known.length}
  <svg class:compact viewBox={`0 0 ${width} ${height}`} role="img" aria-label={label || (metric === "price" ? "Цена по дням" : "Объём закрытых сделок по дням")}>
    <title>{label || (metric === "price" ? "Цена по дням" : "Объём закрытых сделок по дням")}</title>
    {#if !compact}<line class="grid" x1={left} y1={top} x2={right} y2={top}/><line class="grid" x1={left} y1={bottom} x2={right} y2={bottom}/><text x="0" y={top + 4}>{number(high)}</text><text x="0" y={bottom}>{number(low)}</text>{/if}
    {#if metric === "price"}<path d={path}/>{/if}
    {#each dots as d, i}
      {#if d.value !== null}
        {#if metric === "volume"}<rect x={d.x - barWidth / 2} y={d.y} width={barWidth} height={Math.max(1, bottom - d.y)} rx={compact ? 1 : 2}><title>{d.date}: {number(d.value)}</title></rect>
        {:else}<circle cx={d.x} cy={d.y} r={compact ? 1.7 : 3}><title>{d.date}: {number(d.value)} пл.</title></circle>{/if}
      {:else if !compact && points.length <= 14}<text x={d.x} y={bottom - 4} text-anchor="middle">—</text>{/if}
      {#if !compact && (i === 0 || i === dots.length - 1)}<text x={d.x} y={height - 4} text-anchor={i === 0 ? "start" : "end"}>{d.date.slice(5).split("-").reverse().join(".")}</text>{/if}
    {/each}
  </svg>
{:else}<span class="no-chart">Нет данных</span>{/if}

<style>
  svg { display:block; width:100%; height:auto; overflow:visible; color:var(--accent); }
  svg.compact { width:5.5rem; height:1.75rem; }
  path { fill:none; stroke:currentColor; stroke-width:2; stroke-linejoin:round; stroke-linecap:round; }
  rect { fill:var(--accent); opacity:.65; } circle { fill:var(--accent); }
  .grid { stroke:var(--border); stroke-width:1; }
  text { fill:var(--text-muted); font-size:12px; font-family:inherit; }
  .no-chart { color:var(--text-subtle); font-size:.75rem; }
</style>
