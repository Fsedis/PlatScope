import { chanceAtRefinement, componentAvailableQuantity, estimatedBuyPrice, safeOverviewSetPrice, setOpportunity, setRelicSupport, type MissingSetPart, type RelicInsightRow, type RelicRefinement, type SetInsightRow } from "./insights";
import type { LivePricingResult, PriceRecommendation } from "./market";

export type OpportunityGoal = "profit" | "speed";
const positive = (value: number | null | undefined): value is number => value != null && Number.isFinite(value) && value > 0;

/** Для одного комплекта: заявка покупателя должна позволять сделку ровно на одну копию. */
export function saleEstimate(row: SetInsightRow, goal: OpportunityGoal, quote?: LivePricingResult): { price: number | null; buyer: boolean; volume: number | null } {
  const current = quote && quote.quoteState !== "stale_cache" ? quote : undefined;
  const bids = current?.orders.filter((order) => order.side === "buy" && order.userStatus === "in_game"
    && order.perTrade === 1 && Number.isInteger(order.quantity) && order.quantity >= 1 && positive(order.platinum)) ?? [];
  const bid = bids.reduce<number | null>((best, order) => Math.max(best ?? 0, order.platinum), null);
  const recommendation = current?.recommendation ?? row.setRecommendation;
  const volume = recommendation?.closedVolume;
  return {
    price: goal === "speed" && bid !== null ? bid : safeOverviewSetPrice(current ? { ...row, setRecommendation: current.recommendation } : row),
    buyer: goal === "speed" && bid !== null,
    volume: positive(volume) ? volume : null,
  };
}

export interface BudgetChoice { row: SetInsightRow; revenue: number; ownedValue: number; profit: number; cost: number; buyer: boolean; volume: number | null }
export interface ShoppingPart { slug: string; name: string; quantity: number; cost: number }
export interface BudgetPlan { choices: BudgetChoice[]; shopping: ShoppingPart[]; cost: number; revenue: number; ownedValue: number; profit: number; limited: boolean }
interface State { choices: BudgetChoice[]; used: Map<string, number>; purchases: Map<string, number>; cost: number; revenue: number; ownedValue: number; score: number }

