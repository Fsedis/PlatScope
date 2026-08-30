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
  displayName: string;
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
  displayName: string;
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
}

export interface GameMetadataRefreshOutcome {
  metadata: GameMetadataSnapshotMetadata;
  stale: boolean;
  usedLkg: boolean;
  warning: string | null;
}

export type SetOpportunityMode = "relics" | "buy" | "ready";

export interface UsefulRelicReward {
  slug: string;
  displayName: string;
  quantityNeeded: number;
  chancePercent: number;
  imageUrl?: string | null;
}

export interface SetRelicMatch {
  relic: RelicInsightRow;
  usefulRewards: UsefulRelicReward[];
  chancePerRelicPercent: number;
  chanceFromOwnedPercent: number;
  expectedUsefulDrops: number;
}

export interface SetRelicSupport {
  matches: SetRelicMatch[];
  ownedRelicCount: number;
  coveredPartCount: number;
  missingPartCount: number;
  allMissingPartsCovered: boolean;
  aggregateChancePercent: number;
  expectedUsefulDrops: number;
}

export interface RelicSetCompletionTarget {
  setSlug: string;
  displayName: string;
  chancePercent: number;
}

export interface RelicOpeningRecommendation {
  relicSlug: string;
  displayName: string;
  imageUrl?: string | null;
  totalOwnedQuantity: number;
  sourceQuantity: number;
  sourceRefinement: RelicRefinement;
  recommendedRefinement: RelicRefinement;
  traceCost: number;
  expectedPlatinum: number | null;
  pricedChancePercent: number;
  completionChancePercent: number;
  progressChancePercent: number;
  completionTargets: RelicSetCompletionTarget[];
  priorityScore: number;
}

const relicRefinements: RelicRefinement[] = ["intact", "exceptional", "flawless", "radiant"];
const refinementTraceCost: Record<RelicRefinement, number> = {
  intact: 0,
  exceptional: 25,
  flawless: 50,
  radiant: 100,
};
const rewardChanceByRefinement = {
  intact: { common: 25.33, uncommon: 11, rare: 2 },
  exceptional: { common: 23.33, uncommon: 13, rare: 4 },
  flawless: { common: 20, uncommon: 17, rare: 6 },
  radiant: { common: 16.67, uncommon: 20, rare: 10 },
} as const;

type RelicRewardRarity = keyof typeof rewardChanceByRefinement.intact;

interface RelicOpeningOption extends Omit<RelicOpeningRecommendation, "priorityScore"> {
  economicValue: number;
  optionScore: number;
}

/**
 * Ранжирует только реально имеющиеся реликвии и для каждой выбирает осмысленное
 * улучшение. Ценность наград остаётся главным сигналом, а шанс закончить сет
 * добавляет персональный приоритет поверх рыночной цены.
 */
