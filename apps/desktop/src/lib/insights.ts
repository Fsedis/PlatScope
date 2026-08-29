import type { PriceRecommendation } from "./market";
import { localeCode, type UiLocale } from "./i18n";

export type VaultStatus = "available" | "vaulted" | "unknown";
export type RelicRefinement = "intact" | "exceptional" | "flawless" | "radiant";
export type SetSaleMode =
  | "set"
  | "parts"
  | "equivalent"
  | "insufficient_inventory"
  | "insufficient_pricing";
export type RelicPricingCoverage = "complete" | "partial" | "insufficient";
export type RivenWeaponCategory =
  | "primary"
  | "secondary"
  | "melee"
  | "sentinel_weapon"
  | "arch_gun"
  | "arch_melee";

export interface GameMetadataSnapshotMetadata {
  source: "wfcd_warframe_items";
  fetchedAt: string;
  schemaVersion: number;
  setCount: number;
  relicCount: number;
  primePartCount: number;
  rivenDispositionCount: number;
  itemDefinitionCount: number;
  checksumSha256: string;
}

export interface RivenDispositionDefinition {
  weaponNameEn: string;
  weaponGameRef: string;
  category: RivenWeaponCategory;
  disposition: number;
  multiplier: number;
}

export interface PrimeSetComponentDefinition {
  slug: string;
  gameRef: string;
  requiredQuantity: number;
  ducats: number | null;
  imageUrl?: string | null;
}

export interface PrimeSetDefinition {
  setSlug: string;
  setGameRef: string;
  displayNameEn: string;
  vaultStatus: VaultStatus;
  components: PrimeSetComponentDefinition[];
}

export interface SetComparison {
  setSlug: string;
  completeSets: number;
  setFairValue: number | null;
  partsFairValue: number | null;
  setLiquidityAdjustedValue: number | null;
  partsLiquidityAdjustedValue: number | null;
  setPremiumPercent: number | null;
  recommendedMode: SetSaleMode;
  reasons: string[];
}

export interface SetComponentInsight {
  definition: PrimeSetComponentDefinition;
  itemId?: string | null;
  displayName: string;
  imageUrl?: string | null;
  ownedQuantity: number;
  recommendation: PriceRecommendation | null;
}

export interface SetInsightRow {
  definition: PrimeSetDefinition;
  itemId?: string | null;
  displayName: string;
  imageUrl?: string | null;
  setRecommendation: PriceRecommendation | null;
  comparison: SetComparison;
  components: SetComponentInsight[];
}

export type SetViewMode = "finish" | "ready" | "all";

export interface MissingSetPart {
  slug: string;
  displayName: string;
  quantity: number;
  fairPrice: number | null;
  estimatedCost: number | null;
}

export interface SetOpportunity {
  completeSets: number;
  missingParts: MissingSetPart[];
  missingQuantity: number;
  completionCost: number | null;
  setFairValue: number | null;
  partsFairValue: number | null;
  setPremiumValue: number | null;
  setPremiumPercent: number | null;
  quickToComplete: boolean;
  profitableToComplete: boolean;
}

export interface RelicRewardDefinition {
  rewardSlug: string | null;
  rewardGameRef: string;
  displayNameEn: string;
  chancePercent: number;
}

export interface RelicDefinition {
  relicSlug: string;
  relicGameRef: string;
  displayNameEn: string;
  refinement: RelicRefinement;
  vaultStatus: VaultStatus;
  rewards: RelicRewardDefinition[];
}

export interface RelicRewardInsight {
  definition: RelicRewardDefinition;
  imageUrl?: string | null;
  recommendation: PriceRecommendation | null;
}

export interface RelicExpectedValue {
  pricedExpectedValue: number | null;
  pricedChancePercent: number;
  totalChancePercent: number;
  missingRewardCount: number;
  coverage: RelicPricingCoverage;
  reasons: string[];
}

export interface RelicInsightRow {
  definition: RelicDefinition;
  imageUrl?: string | null;
  ownedQuantity: number;
  sellableQuantity: number;
  relicRecommendation: PriceRecommendation | null;
  expectedValue: RelicExpectedValue;
  rewards: RelicRewardInsight[];
}

export interface PrimePartMetadata {
  slug: string;
  gameRef: string;
  ducats: number;
  vaultStatus: VaultStatus;
}

export interface DucatEfficiency {
  fairPrice: number | null;
  ducats: number;
  platinumPerDucat: number | null;
  credible: boolean;
  reasons: string[];
}

