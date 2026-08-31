import type { LiveOrderView, LiveUserStatus, PriceRecommendation } from "./market";
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
  sellableQuantity: number;
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

export interface SetLiveSellOrder {
  pricePerSet: number;
  quantity: number;
  perTrade: number;
  userStatus: LiveUserStatus;
}

/**
 * Оставляет только исполнимые ордера продажи и переводит цену лота в цену одного сета.
 * Результат ограничен пятью самыми дешёвыми актуальными предложениями.
 */
export function setLiveSellOrders(orders: readonly LiveOrderView[]): SetLiveSellOrder[] {
  const statusPriority: Record<LiveUserStatus, number> = { in_game: 0, online: 1, offline: 2 };
  return orders
    .flatMap((order): SetLiveSellOrder[] => {
      if (order.side !== "sell" || order.userStatus === "offline" || order.platinum <= 0
        || order.quantity <= 0 || order.perTrade <= 0) return [];
      const quantity = order.quantity - order.quantity % order.perTrade;
      if (quantity <= 0) return [];
      return [{
        pricePerSet: order.platinum / order.perTrade,
        quantity,
        perTrade: order.perTrade,
        userStatus: order.userStatus,
      }];
    })
    .sort((left, right) => left.pricePerSet - right.pricePerSet
      || statusPriority[left.userStatus] - statusPriority[right.userStatus]
      || right.quantity - left.quantity)
    .slice(0, 5);
}

export function setLiveMinimumPrice(orders: readonly LiveOrderView[]): number | null {
  return setLiveSellOrders(orders)[0]?.pricePerSet ?? null;
}

export interface SetSellReservation {
  itemId: string | null;
  type: "sell" | "buy";
  quantity: number;
  visible: boolean;
  rank: number | null;
  charges: number | null;
  subtype: string | null;
  amberStars: number | null;
  cyanStars: number | null;
}

export type SetViewMode = "finish" | "ready" | "all";

export interface MissingSetPart {
  slug: string;
  displayName: string;
  quantity: number;
  fairPrice: number | null;
  buyPrice: number | null;
  costBasis: "lowest_ask" | "depth_3" | "depth_5" | "market_estimate" | null;
  estimatedCost: number | null;
}

export interface SetOpportunity {
  completeSets: number;
  sellableCompleteSets: number;
  missingParts: MissingSetPart[];
  missingQuantity: number;
  completionCost: number | null;
  setFairValue: number | null;
  partsFairValue: number | null;
  setPremiumValue: number | null;
  setPremiumPercent: number | null;
  completionRevenue: number | null;
  ownedPartsOpportunityValue: number | null;
  completionProfit: number | null;
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
  /** Баланс следов Пустоты пока отсутствует в снимках старых версий. */
  voidTraces?: number | null;
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
  /** Вероятность получить хотя бы одну полезную деталь. */
  atLeastOneUsefulChancePercent: number;
  /** Вероятность закрыть всю текущую нехватку имеющимися реликвиями. */
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
  grossExpectedPlatinum: number | null;
  squadGrossExpectedPlatinum: number | null;
  relicOpportunityCost: number | null;
  traceOpportunityCost: number;
  /** Чистое матожидание для одиночного открытия. */
  expectedPlatinum: number | null;
  /** Чистое матожидание лучшей награды, если отряд открывает ту же реликвию и улучшение. */
  squadExpectedPlatinum: number | null;
  pricedChancePercent: number;
  completionChancePercent: number;
  progressChancePercent: number;
  completionTargets: RelicSetCompletionTarget[];
  priorityScore: number;
}

export interface RelicRankingContext {
  /** Фактический баланс; null означает, что источник его не предоставляет. */
  availableTraces?: number | null;
  /** Оценка альтернативной стоимости одного следа в платине. */
  tracePlatinumValue?: number;
  /** 1 — соло, 4 — полный публичный отряд. */
  squadSize?: number;
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
  economicValue: number | null;
}

