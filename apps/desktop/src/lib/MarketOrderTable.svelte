<script lang="ts">
  import { orderEnglishName } from "./account";
  import { orderAdvice } from "./marketSales";
  import { variantLabel } from "./market";
  import type { TradeShiftRow } from "./tradeShift";

  export let rows: TradeShiftRow[];
  export let selectedIds: Set<string>;
  export let activeId: string | null = null;
  export let inventoryKnown = false;
  export let busy = false;
  export let verified = false;
  export let onToggle: (id: string) => void;
  export let onSelectPage: (ids: string[], selected: boolean) => void;
  export let onActivate: (row: TradeShiftRow) => void;
  let page = 1;
  let pageSize = 50;
  let lastRows = "";
  const number = (value: number | null | undefined) => value == null ? "—" : value.toLocaleString("ru-RU", { maximumFractionDigits: 1 });
  $: signature = rows.map(row => row.order.id).join("|");
  $: if (signature !== lastRows) { lastRows = signature; page = 1; }
  $: pageCount = Math.max(1, Math.ceil(rows.length / pageSize));
  $: if (page > pageCount) page = pageCount;
  $: displayed = rows.slice((page - 1) * pageSize, page * pageSize);
  $: allSelected = displayed.length > 0 && displayed.every(row => selectedIds.has(row.order.id));
  $: someSelected = !allSelected && displayed.some(row => selectedIds.has(row.order.id));
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (Прокручиваемый список доступен с клавиатуры.) -->
<div class="table-scroll" tabindex="0" role="region" aria-label="Список объявлений">
  <table>
    <caption class="sr-only">Предметы, ваша цена, оценка рынка за штуку и количество в объявлении</caption>
    <thead><tr>
      <th class="selection"><input type="checkbox" aria-label="Выбрать все объявления на странице" checked={allSelected} indeterminate={someSelected} disabled={busy || !verified} onchange={() => onSelectPage(displayed.map(row => row.order.id), !allSelected)}/></th>
      <th class="item">Предмет</th><th class="numeric">Ваша цена</th><th class="numeric">Оценка / шт.</th><th class="numeric quantity">Количество</th>
    </tr></thead>
    <tbody>
      {#each displayed as row (row.order.id)}
        {@const name = row.item?.displayName ?? "Неизвестный предмет"}
        {@const variant = row.key ? variantLabel(row.key) : "Вариант не определён"}
        <tr class:active={activeId === row.order.id} class:chosen={selectedIds.has(row.order.id)} class:hidden-order={!row.order.visible}>
          <td class="selection"><input type="checkbox" aria-label={"Выбрать для массовых действий: " + name} checked={selectedIds.has(row.order.id)} disabled={busy || !verified} onchange={() => onToggle(row.order.id)}/></td>
          <th scope="row" class="item">
            <button class="item-button" aria-pressed={activeId === row.order.id} aria-controls="order-detail" disabled={busy} onclick={() => onActivate(row)}>
              {#if row.item?.imageUrl}<img src={row.item.imageUrl} alt="" loading="lazy"/>{:else}<span class="image-placeholder" aria-hidden="true">◇</span>{/if}
              <span class="item-copy"><strong>{name}</strong>{#if orderEnglishName(row.item ?? undefined)}<small class="english" lang="en" translate="no">{orderEnglishName(row.item ?? undefined)}</small>{/if}<span class="item-state"><small class:visible={row.order.visible}>{row.order.visible ? "На рынке" : "Скрыто"}</small>{#if variant !== "базовый вариант"}<small>{variant}</small>{/if}</span></span>
              <span class="chevron" aria-hidden="true">›</span>
            </button>
          </th>
          <td class="numeric"><strong>{number(row.order.platinum)} пл.</strong>{#if (row.order.perTrade ?? 1) > 1}<small>за {row.order.perTrade} шт.</small>{/if}</td>
          <td class="numeric"><strong title={row.recommendation ? `Оценка от ${row.recommendation.sourceDate}` : "Проверьте текущие цены"}>{row.recommendation?.listPrice == null ? "—" : number(row.recommendation.listPrice) + " пл."}</strong>{#if row.needsAction}<small class="advice" title={orderAdvice(row)}>{row.health === "overpriced" ? "Выше рынка" : row.health === "underpriced" ? "Ниже рынка" : row.health === "inventory_mismatch" ? "Проверьте остаток" : row.priceCheckFailed ? "Ошибка проверки" : "Нет оценки"}</small>{/if}</td>
          <td class="numeric quantity"><strong>{row.order.quantity}</strong>{#if row.order.type === "sell"}<small class:danger={row.health === "inventory_mismatch"}>Доступно: {inventoryKnown ? row.inventory?.sellableQuantity ?? 0 : "?"}</small>{/if}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
<nav class="pagination" aria-label="Страницы объявлений"><span>{rows.length ? (page - 1) * pageSize + 1 : 0}–{Math.min(page * pageSize, rows.length)} из {rows.length}</span><div><label>На странице <select bind:value={pageSize} onchange={() => page = 1}><option value={25}>25</option><option value={50}>50</option><option value={100}>100</option></select></label><button class="secondary" disabled={page === 1} onclick={() => page -= 1}>Назад</button><span>{page} / {pageCount}</span><button class="secondary" disabled={page === pageCount} onclick={() => page += 1}>Дальше</button></div></nav>

<style>
  .table-scroll { overflow:auto; max-height:calc(100dvh - 25rem); min-height:12rem; scrollbar-gutter:stable; }
  table { width:100%; min-width:34rem; border-collapse:separate; border-spacing:0; table-layout:fixed; font-size:.8125rem; }
  th,td { padding:.45rem .7rem; border-bottom:1px solid var(--border); vertical-align:middle; text-align:left; }
  thead th { position:sticky; top:0; z-index:2; height:2.5rem; background:var(--surface-1); font-size:.75rem; color:var(--text-muted); font-weight:650; }
  th.selection,td.selection { width:2.1rem; padding-inline:.7rem .15rem; } th.item { width:48%; font-weight:400; text-transform:none; letter-spacing:0; } thead th.item { font-weight:650; }
  tr:hover > td,tr:hover > th { background:var(--surface-2); } tr.chosen > td,tr.chosen > th { background:var(--surface-2); }
  tr.active > td,tr.active > th { background:var(--accent-soft); } tr.active .selection { box-shadow:inset 3px 0 var(--accent); }
  .item-button { display:flex; align-items:center; gap:.65rem; width:100%; min-height:3rem; padding:0; border:0; background:none; box-shadow:none; color:var(--text); text-align:left; white-space:normal; }
  .item-button img,.image-placeholder { width:2.15rem; height:2.5rem; object-fit:contain; flex:none; } .image-placeholder { display:grid; place-items:center; color:var(--text-muted); }
  .item-copy { flex:1; min-width:0; } .item-copy strong { display:block; font-size:.875rem; font-weight:650; line-height:1.35; overflow-wrap:anywhere; }
  small { display:block; font-size:.75rem; font-weight:400; color:var(--text-muted); line-height:1.4; } .english { margin-top:.15rem; overflow-wrap:anywhere; }
  .item-state { display:flex; flex-wrap:wrap; gap:.2rem .55rem; margin-top:.2rem; } .visible { color:var(--success); } .hidden-order .item-copy strong { color:var(--text-muted); }
  .chevron { color:var(--text-muted); font-size:1.2rem; } .numeric { text-align:right; font-variant-numeric:tabular-nums; } .numeric strong { font-weight:650; white-space:nowrap; }
  .advice { color:var(--accent); } .danger { color:var(--danger); } input[type=checkbox] { width:1rem; height:1rem; margin:0; accent-color:var(--accent); }
  .pagination,.pagination > div,.pagination label { display:flex; align-items:center; gap:.6rem; } .pagination { justify-content:space-between; flex-wrap:wrap; padding:.65rem .9rem; font-size:.75rem; color:var(--text-muted); }
  .pagination > div { flex-wrap:wrap; } .pagination button { min-height:1.9rem; padding:.3rem .6rem; font-size:.75rem; } select { background:var(--surface-1); color:var(--text); border:1px solid var(--border); border-radius:.3rem; padding:.3rem; font:inherit; }
  @media(max-width:1200px) { .table-scroll { max-height:calc(100dvh - 28rem); } th.item { width:44%; } }
  @media(max-width:650px) { table { min-width:31rem; } th,td { padding-inline:.45rem; } }
</style>
