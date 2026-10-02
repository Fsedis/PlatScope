import type { RelicPricingCoverage, RelicRefinement } from "./insights";
import type { MarketVariantKey } from "./market";

export type RelicSelectionEra = "lith" | "meso" | "neo" | "axi" | "requiem" | "omnia";

export interface RelicSelectionReward {
  gameRef: string;
  slug: string | null;
  displayName: string;
  displayNameEn: string;
  imageUrl?: string | null;
  chancePercent: number | null;
  price: number | null;
  ducats: number | null;
  ownedQuantity: number | null;
  neededForGoal: boolean;
  neededForCraft: boolean;
  neededForMastery: boolean;
  neededQuantity: number | null;
  goalNames: string[];
  craftingNames: string[];
  masteryNames: string[];
}

export interface RelicSelectionRow {
  key: MarketVariantKey;
  relicSlug: string;
  refinement: RelicRefinement;
  era: RelicSelectionEra;
  displayName: string;
  displayNameEn: string;
  imageUrl: string | null;
  ownedQuantity: number;
  openedQuantity: number;
  remainingQuantity: number | null;
  remainingTotalQuantity: number;
  expectedPlatinum: number | null;
  pricingCoverage: RelicPricingCoverage;
  pricedChancePercent: number;
  expectedDucats: number | null;
  ducatCoveragePercent: number;
  goalChancePercent: number;
  craftingChancePercent: number;
  masteryChancePercent: number;
  rewards: RelicSelectionReward[];
}

export interface RelicSelectionView {
  status: string;
  message: string | null;
  missionName: string | null;
  era: RelicSelectionEra | null;
  inventoryAvailable: boolean;
  selected: RelicSelectionRow | null;
  selectedRelicName: string | null;
  selectedRemainingQuantity: number | null;
  selectedRefinementKnown: boolean;
  selectedBrowsing?: boolean;
  selectedRewards?: RelicSelectionReward[];
  recommendations: RelicSelectionRow[];
  openedThisSession: number | null;
  lastOpenedRelicName?: string | null;
  overlayScale: number;
  theme?: string | null;
  preview?: boolean;
}

export function relicSelectionEraLabel(era: RelicSelectionEra | null): string {
  return era === null ? "Эра не определена" : {
    lith: "Лит", meso: "Мезо", neo: "Нео", axi: "Акси", requiem: "Реквием", omnia: "Все эры",
  }[era];
}

export function relicRemainingLabel(relic: RelicSelectionRow): string {
  return relic.remainingQuantity === null
    ? relic.remainingTotalQuantity == null ? "Остаток не определён" : `Всего этого названия: ${relic.remainingTotalQuantity}`
    : `Осталось: ${relic.remainingQuantity}`;
}

export function relicRewardNeedLabels(reward: RelicSelectionReward): string[] {
  const labels: string[] = [];
  if (reward.neededForGoal) labels.push("Личная цель");
  if (reward.neededForCraft) labels.push("Для сборки");
  if (reward.neededForMastery) labels.push("Для ранга");
  return labels;
}
