import { describe, expect, it, vi } from "vitest";
import { render } from "svelte/server";
import { writable } from "svelte/store";
import OpportunityPlanner from "./OpportunityPlanner.svelte";
import PlanSetDetail from "./PlanSetDetail.svelte";
import { planCompletionBudget, planSetAcquisition, saleEstimate } from "./opportunityPlan";
import type { InsightsView, RelicInsightRow, SetInsightRow } from "./insights";
import type { LivePricingResult, PriceRecommendation } from "./market";

vi.mock("./i18n", async importOriginal => ({ ...await importOriginal<typeof import("./i18n")>(), useLocale: () => writable("ru") }));

function price(slug: string, value: number): PriceRecommendation {
  return { key: { slug, platform: "pc", rank: null, charges: null, subtype: null, amberStars: null, cyanStars: null },
    provider: "relics_run", sourceDate: "2026-09-05", fairPrice: value, listPrice: value, quickSell: null,
    lowestAsk: value, depthThree: value, depthPrice: value, closedVolume: 20, liveSellOrderCount: 5, liveBuyOrderCount: 0,
    confidence: "high", freshness: "fresh", reasons: [] };
}
function set(slug: string, cost: number, profit: number): SetInsightRow {
  const components = [true, false].map((owned, index) => ({ definition: {slug:`${slug}_${index}`,gameRef:`/${slug}/${index}`,requiredQuantity:1,ducats:15},
    displayName:`${slug} part ${index}`, ownedQuantity:Number(owned), tradeableQuantity:Number(owned), sellableQuantity:Number(owned), recommendation:price(`${slug}_${index}`,owned ? 10 : cost) }));
  return { definition:{setSlug:slug,setGameRef:`/${slug}`,displayNameEn:slug,vaultStatus:"available",components:components.map(x=>x.definition)},displayName:slug,
    setRecommendation:price(slug,cost+10+profit), components,
    comparison:{setSlug:slug,completeSets:0,setFairValue:cost+10+profit,partsFairValue:cost+10,setLiquidityAdjustedValue:null,partsLiquidityAdjustedValue:null,setPremiumPercent:20,recommendedMode:"insufficient_inventory",reasons:[]} };
}
function relic(slug: string, rewardSlug: string, chance = 2, count = 3): RelicInsightRow {
  const definition = { rewardSlug, rewardGameRef:`/${rewardSlug}`,displayNameEn:rewardSlug,chancePercent:chance };
  return {definition:{relicSlug:slug,relicGameRef:`/${slug}`,displayNameEn:slug,refinement:"intact",vaultStatus:"available",rewards:[definition]},displayName:slug,
    ownedQuantity:count,sellableQuantity:count,relicRecommendation:price(slug,2),rewards:[{definition,displayName:rewardSlug,recommendation:price(rewardSlug,10)}],
    expectedValue:{pricedExpectedValue:null,pricedChancePercent:chance,totalChancePercent:chance,missingRewardCount:0,coverage:"insufficient",reasons:[]} };
}

