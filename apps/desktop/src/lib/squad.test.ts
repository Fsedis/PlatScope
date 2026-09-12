import { describe, expect, it } from "vitest";
import { buildText, refinementName, shardColor, rivenText, equipmentMatchesSearch, type SquadEquipment } from "./squad";
describe("список для повторения билда", () => {
  it("ищет русское имя и состав сохранённого билда независимо от его заголовка", () => {
    const part = { path:"/Lotus/Test", name:"Висп Прайм", nameEn:"Wisp Prime", kind:"equipment", rank:null };
    const equipment: SquadEquipment = {key:"saved",category:"Варфрейм",item:part,level:30,forma:null,upgrades:[{...part,name:"Поток Прайм",nameEn:"Primed Flow"}],modularParts:[],unreadableUpgrades:0,
      shards:[{color:"ACC_RED",effect:{...part,name:"Сила способностей",nameEn:"Ability Strength"}}],abilityOverride:{ability:{...part,name:"Рёв",nameEn:"Roar"},slot:4}};
    for (const query of ["висп", "WISP PRIME", "  висп   рев ", "Поток", "primed flow", "багровый", "сила способностей", "мой билд"]) {
      expect(equipmentMatchesSearch(equipment, query, ["Мой билд"]), query).toBe(true);
    }
    expect(equipmentMatchesSearch(equipment, "рев нейтрализация")).toBe(false);
    expect(equipmentMatchesSearch(equipment, "")).toBe(true);
  });
  it("сохраняет порядок, пустые и неизвестные позиции, ранги и связанные свойства разлома", () => {
    const part = {path:"/Lotus/Upgrades/Mods/Randomized/Test",name:"Разлом",nameEn:"Riven",kind:"mod",rank:8,slotIndex:2,fingerprint:{weaponPath:"/Lotus/Weapons/Guandao",weaponName:"Гуаньдао",weaponNameEn:"Guandao",masteryRank:12,rerolls:null,polarity:null,buffs:[{tag:"WeaponCritDamageMod",value:920199989}],curses:[{tag:"ComboDurationMod",value:317116412}]}};
    const equipment: SquadEquipment = {key:"configuration:1",category:"Ближний бой",item:{...part,fingerprint:null},level:null,forma:null,source:"memoryConfiguration",configuration:1,inventoryResolved:true,upgrades:[part],modularParts:[],unreadableUpgrades:1,upgradeSlots:[{index:0,status:"empty",part:null},{index:1,status:"unresolved",part:null},{index:2,status:"resolved",part}]};
    const text = buildText(equipment,"Владелец не подтверждён","Билд");
    expect(text).toContain("1. Пусто");
    expect(text).toContain("2. Не удалось определить улучшение");
    expect(text).toContain("3. Разлом / Riven — ранг 8");
    expect(text).toContain("Оружие: Гуаньдао");
    expect(text).toContain("преобразований: неизвестно");
    expect(text).toContain("не являются процентами");
    expect(text).not.toContain("920199989%");
    expect(text).not.toContain("ранги модов, их слоты и параметры модов разлома не подтверждены");
    expect(rivenText({...part,fingerprint:null})).toEqual([]);
  });
  it("экспортирует конфигурацию, осколки и замену без присвоения сопартийцу", () => {
    const part = { path:"/Lotus/Test", name:"Тест", nameEn:"Test", kind:"equipment", rank:null };
    const equipment: SquadEquipment = {key:"configuration:0",category:"Варфрейм",item:part,level:null,forma:null,upgrades:[],modularParts:[],unreadableUpgrades:0,source:"memoryConfiguration",configuration:2,shards:[{color:"ACC_RED",effect:part}],abilityOverride:{ability:part,slot:4}};
    const text = buildText(equipment, "Владелец не подтверждён", "Тест");
    expect(text).toContain("конфигурация 2 из памяти");
    expect(text).toContain("Осколки: 1");
    expect(text).toContain("слот 4");
    expect(text).not.toContain("сопоставлена по размеру");
    expect(buildText({...equipment,shards:null}, "", "")).toContain("Осколки: данные не получены");
    expect(buildText({...equipment,shards:[]}, "", "")).toContain("Осколки: 0");
    expect(refinementName(3)).toBe("Сияющая");
    expect(refinementName(null)).toBe("Улучшение неизвестно");
    expect(shardColor("ACC_BLUE_MYTHIC")).toContain("усиленный");
  });
  it("сохраняет неопределённые данные и не выдаёт неизвестные ранги за максимальные", () => {
    const equipment: SquadEquipment = { key: "NORMAL:0", category: "Варфрейм", item: { path: "/Lotus/Test", name: "Тест", nameEn: "Test", kind: "equipment", rank: null }, level: 30, forma: null,
      upgrades: [{ path: "/Lotus/Unknown", name: "", nameEn: "", kind: "unknown", rank: null }, { path: "/Lotus/Mod", name: "Мод", nameEn: "Mod", kind: "mod", rank: null }], modularParts: [], unreadableUpgrades: 1 };
    const text = buildText(equipment, "Tenno", "Мой билд", "Для выживания");
    expect(text).toContain("Мод / Mod — ранг неизвестен");
    expect(text).toContain("/Lotus/Unknown");
    expect(text).toContain("форм: неизвестно");
    expect(text).toContain("Неполный билд");
    expect(text).toContain("Для выживания");
  });
});
