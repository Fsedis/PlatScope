<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { TradeShiftRow } from "./tradeShift";
  import type { AccountProfile } from "./account";
  import type { LivePricingResult } from "./market";
  import { competingOffers } from "./marketSales";
  export let row: TradeShiftRow;
  export let profile: AccountProfile | null;
  export let onQuote: (row: TradeShiftRow, result: LivePricingResult) => void;
  let live: LivePricingResult | null = null;
  let loading = false;
  let error = "";
  let disposed = false;
  const money = (value: number) => value.toLocaleString("ru-RU", { maximumFractionDigits: 1 }) + " пл.";
  $: offers = competingOffers(live?.orders ?? [], row.order.type, profile);
  $: stale = live?.quoteState === "stale_cache";
  onMount(() => { void refresh(); return () => { disposed = true; }; });
  async function refresh(): Promise<void> {
    if (loading || !row.key) return;
    loading = true; error = "";
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      const result = await Promise.race([
        invoke<LivePricingResult | null>("live_price_current_variant", { key: row.key, itemKind: row.itemKind }),
        new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("timeout")), 20_000); }),
      ]);
      if (disposed) return;
      live = result;
      if (!result) error = "Предложения пока недоступны. Повторите проверку.";
      else onQuote(row, result);
    } catch { if (!disposed) error = "Не удалось обновить цены. Повторите проверку."; }
    finally { clearTimeout(timer); if (!disposed) loading = false; }
  }
</script>

<section class="order-prices" aria-label="Сравнение с рынком">
  <div class="price-heading"><h3>{row.order.type === "sell" ? "Продавцы в игре" : "Покупатели в игре"}</h3><button class="text-action" disabled={loading || !row.key} onclick={refresh}>{loading ? "Проверяем…" : "Обновить цены"}</button></div>
  {#if !row.key}<p>Вариант предмета не определён. Сравнить цены пока нельзя.</p>
  {:else if error}<p class="error" role="alert">{error}{#if live} Ниже — ранее загруженные предложения.{/if}</p>
  {:else if loading && !live}<p role="status">Загружаем предложения для этого варианта…</p>
  {:else if live}<p class:stale class:checked={!stale}>{stale ? "Сохранённые цены могли устареть" : "Проверено"} · {new Date(live.fetchedAt).toLocaleTimeString("ru-RU", {hour:"2-digit", minute:"2-digit"})}</p>{/if}
  {#if live}
    {#if offers.length}<table><thead><tr><th>Цена / шт.</th><th>{row.order.type === "sell" ? "В наличии" : "Купят"}</th><th>За сделку</th></tr></thead><tbody>{#each offers as offer}<tr><td><strong>{money(offer.platinum / Math.max(1, offer.perTrade))}</strong></td><td>{offer.quantity} шт.</td><td>{offer.perTrade} шт.</td></tr>{/each}</tbody></table>
    {:else}<p>{row.order.type === "sell" ? "В полученном списке нет других продавцов в игре." : "В полученном списке нет других покупателей в игре."}</p>{/if}
  {/if}
</section>

<style>
  .price-heading { display:flex; align-items:center; justify-content:space-between; gap:.5rem; margin-bottom:.55rem; }
  h3 { font-size:.8125rem; margin:0; } p { margin:.4rem 0; font-size:.75rem; color:var(--text-muted); line-height:1.5; }
  .text-action { border:0; background:transparent; box-shadow:none; color:var(--accent); padding:.2rem 0; min-height:1.8rem; font-size:.75rem; white-space:nowrap; }
  .error,.stale { color:var(--danger); } .checked { color:var(--success); }
  table { width:100%; table-layout:fixed; border-collapse:collapse; font-size:.8125rem; font-variant-numeric:tabular-nums; margin-top:.65rem; }
  th,td { padding:.4rem .45rem; border-bottom:1px solid var(--border); text-align:right; } th { background:var(--surface-2); font-size:.75rem; color:var(--text-muted); text-transform:none; letter-spacing:0; } th:first-child,td:first-child { text-align:left; }
</style>
