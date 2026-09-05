import { describe, it, expect } from "vitest";
import { filterRelicChoices, knownRewardPrice, relicNet, relicProgressSets, rewardChoiceChance, selectedRelicRewards } from "./relicBrowser";
import { rankRelicsToOpen, type RelicInsightRow, type SetInsightRow } from "./insights";
import type { PriceRecommendation } from "./market";

function price(value:number):PriceRecommendation {
  return {key:{slug:"test",platform:"pc",rank:null,charges:null,subtype:null,amberStars:null,cyanStars:null},provider:"relics_run",sourceDate:"2026-09-05",fairPrice:value,listPrice:value,quickSell:null,lowestAsk:value,depthThree:value,depthPrice:value,closedVolume:20,liveSellOrderCount:5,liveBuyOrderCount:0,confidence:"high",freshness:"fresh",reasons:[]};
}
function relic(slug="lith_n1_relic"):RelicInsightRow {
  const rewards=[25.33,25.33,25.33,11,11,2].map((chancePercent,index)=>({definition:{rewardSlug:`part_${index}`,rewardGameRef:`/part/${index}`,displayNameEn:index===5?"Nyx Prime Systems":"Prime part",chancePercent},displayName:index===5?"Никс Прайм: система":"Деталь",recommendation:price(index===5?200:2)}));
  return {definition:{relicSlug:slug,relicGameRef:`/${slug}`,displayNameEn:"Lith N1 Relic",refinement:"intact",vaultStatus:"available",rewards:rewards.map(row=>row.definition)},displayName:"Реликвия Лит N1",ownedQuantity:3,sellableQuantity:2,relicRecommendation:price(2),expectedValue:{pricedExpectedValue:null,pricedChancePercent:100,totalChancePercent:100,missingRewardCount:0,coverage:"complete",reasons:[]},rewards};
}
function set(owned:number):SetInsightRow {
  const components=["own","part_5"].map((slug,index)=>({definition:{slug,gameRef:`/${slug}`,requiredQuantity:1,ducats:15},displayName:slug,ownedQuantity:index===0?owned:0,tradeableQuantity:index===0?owned:0,sellableQuantity:0,recommendation:price(2)}));
  return {definition:{setSlug:"nyx",setGameRef:"/nyx",displayNameEn:"Nyx Prime",vaultStatus:"available",components:components.map(row=>row.definition)},displayName:"Никс",components,setRecommendation:price(10),comparison:{setSlug:"nyx",completeSets:0,setFairValue:10,partsFairValue:4,setLiquidityAdjustedValue:null,partsLiquidityAdjustedValue:null,setPremiumPercent:150,recommendedMode:"set",reasons:[]}};
}
describe("выбор реликвии",()=>{
  it("ищет по русским и английским названиям награды, не только по реликвии",()=>{
    const source=relic();const ranked=rankRelicsToOpen([source],[],{availableTraces:0,squadSize:1});
    for(const query of ["лит n1","lith n1","НИКС система","Nyx Systems"]) expect(filterRelicChoices(ranked,[source],query,"value","solo")).toHaveLength(1);
    expect(filterRelicChoices(ranked,[source],"не существует","value","solo")).toEqual([]);
  });
  it("сортирует по выбранному сценарию и оставляет неизвестную оценку последней",()=>{
    const base=rankRelicsToOpen([relic()],[],{availableTraces:0,squadSize:1})[0];
    const a={...base,relicSlug:"a",displayName:"A",expectedPlatinum:10,squadExpectedPlatinum:5};
    const b={...base,relicSlug:"b",displayName:"B",expectedPlatinum:5,squadExpectedPlatinum:20};
    const c={...base,relicSlug:"c",displayName:"C",expectedPlatinum:null,squadExpectedPlatinum:null};
    expect(filterRelicChoices([c,b,a],[],"","value","solo")).toEqual([a,b,c]);
    expect(filterRelicChoices([c,b,a],[],"","value","matching_squad")).toEqual([b,a,c]);
    expect(relicNet(c,"solo")).toBeNull();
  });
  it("сортирует по нужным деталям и количеству отдельно от цены",()=>{
    const base=rankRelicsToOpen([relic()],[],{availableTraces:0,squadSize:1})[0];
    const a={...base,progressChancePercent:10,totalOwnedQuantity:1};
    const b={...base,relicSlug:"b",progressChancePercent:20,totalOwnedQuantity:5};
    for(const sort of ["progress","owned"] as const) expect(filterRelicChoices([a,b],[],"",sort,"solo")[0]).toBe(b);
  });
  it("четыре одинаковые реликвии дают шанс увидеть награду, не четырёхкратный шанс",()=>{
    expect(rewardChoiceChance(10,"matching_squad")).toBeCloseTo(34.39);
    expect(rewardChoiceChance(10,"solo")).toBeCloseTo(10);
    for(const value of [-1,NaN]) expect(rewardChoiceChance(value,"solo")).toBe(0);
    expect(rewardChoiceChance(150,"matching_squad")).toBe(100);
  });
  it("показывает шансы того улучшения, которое выбрано в расчёте",()=>{
    const source=relic();const selected=rankRelicsToOpen([source],[],{availableTraces:100,squadSize:1})[0];
    expect(selected.recommendedRefinement).toBe("radiant");
    expect(selectedRelicRewards(selected,[source],[]).find(row=>row.definition.rewardSlug==="part_5")?.chance).toBe(10);
    expect(source.rewards[5].definition.chancePercent).toBe(2);
  });
  it("не предлагает тратить следы при нулевом доступном балансе",()=>{
    expect(rankRelicsToOpen([relic()],[],{availableTraces:0,squadSize:1})[0].traceCost).toBe(0);
  });
  it("не называет нужными все детали сетов, которых нет в инвентаре",()=>{
    expect(relicProgressSets([set(0)])).toEqual([]);
    const partial=set(1); expect(relicProgressSets([partial])).toEqual([partial]);
    const source=relic();const selected=rankRelicsToOpen([source],[],{availableTraces:0,squadSize:1})[0];
    expect(selectedRelicRewards(selected,[source],[partial]).find(row=>row.definition.rewardSlug==="part_5")?.targets[0].finishes).toBe(true);
    partial.components[0].availableQuantity=0;
    expect(relicProgressSets([partial])).toEqual([]);
  });
  it("не маскирует неизвестные и устаревшие цены наград",()=>{
    expect(knownRewardPrice(null)).toBeNull();
    expect(knownRewardPrice({...price(10),freshness:"stale"})).toBeNull();
    expect(knownRewardPrice({...price(10),confidence:"low"})).toBeNull();
    expect(knownRewardPrice(price(10))).toBe(10);
  });
});
