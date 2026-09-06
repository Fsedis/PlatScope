import type { InsightsView, SetInsightRow, RelicInsightRow } from "./insights";
import type { PriceRecommendation } from "./market";

/** Репрезентативные, вымышленные цены для проверки «Моего плана» без аккаунта и сети. */
export function makeOpportunityPlanMock(base: InsightsView, scenario: string): InsightsView {
  const price = (slug: string, value: number): PriceRecommendation => ({
    ...base.sets[0].setRecommendation!, key:{...base.sets[0].setRecommendation!.key,slug},
    fairPrice:value,listPrice:value,lowestAsk:value,depthThree:value,depthPrice:value,
    confidence:"high",freshness:"fresh",closedVolume:20,sourceDate:"2026-09-06",
  });
  const specs: [string,string,number,number][] = [
    ["tiberon","Тайберон",25,13],["helios","Гелиос",12,12],["guandao","Гуаньдао",8,11],
    ["strun","Стран",10,10],["styanax","Стианакс",9,9],["stradavar","Страдавар",7,8],
    ["sarofang","Сарофанг",6,7],["ivara","Ивара",5,6],["mirage","Мираж",12,5],
    ["vauban","Вобан",120,30],["ash","Эш",150,40],["akbolto","Акболто",20,12],
  ];
  const sets: SetInsightRow[] = specs.map(([slug,ru,cost,profit], index) => {
    const row = structuredClone(base.sets[0]);
    row.itemId = "plan-" + slug;
    row.imageUrl = null;
    row.displayName = ru + " Прайм: Комплект";
    row.definition = {...row.definition,setSlug:slug + "_prime_set",setGameRef:"/PlanPreview/" + slug,displayNameEn:slug + " Prime Set"};
    row.components = row.components.map((part,j) => ({
      ...part, imageUrl:null, itemId:"plan-" + slug + "-" + j,
      definition:{...part.definition,slug:slug + "_prime_" + j,gameRef:"/PlanPreview/" + slug + "/" + j},
      displayName:ru + " Прайм: " + ["Чертёж","Основная деталь","Дополнительная деталь","Недостающая деталь"][j],
      ownedQuantity:j === 3 ? 0 : 1,tradeableQuantity:j === 3 ? 0 : 1,sellableQuantity:j === 3 ? 0 : 1,
      recommendation:price(slug + "_prime_" + j,j === 3 ? cost : 10),
    }));
    row.definition.components = row.components.map(part => part.definition);
    row.setRecommendation = {...price(row.definition.setSlug,cost + 30 + profit),closedVolume:index === 0 ? 4 : 20 + index * 10};
    row.comparison = {...row.comparison,completeSets:0,setSlug:row.definition.setSlug,setFairValue:cost+30+profit,partsFairValue:cost+30,recommendedMode:"insufficient_inventory"};
    return row;
  });
  const ready = structuredClone(base.sets[0]);
  ready.components.forEach(part => { part.ownedQuantity=3;part.tradeableQuantity=3;part.sellableQuantity=2; });
  ready.setRecommendation = price(ready.definition.setSlug,95);
  ready.comparison = {...ready.comparison,completeSets:3,setFairValue:95,partsFairValue:79};
  const relic: RelicInsightRow = structuredClone(base.relics[0]);
  relic.displayName = "Реликвия Лит T2";
  relic.definition = {...relic.definition,relicSlug:"lith_t2_relic",refinement:"intact",displayNameEn:"Lith T2 Relic"};
  relic.ownedQuantity = 3;
  relic.relicRecommendation = price("lith_t2_relic",4);
  relic.rewards = [{...relic.rewards[0],displayName:sets[0].components[3].displayName,recommendation:sets[0].components[3].recommendation,definition:{...relic.rewards[0].definition,rewardSlug:sets[0].components[3].definition.slug,rewardGameRef:sets[0].components[3].definition.gameRef,chancePercent:2}}];
  relic.definition.rewards = relic.rewards.map(reward => reward.definition);
  const view = {...base,metadata:{...base.metadata,fetchedAt:"2026-09-06T03:00:00Z"},sets:[...sets,ready],relics:[relic],voidTraces:300};
  if (scenario === "empty") { view.sets=[];view.relics=[]; }
  if (scenario === "no-inventory") view.inventoryAvailable=false;
  if (scenario === "ready") view.sets=[ready];
  if (scenario === "unknown-prices") view.sets.forEach(row => {row.setRecommendation=null;row.components.forEach(part => part.recommendation=null);});
  return view;
}