const COMPLETE_RELIC_PRICE_COVERAGE = 99;
const MAX_COMPLETE_RELIC_CHANCE = 101;
const RELIC_CHANCE_TOLERANCE = 0.05;
/** Условная альтернативная стоимость следа: 100 следов = 2p. */
export const DEFAULT_TRACE_PLATINUM_VALUE = 0.02;

/**
 * Ранжирует только реально имеющиеся реликвии и для каждой выбирает осмысленное
 * улучшение. Ценность наград остаётся главным сигналом, а шанс закончить сет
 * добавляет персональный приоритет поверх рыночной цены.
 */
export function rankRelicsToOpen(
  relics: RelicInsightRow[],
  sets: SetInsightRow[],
  context: RelicRankingContext = {},
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

  const tracePlatinumValue = Number.isFinite(context.tracePlatinumValue)
    ? Math.max(0, context.tracePlatinumValue ?? 0)
    : DEFAULT_TRACE_PLATINUM_VALUE;
  const squadSize = Number.isFinite(context.squadSize)
    ? Math.min(4, Math.max(1, Math.trunc(context.squadSize ?? 4)))
    : 4;
  const selected = [...groups.values()].flatMap((group): RelicOpeningOption[] => {
    const options = relicOpeningOptions(
      group,
      completionByReward,
      progressRewards,
      context.availableTraces,
      tracePlatinumValue,
      squadSize,
    );
    if (options.length === 0) return [];
    return [[...options].sort(compareOpeningOptions)[0]];
  });

  const maxEconomicValue = Math.max(
    0,
    ...selected.map((option) => option.economicValue ?? Number.NEGATIVE_INFINITY),
  );
  return selected
    .map(({ economicValue, ...option }) => ({
      ...option,
      priorityScore: economicValue !== null && economicValue > 0 && maxEconomicValue > 0
        ? Math.round(economicValue / maxEconomicValue * 100)
        : 0,
    }))
    .sort((left, right) =>
      right.priorityScore - left.priorityScore
      || nullableOpportunity(right.squadExpectedPlatinum) - nullableOpportunity(left.squadExpectedPlatinum)
      || nullableOpportunity(right.expectedPlatinum) - nullableOpportunity(left.expectedPlatinum)
      || left.displayName.localeCompare(right.displayName, "ru-RU")
    );
}

