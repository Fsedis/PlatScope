<script lang="ts">
  import { orderEnglishName } from "./account";
  import { orderAdvice } from "./marketSales";
  import { variantLabel } from "./market";
  import type { TradeShiftRow } from "./tradeShift";
  import { marketAnalyticsKey, type MarketAnalyticsSummary } from "./marketAnalytics";
  import MarketActionIcon from "./MarketActionIcon.svelte";
  import MarketTrendChart from "./MarketTrendChart.svelte";
  import { marketChangeLabel } from "./marketPeriod";
  import MarketOrderInsight from "./MarketOrderInsight.svelte";
  export let rows: TradeShiftRow[];
  export let selectedIds: Set<string>;
  export let analytics = new Map<string, MarketAnalyticsSummary>();
  export let analyticsLoading = false;
  export let analyticsError = "";
  export let inventoryKnown = false;
  export let busy = false;
  export let verified = false;
  export let onToggle: (id: string) => void;
  export let onSelectPage: (ids: string[], selected: boolean) => void;
  export let onVisibility: (row: TradeShiftRow) => void;
  export let onEdit: (row: TradeShiftRow) => void;
  export let onReloadAnalytics: () => void;
  export let onMarket: (row: TradeShiftRow) => void;
  let sort: "priority" | "name" | "price" | "quantity" | "volume" | "trend" = "priority";
  let descending = true;
  let page = 1;
  let pageSize = 50;
  let expanded: string | null = null;
  let lastRows = "";
  const number = (v: number | null | undefined) => v == null ? "—" : v.toLocaleString("ru-RU", { maximumFractionDigits: 1 });
  const chartPoints = (data: MarketAnalyticsSummary | undefined) => data?.days.slice(7).map(day => ({ sourceDate:day.date, closedMedian:day.price, closedVolume:day.volume })) ?? [];
  function value(row: TradeShiftRow): number | string | null {
    if (sort === "name") return row.item?.displayName ?? "";
    if (sort === "price") return row.order.platinum / (row.order.perTrade ?? 1);
    if (sort === "quantity") return row.order.quantity;
    if (sort === "volume") return analytics.get(row.key ? marketAnalyticsKey(row.key) : "")?.current.dailyVolume ?? null;
    if (sort === "trend") return analytics.get(row.key ? marketAnalyticsKey(row.key) : "")?.priceChangePct ?? null;
    return row.needsAction ? 1 : 0;
  }
  $: sorted = sortRows(rows, sort, descending, analytics);
  function sortRows(source: TradeShiftRow[], _sort: typeof sort, _descending: boolean, _analytics: typeof analytics): TradeShiftRow[] {
    return [...source].sort((a,b) => {
      const av = value(a), bv = value(b);
      if (av === null || bv === null) return av === bv ? 0 : av === null ? 1 : -1;
      return (typeof av === "string" ? av.localeCompare(String(bv), "ru") : av - Number(bv)) * (descending ? -1 : 1);
    });
  }
  $: signature = rows.map(row => row.order.id).join("|");
  $: if (signature !== lastRows) { lastRows = signature; page = 1; }
  $: pageCount = Math.max(1, Math.ceil(sorted.length / pageSize));
  $: if (page > pageCount) page = pageCount;
  $: displayed = sorted.slice((page - 1) * pageSize, page * pageSize);
  $: allSelected = displayed.length > 0 && displayed.every(row => selectedIds.has(row.order.id));
  function setSort(next: typeof sort): void { if (sort === next) descending = !descending; else { sort = next; descending = next !== "name"; } page = 1; }
  const marker = (column: typeof sort) => sort === column ? descending ? "↓" : "↑" : "";
  const sortAria = (column: typeof sort) => sort === column ? descending ? "descending" as const : "ascending" as const : "none" as const;
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (Прокручиваемая таблица доступна с клавиатуры.) -->
<div class="table-scroll" tabindex="0" role="region" aria-label="Объявления. В узком окне таблицу можно прокрутить горизонтально">
  <table>
    <caption class="sr-only">Компактный список объявлений с недельной статистикой и быстрыми действиями</caption>
    <thead><tr>
      <th class="selection"><input type="checkbox" aria-label="Выбрать все объявления на странице" checked={allSelected} disabled={busy || !verified} onchange={() => onSelectPage(displayed.map(row => row.order.id), !allSelected)}/></th>
      <th class="item" aria-sort={sortAria("name")}><button class="sort" onclick={() => setSort("name")}>Предмет {marker("name")}</button></th>
      <th aria-sort={sortAria("price")}><button class="sort" onclick={() => setSort("price")}>Ваша цена {marker("price")}</button></th>
      <th><span>Оценка рынка</span><small>за штуку</small></th>
      <th aria-sort={sortAria("quantity")}><button class="sort" onclick={() => setSort("quantity")}>Количество {marker("quantity")}</button></th>
      <th class="trend" aria-sort={sortAria("trend")}><button class="sort" onclick={() => setSort("trend")}>Цена сделок {marker("trend")}</button><small>7 дней · % к прошлой неделе</small></th>
      <th class="trend" aria-sort={sortAria("volume")}><button class="sort" onclick={() => setSort("volume")}>Объём в день {marker("volume")}</button><small>7 дней · % к прошлой неделе</small></th>
      <th class="actions-heading">Действия</th>
    </tr></thead>
    <tbody>
      {#each displayed as row (row.order.id)}
        {@const data = analytics.get(row.key ? marketAnalyticsKey(row.key) : "")}
        {@const name = row.item?.displayName ?? "Неизвестный предмет"}
        {@const variant = row.key ? variantLabel(row.key) : "Вариант не определён"}
        <tr class:attention={row.needsAction} class:hidden-order={!row.order.visible} class:chosen={selectedIds.has(row.order.id)}>
          <td class="selection"><input type="checkbox" aria-label={"Выбрать объявление: " + name} checked={selectedIds.has(row.order.id)} disabled={busy || !verified} onchange={() => onToggle(row.order.id)}/></td>
          <th scope="row" class="item"><div class="item-line">{#if row.item?.imageUrl}<img src={row.item.imageUrl} alt="" loading="lazy"/>{/if}<div class="item-copy"><button class="item-name" title={name} aria-expanded={expanded === row.order.id} onclick={() => expanded = expanded === row.order.id ? null : row.order.id}>{name}{#if variant !== "базовый вариант"}<span class="variant">{variant}</span>{/if}</button>{#if orderEnglishName(row.item ?? undefined)}<small class="english" lang="en" translate="no" title={orderEnglishName(row.item ?? undefined) ?? ""}>{orderEnglishName(row.item ?? undefined)}</small>{/if}</div></div></th>
          <td class="numeric"><button class="price-edit" disabled={busy || !verified} onclick={() => onEdit(row)} title="Изменить цену и количество">{number(row.order.platinum)} <span>пл.</span></button>{#if (row.order.perTrade ?? 1) > 1}<small>за {row.order.perTrade} шт.</small>{/if}</td>
          <td class="numeric"><strong title={row.recommendation ? `Оценка от ${row.recommendation.sourceDate}` : "Проверьте текущие цены"}>{number(row.recommendation?.listPrice)} <span class="unit">пл.</span></strong>{#if row.needsAction}<small class="advice" title={orderAdvice(row)}>{row.health === "overpriced" ? "Выше рынка" : row.health === "underpriced" ? "Ниже рынка" : row.health === "inventory_mismatch" ? "Проверьте остаток" : row.priceCheckFailed ? "Ошибка проверки" : "Нет оценки"}</small>{/if}</td>
          <td class="numeric"><strong>{row.order.quantity}</strong>{#if row.order.type === "sell"}<small class:danger={row.health === "inventory_mismatch"} title="Свободно в вашем инвентаре">Доступно: {inventoryKnown ? row.inventory?.sellableQuantity ?? 0 : "?"}</small>{/if}</td>
          <td class="trend"><button class="chart-link" aria-expanded={expanded === row.order.id} aria-label={"Статистика цены: " + name} title="Открыть графики и данные за период" onclick={() => expanded = expanded === row.order.id ? null : row.order.id}>
            {#if analyticsLoading && !data}<span class="muted">Загрузка…</span>{:else if data?.supported && data.current.pricedDays > 0}<MarketTrendChart points={chartPoints(data)} compact/><span class="stat-copy"><strong>{number(data.current.medianPrice)} пл.</strong><small class:up={(data.priceChangePct ?? 0) > 0} class:down={(data.priceChangePct ?? 0) < 0} title="Сравнение с предыдущими 7 полными днями">{marketChangeLabel(data.priceChangePct)}</small></span>{:else}<span class="muted">{analyticsError ? "Недоступно" : "Нет истории цены"}</span>{/if}
          </button></td>
          <td class="trend"><button class="chart-link" aria-expanded={expanded === row.order.id} aria-label={"Статистика объёма: " + name} onclick={() => expanded = expanded === row.order.id ? null : row.order.id}>
            {#if analyticsLoading && !data}<span class="muted">Загрузка…</span>{:else if data?.supported && data.current.observedDays > 0}<MarketTrendChart points={chartPoints(data)} metric="volume" compact/><span class="stat-copy"><strong>{data.current.dailyVolume === null ? "—" : (Number.isInteger(data.current.dailyVolume) ? "" : "≈ ") + number(data.current.dailyVolume)}{#if data.current.dailyVolume !== null}<span> в день</span>{/if}</strong><small class:up={(data.volumeChangePct ?? 0) > 0} class:down={(data.volumeChangePct ?? 0) < 0} title="Сравнение с предыдущими 7 полными днями">{data.current.dailyVolume === null ? `История ${data.current.observedDays} из 7 дней` : marketChangeLabel(data.volumeChangePct)}</small></span>{:else}<span class="muted">{analyticsError ? "Недоступно" : "Нет истории объёма"}</span>{/if}
          </button></td>
          <td class="row-actions">
            <button class="icon-button visibility" class:visible={row.order.visible} disabled={busy || !verified} title={row.order.visible ? "Скрыть объявление" : "Показать объявление"} aria-label={(row.order.visible ? "Скрыть объявление: " : "Показать объявление: ") + name} onclick={() => onVisibility(row)}><MarketActionIcon name={row.order.visible ? "show" : "hide"}/></button>
            <button class="icon-button" disabled={busy || !verified} title="Изменить объявление" aria-label={"Изменить объявление: " + name} onclick={() => onEdit(row)}><MarketActionIcon name="edit"/></button>
            <button class="icon-button" disabled={!row.item} title="Открыть предмет на Warframe Market" aria-label={"Открыть Warframe Market: " + name} onclick={() => onMarket(row)}><MarketActionIcon name="external"/></button>
          </td>
        </tr>
        {#if expanded === row.order.id}<tr class="expanded"><td colspan="8"><div class="expanded-content">{#key row.key ? marketAnalyticsKey(row.key) : row.order.id}<MarketOrderInsight {row} summary={data ?? null} loading={analyticsLoading && !data} unavailable={analyticsError || (data && !data.supported ? "Для этой платформы история недоступна." : "")} onRetry={onReloadAnalytics} onClose={() => expanded = null}/>{/key}</div></td></tr>{/if}
      {/each}
    </tbody>
  </table>
</div>
<footer class="pagination"><span>{(page - 1) * pageSize + 1}–{Math.min(page * pageSize, sorted.length)} из {sorted.length}</span><div><label>На странице <select bind:value={pageSize} onchange={() => page = 1}><option value={25}>25</option><option value={50}>50</option><option value={100}>100</option></select></label><button class="secondary" disabled={page === 1} onclick={() => page -= 1}>← Назад</button><span>{page} / {pageCount}</span><button class="secondary" disabled={page === pageCount} onclick={() => page += 1}>Далее →</button></div></footer>

<style>
  .table-scroll { container:order-list / inline-size; overflow:auto; max-height:calc(100dvh - 26rem); min-height:8rem; scrollbar-gutter:stable; }
  table { width:100%; min-width:64rem; border-collapse:separate; border-spacing:0; table-layout:fixed; font-size:.8125rem; }
  th,td { padding:.4rem .55rem; border-bottom:1px solid var(--border); vertical-align:middle; text-align:left; }
  thead th { position:sticky; top:0; z-index:2; height:2.6rem; background:var(--surface-2); font-size:.75rem; color:var(--text-muted); font-weight:600; text-transform:none; letter-spacing:0; }
  th.selection,td.selection { width:2rem; padding-inline:.6rem .2rem; }
  th.item { width:25%; font-weight:400; letter-spacing:0; text-transform:none; }
  th.trend,td.trend { width:13rem; } .actions-heading { width:7.5rem; }
  tr:hover > td,tr:hover > th { background:var(--surface-2); } tr.chosen > td,tr.chosen > th { background:var(--accent-soft); }
  tr.attention .selection { box-shadow:inset 2px 0 var(--accent); } .hidden-order .item-name { color:var(--text-muted); }
  .sort { border:0; background:none; padding:0; min-height:1.8rem; color:inherit; text-align:left; font-size:.75rem; box-shadow:none; }
  .item-line { display:flex; align-items:center; gap:.5rem; min-width:0; }
  .item-line img { width:1.9rem; height:2.2rem; object-fit:contain; flex:none; }
  .item-copy { min-width:0; } .item-name { display:block; max-width:100%; text-align:left; padding:0; min-height:0; border:0; background:none; box-shadow:none; color:var(--text); font-size:.8125rem; font-weight:650; line-height:1.35; white-space:normal; overflow-wrap:anywhere; }
  small { display:block; font-size:.75rem; font-weight:400; color:var(--text-muted); line-height:1.35; }
  .english { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; margin-top:.1rem; } .variant { display:inline-block; vertical-align:baseline; margin-left:.4rem; padding:0 .3rem; border:1px solid var(--border); border-radius:.2rem; font-size:.75rem; font-weight:400; color:var(--text-muted); white-space:normal; }
  .numeric { font-variant-numeric:tabular-nums; } .numeric strong { font-weight:600; white-space:nowrap; } .unit,.price-edit span { font-size:.75rem; font-weight:400; }
  .price-edit { border:0; padding:0; min-height:1.6rem; font:inherit; font-weight:700; color:var(--text); background:none; box-shadow:none; border-bottom:1px dashed var(--border-strong); border-radius:0; white-space:nowrap; }
  .advice { color:var(--accent); } .danger { color:var(--danger); }
  .chart-link { display:flex; align-items:center; gap:.4rem; padding:0; width:100%; min-height:2rem; border:0; background:none; box-shadow:none; color:var(--text); text-align:left; font-size:.8125rem; }
  .stat-copy { min-width:0; font-variant-numeric:tabular-nums; } .stat-copy strong { font-size:.8125rem; font-weight:650; white-space:nowrap; } .stat-copy strong span { font-weight:400; font-size:.75rem; margin-left:.25rem; } .stat-copy small { font-size:.75rem; line-height:1.3; }
  .up { color:var(--success); } .down { color:var(--danger); } .muted { font-size:.75rem; color:var(--text-muted); }
  .row-actions { position:sticky; right:0; z-index:1; background:var(--surface-1); white-space:nowrap; padding-inline:.3rem; box-shadow:-1px 0 var(--border); } .actions-heading { right:0; z-index:3; box-shadow:-1px 0 var(--border); } .icon-button { display:inline-flex; align-items:center; justify-content:center; min-height:2rem; height:2rem; width:2rem; padding:0; border:1px solid transparent; box-shadow:none; background:transparent; color:var(--text-muted); border-radius:.3rem; }
  .icon-button:hover { background:var(--accent-soft); border-color:var(--border-strong); color:var(--text); } .visibility.visible { color:var(--success); }
  input[type=checkbox] { width:1rem; height:1rem; margin:0; accent-color:var(--accent); }
  .expanded > td { padding:.55rem .8rem; background:var(--surface-2); } .expanded-content { width:calc(100cqw - 1.6rem); max-width:100%; position:sticky; left:.8rem; }
  .pagination { display:flex; justify-content:space-between; align-items:center; gap:1rem; padding:.6rem .8rem; font-size:.75rem; color:var(--text-muted); }
  .pagination > div,.pagination label { display:flex; align-items:center; gap:.6rem; white-space:nowrap; }
  .pagination button { min-height:1.9rem; padding:.3rem .6rem; font-size:.75rem; }
  select { background:var(--surface-1); color:var(--text); border:1px solid var(--border); border-radius:.3rem; padding:.3rem; font:inherit; }
  .table-scroll:has(.expanded) { min-height:25rem; }
  @media(max-width:1100px) { .table-scroll { max-height:calc(100dvh - 37rem); min-height:15rem; } }
  @media(max-width:700px) { .pagination { flex-wrap:wrap; } }
</style>
