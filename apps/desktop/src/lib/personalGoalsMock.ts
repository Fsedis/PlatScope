import type { InsightsView } from "./insights";
import { goalProgress, type PersonalGoalsView } from "./personalGoals";

export interface MockSavedGoal { setSlug: string; completedAt: string | null; completionPending: boolean }
const image = (file: string) => `https://cdn.warframestat.us/img/${file}`;

/** Только для изолированной проверки интерфейса через ?mock&mockGoals. */
export function makePersonalGoalsMock(source: InsightsView, saved: (MockSavedGoal | string)[], scenario: string | null): PersonalGoalsView {
  const catalog = source.sets.map(row => ({setSlug:row.definition.setSlug,displayName:row.displayName,displayNameEn:row.definition.displayNameEn.replace(/^./, letter => letter.toUpperCase()),
    imageUrl:scenario === "broken-images" ? image("missing-goal-image.png") : row.definition.setSlug === "ash_prime_set" ? image("AshPrime.png") : row.definition.setSlug === "nyx_prime_set" ? image("NyxPrime.png") : row.imageUrl ?? null}));
  const stock = new Map(source.sets.flatMap(row => row.components.map(part => [part.definition.slug, scenario === "ready" ? part.definition.requiredQuantity : part.ownedQuantity] as const)));
  const goals = saved.flatMap(entry => {
    const record = typeof entry === "string" ? {setSlug:entry,completedAt:null,completionPending:false} : entry;
    const slug = record.setSlug;
    const row = source.sets.find(row => row.definition.setSlug === slug);
    if (!row) return [];
    const goal = {...record,...catalog.find(choice => choice.setSlug === slug)!,parts:row.components.map((part,index) => {
      const requiredQuantity = scenario === "two-copies" && index === 1 ? 2 : part.definition.requiredQuantity;
      const allocatedQuantity = scenario === "no-inventory" ? 0 : Math.min(requiredQuantity,stock.get(part.definition.slug) ?? 0);
      stock.set(part.definition.slug,(stock.get(part.definition.slug) ?? 0)-allocatedQuantity);
      const warframe = ["ash_prime_set","nyx_prime_set"].includes(slug);
      return {slug:part.definition.slug,
        displayName:warframe ? `${slug === "ash_prime_set" ? "Эш" : "Никс"} Прайм: ${["Чертёж","Нейрооптика","Каркас","Система"][index]}` : part.displayName,
        displayNameEn:warframe ? `${slug === "ash_prime_set" ? "Ash" : "Nyx"} Prime ${["Blueprint","Neuroptics","Chassis","Systems"][index]}` : part.definition.slug.replaceAll("_"," "),
        imageUrl:scenario === "broken-images" ? image("missing-part-image.png") : warframe ? image(["blueprint.png","GenericWarframePrimeHelmet.png","GenericWarframePrimeChassis.png","GenericWarframePrimeSystem.png"][index]) : part.imageUrl ?? part.definition.imageUrl ?? null,
        requiredQuantity,allocatedQuantity};
    })};
    if (!goal.completedAt && scenario !== "no-inventory" && goalProgress(goal).complete) {
      goal.completedAt = new Date().toISOString(); goal.completionPending = true;
    }
    return [goal];
  });
  return {inventoryAvailable:scenario !== "no-inventory",metadataAvailable:true,observedAt:scenario === "no-inventory" ? null : "2026-09-10T08:00:00Z",catalog,goals,
    relics:source.relics.map(relic => ({definition:relic.definition,displayName:relic.displayName,ownedQuantity:scenario === "no-relics" || scenario === "no-inventory" ? 0 : relic.ownedQuantity}))};
}

export function reserveMockGoals(source: InsightsView, slugs: string[]): InsightsView {
  const goals = makePersonalGoalsMock(source,slugs,null);
  const reserved = new Map<string,number>();
  goals.goals.flatMap(goal => goal.parts).forEach(part => reserved.set(part.slug,(reserved.get(part.slug) ?? 0)+part.allocatedQuantity));
  return {...source,sets:source.sets.map(row => ({...row,components:row.components.map(part => ({...part,
    availableQuantity:Math.max(0,part.tradeableQuantity-(reserved.get(part.definition.slug) ?? 0)),
    sellableQuantity:Math.min(part.sellableQuantity,Math.max(0,part.tradeableQuantity-(reserved.get(part.definition.slug) ?? 0))),
  }))}))};
}
