<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { LivePricingResult, LiveOrderView, MarketVariantKey } from "./market";
  import { marketWhisper } from "./marketWhisper";
  export let itemKey: MarketVariantKey | null = null;
  export let live: LivePricingResult;
  export let itemNameEn = "";
  export let showHeading = true;
  let side: "sell" | "buy" = "sell";
  let message = "";
  const money = (value:number) => value.toLocaleString("ru-RU",{maximumFractionDigits:1}) + " пл.";
  async function profile(offer:LiveOrderView) {
    if (!offer.userSlug) return;
    message = "";
    try { await invoke("open_market_user_profile",{userSlug:offer.userSlug}); }
    catch { message = "Не удалось открыть профиль игрока."; }
  }
  async function copyWhisper(offer:LiveOrderView) {
    if (!offer.userIngameName || !itemNameEn) return;
    const text = marketWhisper(offer,itemNameEn,itemKey);
    if (!text) return;
    try { await navigator.clipboard.writeText(text); message = "Сообщение скопировано. Вставьте его в чат игры."; }
    catch { message = text; }
  }
  $: offers = live.orders.filter(offer => offer.side === side);
</script>

<section class="market-offers" aria-label="Предложения игроков">
  <div class="offer-head">{#if showHeading}<h3>{live.quoteState === "stale_cache" ? "Сохранённые предложения" : "Предложения игроков"}</h3>{/if}<div role="group" aria-label="Тип предложений"><button class="secondary" aria-pressed={side === "sell"} onclick={() => side="sell"}>Продают · {live.sellOrderCount}</button><button class="secondary" aria-pressed={side === "buy"} onclick={() => side="buy"}>Покупают · {live.buyOrderCount}</button></div></div>
  <p class="hint">Лучшие предложения игроков в игре, до 5 с каждой стороны. Количество в списке не означает число всех продавцов или покупателей.</p>
  <div class="offer-list">{#each offers as offer}<div class="offer"><strong>{money(offer.platinum)}</strong><span>{offer.perTrade > 1 ? `за ${offer.perTrade} шт.` : "за штуку"}<small>Всего: {offer.quantity} шт.</small></span><div class="trader">{#if offer.userSlug}<button class="profile" title="Открыть профиль Warframe Market" onclick={() => profile(offer)}>{offer.userIngameName || offer.userSlug} ↗</button>{:else}<span>{offer.userIngameName ?? "Игрок"}</span>{/if}{#if offer.userReputation != null}<small>Репутация: {offer.userReputation}</small>{/if}</div>{#if marketWhisper(offer,itemNameEn,itemKey)}<button class="secondary whisper" onclick={() => copyWhisper(offer)}>Скопировать сообщение</button>{/if}</div>{:else}<p>Подходящих предложений сейчас нет.</p>{/each}</div>
  {#if message}<p class="message" role="status">{message}</p>{/if}
</section>

<style>
  h3,p { margin:0; } h3 { font-size:.9375rem; } p,.hint,small { font-size:.75rem; line-height:1.5; color:var(--text-muted); }
  .hint { margin:.5rem 0; } .offer-head,.offer-head > div { display:flex; align-items:center; justify-content:space-between; gap:.4rem; flex-wrap:wrap; }
  button { min-height:2rem; padding:.3rem .6rem; font-size:.75rem; } button[aria-pressed=true] { background:var(--accent-soft); border-color:var(--accent); }
  .offer { display:flex; align-items:center; gap:.8rem; border-bottom:1px solid var(--border); padding:.45rem 0; font-size:.8125rem; }
  .offer > strong { min-width:4rem; white-space:nowrap; } .offer > span { min-width:5rem; } small { display:block; }
  .trader { flex:1; min-width:0; overflow-wrap:anywhere; } .profile { border:0; padding:0; background:none; color:var(--accent); box-shadow:none; font-size:.8125rem; text-align:left; }
  .message { margin-top:.5rem; overflow-wrap:anywhere; } .whisper { max-width:8rem; line-height:1.3; }
  @media(max-width:650px) { .offer { flex-wrap:wrap; } .whisper { max-width:none; } }
</style>
