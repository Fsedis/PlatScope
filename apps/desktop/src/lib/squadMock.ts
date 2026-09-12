import type { SavedBuild, SquadEquipment, SquadPart, SquadView } from "./squad";
const part = (path: string, name: string, nameEn: string, kind = "mod"): SquadPart => ({ path, name, nameEn, kind, rank: null });
const gear: SquadEquipment = {
  key: "NORMAL:0", category: "Варфрейм", item: part("/Lotus/Powersuits/Wisp/WispPrime", "Висп Прайм", "Wisp Prime", "equipment"), level: 30, forma: 4,
  upgrades: [part("/Lotus/Upgrades/Mods/Warframe/Expert/AvatarPowerMaxModExpert", "Поток Прайм", "Primed Flow"), part("/Lotus/Upgrades/Mods/Warframe/DualStat/CorruptedPowerEfficiencyWarframe", "Слепая Ярость", "Blind Rage"), part("/Lotus/Upgrades/Mods/Warframe/AvatarAbilityStrengthMod", "Усиление", "Intensify"), part("/Lotus/Upgrades/Mods/Warframe/AvatarAbilityDurationMod", "Непрерывность", "Continuity"), part("/Lotus/Upgrades/CosmeticEnhancers/Utility/GolemArcaneRadialEnergyOnEnergyPickup", "Мистическая Зарядка", "Arcane Energize", "arcane"), part("/Lotus/Unknown/LongUnresolvedUpgradeNameForVerification", "", "", "unknown")],
  modularParts: [], unreadableUpgrades: 1, shards: null, abilityOverride: null, source: "squad",
  context: { focus: part("/Lotus/Upgrades/Focus/Power/PowerFocusAbility", "Зенурик", "Zenurik", "focus"), relic: part("/Lotus/TestRelic", "Лит G14", "Lith G14", "relic"), refinement: 3 },
};
export function makeSquadMock() {
  const scenario = new URLSearchParams(location.search).get("squadScenario");
  let saved: SavedBuild[] = JSON.parse(localStorage.getItem("platscope.mock.squad.builds") ?? "[]");
  let recording = { active: false, scanning: false, path: null as string | null, startedAt: null as string | null, samples: 0, bytes: 0, changedFields: [] as string[], error: null, stopReason: null as string | null };
  let binary = { active:false, stopping:false, scanning:false, path:null as string | null, samples:0, bytes:0, readBytes:0, holes:0, lastComplete:null as boolean | null, error:null as string | null, stopReason:null as string | null };
  const configurations: SquadEquipment[] = [
    { ...structuredClone(gear), key: "configuration:0", configuration: 1, source: "memoryConfiguration", context: undefined, shards: Array.from({length:5}, (_,i) => ({color: i < 2 ? "ACC_RED" : "ACC_YELLOW_MYTHIC", effect: part(`/Lotus/Upgrades/Invigorations/ArchonCrystalUpgrades/${i < 2 ? "ArchonCrystalUpgradeWarframeAbilityStrength" : "ArchonCrystalUpgradeWarframeCastingSpeed"}`, i < 2 ? "Сила способностей" : "Скорость применения способностей", i < 2 ? "Ability Strength" : "Casting Speed", "effect")})), abilityOverride: { ability: part("/Lotus/Powersuits/Hoplite/Abilities/HopliteBashAbility", "Удар Тарроса", "Tharros Strike", "ability"), slot: 2 } },
    { ...structuredClone(gear), key: "configuration:1", configuration: 2, source: "memoryConfiguration", context: undefined, shards: [], abilityOverride: null },
  ];
  configurations[0].inventoryResolved = true;
  configurations[0].upgradeSlots = [
    {index:0,status:"resolved",part:{...structuredClone(gear.upgrades[0]),rank:10,slotIndex:0}},
    {index:1,status:"empty",part:null},
    {index:2,status:"unresolved",part:null},
    {index:3,status:"resolved",part:{...structuredClone(gear.upgrades[2]),rank:null,slotIndex:3}},
  ];
  configurations[0].upgrades = configurations[0].upgradeSlots.flatMap(s => s.part ? [s.part] : []);
  const riven: SquadPart = {...part("/Lotus/Upgrades/Mods/Randomized/PlayerMeleeWeaponRandomModRare", "Мод разлома", "Riven Mod"),rank:8,fingerprint:{weaponPath:"/Lotus/Weapons/Tenno/Melee/Polearms/TnGuandaoPolearm/TnGuandaoPolearmWeapon",weaponName:"Гуаньдао",weaponNameEn:"Guandao",masteryRank:12,rerolls:68,polarity:"AP_TACTIC",buffs:[{tag:"WeaponSlashDamageMod",value:912519347},{tag:"WeaponCritDamageMod",value:920199989},{tag:"WeaponCritChanceMod",value:189288249}],curses:[{tag:"ComboDurationMod",value:317116412}]}};
  configurations.push({...structuredClone(configurations[0]),key:"configuration:2",category:"Ближний бой",item:part("/Lotus/Weapons/Tenno/Melee/Polearms/TnGuandaoPolearm/TnGuandaoPolearmWeapon","Гуаньдао","Guandao","equipment"),shards:null,abilityOverride:null,upgrades:[riven],upgradeSlots:[{index:0,status:"resolved",part:riven}],unreadableUpgrades:0});
  configurations.push({...structuredClone(configurations[0]),key:"configuration:3",category:"Мод разлома",item:riven,level:null,forma:null,configuration:null,shards:null,abilityOverride:null,upgrades:[],upgradeSlots:null,unreadableUpgrades:0});
  const view: SquadView = { enabled: true, running: scenario !== "offline", scanning: false, error: scenario === "error" ? "Не удалось прочитать процесс Warframe. Повторите попытку." : null,
    members: scenario === "empty" || scenario === "offline" ? [] : [
      { name: "LotusGuide", platform: "PC", isHost: true, status: "matched", capturedAt: "2026-09-12T02:00:00Z", masteryRank: 30, equipment: [gear, { ...gear, key: "NORMAL:2", category: "Основное оружие", item: part("/Lotus/Weapons/Tenno/Shotgun/PrimePhantasma", "Фантазма Прайм", "Phantasma Prime", "equipment"), upgrades: [part("/Lotus/Upgrades/Mods/Shotgun/WeaponDamageAmountMod", "В упор", "Point Blank")] }] },
      { name: "Tenno_With_A_Long_Name", platform: "PS", isHost: false, status: "ambiguous", capturedAt: null, masteryRank: null, equipment: [] },
    ], saved, configurations: [], configurationsAt: null, readingConfigurations: false };
  return (command: string, args: Record<string, unknown> = {}): SquadView | string => {
    if (command.startsWith("binary_recording_")) {
      if (command === "binary_recording_start") {
        if (scenario === "offline") throw "Запустите Warframe перед записью.";
        binary = {...binary, active:true, path:"C:/PlatScope/diagnostics/binary-memory/warframe-binary-test", samples:1, bytes:2.4 * 1024 ** 3, readBytes:7.5 * 1024 ** 3, lastComplete:true, error:null, stopReason:null};
      }
      if (command === "binary_recording_sample") { binary.samples++; binary.bytes += 0.3 * 1024 ** 3; }
      if (command === "binary_recording_stop") { binary.active = false; binary.lastComplete = false; binary.stopReason = "Остановлено пользователем."; }
      return structuredClone(binary) as unknown as SquadView;
    }
    if (command.startsWith("memory_recording_")) {
      if (command === "memory_recording_start") recording = {...recording, active:true, path:"C:\\PlatScope\\diagnostics\\memory\\warframe-memory-test.jsonl", startedAt: new Date().toISOString(), samples:1, bytes:204800, stopReason:null};
      if (command === "memory_recording_sample") { recording.samples++; recording.bytes += 204800; }
      if (command === "memory_recording_stop") { recording.active = false; recording.stopReason = "Остановлено пользователем."; }
      return structuredClone(recording) as unknown as SquadView;
    }
    if (command === "squad_read_configurations") { view.configurations = structuredClone(configurations); view.configurationsAt = new Date().toISOString(); }
    if (command === "squad_export_text") return "C:\\PlatScope\\build-exports\\build-test.txt";
    if (command === "squad_set_enabled") view.enabled = Boolean(args.enabled);
    if (command === "squad_save_build") {
      const fromConfiguration = args.source === "configuration";
      const member = fromConfiguration ? { name: "Владелец не подтверждён", platform:"", capturedAt: view.configurationsAt, equipment:view.configurations } : view.members.find(m => m.name === args.player);
      const equipment = member?.equipment.find(e => e.key === args.equipmentKey);
      if (!member || !equipment) throw "Экипировка уже недоступна.";
      saved.unshift({ id: crypto.randomUUID(), title: equipment.item.name, player: member.name, platform: member.platform, capturedAt: member.capturedAt!, savedAt: new Date().toISOString(), note: "", equipment: structuredClone(equipment) });
    }
    if (command === "squad_edit_build") { const b = saved.find(b => b.id === args.id); if (b) { b.title = String(args.title); b.note = String(args.note); } }
    if (command === "squad_delete_build") saved = saved.filter(b => b.id !== args.id);
    if (command !== "squad_status") localStorage.setItem("platscope.mock.squad.builds", JSON.stringify(saved));
    view.saved = saved;
    return structuredClone(view);
  };
}
