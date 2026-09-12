import { describe, expect, it } from "vitest";
import { addFilterObjects, objectRule, objectIsVisible, filterColor, parseMissionFilters, serializeMissionFilters, type CustomMissionFilter } from "./missionFilters";
import type { MissionObject, MissionFilter } from "./missionResearch";
const object = (extra: Partial<MissionObject> = {}): MissionObject => ({ key:"address-old", kind:"decoration", label:"Деталь окружения", nameEn:"Panel", itemPath:null, position:[1,2,3], typeNames:["Decoration"], availability:"unknown", details:[{label:"Ресурс",value:"/Lotus/Levels/Panel"}], ...extra });
const filter = (extra: Partial<CustomMissionFilter> = {}): CustomMissionFilter => ({id:"custom",name:"Мои панели",color:"#cc55aa",enabled:true,rules:[],...extra});
const standard: Record<MissionFilter, boolean> = {feather:true,pickup:true,players:true,npc:false,other:true,goals:true,lootspots:false,caches:true};

describe("свои фильтры карты", () => {
  it("включает все экземпляры при полном и коротком пути, в том числе из сохранённого фильтра", () => {
    const copies = [object(), object({ key: "another", details: [{ label: "Ресурс", value: "Panel" }] })];
    const saved = filter({ rules: [{ key: JSON.stringify(["decoration", "resource", "/Lotus/Levels/Panel"]), label: "Панель", nameEn: "Panel" }] });
    expect(copies.every(copy => objectIsVisible(copy, {...standard, other: false}, [saved]))).toBe(true);
  });
  it("отличает варианты одной модели и сохраняет выбор всех экземпляров варианта", () => {
    const first = object({variantKey:"type-v1:a"});
    const copy = object({key:"next-mission",position:[10,20,30],variantKey:"type-v1:a"});
    const different = object({variantKey:"type-v1:b"});
    const stored = parseMissionFilters(serializeMissionFilters([addFilterObjects(filter(),[first])]))[0];
    expect(objectIsVisible(copy,{...standard,other:false},[stored])).toBe(true);
    expect(objectIsVisible(different,{...standard,other:false},[stored])).toBe(false);
    const all = addFilterObjects(filter(),[first],"model");
    expect(objectIsVisible(different,{...standard,other:false},[all])).toBe(true);
  });
  it("сопоставляет объект следующей миссии по ресурсу, без адресов, координат и русского имени", () => {
    const first = object();
    const next = object({key:"address-new",position:[90,7,15],label:"Новое русское имя"});
    expect(objectRule(next).key).toBe(objectRule(first).key);
    expect(objectRule(object({details:[{label:"Ресурс",value:"/Lotus/Levels/OtherPanel"}]})).key).not.toBe(objectRule(first).key);
    const stored = serializeMissionFilters([addFilterObjects(filter(),[first,next])]);
    expect(stored).not.toContain("address-old");
    expect(stored).not.toContain("position");
    expect(parseMissionFilters(stored)[0].rules).toHaveLength(1);
  });
  it("нормализует StoreItems, различает виды объектов и не смешивает NPC по разным моделям", () => {
    expect(objectRule(object({itemPath:"/Lotus/StoreItems/Item"})).key).toBe(objectRule(object({itemPath:"/Lotus/Item"})).key);
    expect(objectRule(object({kind:"npc",details:[],nameEn:"CorpusMesh"})).key).not.toBe(objectRule(object({kind:"npc",details:[],nameEn:"GrineerMesh"})).key);
    expect(objectRule(object({kind:"npc"})).key).not.toBe(objectRule(object()).key);
  });
  it("свои переключатели заменяют общий для назначенного типа, несколько групп объединяются", () => {
    const item = object();
    const disabled = addFilterObjects(filter({enabled:false}),[item]);
    expect(objectIsVisible(item,standard,[disabled])).toBe(false);
    const enabled = {...disabled,id:"second",enabled:true,color:"#00ff00"};
    expect(objectIsVisible(item,{...standard,other:false},[disabled,enabled])).toBe(true);
    expect(filterColor(item,[disabled,enabled])).toBe("#00ff00");
    expect(objectIsVisible(object({itemPath:"/Lotus/Other"}),standard,[disabled])).toBe(true);
    expect(objectIsVisible(item,standard,[])).toBe(true);
  });
  it("сохраняет название, цвет, состояние и состав, включая пустые фильтры", () => {
    const filters = [addFilterObjects(filter({enabled:false}),[object()]), filter({id:"empty",name:"Пустой"})];
    expect(parseMissionFilters(serializeMissionFilters(filters))).toEqual(filters);
    expect(parseMissionFilters(null)).toEqual([]);
    expect(() => parseMissionFilters('{broken')).toThrow();
    expect(() => parseMissionFilters(JSON.stringify({version:2,filters}))).toThrow();
    expect(() => serializeMissionFilters([filter({color:"url(bad)"})])).toThrow();
    expect(() => serializeMissionFilters([filter(),filter()])).toThrow();
  });
});