describe("план возможностей", () => {
  it("выбирает сочетание в бюджет, а не одну самую прибыльную строку", () => {
    const result = planCompletionBudget([set("large",80,30),set("small1",50,22),set("small2",50,22)],100,"profit");
    expect(result.choices.map(x=>x.row.displayName).sort()).toEqual(["small1","small2"]);
    expect(result).toMatchObject({cost:100,profit:44,ownedValue:20,revenue:164});
  });
  it("не тратит одну общую деталь дважды", () => {
    const a=set("a",30,20), b=set("b",30,25);
    b.components[0].definition.slug=a.components[0].definition.slug;
    const result=planCompletionBudget([a,b],100,"profit");
    expect(result.choices).toHaveLength(1);
    expect(result.choices[0].row).toBe(b);
  });
  it("пересчитывает глубину общей покупки и не превышает бюджет", () => {
    const a=set("a",10,30), b=set("b",10,30);
    b.components[1].definition.slug=a.components[1].definition.slug;
    for (const row of [a,b]) row.components[1].recommendation!.depthThree=20;
    expect(planCompletionBudget([a,b],25,"profit").choices).toHaveLength(1);
    const result=planCompletionBudget([a,b],40,"profit");
    expect(result.shopping).toHaveLength(1);
    expect(result.shopping[0]).toMatchObject({quantity:2,cost:40});
    expect(result.profit).toBe(40);
    expect(result.choices.map(choice => choice.cost)).toEqual([20,20]);
    expect(result.choices.map(choice => choice.profit)).toEqual([20,20]);
    expect(result.choices.flatMap(choice => choice.purchases).reduce((sum,part) => sum + part.cost,0)).toBe(result.cost);
  });
  it("не скрывает убыточный комплект внутри выгодной общей покупки", () => {
    const a=set("a",10,100), b=set("b",10,5);
    b.components[1].definition.slug=a.components[1].definition.slug;
    for (const row of [a,b]) row.components[1].recommendation!.depthThree=20;
    const result=planCompletionBudget([a,b],100,"profit");
    expect(result.choices.map(choice => choice.row.displayName)).toEqual(["a"]);
    expect(result.profit).toBe(100);
  });
  it("сводит дробную стоимость общих деталей и выгоду строк с итогом", () => {
    const a=set("a",1,10), b=set("b",1,10), c=set("c",1,10);
    for (const row of [a,b,c]) {
      row.components[1].definition.slug="shared";
      row.components[1].recommendation!.depthThree=1.335;
    }
    const result=planCompletionBudget([a,b,c],10,"profit");
    expect(result.choices).toHaveLength(3);
    expect(result.cost).toBe(4.01);
    expect(result.choices.reduce((sum,choice) => sum + choice.cost,0)).toBeCloseTo(result.cost,10);
    expect(result.choices.reduce((sum,choice) => sum + choice.profit,0)).toBeCloseTo(result.profit,10);
    expect(result.choices.every(choice => choice.purchases[0].cost === choice.cost)).toBe(true);
  });
  it("отказывается от некорректного бюджета и устаревшей цены", () => {
    const row=set("a",10,20);
    for(const budget of [0,-1,NaN,Infinity]) expect(planCompletionBudget([row],budget,"profit").choices).toEqual([]);
    row.components[1].recommendation!.freshness="stale";
    expect(planCompletionBudget([row],100,"profit").choices).toEqual([]);
  });
  it("для скорости предпочитает подтверждённый спрос", () => {
    const a=set("rare",50,40),b=set("popular",50,20);
    a.setRecommendation!.closedVolume=1;b.setRecommendation!.closedVolume=100;
    expect(planCompletionBudget([a,b],50,"profit").choices[0].row).toBe(a);
    expect(planCompletionBudget([a,b],50,"speed").choices[0].row).toBe(b);
  });
  it("покупателя подтверждают только свежие исполнимые заявки", () => {
    const row=set("a",10,20);
    const quote={quoteState:"network",recommendation:row.setRecommendation!,orders:[{side:"buy",platinum:35,quantity:1,perTrade:1,userStatus:"in_game"}]} as LivePricingResult;
    expect(saleEstimate(row,"speed",quote)).toMatchObject({price:35,buyer:true});
    expect(saleEstimate(row,"speed",{...quote,quoteState:"stale_cache"}).buyer).toBe(false);
    quote.orders[0].perTrade=2;
    expect(saleEstimate(row,"speed",quote).buyer).toBe(false);
    expect(saleEstimate(row,"profit",quote).buyer).toBe(false);
  });
  it("план реликвий не расходует лишние копии и неизвестные следы", () => {
    const row=set("a",10,20), source=relic("r",row.components[1].definition.slug,2,3);
    const result=planSetAcquisition(row,[source],null,10);
    expect(result).toMatchObject({openings:3,traces:0,buyCost:0,relicValue:6});
    expect(result.chance).toBeCloseTo((1-.98**3)*100);
    expect(result.steps[0].target).toBe("intact");
    expect(source.ownedQuantity).toBe(3);
  });
  it("улучшает только доступное количество за имеющиеся следы", () => {
    const row=set("a",10,20), source=relic("r",row.components[1].definition.slug,2,3);
    const result=planSetAcquisition(row,[source],100,2);
    expect(result).toMatchObject({openings:2,traces:100});
    expect(result.chance).toBeCloseTo((1-.9*.98)*100);
    expect(result.steps.map(x=>x.quantity).reduce((a,b)=>a+b,0)).toBe(2);
  });
  it("предлагает докупку деталей без подходящих реликвий", () => {
    const row=set("a",10,20);
    expect(planSetAcquisition(row,[],100)).toMatchObject({openings:0,chance:0,buyCost:10});
  });
  it("смешанный путь считает вероятность только оставшихся после докупки деталей", () => {
    const row=set("a",10,20);
    const extra={...row.components[1],definition:{...row.components[1].definition,slug:"other"}};
    row.components.push(extra);
    const source=relic("r",row.components[1].definition.slug,2,1);
    const result=planSetAcquisition(row,[source],0,10);
    expect(result).toMatchObject({openings:1,traces:0,buyCost:10,chance:2});
    expect(result.buy.map(part=>part.slug)).toEqual(["other"]);
  });
});