function relicOpeningOptions(
  group: RelicInsightRow[],
  completionByReward: Map<string, Array<{ setSlug: string; displayName: string; premium: number }>>,
  progressRewards: Set<string>,
  availableTraces: number | null | undefined,
  tracePlatinumValue: number,
  squadSize: number,
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

    const traceCost = refinementTraceCost[targetRefinement] - refinementTraceCost[source.definition.refinement];
    if (availableTraces !== null && availableTraces !== undefined && traceCost > availableTraces) return [];

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
    let grossExpectedPlatinum = 0;
    let pricedChancePercent = 0;
    let totalChancePercent = 0;
    let completionChancePercent = 0;
    let progressChancePercent = 0;
    let expectedSetPremium = 0;
    const pricedOutcomes: Array<{ value: number; probability: number }> = [];
    const economicOutcomes: Array<{ value: number; probability: number }> = [];
    const completionTargets: RelicSetCompletionTarget[] = [];

    for (const reward of rewards) {
      const chancePercent = clampPercent(reward.chancePercent);
      totalChancePercent += chancePercent;
      const price = credibleFairPrice(reward.recommendation);
      let completionPremium = 0;
      if (price !== null) {
        grossExpectedPlatinum += chancePercent / 100 * price;
        pricedChancePercent += chancePercent;
      }
      const rewardSlug = reward.definition.rewardSlug;
      if (!rewardSlug) continue;
      if (progressRewards.has(rewardSlug)) progressChancePercent += chancePercent;
      const targets = completionByReward.get(rewardSlug) ?? [];
      if (targets.length > 0) {
        completionChancePercent += chancePercent;
        completionPremium = Math.max(...targets.map((target) => target.premium));
        expectedSetPremium += chancePercent / 100 * completionPremium;
        for (const target of targets) {
          completionTargets.push({
            setSlug: target.setSlug,
            displayName: target.displayName,
            chancePercent,
          });
        }
      }
      if (price !== null) {
        pricedOutcomes.push({ value: price, probability: chancePercent / 100 });
        economicOutcomes.push({ value: price + completionPremium, probability: chancePercent / 100 });
      }
    }

    const hasCompletePricing = totalChancePercent >= COMPLETE_RELIC_PRICE_COVERAGE
      && totalChancePercent <= MAX_COMPLETE_RELIC_CHANCE
      && totalChancePercent - pricedChancePercent <= RELIC_CHANCE_TOLERANCE;
    const coveredGrossExpectedPlatinum = hasCompletePricing ? grossExpectedPlatinum : null;
    const squadGrossExpectedPlatinum = hasCompletePricing
      ? expectedBestOf(pricedOutcomes, squadSize)
      : null;
    const soloEconomicGross = hasCompletePricing ? grossExpectedPlatinum + expectedSetPremium : null;
    const squadEconomicGross = hasCompletePricing ? expectedBestOf(economicOutcomes, squadSize) : null;
    const relicOpportunityCost = credibleFairPrice(source.relicRecommendation);
    const traceOpportunityCost = traceCost * tracePlatinumValue;
    const expectedPlatinum = soloEconomicGross !== null && relicOpportunityCost !== null
      ? soloEconomicGross - relicOpportunityCost - traceOpportunityCost
      : null;
    const squadExpectedPlatinum = squadEconomicGross !== null && relicOpportunityCost !== null
      ? squadEconomicGross - relicOpportunityCost - traceOpportunityCost
      : null;
    const economicValue = squadSize > 1 ? squadExpectedPlatinum : expectedPlatinum;
    return [{
      relicSlug: representative.definition.relicSlug,
      displayName: representative.displayName,
      imageUrl: representative.imageUrl,
      totalOwnedQuantity,
      sourceQuantity: source.ownedQuantity,
      sourceRefinement: source.definition.refinement,
      recommendedRefinement: targetRefinement,
      traceCost,
      grossExpectedPlatinum: coveredGrossExpectedPlatinum,
      squadGrossExpectedPlatinum,
      relicOpportunityCost,
      traceOpportunityCost,
      expectedPlatinum,
      squadExpectedPlatinum,
      pricedChancePercent: clampPercent(pricedChancePercent),
      completionChancePercent: clampPercent(completionChancePercent),
      progressChancePercent: clampPercent(progressChancePercent),
      completionTargets,
      economicValue,
    }];
  });
  return candidates;
}

function compareOpeningOptions(left: RelicOpeningOption, right: RelicOpeningOption): number {
  return nullableOpportunity(right.economicValue) - nullableOpportunity(left.economicValue)
    || left.traceCost - right.traceCost
    || right.completionChancePercent - left.completionChancePercent
    || refinementIndex(left.recommendedRefinement) - refinementIndex(right.recommendedRefinement);
}

function expectedBestOf(
  outcomes: Array<{ value: number; probability: number }>,
  squadSize: number,
): number {
  const totalProbability = outcomes.reduce((sum, outcome) => sum + outcome.probability, 0);
  if (!(totalProbability > 0)) return 0;
  const scale = totalProbability > 1 ? 1 / totalProbability : 1;
  const grouped = new Map<number, number>();
  for (const outcome of outcomes) {
    grouped.set(outcome.value, (grouped.get(outcome.value) ?? 0) + outcome.probability * scale);
  }
  const coveredProbability = Math.min(1, totalProbability * scale);
  if (coveredProbability < 1) grouped.set(0, 1 - coveredProbability);
  let cumulative = 0;
  let expected = 0;
  for (const [value, probability] of [...grouped.entries()].sort((left, right) => left[0] - right[0])) {
    const next = Math.min(1, cumulative + probability);
    expected += value * (next ** squadSize - cumulative ** squadSize);
    cumulative = next;
  }
  return expected;
}

