import type { InventoryView } from "./inventory";
import type { UiLocale } from "./i18n";
import type { SellNowRow, SellNowView } from "./sellNow";

export type LiquidityBand = "unpriced" | "thin" | "limited" | "active";

export interface DashboardSummary {
  ownedCopies: number;
  sellableCopies: number;
  nominalInventoryValue: number;
  nominalSellableValue: number;
  attentionRows: number;
  pricedCoveragePercent: number;
}

export function dashboardSummary(
  inventory: InventoryView,
  sellNow: SellNowView | null,
): DashboardSummary {
  const candidateRows = sellNow?.summary.candidateRows ?? 0;
  const pricedRows = sellNow?.summary.pricedRows ?? 0;
  return {
    ownedCopies: inventory.summary.ownedQuantity,
    sellableCopies: inventory.summary.sellableQuantity,
    nominalInventoryValue: sellNow?.summary.inventoryNominalValue ?? 0,
    nominalSellableValue: sellNow?.summary.nominalValue ?? 0,
    attentionRows: inventory.summary.attentionRows + Math.max(0, candidateRows - pricedRows),
    pricedCoveragePercent:
      candidateRows === 0 ? 0 : Math.round((pricedRows / candidateRows) * 100),
  };
}

export function bestSellRows(rows: SellNowRow[], limit = 4): SellNowRow[] {
  return [...rows]
    .filter((row) => row.recommendation?.fairPrice != null)
    .sort(
      (left, right) =>
        right.priority.score - left.priority.score ||
        left.inventory.displayName.localeCompare(right.inventory.displayName, "ru"),
    )
    .slice(0, Math.max(0, limit));
}

export function liquidityBand(row: SellNowRow): LiquidityBand {
  const volume = row.recommendation?.closedVolume ?? null;
  if (row.recommendation?.fairPrice === null || volume === null) return "unpriced";
  if (volume < 5) return "thin";
  if (volume < 15) return "limited";
  return "active";
}

export function liquidityLabel(band: LiquidityBand, locale: UiLocale = "ru"): string {
  return (locale === "en" ? {
    unpriced: "No reliable price",
    thin: "Thin market",
    limited: "Limited volume",
    active: "Active market",
  } : {
    unpriced: "Нет надёжной цены",
    thin: "Тонкий рынок",
    limited: "Ограниченный объём",
    active: "Активный рынок",
  })[band];
}

export function rowsWorthChecking(rows: SellNowRow[], limit = 5): SellNowRow[] {
  return [...rows]
    .filter((row) => liquidityBand(row) !== "active")
    .sort((left, right) => {
      const bandOrder: Record<LiquidityBand, number> = {
        unpriced: 0,
        thin: 1,
        limited: 2,
        active: 3,
      };
      return (
        bandOrder[liquidityBand(left)] - bandOrder[liquidityBand(right)] ||
        left.inventory.displayName.localeCompare(right.inventory.displayName, "ru")
      );
    })
    .slice(0, Math.max(0, limit));
}