describe("действия в моём плане", () => {
  function detail(row: SetInsightRow, mode: "complete" | "ready") {
    return render(PlanSetDetail, {props:{row,mode,goal:"profit",quote:undefined,choice:undefined,
      view:{sets:[row],relics:[],voidTraces:0} as unknown as InsightsView,
      onCheck:vi.fn(),onOpenSet:vi.fn(),onOpenParts:vi.fn(),onOpenRelic:vi.fn(),onExclude:vi.fn(),onRestore:vi.fn()}}).body;
  }
  it("для готового комплекта открывает продажу и не предлагает собрать следующий", () => {
    const row=set("ready",10,20);
    row.components.forEach(part => {part.ownedQuantity=1;part.tradeableQuantity=1;part.sellableQuantity=1;});
    const body=detail(row,"ready");
    expect(body).toContain("Перейти к продаже комплекта");
    expect(body).toContain("Ничего докупать не нужно");
    expect(body).not.toContain("Купите недостающие детали");
    expect(body).not.toContain("Можно ли обойтись своими реликвиями");
  });
  it("не подменяет отсутствующие цены обещанной выгодой", () => {
    const row=set("unknown",10,20);
    row.setRecommendation=null;
    row.components[1].recommendation=null;
    const body=detail(row,"complete");
    expect(body).toContain("Для оценки выгоды не хватает надёжных цен");
    expect(body).not.toContain("+ 0p");
    expect(body).toContain("Найти продавцов деталей");
  });
  it("после падения цены объясняет, почему докупка невыгодна", () => {
    const row=set("loss",10,20);
    row.setRecommendation=price("loss",5);
    expect(detail(row,"complete")).toContain("При этой цене докупка не выгоднее продажи своих деталей");
  });
  it("без инвентаря не строит план и предлагает загрузить данные", () => {
    const row=set("inventory_missing",10,20);
    const body=render(OpportunityPlanner,{props:{view:{inventoryAvailable:false,sets:[row],relics:[]} as unknown as InsightsView,
      sets:[row],quotes:new Map(),errors:new Map(),onCheck:vi.fn(),onOpenSet:vi.fn(),onOpenParts:vi.fn(),onOpenRelic:vi.fn(),onOpenSettings:vi.fn()}}).body;
    expect(body).toContain("Сначала загрузите инвентарь");
    expect(body).toContain("Открыть настройки данных");
    expect(body).not.toContain("Итог подобранного плана");
    expect(body).not.toContain("inventory_missing");
  });
});