export function rankRelicsToOpen(
  relics: RelicInsightRow[],
  sets: SetInsightRow[],
): RelicOpeningRecommendation[] {
  const groups = new Map<string, RelicInsightRow[]>();
  for (const relic of relics) {
    if (relic.ownedQuantity <= 0) continue;
    const group = groups.get(relic.definition.relicSlug) ?? [];
    group.push(relic);
    groups.set(relic.definition.relicSlug, group);
  }

  const completionByReward = new Map<string, Array<{ setSlug: string; displayName: string; premium: number }>>();
  const progressRewards = new Set<string>();
  for (const set of sets) {
    const opportunity = setOpportunity(set);
    for (const part of opportunity.missingParts) progressRewards.add(part.slug);
    if (opportunity.missingQuantity !== 1 || opportunity.missingParts.length !== 1) continue;
    const rewardSlug = opportunity.missingParts[0].slug;
    const targets = completionByReward.get(rewardSlug) ?? [];
    targets.push({
      setSlug: set.definition.setSlug,
      displayName: set.displayName,
      premium: Math.max(0, opportunity.setPremiumValue ?? 0),
    });
    completionByReward.set(rewardSlug, targets);
  }

  const selected = [...groups.values()].flatMap((group): RelicOpeningOption[] => {
    const options = relicOpeningOptions(group, completionByReward, progressRewards);
    if (options.length === 0) return [];
    const freeOption = options
      .filter((option) => option.traceCost === 0)
      .sort(compareOpeningOptions)[0] ?? options.sort(compareOpeningOptions)[0];
    const eligible = options.filter((option) => {
      if (option.traceCost === 0 || option.optionScore <= freeOption.optionScore) return option.traceCost === 0;
      const traceSteps = option.traceCost / 25;
      const economicGain = option.economicValue - freeOption.economicValue;
      const completionGain = option.completionChancePercent - freeOption.completionChancePercent;
      return economicGain >= traceSteps * 0.5 || completionGain >= traceSteps;
    });
    return [eligible.sort(compareOpeningOptions)[0] ?? freeOption];
  });

  const maxEconomicValue = Math.max(0, ...selected.map((option) => option.economicValue));
  return selected
    .map(({ economicValue, optionScore: _optionScore, ...option }) => ({
      ...option,
      priorityScore: Math.round(
        (maxEconomicValue > 0 ? economicValue / maxEconomicValue * 70 : 0)
        + option.completionChancePercent * 0.3,
      ),
    }))
    .sort((left, right) =>
      right.priorityScore - left.priorityScore
      || nullableOpportunity(right.expectedPlatinum) - nullableOpportunity(left.expectedPlatinum)
      || right.completionChancePercent - left.completionChancePercent
      || right.progressChancePercent - left.progressChancePercent
      || left.displayName.localeCompare(right.displayName, "ru-RU")
    );
}

function relicOpeningOptions(
  group: RelicInsightRow[],
  completionByReward: Map<string, Array<{ setSlug: string; displayName: string; premium: number }>>,
  progressRewards: Set<string>,
): RelicOpeningOption[] {
  const representative = group[0];
  const totalOwnedQuantity = group.reduce((sum, relic) => sum + relic.ownedQuantity, 0);
  const candidates = relicRefinements.flatMap((targetRefinement): RelicOpeningOption[] => {
    const source = group
      .filter((relic) => refinementIndex(relic.definition.refinement) <= refinementIndex(targetRefinement))
      .sort((left, right) =>
        refinementIndex(right.definition.refinement) - refinementIndex(left.definition.refinement)
        || right.ownedQuantity - left.ownedQuantity
      )[0];
    if (!source) return [];

    const exactTarget = group.find((relic) => relic.definition.refinement === targetRefinement);
    const rewards = (exactTarget ?? representative).rewards.map((reward) => ({
      ...reward,
      chancePercent: exactTarget
        ? reward.definition.chancePercent
        : chanceAtRefinement(
            reward.definition.chancePercent,
            representative.definition.refinement,
            targetRefinement,
          ),
    }));
    let expectedPlatinum = 0;
    let pricedChancePercent = 0;
    let completionChancePercent = 0;
    let progressChancePercent = 0;
    let expectedSetPremium = 0;
    const completionTargets: RelicSetCompletionTarget[] = [];

    for (const reward of rewards) {
      const chancePercent = clampPercent(reward.chancePercent);
      const price = reward.recommendation?.fairPrice;
      if (price !== null && price !== undefined && Number.isFinite(price) && price > 0
        && reward.recommendation?.confidence !== "unknown") {
        expectedPlatinum += chancePercent / 100 * price;
        pricedChancePercent += chancePercent;
      }
      const rewardSlug = reward.definition.rewardSlug;
      if (!rewardSlug) continue;
      if (progressRewards.has(rewardSlug)) progressChancePercent += chancePercent;
      const targets = completionByReward.get(rewardSlug) ?? [];
      if (targets.length === 0) continue;
      completionChancePercent += chancePercent;
      expectedSetPremium += chancePercent / 100 * Math.max(...targets.map((target) => target.premium));
      for (const target of targets) {
        completionTargets.push({
          setSlug: target.setSlug,
          displayName: target.displayName,
          chancePercent,
        });
      }
    }

    const coveredExpectedPlatinum = pricedChancePercent >= 50 ? expectedPlatinum : null;
    const economicValue = (coveredExpectedPlatinum ?? 0) + expectedSetPremium;
    return [{
      relicSlug: representative.definition.relicSlug,
      displayName: representative.displayName,
      imageUrl: representative.imageUrl,
      totalOwnedQuantity,
      sourceQuantity: source.ownedQuantity,
      sourceRefinement: source.definition.refinement,
      recommendedRefinement: targetRefinement,
      traceCost: refinementTraceCost[targetRefinement] - refinementTraceCost[source.definition.refinement],
      expectedPlatinum: coveredExpectedPlatinum,
      pricedChancePercent: clampPercent(pricedChancePercent),
      completionChancePercent: clampPercent(completionChancePercent),
      progressChancePercent: clampPercent(progressChancePercent),
      completionTargets,
      economicValue,
      optionScore: 0,
    }];
  });

  const maxEconomicValue = Math.max(0, ...candidates.map((candidate) => candidate.economicValue));
  return candidates.map((candidate) => ({
    ...candidate,
    optionScore: (maxEconomicValue > 0 ? candidate.economicValue / maxEconomicValue * 70 : 0)
      + candidate.completionChancePercent * 0.3,
  }));
}

