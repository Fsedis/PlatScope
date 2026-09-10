import { describe, expect, it } from "vitest";
import { goalProgress, partRelics, personalAcquisition, personalGoalListedParts, unseenGoalCompletions, type PersonalSetGoal, type PersonalGoalRelic } from "./personalGoals";
import type { AccountOrder } from "./account";

const goal = (): PersonalSetGoal => ({setSlug:"set",displayName:"Комплект",displayNameEn:"Set",imageUrl:null,completedAt:null,completionPending:false,parts:[
  {slug:"blade",displayName:"Лезвие",displayNameEn:"Blade",imageUrl:null,requiredQuantity:2,allocatedQuantity:1},
  {slug:"blueprint",displayName:"Чертёж",displayNameEn:"Blueprint",imageUrl:null,requiredQuantity:1,allocatedQuantity:1},
]});
const relic = (ownedQuantity = 1, slug = "blade", refinement: "intact" | "radiant" = "intact"): PersonalGoalRelic => ({ownedQuantity,displayName:"Реликвия Лит A1",
  definition:{relicSlug:"lith_a1_relic",relicGameRef:"/relic",displayNameEn:"Lith A1",refinement,vaultStatus:"available",
    rewards:[{rewardSlug:slug,rewardGameRef:"/part",displayNameEn:"Part",chancePercent:refinement === "radiant" ? 10 : 2}]}});

describe("личная сборка", () => {
  it("сохраняет выполнение при расходовании деталей и не предлагает повторную сборку", () => {
    const completed = {...goal(), completedAt:"2026-09-10T09:00:00Z"};
    expect(goalProgress(completed).complete).toBe(false);
    expect(personalAcquisition(completed,[relic()])).toBeNull();
  });
  it("уведомляет один раз для каждого выполнения, включая повторно добавленную цель", () => {
    const completed = {...goal(), completedAt:"2026-09-10T09:00:00Z", completionPending:true};
    const seen = new Set<string>();
    expect(unseenGoalCompletions([goal(),completed],seen)).toEqual([completed]);
    seen.add(`${completed.setSlug}:${completed.completedAt}`);
    expect(unseenGoalCompletions([completed],seen)).toEqual([]);
    expect(unseenGoalCompletions([{...completed,completionPending:false}],new Set())).toEqual([]);
    expect(unseenGoalCompletions([{...completed,completedAt:"2026-09-11T09:00:00Z"}],seen)).toHaveLength(1);
  });
  it("находит ордера на детали общих комплектов, включая скрытые, с проверкой точного варианта", () => {
    const order: AccountOrder = {id:"order",itemId:"set-id",type:"sell",platinum:20,quantity:1,perTrade:1,rank:null,charges:null,subtype:null,amberStars:null,cyanStars:null,visible:false,createdAt:"",updatedAt:""};
    const account = {connected:true,profile:null,orders:[order],orderItems:{"set-id":{slug:"another_set",displayName:"",displayNameEn:"",imageUrl:null,itemKind:"standard" as const,setComponents:[{slug:"blade",requiredQuantity:2,displayName:"",displayNameEn:""}]}}};
    expect([...personalGoalListedParts(account)]).toEqual(["another_set","blade"]);
    expect(personalGoalListedParts({...account,orders:[{...order,rank:0}]}).size).toBe(0);
    expect(personalGoalListedParts({...account,orders:[{...order,type:"buy"}]}).size).toBe(0);
  });
  it("учитывает нужное количество деталей и не предлагает второй комплект после завершения", () => {
    const selected = goal();
    expect(goalProgress(selected)).toEqual({owned:2,required:3,complete:false});
    selected.parts[0].allocatedQuantity = 2;
    expect(goalProgress(selected).complete).toBe(true);
    expect(personalAcquisition(selected,[relic()])).toBeNull();
    expect(partRelics(selected.parts[0],[relic()])).toEqual([]);
  });
  it("подбирает только недостающие детали из реально имеющихся улучшений", () => {
    const route = personalAcquisition(goal(),[relic(2),relic(10,"blueprint"),relic(0,"blade","radiant")])!;
    expect(route.openings).toBe(2);
    expect(route.traces).toBe(0);
    expect(route.steps).toHaveLength(1);
    expect(route.steps[0].target).toBe("intact");
    expect(route.chance).toBeCloseTo(3.96);
  });
  it("показывает реликвии из каталога, но не считает отсутствующие копии доступными", () => {
    const sources = [relic(0,"blade","radiant"),relic(1),relic(0,"blueprint")];
    expect(partRelics(goal().parts[0],sources).map(row => row.ownedQuantity)).toEqual([1,0]);
    const route = personalAcquisition(goal(),[relic(0)])!;
    expect(route.openings).toBe(0);
    expect(route.buy[0]).toMatchObject({slug:"blade",quantity:1});
  });
});