export interface DucatInsightRow {
  metadata: PrimePartMetadata;
  displayName: string;
  imageUrl?: string | null;
  ownedQuantity: number;
  sellableQuantity: number;
  recommendation: PriceRecommendation | null;
  efficiency: DucatEfficiency;
}

export interface InsightsView {
  metadata: GameMetadataSnapshotMetadata;
  inventoryAvailable: boolean;
  sets: SetInsightRow[];
  relics: RelicInsightRow[];
  ducats: DucatInsightRow[];
  rivenDispositions: RivenDispositionDefinition[];
}

export interface GameMetadataRefreshOutcome {
  metadata: GameMetadataSnapshotMetadata;
  stale: boolean;
  usedLkg: boolean;
  warning: string | null;
}

export function setOpportunity(row: SetInsightRow): SetOpportunity {
  const targetSetCount = row.comparison.completeSets + 1;
  const missingParts = row.components.flatMap((component): MissingSetPart[] => {
    const targetQuantity = component.definition.requiredQuantity * targetSetCount;
    const quantity = Math.max(0, targetQuantity - component.ownedQuantity);
    if (quantity === 0) return [];
    const fairPrice = component.recommendation?.fairPrice ?? null;
    return [{
      slug: component.definition.slug,
      displayName: component.displayName,
      quantity,
      fairPrice,
      estimatedCost: fairPrice === null ? null : fairPrice * quantity,
    }];
  });
  const completionCost = missingParts.length > 0 && missingParts.every((part) => part.estimatedCost !== null)
    ? missingParts.reduce((sum, part) => sum + (part.estimatedCost ?? 0), 0)
    : null;
  const setFairValue = row.comparison.setFairValue;
  const partsFairValue = row.comparison.partsFairValue;
  const setPremiumValue = setFairValue !== null && partsFairValue !== null
    ? setFairValue - partsFairValue
    : null;
  const missingQuantity = missingParts.reduce((sum, part) => sum + part.quantity, 0);
  const credibleSetPrice = row.setRecommendation !== null
    && (row.setRecommendation.confidence === "high" || row.setRecommendation.confidence === "medium");
  const credibleMissingPrices = missingParts.every((part) => {
    const component = row.components.find((candidate) => candidate.definition.slug === part.slug);
    const confidence = component?.recommendation?.confidence;
    return confidence === "high" || confidence === "medium";
  });
  const quickToComplete = missingParts.length > 0 && missingParts.length <= 2 && missingQuantity <= 3;
  return {
    completeSets: row.comparison.completeSets,
    missingParts,
    missingQuantity,
    completionCost,
    setFairValue,
    partsFairValue,
    setPremiumValue,
    setPremiumPercent: row.comparison.setPremiumPercent,
    quickToComplete,
    profitableToComplete: quickToComplete
      && credibleSetPrice
      && credibleMissingPrices
      && completionCost !== null
      && setPremiumValue !== null
      && setPremiumValue > 0,
  };
}

export function filterAndSortSets(
  rows: SetInsightRow[],
  mode: SetViewMode,
  query = "",
  locale: UiLocale = "ru",
): SetInsightRow[] {
  const normalizedQuery = query.trim().toLocaleLowerCase(localeCode(locale));
  return rows
    .filter((row) => row.displayName.toLocaleLowerCase(localeCode(locale)).includes(normalizedQuery))
    .filter((row) => {
      const opportunity = setOpportunity(row);
      if (mode === "finish") return opportunity.profitableToComplete;
      if (mode === "ready") return opportunity.completeSets > 0;
      return true;
    })
    .sort((left, right) => {
      const leftOpportunity = setOpportunity(left);
      const rightOpportunity = setOpportunity(right);
      if (mode === "ready") {
        return rightOpportunity.completeSets - leftOpportunity.completeSets
          || nullableOpportunity(rightOpportunity.setPremiumValue) - nullableOpportunity(leftOpportunity.setPremiumValue)
          || left.displayName.localeCompare(right.displayName, localeCode(locale));
      }
      return Number(rightOpportunity.profitableToComplete) - Number(leftOpportunity.profitableToComplete)
        || leftOpportunity.missingParts.length - rightOpportunity.missingParts.length
        || leftOpportunity.missingQuantity - rightOpportunity.missingQuantity
        || nullableOpportunity(rightOpportunity.setPremiumValue) - nullableOpportunity(leftOpportunity.setPremiumValue)
        || left.displayName.localeCompare(right.displayName, localeCode(locale));
    });
}

