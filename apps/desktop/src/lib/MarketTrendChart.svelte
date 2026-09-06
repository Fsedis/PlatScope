<script lang="ts">
  export let points: { sourceDate: string; closedMedian: number | null; closedVolume: number | null }[];
  export let metric: "price" | "volume" = "price";
  export let compact = false;
  export let label = "";
  export let activeDate: string | null = null;
  let measuredWidth = 360;
  const number = (value: number) => value.toLocaleString("ru-RU", { maximumFractionDigits: 1 });
  const dateLabel = (date: string) => date.slice(5).split("-").reverse().join(".");
  $: values = points.map(point => {
    const value = metric === "price" ? point.closedMedian : point.closedVolume;
    return value !== null && Number.isFinite(value) && value >= 0 ? value : null;
  });
  $: known = values.filter((value): value is number => value !== null);
  $: minimum = known.length ? Math.min(...known) : 0;
  $: maximum = known.length ? Math.max(...known) : 1;
  $: padding = Math.max((maximum - minimum) * .15, maximum * .04, .2);
  $: low = metric === "volume" ? 0 : Math.max(0, minimum - padding);
  $: high = Math.max(low + .1, maximum + padding);
  $: width = compact ? 64 : Math.max(180, measuredWidth);
  $: height = compact ? 26 : 96;
  $: top = compact ? 3 : 22;
  $: bottom = compact ? 23 : 70;
  $: left = compact ? 3 : 22;
  $: right = width - left;
  $: barWidth = Math.min(compact ? 5 : 20, (right - left) / Math.max(1, values.length) * .55);
  $: dots = values.map((value, index) => ({
    x: left + index / Math.max(1, values.length - 1) * (right - left),
    y: value === null ? bottom : bottom - (value - low) / (high - low) * (bottom - top),
    value, date: points[index].sourceDate,
  }));
  $: line = dots.map((dot, index) => dot.value === null ? "" : `${index === 0 || dots[index - 1].value === null ? "M" : "L"}${dot.x},${dot.y}`).join(" ");
  $: selected = dots.findIndex(dot => dot.date === activeDate);
  $: cursor = selected >= 0 ? selected : Math.max(0, dots.length - 1);
  $: title = label || (metric === "price" ? "Цена сделок по дням, платина за штуку" : "Объём торгов по дням");
  $: valueText = dots[cursor] ? `${dateLabel(dots[cursor].date)}: ${dots[cursor].value === null ? "нет данных" : number(dots[cursor].value!)}${metric === "price" ? " пл. за штуку" : " — объём торгов"}` : "Нет данных";
  function pointAt(event: PointerEvent) {
    if (compact || !points.length) return;
    const rect = event.currentTarget as HTMLDivElement;
    const x = event.clientX - rect.getBoundingClientRect().left;
    const index = Math.min(points.length - 1, Math.max(0, Math.round((x - left) / (right - left) * (points.length - 1))));
    activeDate = points[index].sourceDate;
  }
  function keydown(event: KeyboardEvent) {
    if (compact || !points.length) return;
    let next = cursor;
    if (event.key === "ArrowLeft") next--;
    else if (event.key === "ArrowRight") next++;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = points.length - 1;
    else return;
    event.preventDefault();
    activeDate = points[Math.min(points.length - 1, Math.max(0, next))].sourceDate;
  }
</script>

<div class="chart" class:compact bind:clientWidth={measuredWidth}>
  {#if known.length}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Фокус доступен только подробному графику с ролью slider и управлением стрелками.) -->
    <div class="plot" role={compact ? undefined : "slider"} tabindex={compact ? undefined : 0} aria-label={compact ? undefined : title + ". Стрелки — выбрать день"} aria-valuemin={compact ? undefined : 0} aria-valuemax={compact ? undefined : Math.max(0, points.length - 1)} aria-valuenow={compact ? undefined : cursor} aria-valuetext={compact ? undefined : valueText} onpointermove={pointAt} onpointerleave={() => { if (!compact) activeDate = null; }} onkeydown={keydown} onblur={() => { if (!compact) activeDate = null; }}>
      <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={title}>
        <title>{title}</title>
        {#if !compact}<line class="grid" x1={left} y1={top} x2={right} y2={top}/><line class="grid" x1={left} y1={bottom} x2={right} y2={bottom}/>{/if}
        {#if !compact && selected >= 0}<line class="cursor" x1={dots[selected].x} x2={dots[selected].x} y1={top - 8} y2={bottom}/>{/if}
        {#if metric === "price"}<path d={line}/>{/if}
        {#each dots as dot, index}
          {#if dot.value !== null}
            {#if metric === "volume"}<rect class:active={dot.date === activeDate} x={dot.x - barWidth / 2} y={dot.y} width={barWidth} height={Math.max(1, bottom - dot.y)} rx="1.5"><title>{dot.date}: {number(dot.value)}</title></rect>
            {:else}<circle cx={dot.x} cy={dot.y} r={compact ? 1.5 : dot.date === activeDate ? 4 : 2.5}><title>{dot.date}: {number(dot.value)} пл.</title></circle>{/if}
          {/if}
          {#if !compact && (points.length <= 7 || index === selected)}
            <text class="point-value" x={dot.x} y={dot.y - 9} text-anchor="middle">{dot.value === null ? "—" : number(dot.value)}</text>
          {/if}
          {#if !compact && (points.length <= 7 || index === 0 || index === dots.length - 1 || index === Math.round((dots.length - 1) / 3) || index === Math.round((dots.length - 1) * 2 / 3))}
            <text class="date" x={dot.x} y="90" text-anchor="middle">{dateLabel(dot.date)}</text>
          {/if}
        {/each}
      </svg>
    </div>
  {:else}<span class="no-chart">Нет истории за этот период</span>{/if}
</div>

<style>
  .chart { min-width:0; width:100%; height:96px; color:var(--accent); }
  .chart.compact { width:3.5rem; height:1.625rem; flex:none; }
  .plot,svg { display:block; width:100%; height:100%; }
  .plot:focus-visible { outline:2px solid var(--accent); outline-offset:2px; border-radius:.2rem; }
  path { fill:none; stroke:currentColor; stroke-width:1.8; stroke-linejoin:round; stroke-linecap:round; }
  rect { fill:var(--accent); opacity:.58; } rect.active { opacity:1; } circle { fill:var(--accent); }
  .grid { stroke:var(--border); stroke-width:1; stroke-dasharray:3 4; opacity:.7; }
  .cursor { stroke:var(--accent); opacity:.5; stroke-dasharray:2 3; }
  text { fill:var(--text-muted); font-size:12px; font-family:inherit; font-variant-numeric:tabular-nums; }
  .point-value { fill:var(--text); font-weight:600; paint-order:stroke; stroke:var(--surface-2); stroke-width:4px; stroke-linejoin:round; }
  .no-chart { display:flex; align-items:center; justify-content:center; height:100%; color:var(--text-muted); font-size:.75rem; border-bottom:1px dashed var(--border); }
</style>
