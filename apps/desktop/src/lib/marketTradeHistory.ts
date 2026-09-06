import type { AccountView } from "./account";
import {
  isSaleTrade,
  normalizeTradeName,
  planTradeReconciliation,
  type TradeEvent,
  type TradeItem,
  type TradeReconciliationAction,
} from "./tradeShift";

export type TradeHistoryKind = "all" | "sell" | "buy" | "exchange";
export type TradeHistoryPeriod = "all" | "7" | "30";
export interface HistoryItem {
  name: string;
  english: string | null;
  quantity: number;
  direction: "given" | "received";
}

export function tradeHistoryKind(event: TradeEvent): Exclude<TradeHistoryKind, "all"> {
  if (isSaleTrade(event)) return "sell";
  if (event.platinumGiven > 0 && event.platinumReceived === 0
    && event.receivedItems.length > 0 && event.givenItems.length === 0) return "buy";
  return "exchange";
}

export function tradeReconciliationActions(event: TradeEvent): TradeReconciliationAction[] {
  if (!event.reconciliationJson) return [];
  try {
    const parsed: unknown = JSON.parse(event.reconciliationJson);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((value): value is TradeReconciliationAction => value
      && typeof value === "object" && ["close", "update", "delete"].includes(value.kind)
      && typeof value.itemName === "string" && typeof value.soldQuantity === "number"
      && value.before && typeof value.before === "object");
  } catch {
    return [];
  }
}

export function tradeWasClosed(event: TradeEvent): boolean {
  return tradeReconciliationActions(event).some((action) => action.kind === "close");
}

export function historyItems(event: TradeEvent, account: AccountView | null): HistoryItem[] {
  const names = Object.values(account?.orderItems ?? {}).flatMap((item) => [item, ...(item.setComponents ?? [])]);
  const localize = (item: TradeItem, direction: HistoryItem["direction"]): HistoryItem => {
    const normalized = normalizeTradeName(item.name);
    const matched = names.find((candidate) => [candidate.displayName, candidate.displayNameEn]
      .some((name) => normalizeTradeName(name) === normalized));
    const name = matched?.displayName ?? item.name;
    const english = matched?.displayNameEn?.trim();
    return { name, english: english && english !== name ? english : null, quantity: item.quantity, direction };
  };
  let given = event.givenItems.map((item) => localize(item, "given"));
  // Учитываем уже проверенное сопоставление, но сохраняем исходные строки при
  // неполном плане: история не должна терять часть обмена.
  if (isSaleTrade(event)) {
    const plan = event.status === "pending" && account ? planTradeReconciliation(event, account) : null;
    const actions = plan && !plan.unmatched.length && !plan.unsafe.length
      ? plan.actions : tradeReconciliationActions(event);
    if (actions.length === event.givenItems.length) {
      given = actions.map((action) => localize({ name: action.itemName, quantity: action.soldQuantity }, "given"));
    }
  }
  return [...given, ...event.receivedItems.map((item) => localize(item, "received"))];
}

export function filterTradeHistory(
  events: readonly TradeEvent[],
  query: string,
  kind: TradeHistoryKind,
  period: TradeHistoryPeriod,
  account: AccountView | null,
  now = Date.now(),
): TradeEvent[] {
  const terms = normalizeTradeName(query).split(/\s+/).filter(Boolean);
  const cutoff = period === "all" ? null : now - Number(period) * 86_400_000;
  return events.filter((event) => {
    if (kind !== "all" && tradeHistoryKind(event) !== kind) return false;
    const timestamp = Date.parse(event.occurredAt);
    if (cutoff !== null && (!Number.isFinite(timestamp) || timestamp < cutoff || timestamp > now)) return false;
    const haystack = normalizeTradeName([
      event.partner ?? "", ...historyItems(event, account).flatMap((item) => [item.name, item.english ?? ""]),
      ...event.givenItems.map((item) => item.name), ...event.receivedItems.map((item) => item.name),
    ].join(" "));
    return terms.every((term) => haystack.includes(term));
  }).sort((a, b) => (Date.parse(b.occurredAt) || 0) - (Date.parse(a.occurredAt) || 0) || b.id - a.id);
}

function csvCell(value: string | number): string {
  const string = String(value);
  // CSV открывают в Excel: имена из игрового журнала не должны стать формулой.
  const safe = /^[\s]*[=+\-@]/.test(string) ? `'${string}` : string;
  return `"${safe.replaceAll('"', '""')}"`;
}

export function tradeHistoryCsv(events: readonly TradeEvent[], account: AccountView | null): string {
  const titles = { sell: "Продажа", buy: "Покупка", exchange: "Обмен" };
  const rows: (string | number)[][] = [["Дата", "Сделка", "Игрок", "Получено платины", "Отдано платины", "Отдано предметов", "Получено предметов", "Учёт на рынке"]];
  for (const event of events) {
    const items = historyItems(event, account);
    const itemText = (direction: HistoryItem["direction"]) => items.filter((item) => item.direction === direction)
      .map((item) => `${item.name}${item.english ? ` / ${item.english}` : ""} ×${item.quantity}`).join("; ");
    rows.push([event.occurredAt, titles[tradeHistoryKind(event)], event.partner ?? "", event.platinumReceived,
      event.platinumGiven, itemText("given"), itemText("received"),
      event.status === "reconciled" ? "Учтено" : event.status === "ignored" ? "Без изменений" : "Не учтено"]);
  }
  return `\uFEFF${rows.map((row) => row.map(csvCell).join(";")).join("\r\n")}\r\n`;
}