/** Ограниченный поиск сочетаний. Это подобранный план, а не обещание глобального оптимума. */
export function planCompletionBudget(rows: SetInsightRow[], budget: number, goal: OpportunityGoal, quotes: ReadonlyMap<string, LivePricingResult> = new Map()): BudgetPlan {
  const empty: BudgetPlan = { choices: [], shopping: [], cost: 0, revenue: 0, ownedValue: 0, profit: 0, limited: false };
  if (!Number.isFinite(budget) || budget <= 0) return empty;
  const limit = Math.floor(Math.min(1_000_000, budget) * 100);
  const pool = new Map<string, number>();
  const pricing = new Map<string, { name: string; recommendations: PriceRecommendation[] }>();
  const candidates = rows.flatMap((row) => {
    const opportunity = setOpportunity(row);
    const sale = saleEstimate(row, goal, quotes.get(row.definition.setSlug));
    if (!opportunity.quickToComplete || sale.price === null || opportunity.completionCost === null || opportunity.ownedPartsOpportunityValue === null) return [];
    const profit = sale.price - opportunity.completionCost - opportunity.ownedPartsOpportunityValue;
    if (profit <= 0 || (goal === "speed" && sale.volume === null && !sale.buyer)) return [];
    const used = new Map<string, number>();
    for (const part of row.components) {
      const slug = part.definition.slug;
      const remainder = Math.max(0, componentAvailableQuantity(part) - opportunity.availableCompleteSets * part.definition.requiredQuantity);
      pool.set(slug, Math.min(pool.get(slug) ?? remainder, remainder));
      used.set(slug, Math.min(part.definition.requiredQuantity, remainder));
      if (part.recommendation) {
        const entry = pricing.get(slug) ?? { name: part.displayName, recommendations: [] };
        entry.recommendations.push(part.recommendation);
        pricing.set(slug, entry);
      }
    }
    const choice: BudgetChoice = { row, revenue: sale.price, ownedValue: opportunity.ownedPartsOpportunityValue, cost: opportunity.completionCost, profit, buyer: sale.buyer, volume: sale.volume };
    return [{ choice, used, missing: opportunity.missingParts }];
  });
  const weight = (choice: BudgetChoice) => goal === "profit" ? 1 : choice.buyer ? 1 : (choice.volume ?? 0) / ((choice.volume ?? 0) + 10);
  candidates.sort((a, b) => b.choice.profit * weight(b.choice) - a.choice.profit * weight(a.choice) || a.choice.row.definition.setSlug.localeCompare(b.choice.row.definition.setSlug));
  const purchaseCost = (slug: string, quantity: number): number | null => {
    const estimates = pricing.get(slug)?.recommendations.map((rec) => estimatedBuyPrice(rec, quantity)?.unitPrice ?? null) ?? [];
    if (!estimates.length || estimates.some((price) => !positive(price))) return null;
    return Math.ceil(Math.max(...estimates as number[]) * quantity * 100);
  };
  let states: State[] = [{ choices: [], used: new Map(), purchases: new Map(), cost: 0, revenue: 0, ownedValue: 0, score: 0 }];
  let limited = false;
  for (const candidate of candidates) {
    const next = [...states];
    for (const state of states) {
      const used = new Map(state.used);
      let fits = true;
      for (const [slug, quantity] of candidate.used) {
        used.set(slug, (used.get(slug) ?? 0) + quantity);
        if (used.get(slug)! > (pool.get(slug) ?? 0)) fits = false;
      }
      if (!fits) continue;
      const purchases = new Map(state.purchases);
      for (const part of candidate.missing) purchases.set(part.slug, (purchases.get(part.slug) ?? 0) + part.quantity);
      let cost = 0;
      for (const [slug, quantity] of purchases) { const price = purchaseCost(slug, quantity); if (price === null) { fits = false; break; } cost += price; }
      if (!fits || cost > limit) continue;
      const marginalProfit = candidate.choice.revenue - candidate.choice.ownedValue - (cost - state.cost) / 100;
      if (marginalProfit <= 0) continue;
      next.push({ choices: [...state.choices, candidate.choice], used, purchases, cost,
        revenue: state.revenue + candidate.choice.revenue, ownedValue: state.ownedValue + candidate.choice.ownedValue,
        score: state.score + marginalProfit * weight(candidate.choice) });
    }
    next.sort((a, b) => b.score - a.score || a.cost - b.cost);
    if (next.length > 256) limited = true;
    states = next.slice(0, 256);
  }
  const best = states[0];
  return { choices: best.choices, shopping: [...best.purchases].map(([slug, quantity]) => ({ slug, name: pricing.get(slug)!.name, quantity, cost: purchaseCost(slug, quantity)! / 100 })),
    cost: best.cost / 100, revenue: best.revenue, ownedValue: best.ownedValue, profit: best.revenue - best.ownedValue - best.cost / 100, limited };
}

const traceCosts: Record<RelicRefinement, number> = { intact: 0, exceptional: 25, flawless: 50, radiant: 100 };
export interface RelicPlanStep { source: RelicInsightRow; target: RelicRefinement; quantity: number; traceCost: number }
export interface AcquisitionPlan { steps: RelicPlanStep[]; chance: number; openings: number; traces: number; buy: MissingSetPart[]; buyCost: number | null; relicValue: number | null; capped: boolean }

