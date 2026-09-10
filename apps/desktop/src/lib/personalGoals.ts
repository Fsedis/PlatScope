import type { RelicDefinition, RelicInsightRow, SetInsightRow } from "./insights";
import { planSetAcquisition } from "./opportunityPlan";
import type { AccountView } from "./account";

export interface PersonalSetChoice { setSlug: string; displayName: string; displayNameEn: string; imageUrl: string | null }
export interface PersonalGoalPart { slug: string; displayName: string; displayNameEn: string; imageUrl: string | null; requiredQuantity: number; allocatedQuantity: number }
export interface PersonalSetGoal extends PersonalSetChoice { parts: PersonalGoalPart[]; completedAt: string | null; completionPending: boolean }
export interface PersonalGoalCompletion { setSlug: string; completedAt: string }
export const PERSONAL_GOALS_VIEW_EVENT = "platscope-personal-goals-view";
export interface PersonalGoalRelic { definition: RelicDefinition; displayName: string; ownedQuantity: number }
export interface PersonalGoalsView {
  inventoryAvailable: boolean;
  metadataAvailable: boolean;
  observedAt: string | null;
  catalog: PersonalSetChoice[];
  goals: PersonalSetGoal[];
  relics: PersonalGoalRelic[];
}

export function unseenGoalCompletions(goals: PersonalSetGoal[], seen: Set<string>) {
  return goals.filter(goal => goal.completedAt && goal.completionPending
    && !seen.has(`${goal.setSlug}:${goal.completedAt}`));
}

/** Существующие ордера требуют внимания владельца; цель не отменяет их на рынке. */
export function personalGoalListedParts(account: AccountView | null): Set<string> {
  return new Set((account?.orders ?? []).flatMap(order => {
    if (order.type !== "sell" || order.quantity <= 0 || order.itemId === null
      || [order.rank,order.charges,order.subtype,order.amberStars,order.cyanStars].some(value => value != null)) return [];
    const item = account?.orderItems?.[order.itemId];
    return item ? [item.slug,...(item.setComponents ?? []).map(part => part.slug)] : [];
  }));
}

export function goalProgress(goal: PersonalSetGoal) {
  const required = goal.parts.reduce((sum, part) => sum + part.requiredQuantity, 0);
  const owned = goal.parts.reduce((sum, part) => sum + Math.min(part.requiredQuantity, part.allocatedQuantity), 0);
  return { required, owned, complete: required > 0 && required === owned };
}

/** Используем существующий расчёт открытий с количеством, выделенным именно этой цели. */
export function personalAcquisition(goal: PersonalSetGoal, relics: PersonalGoalRelic[]) {
  if (goal.completedAt) return null;
  const components = goal.parts.map(part => ({ definition: {slug:part.slug,gameRef:"",requiredQuantity:part.requiredQuantity,ducats:null},
    displayName:part.displayName,ownedQuantity:part.allocatedQuantity,tradeableQuantity:part.allocatedQuantity,
    availableQuantity:part.allocatedQuantity,sellableQuantity:0,recommendation:null }));
  const row: SetInsightRow = {definition:{setSlug:goal.setSlug,setGameRef:"",displayNameEn:goal.displayNameEn,vaultStatus:"unknown",components:components.map(part=>part.definition)},
    displayName:goal.displayName,setRecommendation:null,components,
    comparison:{setSlug:goal.setSlug,completeSets:0,setFairValue:null,partsFairValue:null,setLiquidityAdjustedValue:null,partsLiquidityAdjustedValue:null,setPremiumPercent:null,recommendedMode:"insufficient_inventory",reasons:[]} };
  const sources: RelicInsightRow[] = relics.map(relic => ({...relic,sellableQuantity:0,relicRecommendation:null,
    rewards:relic.definition.rewards.map(definition=>({definition,displayName:definition.displayNameEn,recommendation:null})),
    expectedValue:{pricedExpectedValue:null,pricedChancePercent:0,totalChancePercent:100,missingRewardCount:0,coverage:"insufficient",reasons:[]} }));
  // Предлагаем открывать уже имеющиеся улучшения: дополнительного расхода следов нет.
  return goalProgress(goal).complete ? null : planSetAcquisition(row, sources, 0);
}

export function partRelics(part: PersonalGoalPart, relics: PersonalGoalRelic[]) {
  if (part.allocatedQuantity >= part.requiredQuantity) return [];
  return relics.flatMap(relic => {
    const reward = relic.definition.rewards.find(reward => reward.rewardSlug === part.slug && reward.chancePercent > 0);
    return reward ? [{...relic,chance:reward.chancePercent}] : [];
  }).sort((a,b) => Number(b.ownedQuantity > 0) - Number(a.ownedQuantity > 0)
    || Number(a.definition.vaultStatus === "vaulted") - Number(b.definition.vaultStatus === "vaulted")
    || b.chance - a.chance || a.definition.displayNameEn.localeCompare(b.definition.displayNameEn));
}