function compareOpeningOptions(left: RelicOpeningOption, right: RelicOpeningOption): number {
  return right.optionScore - left.optionScore
    || right.economicValue - left.economicValue
    || right.completionChancePercent - left.completionChancePercent
    || left.traceCost - right.traceCost
    || refinementIndex(left.recommendedRefinement) - refinementIndex(right.recommendedRefinement);
}

function chanceAtRefinement(
  chancePercent: number,
  currentRefinement: RelicRefinement,
  targetRefinement: RelicRefinement,
): number {
  const currentChances = rewardChanceByRefinement[currentRefinement];
  const rarity = (Object.keys(currentChances) as RelicRewardRarity[])
    .map((candidate) => ({ candidate, distance: Math.abs(currentChances[candidate] - chancePercent) }))
    .sort((left, right) => left.distance - right.distance)[0];
  if (!rarity || rarity.distance > 1.1) return chancePercent;
  return rewardChanceByRefinement[targetRefinement][rarity.candidate];
}

function refinementIndex(refinement: RelicRefinement): number {
  return relicRefinements.indexOf(refinement);
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

export function setRelicSupport(
  row: SetInsightRow,
  relics: RelicInsightRow[],
): SetRelicSupport {
  const opportunity = setOpportunity(row);
  const missingBySlug = new Map(opportunity.missingParts.map((part) => [part.slug, part]));
  const coveredSlugs = new Set<string>();
  let noUsefulDropProbability = 1;
  let expectedUsefulDrops = 0;

  const matches = relics.flatMap((relic): SetRelicMatch[] => {
    if (relic.ownedQuantity <= 0) return [];
    const usefulRewards = relic.rewards.flatMap((reward): UsefulRelicReward[] => {
      const slug = reward.definition.rewardSlug;
      if (!slug) return [];
      const missingPart = missingBySlug.get(slug);
      if (!missingPart) return [];
      const chancePercent = clampPercent(reward.definition.chancePercent);
      if (chancePercent <= 0) return [];
      coveredSlugs.add(slug);
      return [{
        slug,
        displayName: reward.displayName,
        quantityNeeded: missingPart.quantity,
        chancePercent,
        imageUrl: reward.imageUrl,
      }];
    });
    if (usefulRewards.length === 0) return [];

    const chancePerRelicPercent = clampPercent(
      usefulRewards.reduce((sum, reward) => sum + reward.chancePercent, 0),
    );
    const missChance = 1 - chancePerRelicPercent / 100;
    const chanceFromOwnedPercent = (1 - missChance ** relic.ownedQuantity) * 100;
    const expectedFromRelic = relic.ownedQuantity * chancePerRelicPercent / 100;
    noUsefulDropProbability *= missChance ** relic.ownedQuantity;
    expectedUsefulDrops += expectedFromRelic;
    return [{
      relic,
      usefulRewards,
      chancePerRelicPercent,
      chanceFromOwnedPercent,
      expectedUsefulDrops: expectedFromRelic,
    }];
  }).sort((left, right) =>
    right.usefulRewards.length - left.usefulRewards.length
    || right.chanceFromOwnedPercent - left.chanceFromOwnedPercent
    || right.relic.ownedQuantity - left.relic.ownedQuantity
    || left.relic.displayName.localeCompare(right.relic.displayName, "ru-RU")
  );

  return {
    matches,
    ownedRelicCount: matches.reduce((sum, match) => sum + match.relic.ownedQuantity, 0),
    coveredPartCount: coveredSlugs.size,
    missingPartCount: missingBySlug.size,
    allMissingPartsCovered: missingBySlug.size > 0 && coveredSlugs.size === missingBySlug.size,
    aggregateChancePercent: clampPercent((1 - noUsefulDropProbability) * 100),
    expectedUsefulDrops,
  };
}

export function filterAndSortOpportunitySets(
  rows: SetInsightRow[],
  relics: RelicInsightRow[],
  mode: SetOpportunityMode,
  query = "",
  locale: UiLocale = "ru",
): SetInsightRow[] {
  const normalizedQuery = query.trim().toLocaleLowerCase(localeCode(locale));
  return rows
    .filter((row) => row.displayName.toLocaleLowerCase(localeCode(locale)).includes(normalizedQuery))
    .filter((row) => {
      const opportunity = setOpportunity(row);
      if (mode === "ready") return opportunity.completeSets > 0;
      if (mode === "buy") return opportunity.profitableToComplete;
      return opportunity.missingParts.length > 0 && setRelicSupport(row, relics).matches.length > 0;
    })
    .sort((left, right) => {
      const leftOpportunity = setOpportunity(left);
      const rightOpportunity = setOpportunity(right);
      if (mode === "relics") {
        const leftRelics = setRelicSupport(left, relics);
        const rightRelics = setRelicSupport(right, relics);
        return Number(rightRelics.allMissingPartsCovered) - Number(leftRelics.allMissingPartsCovered)
          || rightRelics.coveredPartCount - leftRelics.coveredPartCount
          || rightRelics.aggregateChancePercent - leftRelics.aggregateChancePercent
          || nullableOpportunity(rightOpportunity.setPremiumValue) - nullableOpportunity(leftOpportunity.setPremiumValue)
          || left.displayName.localeCompare(right.displayName, localeCode(locale));
      }
      if (mode === "ready") {
        return rightOpportunity.completeSets - leftOpportunity.completeSets
          || nullableOpportunity(rightOpportunity.setPremiumValue) - nullableOpportunity(leftOpportunity.setPremiumValue)
          || left.displayName.localeCompare(right.displayName, localeCode(locale));
      }
      return nullableOpportunity(rightOpportunity.setPremiumValue) - nullableOpportunity(leftOpportunity.setPremiumValue)
        || leftOpportunity.missingQuantity - rightOpportunity.missingQuantity
        || nullableCost(leftOpportunity.completionCost) - nullableCost(rightOpportunity.completionCost)
        || left.displayName.localeCompare(right.displayName, localeCode(locale));
    });
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(100, Math.max(0, value));
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

function nullableCost(value: number | null): number {
  return value ?? Number.POSITIVE_INFINITY;
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