function credibleFairPrice(recommendation: PriceRecommendation | null | undefined): number | null {
  if (!recommendation || (recommendation.confidence !== "high" && recommendation.confidence !== "medium")) {
    return null;
  }
  const price = recommendation.fairPrice;
  return price !== null && Number.isFinite(price) && price > 0 ? price : null;
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
  const sellableCompleteSets = row.components
    .filter((component) => component.definition.requiredQuantity > 0)
    .reduce<number | null>((count, component) => {
      const componentSets = Math.floor(component.sellableQuantity / component.definition.requiredQuantity);
      return count === null ? componentSets : Math.min(count, componentSets);
    }, null) ?? 0;
  const targetSetCount = sellableCompleteSets + 1;
  const missingParts = row.components.flatMap((component): MissingSetPart[] => {
    const targetQuantity = component.definition.requiredQuantity * targetSetCount;
    const quantity = Math.max(0, targetQuantity - component.sellableQuantity);
    if (quantity === 0) return [];
    const fairPrice = component.recommendation?.fairPrice ?? null;
    const executable = estimatedBuyPrice(component.recommendation, quantity);
    return [{
      slug: component.definition.slug,
      displayName: component.displayName,
      quantity,
      fairPrice,
      buyPrice: executable?.unitPrice ?? null,
      costBasis: executable?.basis ?? null,
      estimatedCost: executable === null ? null : executable.unitPrice * quantity,
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
  const completionRevenue = credibleListingPrice(row.setRecommendation);
  const ownedPartsOpportunityValue = row.components.reduce<number | null>((total, component) => {
    if (total === null) return null;
    const allocatedToCompleteSets = sellableCompleteSets * component.definition.requiredQuantity;
    const availableForNextSet = Math.max(0, component.sellableQuantity - allocatedToCompleteSets);
    const usedForNextSet = Math.min(component.definition.requiredQuantity, availableForNextSet);
    if (usedForNextSet === 0) return total;
    const price = credibleFairPrice(component.recommendation);
    return price === null ? null : total + price * usedForNextSet;
  }, 0);
  const completionProfit = completionRevenue !== null
    && completionCost !== null
    && ownedPartsOpportunityValue !== null
    ? completionRevenue - completionCost - ownedPartsOpportunityValue
    : null;
  const quickToComplete = missingParts.length > 0 && missingParts.length <= 2 && missingQuantity <= 3;
  return {
    completeSets: row.comparison.completeSets,
    sellableCompleteSets,
    missingParts,
    missingQuantity,
    completionCost,
    setFairValue,
    partsFairValue,
    setPremiumValue,
    setPremiumPercent: row.comparison.setPremiumPercent,
    completionRevenue,
    ownedPartsOpportunityValue,
    completionProfit,
    quickToComplete,
    profitableToComplete: sellableCompleteSets === 0
      && quickToComplete
      && completionProfit !== null
      && completionProfit > 0,
  };
}

export function reservePublishedSetListings(
  row: SetInsightRow,
  orders: readonly SetSellReservation[],
  knownSets: readonly SetInsightRow[] = [row],
): SetInsightRow {
  const published = orders.filter((order) => order.visible && order.type === "sell");
  const exactBaseVariant = (order: SetSellReservation): boolean =>
    order.rank === null
      && order.charges === null
      && order.subtype === null
      && order.amberStars === null
      && order.cyanStars === null;
  const reservedSetComponents = new Map<string, number>();
  for (const set of knownSets) {
    const reservedSets = published
      .filter((order) => set.itemId != null
        && order.itemId === set.itemId
        && exactBaseVariant(order))
      .reduce((sum, order) => sum + order.quantity, 0);
    if (reservedSets === 0) continue;
    for (const component of set.components) {
      reservedSetComponents.set(
        component.definition.slug,
        (reservedSetComponents.get(component.definition.slug) ?? 0)
          + reservedSets * component.definition.requiredQuantity,
      );
    }
  }
  const components = row.components.map((component) => {
    const directlyReserved = published
      .filter((order) => component.itemId != null
        && order.itemId === component.itemId
        && exactBaseVariant(order))
      .reduce((sum, order) => sum + order.quantity, 0);
    const reservedForSets = reservedSetComponents.get(component.definition.slug) ?? 0;
    return {
      ...component,
      sellableQuantity: Math.max(
        0,
        component.sellableQuantity - directlyReserved - reservedForSets,
      ),
    };
  });
  const completeSets = components
    .filter((component) => component.definition.requiredQuantity > 0)
    .reduce<number | null>((count, component) => {
      const available = Math.floor(component.sellableQuantity / component.definition.requiredQuantity);
      return count === null ? available : Math.min(count, available);
    }, null) ?? 0;
  return {
    ...row,
    comparison: { ...row.comparison, completeSets },
    components,
  };
}

function estimatedBuyPrice(
  recommendation: PriceRecommendation | null | undefined,
  quantity: number,
): { unitPrice: number; basis: MissingSetPart["costBasis"] } | null {
  if (!recommendation || quantity <= 0 || recommendation.freshness === "stale"
    || recommendation.freshness === "unknown") {
    return null;
  }
  const candidate = quantity === 1
    ? { price: recommendation.lowestAsk, basis: "lowest_ask" as const }
    : quantity <= 3
      ? { price: recommendation.depthThree, basis: "depth_3" as const }
      : quantity <= 5
        ? { price: recommendation.depthPrice, basis: "depth_5" as const }
        : null;
  if (candidate && candidate.price !== null && Number.isFinite(candidate.price) && candidate.price > 0) {
    return { unitPrice: candidate.price, basis: candidate.basis };
  }
  const marketEstimate = credibleListingPrice(recommendation);
  return marketEstimate === null
    ? null
    : { unitPrice: marketEstimate, basis: "market_estimate" };
}

function credibleListingPrice(recommendation: PriceRecommendation | null | undefined): number | null {
  if (!recommendation || (recommendation.confidence !== "high" && recommendation.confidence !== "medium")) {
    return null;
  }
  const price = recommendation.listPrice ?? recommendation.fairPrice;
  return price !== null && Number.isFinite(price) && price > 0 ? price : null;
}

export function setRelicSupport(
  row: SetInsightRow,
  relics: RelicInsightRow[],
): SetRelicSupport {
  const opportunity = setOpportunity(row);
  const missingBySlug = new Map(opportunity.missingParts.map((part) => [part.slug, part]));
  const coveredSlugs = new Set<string>();
  const openings: RelicOpeningDistribution[] = [];

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
    const localNeeds = new Map(usefulRewards.map((reward) => [reward.slug, reward.quantityNeeded]));
    const localDistribution = simulateRelicDrops(localNeeds, [{
      copies: relic.ownedQuantity,
      rewards: usefulRewards.map((reward) => ({ slug: reward.slug, chancePercent: reward.chancePercent })),
    }]);
    const expectedFromRelic = expectedDropsFromDistribution(localDistribution, localNeeds);
    openings.push({
      copies: relic.ownedQuantity,
      rewards: usefulRewards.map((reward) => ({ slug: reward.slug, chancePercent: reward.chancePercent })),
    });
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

  const distribution = simulateRelicDrops(
    new Map([...missingBySlug].map(([slug, part]) => [slug, part.quantity])),
    openings,
  );
  const zeroKey = [...missingBySlug].map(() => 0).join(",");
  const completeKey = [...missingBySlug.values()].map((part) => part.quantity).join(",");
  const noUsefulDropProbability = distribution.get(zeroKey) ?? 1;
  const allMissingDropProbability = missingBySlug.size > 0 ? distribution.get(completeKey) ?? 0 : 0;
  const expectedUsefulDrops = expectedDropsFromDistribution(
    distribution,
    new Map([...missingBySlug].map(([slug, part]) => [slug, part.quantity])),
  );

  return {
    matches,
    ownedRelicCount: matches.reduce((sum, match) => sum + match.relic.ownedQuantity, 0),
    coveredPartCount: coveredSlugs.size,
    missingPartCount: missingBySlug.size,
    allMissingPartsCovered: missingBySlug.size > 0 && coveredSlugs.size === missingBySlug.size,
    atLeastOneUsefulChancePercent: clampPercent((1 - noUsefulDropProbability) * 100),
    aggregateChancePercent: clampPercent(allMissingDropProbability * 100),
    expectedUsefulDrops,
  };
}

interface RelicOpeningDistribution {
  copies: number;
  rewards: Array<{ slug: string; chancePercent: number }>;
}

function simulateRelicDrops(
  needsBySlug: Map<string, number>,
  openings: RelicOpeningDistribution[],
): Map<string, number> {
  const slugs = [...needsBySlug.keys()];
  const needs = slugs.map((slug) => Math.max(0, Math.trunc(needsBySlug.get(slug) ?? 0)));
  let states = new Map<string, number>([[slugs.map(() => 0).join(","), 1]]);
  for (const opening of openings) {
    const outcomes = opening.rewards.flatMap((reward) => {
      const index = slugs.indexOf(reward.slug);
      const probability = clampPercent(reward.chancePercent) / 100;
      return index >= 0 && probability > 0 ? [{ index, probability }] : [];
    });
    const usefulTotal = outcomes.reduce((sum, outcome) => sum + outcome.probability, 0);
    const scale = usefulTotal > 1 ? 1 / usefulTotal : 1;
    const missProbability = Math.max(0, 1 - usefulTotal * scale);
    for (let copy = 0; copy < opening.copies; copy += 1) {
      const next = new Map<string, number>();
      for (const [key, stateProbability] of states) {
        addProbability(next, key, stateProbability * missProbability);
        const counts = key === "" ? [] : key.split(",").map(Number);
        for (const outcome of outcomes) {
          const updated = [...counts];
          updated[outcome.index] = Math.min(needs[outcome.index], (updated[outcome.index] ?? 0) + 1);
          addProbability(next, updated.join(","), stateProbability * outcome.probability * scale);
        }
      }
      states = next;
    }
  }
  return states;
}

function addProbability(states: Map<string, number>, key: string, probability: number): void {
  if (probability <= 0) return;
  states.set(key, (states.get(key) ?? 0) + probability);
}

function expectedDropsFromDistribution(
  distribution: Map<string, number>,
  needsBySlug: Map<string, number>,
): number {
  const maximum = [...needsBySlug.values()].reduce((sum, quantity) => sum + quantity, 0);
  const expected = [...distribution].reduce((sum, [key, probability]) => {
    const drops = key === "" ? 0 : key.split(",").reduce((count, value) => count + Number(value), 0);
    return sum + probability * drops;
  }, 0);
  return Math.min(maximum, expected);
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
      if (mode === "ready") return opportunity.sellableCompleteSets > 0;
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
        return rightOpportunity.sellableCompleteSets - leftOpportunity.sellableCompleteSets
          || nullableOpportunity(rightOpportunity.setPremiumValue) - nullableOpportunity(leftOpportunity.setPremiumValue)
          || left.displayName.localeCompare(right.displayName, localeCode(locale));
      }
      return nullableOpportunity(rightOpportunity.completionProfit) - nullableOpportunity(leftOpportunity.completionProfit)
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
      if (mode === "ready") return opportunity.sellableCompleteSets > 0;
      return true;
    })
    .sort((left, right) => {
      const leftOpportunity = setOpportunity(left);
      const rightOpportunity = setOpportunity(right);
      if (mode === "ready") {
        return rightOpportunity.sellableCompleteSets - leftOpportunity.sellableCompleteSets
          || nullableOpportunity(rightOpportunity.setPremiumValue) - nullableOpportunity(leftOpportunity.setPremiumValue)
          || left.displayName.localeCompare(right.displayName, localeCode(locale));
      }
      return Number(rightOpportunity.profitableToComplete) - Number(leftOpportunity.profitableToComplete)
        || nullableOpportunity(rightOpportunity.completionProfit) - nullableOpportunity(leftOpportunity.completionProfit)
        || leftOpportunity.missingParts.length - rightOpportunity.missingParts.length
        || leftOpportunity.missingQuantity - rightOpportunity.missingQuantity
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
