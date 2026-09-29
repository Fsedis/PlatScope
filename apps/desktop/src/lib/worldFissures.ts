import { chanceAtRefinement, rankRelicsToOpen, type InsightsView, type RelicOpeningRecommendation } from "./insights";
import { relicProgressSets } from "./relicBrowser";
import type { PersonalGoalsView } from "./personalGoals";
import { periodState, sectionStale, type FissureFilterRule, type WorldActivityView, type WorldFissure } from "./worldActivity";

type RelicEra = "lith" | "meso" | "neo" | "axi" | "requiem";

export interface PreparedFissureRelic {
  era: RelicEra;
  relic: RelicOpeningRecommendation;
  goalChancePercent: number;
  goalNames: string[];
}

export interface FissureMatch {
  fissure: WorldFissure;
  relic: RelicOpeningRecommendation;
  /** Шанс получить хотя бы одну недостающую деталь сохранённой цели за одно открытие. */
  goalChancePercent: number;
  goalNames: string[];
}

function relicEra(slug: string): RelicEra | null {
  const era = /^(lith|meso|neo|axi|requiem)_/i.exec(slug)?.[1]?.toLowerCase();
  return era === "lith" || era === "meso" || era === "neo" || era === "axi" || era === "requiem" ? era : null;
}

function fissureEras(tier: string): readonly RelicEra[] {
  const normalized = tier.toLowerCase();
  if (normalized === "omnia") return ["lith", "meso", "neo", "axi"];
  const era = relicEra(`${normalized}_`);
  return era ? [era] : [];
}

function matchesRule(fissure: WorldFissure, rule: FissureFilterRule): boolean {
  return (rule.tier === "any" || rule.tier.toLowerCase() === fissure.tier.toLowerCase())
    && (rule.missionType === "any" || rule.missionType.toLowerCase() === fissure.missionTypeKey.toLowerCase())
    && (rule.mode === "any" || (rule.mode === "normal" && !fissure.isHard && !fissure.isStorm)
      || (rule.mode === "steel" && fissure.isHard) || (rule.mode === "storm" && fissure.isStorm));
}

function personalGoalNeeds(goals: PersonalGoalsView | null): Map<string, Set<string>> {
  const needs = new Map<string, Set<string>>();
  if (!goals?.inventoryAvailable || !goals.metadataAvailable) return needs;
  for (const goal of goals.goals) {
    if (goal.completedAt) continue;
    for (const part of goal.parts) {
      if (part.allocatedQuantity >= part.requiredQuantity) continue;
      const names = needs.get(part.slug) ?? new Set<string>();
      names.add(goal.displayName);
      needs.set(part.slug, names);
    }
  }
  return needs;
}

function personalChance(
  recommendation: RelicOpeningRecommendation,
  goals: PersonalGoalsView | null,
  needs: Map<string, Set<string>>,
): Pick<FissureMatch, "goalChancePercent" | "goalNames"> {
  if (!goals || needs.size === 0) return { goalChancePercent: 0, goalNames: [] };
  const source = goals.relics.find(relic => relic.definition.relicSlug === recommendation.relicSlug
    && relic.definition.refinement === recommendation.sourceRefinement && relic.ownedQuantity > 0);
  if (!source) return { goalChancePercent: 0, goalNames: [] };
  const target = goals.relics.find(relic => relic.definition.relicSlug === recommendation.relicSlug
    && relic.definition.refinement === recommendation.recommendedRefinement);
  const names = new Set<string>();
  let chance = 0;
  for (const reward of (target ?? source).definition.rewards) {
    const targetNames = reward.rewardSlug ? needs.get(reward.rewardSlug) : undefined;
    if (!targetNames) continue;
    const rewardChance = target ? reward.chancePercent : chanceAtRefinement(
      reward.chancePercent, source.definition.refinement, recommendation.recommendedRefinement,
    );
    if (!Number.isFinite(rewardChance) || rewardChance <= 0) continue;
    chance += rewardChance;
    for (const name of targetNames) names.add(name);
  }
  return { goalChancePercent: Math.min(100, chance), goalNames: [...names] };
}

function highestDropPrice(relic: RelicOpeningRecommendation): number {
  return relic.highestDrop?.price ?? -1;
}

/** Рейтинг меняется только при обновлении инвентаря, цен или личных целей. */
export function prepareFissureRelics(
  insights: InsightsView | null,
  goals: PersonalGoalsView | null,
): PreparedFissureRelic[] {
  if (!insights?.inventoryAvailable) return [];
  const needs = personalGoalNeeds(goals);
  // Сценарий публичного разлома не предполагает четыре одинаковые реликвии в отряде.
  const ranked = rankRelicsToOpen(insights.relics, relicProgressSets(insights.sets), {
    availableTraces: insights.voidTraces ?? 0,
    squadSize: 1,
    priorityRewardSlugs: [...needs.keys()],
  });
  return ranked.flatMap(relic => {
    const era = relicEra(relic.relicSlug);
    return era ? [{ era, relic, ...personalChance(relic, goals, needs) }] : [];
  }).sort((left, right) => right.goalChancePercent - left.goalChancePercent
    || highestDropPrice(right.relic) - highestDropPrice(left.relic)
    || (right.relic.highestDrop?.chancePercent ?? 0) - (left.relic.highestDrop?.chancePercent ?? 0)
    || left.relic.displayName.localeCompare(right.relic.displayName, "ru-RU"));
}

/** Выбирает одну имеющуюся реликвию для каждого активного разлома по правилам пользователя. */
export function selectFissureMatches(
  view: WorldActivityView,
  insights: InsightsView | null,
  goals: PersonalGoalsView | null,
  rules: FissureFilterRule[],
  now: number,
  preparedRelics?: readonly PreparedFissureRelic[],
): FissureMatch[] {
  if (!insights?.inventoryAvailable || sectionStale(view, "fissures", now)) return [];
  const candidates = preparedRelics ?? prepareFissureRelics(insights, goals);
  return (view.fissures ?? []).flatMap((fissure): FissureMatch[] => {
    if (periodState(fissure, now) !== "active" || !rules.some(rule => matchesRule(fissure, rule))) return [];
    const eras = fissureEras(fissure.tier);
    const best = candidates.find(candidate => eras.includes(candidate.era));
    return best ? [{
      fissure,
      relic: best.relic,
      goalChancePercent: best.goalChancePercent,
      goalNames: best.goalNames,
    }] : [];
  }).sort((left, right) => Date.parse(left.fissure.expiry) - Date.parse(right.fissure.expiry));
}
