import type { LivePricingResult } from "./market";

export type CheckedPrice = { state: "priced"; price: number } | { state: "empty" | "failed"; price: null };

/** Сохранённый ответ после сетевой ошибки не подтверждает текущую цену. */
export function checkedLivePrice(result: LivePricingResult | null): CheckedPrice {
  if (result?.quoteState === "stale_cache") return { state: "failed", price: null };
  const price = result?.recommendation.listPrice ?? result?.recommendation.fairPrice;
  return price != null && Number.isFinite(price) && price > 0
    ? { state: "priced", price }
    : { state: "empty", price: null };
}

export function priceCheckSummary(updated: number, empty: number, failed: number): string {
  if (failed && !updated && !empty) return "Не удалось проверить цены. Сохранённые оценки оставлены; повторите проверку.";
  const parts: string[] = [];
  if (updated) parts.push(`Цены проверены: ${updated}`);
  if (empty) parts.push(`без подходящих предложений: ${empty}`);
  if (failed) parts.push(`не удалось проверить: ${failed}`);
  return `${parts.join(" · ")}.`;
}
