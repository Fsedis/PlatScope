import { describe, expect, it } from "vitest";
import { addFilterObjects, objectRule, objectIsVisible, filterColor, parseMissionFilters, serializeMissionFilters, missionDiscoveryKeys, groupMissionObjects, type CustomMissionFilter } from "./missionFilters";
import type { MissionObject, MissionFilter } from "./missionResearch";
import { indexHiddenMissionNames, parseHiddenMissionNames, serializeHiddenMissionNames } from "./missionFilters";
const object = (extra: Partial<MissionObject> = {}): MissionObject => ({ key:"address-old", kind:"decoration", label:"Деталь окружения", nameEn:"Panel", itemPath:null, position:[1,2,3], typeNames:["Decoration"], availability:"unknown", details:[{label:"Ресурс",value:"/Lotus/Levels/Panel"}], ...extra });
const filter = (extra: Partial<CustomMissionFilter> = {}): CustomMissionFilter => ({id:"custom",name:"Мои панели",color:"#cc55aa",enabled:true,rules:[],...extra});
const standard: Record<MissionFilter, boolean> = {feather:true,pickup:true,players:true,npc:false,other:true,goals:true,lootspots:false,dragon_doors:true,caches:true};

describe("свои фильтры карты", () => {
  it("сворачивает одинаковые экземпляры, сохраняя разные варианты, состояния и игроков", () => {
    const first = object({ variantKey: "variant-a", kind: "cache", availability: "available" });
    const copy = {...first, key: "copy"};
    const opened = {...first, key: "opened", availability: "opened" as const};
    const other = {...first, key: "other", variantKey: "variant-b"};
    const players = [object({key:"player-1", kind:"avatar"}), object({key:"player-2", kind:"avatar"})];
    const entries = groupMissionObjects([first,copy,opened,other,...players],true);
    expect(entries.map(entry => entry.count)).toEqual([2,1,1,1,1]);
    expect(entries[0].object).toBe(first);
    expect(groupMissionObjects([first,copy],false)).toHaveLength(2);
  });
  it("передаёт сохранённые типы в поиск без дублей, включая скрытые группы", () => {
    const first = addFilterObjects(filter(), [object()]);
    const hidden = { ...first, id: "hidden", enabled: false };
    const restored = parseMissionFilters(serializeMissionFilters([first, hidden]));
    expect(missionDiscoveryKeys(restored)).toEqual([objectRule(object()).key]);
    expect(missionDiscoveryKeys([hidden])).toEqual(missionDiscoveryKeys(restored));
    expect(missionDiscoveryKeys([])).toEqual([]);
  });
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
    // Скрытие по названию сильнее включённых групп и переживает новую миссию.
    const hidden = indexHiddenMissionNames(parseHiddenMissionNames(serializeHiddenMissionNames([{label:item.label,nameEn:item.nameEn}])));
    expect(objectIsVisible({...item,key:"new-address",variantKey:"different-variant"},standard,[enabled],hidden)).toBe(false);
    expect(objectIsVisible({...item,label:"  ДЕТАЛЬ   ОКРУЖЕНИЯ  ",nameEn:"Other"},standard,[enabled],hidden)).toBe(true);
    expect(objectIsVisible({...item,label:"Другое имя"},standard,[enabled],hidden)).toBe(false);
    expect(objectIsVisible({...item,label:"Деталь окружения редкая",nameEn:"Panel rare"},standard,[enabled],hidden)).toBe(true);
    expect(objectIsVisible(item,standard,[enabled],indexHiddenMissionNames([]))).toBe(true);
    const moa = object({kind:"npc",label:"NPC",nameEn:"SuperMoa_skel.fbx"});
    // Формат уже сохранённой записи 0.1.85: NPC не должен стать общим правилом.
    const oldHidden = indexHiddenMissionNames(parseHiddenMissionNames('{"version":1,"names":[{"label":"NPC","nameEn":"SuperMoa_skel.fbx"}]}'));
    expect(objectIsVisible({...moa,key:"new-moa",nameEn:"  SUPERMOA_skel.fbx "},{...standard,npc:true},[],oldHidden)).toBe(false);
    expect(objectIsVisible({...moa,nameEn:"CorpusCrewman_skel.fbx"},{...standard,npc:true},[],oldHidden)).toBe(true);
    expect(objectIsVisible({...moa,nameEn:"SuperMoaRare_skel.fbx"},{...standard,npc:true},[],oldHidden)).toBe(true);
    expect(objectIsVisible(moa,{...standard,npc:true},[],indexHiddenMissionNames([{label:"NPC",nameEn:""}]))).toBe(true);
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