/** Соло, без повторного использования одной копии. Подбирает до заданного числа открытий. */
export function planSetAcquisition(row: SetInsightRow, relics: RelicInsightRow[], traces: number | null | undefined, maxOpenings = 10): AcquisitionPlan {
  const missing = setOpportunity(row).missingParts;
  const missingSlugs = new Set(missing.map((part) => part.slug));
  const sourceRelics = relics.filter((relic) => Number.isInteger(relic.ownedQuantity) && relic.ownedQuantity > 0
    && relic.rewards.some((reward) => reward.definition.rewardSlug !== null && missingSlugs.has(reward.definition.rewardSlug) && reward.definition.chancePercent > 0));
  const covered = new Set(sourceRelics.flatMap((relic) => relic.rewards.filter((reward) => reward.definition.chancePercent > 0).map((reward) => reward.definition.rewardSlug).filter((slug): slug is string => slug !== null)));
  const farmSlugs = new Set(missing.filter((part) => covered.has(part.slug)).map((part) => part.slug));
  const buy = missing.filter((part) => !covered.has(part.slug));
  const buyCost = buy.every((part) => part.estimatedCost !== null) ? buy.reduce((sum, part) => sum + part.estimatedCost!, 0) : null;
  const steps: RelicPlanStep[] = [];
  const selected: RelicInsightRow[] = [];
  const used = new Map<RelicInsightRow, number>();
  let traceTotal = 0;
  let support = setRelicSupport(row, [], farmSlugs);
  const cap = Number.isFinite(maxOpenings) ? Math.max(1, Math.min(20, Math.floor(maxOpenings))) : 10;
  for (let opening = 0; opening < cap && farmSlugs.size > 0 && support.aggregateChancePercent < 80; opening++) {
    let best: { source: RelicInsightRow; target: RelicRefinement; copy: RelicInsightRow; support: typeof support; cost: number; gain: number } | null = null;
    for (const source of sourceRelics) {
      if ((used.get(source) ?? 0) >= source.ownedQuantity) continue;
      const targets: RelicRefinement[] = source.definition.refinement === "radiant" ? ["radiant"] : [source.definition.refinement, "radiant"];
      for (const target of targets) {
        const cost = traceCosts[target] - traceCosts[source.definition.refinement];
        if (cost > 0 && (traces == null || !Number.isFinite(traces) || traceTotal + cost > traces)) continue;
        const copy: RelicInsightRow = { ...source, ownedQuantity: 1, definition: { ...source.definition, refinement: target }, rewards: source.rewards.map((reward) => ({ ...reward, definition: { ...reward.definition, chancePercent: target === source.definition.refinement ? reward.definition.chancePercent : chanceAtRefinement(reward.definition.chancePercent, source.definition.refinement, target) } })) };
        const nextSupport = setRelicSupport(row, [...selected, copy], farmSlugs);
        const gain = nextSupport.expectedUsefulDrops - support.expectedUsefulDrops;
        if (gain > 1e-9 && (!best || gain > best.gain + 1e-9 || (Math.abs(gain - best.gain) < 1e-9 && cost < best.cost))) best = { source, target, copy, support: nextSupport, cost, gain };
      }
    }
    if (!best) break;
    selected.push(best.copy);
    used.set(best.source, (used.get(best.source) ?? 0) + 1);
    traceTotal += best.cost;
    support = best.support;
    const step = steps.find((step) => step.source === best!.source && step.target === best!.target);
    if (step) { step.quantity++; step.traceCost += best.cost; }
    else steps.push({ source: best.source, target: best.target, quantity: 1, traceCost: best.cost });
  }
  let relicValue: number | null = 0;
  for (const step of steps) {
    const price = step.source.relicRecommendation;
    if (!price || !positive(price.fairPrice) || !["fresh", "aging"].includes(price.freshness) || !["high", "medium"].includes(price.confidence)) { relicValue = null; break; }
    relicValue += price.fairPrice * step.quantity;
  }
  return { steps, chance: support.aggregateChancePercent, openings: selected.length, traces: traceTotal, buy, buyCost, relicValue, capped: selected.length >= cap && support.aggregateChancePercent < 80 };
}
