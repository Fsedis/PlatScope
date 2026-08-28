<script lang="ts">
  import { buildHistoryChart, type MarketHistoryPoint } from "./history";
  import { useLocale } from "./i18n";
  import { formatPlatinum, formatVolume } from "./market";

  export let points: MarketHistoryPoint[];

  const locale = useLocale();
  const copy = {
    ru: { caption: "Медиана закрытых сделок по дням", title: "История closed median", range: (min: string, max: string, from: string, to: string) => `От ${min} до ${max} за период с ${from} по ${to}.`, volume: "объём", data: "Данные графика истории цены", date: "Дата", closed: "Closed median", volumeHeading: "Объём" },
    en: { caption: "Daily median of closed trades", title: "Closed-median history", range: (min: string, max: string, from: string, to: string) => `From ${min} to ${max} between ${from} and ${to}.`, volume: "volume", data: "Price history chart data", date: "Date", closed: "Closed median", volumeHeading: "Volume" },
  } as const;
  $: c = copy[$locale];

  $: model = buildHistoryChart(points);
</script>

{#if model}
  <figure class="history-chart" aria-labelledby="history-chart-title">
    <figcaption id="history-chart-title">{c.caption}</figcaption>
    <svg viewBox="0 0 320 150" role="img" aria-labelledby="history-svg-title history-svg-desc">
      <title id="history-svg-title">{c.title}</title>
      <desc id="history-svg-desc">
        {c.range(formatPlatinum(model.minPrice, $locale), formatPlatinum(model.maxPrice, $locale), model.firstDate, model.lastDate)}
      </desc>
      <line class="history-chart__grid" x1="34" y1="10" x2="34" y2="126" />
      <line class="history-chart__grid" x1="34" y1="126" x2="312" y2="126" />
      <line class="history-chart__grid" x1="34" y1="68" x2="312" y2="68" />
      <text class="history-chart__axis" x="2" y="14">{formatPlatinum(model.maxPrice, $locale)}</text>
      <text class="history-chart__axis" x="2" y="130">{formatPlatinum(model.minPrice, $locale)}</text>
      <text class="history-chart__axis" x="34" y="146">{model.firstDate.slice(5)}</text>
      <text class="history-chart__axis history-chart__axis--end" x="312" y="146">{model.lastDate.slice(5)}</text>
      <path class="history-chart__line" d={model.path} />
      {#each model.dots as dot}
        <circle class="history-chart__dot" cx={dot.x} cy={dot.y} r="3">
          <title>{dot.point.sourceDate}: {formatPlatinum(dot.point.closedMedian, $locale)}, {c.volume} {formatVolume(dot.point.closedVolume, $locale)}</title>
        </circle>
      {/each}
    </svg>

    <table class="sr-only">
      <caption>{c.data}</caption>
      <thead><tr><th>{c.date}</th><th>{c.closed}</th><th>{c.volumeHeading}</th></tr></thead>
      <tbody>
        {#each points as point}
          <tr>
            <td>{point.sourceDate}</td>
            <td>{formatPlatinum(point.closedMedian, $locale)}</td>
            <td>{formatVolume(point.closedVolume, $locale)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </figure>
{/if}
