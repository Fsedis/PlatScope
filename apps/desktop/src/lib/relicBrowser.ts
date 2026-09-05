import { chanceAtRefinement, componentAvailableQuantity, setOpportunity, type RelicInsightRow, type RelicOpeningRecommendation, type RelicOverviewScenario, type SetInsightRow } from "./insights";
import type { PriceRecommendation } from "./market";

export type RelicSort = "value" | "progress" | "owned";
/** Нужными называем детали начатого следующего сета, не весь каталог с нуля. */
export function relicProgressSets(sets: SetInsightRow[]): SetInsightRow[] {
  return sets.filter(row => {
    const complete = setOpportunity(row).availableCompleteSets;
    return row.components.some(part => componentAvailableQuantity(part) > complete * part.definition.requiredQuantity);
  });
}
export function relicNet(row: RelicOpeningRecommendation, scenario: RelicOverviewScenario): number | null {
  return scenario === "solo" ? row.expectedPlatinum : row.squadExpectedPlatinum;
}
export function rewardChoiceChance(chance: number, scenario: RelicOverviewScenario): number {
  const probability = Math.min(1, Math.max(0, Number.isFinite(chance) ? chance / 100 : 0));
  return 100 * (1 - (1 - probability) ** (scenario === "solo" ? 1 : 4));
}
export function knownRewardPrice(price: PriceRecommendation | null): number | null {
  return price && ["fresh", "aging"].includes(price.freshness) && ["high", "medium"].includes(price.confidence)
    && price.fairPrice !== null && Number.isFinite(price.fairPrice) && price.fairPrice > 0 ? price.fairPrice : null;
}
export function filterRelicChoices(rows: RelicOpeningRecommendation[], relics: RelicInsightRow[], query: string, sort: RelicSort, scenario: RelicOverviewScenario): RelicOpeningRecommendation[] {
  const search = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  const names = new Map<string, string>();
  for (const relic of relics) {
    const slug = relic.definition.relicSlug;
    names.set(slug, `${names.get(slug) ?? ""} ${relic.displayName} ${relic.definition.displayNameEn} ${slug} ${relic.rewards.map(reward => `${reward.displayName} ${reward.definition.displayNameEn}`).join(" ")}`.toLocaleLowerCase());
  }
  const value = (row: RelicOpeningRecommendation) => relicNet(row, scenario) ?? -Infinity;
  return rows.filter(row => search.every(word => (names.get(row.relicSlug) ?? row.displayName.toLocaleLowerCase()).includes(word)))
    .sort((a,b) => (sort === "progress" ? b.progressChancePercent - a.progressChancePercent : sort === "owned" ? b.totalOwnedQuantity - a.totalOwnedQuantity : 0)
      || value(b) - value(a) || a.displayName.localeCompare(b.displayName));
}

/** Те же исходные награды и улучшение, что использованы в рейтинге. */
export function selectedRelicRewards(selected: RelicOpeningRecommendation, relics: RelicInsightRow[], sets: SetInsightRow[]) {
  const group = relics.filter(row => row.definition.relicSlug === selected.relicSlug);
  const exact = group.find(row => row.definition.refinement === selected.recommendedRefinement);
  const source = exact ?? group[0];
  if (!source) return [];
  const needs = relicProgressSets(sets).map(row => ({ row, missing: setOpportunity(row).missingParts }));
  return source.rewards.map(reward => ({
    ...reward,
    chance: exact ? reward.definition.chancePercent : chanceAtRefinement(reward.definition.chancePercent, source.definition.refinement, selected.recommendedRefinement),
    price: knownRewardPrice(reward.recommendation),
    targets: needs.filter(({missing}) => missing.some(part => part.slug === reward.definition.rewardSlug))
      .map(({row,missing}) => ({slug:row.definition.setSlug,name:row.displayName,finishes:missing.length === 1 && missing[0].quantity === 1})),
  })).sort((a,b) => b.targets.length - a.targets.length || (b.price ?? -1) - (a.price ?? -1));
}