function nullableOpportunity(value: number | null): number {
  return value ?? Number.NEGATIVE_INFINITY;
}

export function vaultLabel(status: VaultStatus, locale: UiLocale = "ru"): string {
  if (status === "available") return locale === "en" ? "Available" : "Доступно";
  if (status === "vaulted") return locale === "en" ? "Vaulted" : "В хранилище";
  return locale === "en" ? "Status unknown" : "Статус неизвестен";
}

export function refinementLabel(refinement: RelicRefinement, locale: UiLocale = "ru"): string {
  const labels: Record<RelicRefinement, string> = locale === "en" ? {
    intact: "Intact", exceptional: "Exceptional", flawless: "Flawless", radiant: "Radiant",
  } : {
    intact: "Нетронутая",
    exceptional: "Исключительная",
    flawless: "Безупречная",
    radiant: "Сияющая",
  };
  return labels[refinement];
}

export function setModeLabel(mode: SetSaleMode, locale: UiLocale = "ru"): string {
  const labels: Record<SetSaleMode, string> = locale === "en" ? {
    set: "Set is better", parts: "Parts are better", equivalent: "Options are comparable", insufficient_inventory: "Set is incomplete", insufficient_pricing: "Not enough prices",
  } : {
    set: "Выгоднее комплектом",
    parts: "Выгоднее по деталям",
    equivalent: "Варианты сопоставимы",
    insufficient_inventory: "Комплект не собран",
    insufficient_pricing: "Не хватает цен",
  };
  return labels[mode];
}

export function coverageLabel(coverage: RelicPricingCoverage, locale: UiLocale = "ru"): string {
  const labels: Record<RelicPricingCoverage, string> = locale === "en" ? {
    complete: "Complete price coverage", partial: "Partial EV", insufficient: "Not enough prices for EV",
  } : {
    complete: "Цены есть для всех наград",
    partial: "Цены есть для части наград",
    insufficient: "Недостаточно цен для расчёта",
  };
  return labels[coverage];
}

export function formatRatio(value: number | null, locale: UiLocale = "ru"): string {
  return value === null ? "—" : value.toLocaleString(localeCode(locale), { maximumFractionDigits: 3 });
}

export function formatPercent(value: number | null, locale: UiLocale = "ru"): string {
  if (value === null) return "—";
  const sign = value > 0 ? "+" : "";
  return `${sign}${value.toLocaleString(localeCode(locale), { maximumFractionDigits: 1 })}%`;
}

export function rivenCategoryLabel(
  category: RivenWeaponCategory,
  locale: UiLocale = "ru",
): string {
  const labels: Record<RivenWeaponCategory, string> = locale === "en" ? {
    primary: "Primary",
    secondary: "Secondary",
    melee: "Melee",
    sentinel_weapon: "Sentinel weapon",
    arch_gun: "Arch-gun",
    arch_melee: "Arch-melee",
  } : {
    primary: "Основное",
    secondary: "Вторичное",
    melee: "Ближний бой",
    sentinel_weapon: "Оружие стража",
    arch_gun: "Арч-пушка",
    arch_melee: "Арч-ближний бой",
  };
  return labels[category];
}

export function setReasonMessages(row: SetInsightRow, locale: UiLocale = "ru"): string[] {
  if (locale === "ru") return row.comparison.reasons;
  const reasons = [`Current parts can build ${row.comparison.completeSets} complete set(s).`];
  if (row.comparison.setFairValue !== null && row.comparison.partsFairValue !== null) {
    reasons.push(`Set fair value is ${row.comparison.setFairValue.toFixed(1)}p; parts total ${row.comparison.partsFairValue.toFixed(1)}p.`);
  } else {
    reasons.push("Some fair prices are unavailable; missing values were not replaced with zero.");
  }
  reasons.push(setModeLabel(row.comparison.recommendedMode, "en"));
  return reasons;
}

export function relicReasonMessages(row: RelicInsightRow, locale: UiLocale = "ru"): string[] {
  if (locale === "ru") return row.expectedValue.reasons;
  return [
    `${row.expectedValue.pricedChancePercent.toLocaleString("en-US", { maximumFractionDigits: 1 })}% of reward chance has credible prices.`,
    row.expectedValue.coverage === "complete"
      ? "Expected value uses complete credible price coverage."
      : row.expectedValue.coverage === "partial"
        ? "Expected value is partial and is not normalized to 100%."
        : "There is not enough credible price coverage to expose expected value.",
  ];
}
