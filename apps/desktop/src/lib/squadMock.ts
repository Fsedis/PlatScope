import type { SavedBuild, SquadEquipment, SquadPart, SquadView } from "./squad";

// Миниатюры из локального снимка публичного каталога Warframe Market от 06.09.2026.
const images: Record<string, string> = {
  "/Lotus/Powersuits/Wisp/WispPrime": "en/thumbs/wisp_prime_set.609bc4e8676a77247cc126ca150cb132.128x128.png",
  "/Lotus/Powersuits/Jade/NyxPrime": "en/thumbs/nyx_prime_set.fd41c04c9e9bcc7e0e6963914f68f880.128x128.png",
  "/Lotus/Weapons/Tenno/LongGuns/PrimePhantasma/PhantasmaPrimeShotgun": "en/thumbs/phantasma_prime_set.a296718b56d5a40b6df80bbedeeb4b75.128x128.png",
  "/Lotus/Weapons/Tenno/Pistols/PrimeLex/PrimeLex": "en/thumbs/lex_prime_set.13776cf63dfd68968067b517184a6ac0.128x128.png",
  "/Lotus/Weapons/Tenno/Melee/Polearms/PrimeGuandao/PrimeGuandaoWeapon": "en/thumbs/guandao_prime_set.cded3d77be7d60eef1d3eb82f189e70f.128x128.png",
  "/Lotus/Types/Sentinels/SentinelPowersuits/PrimeHeliosPowerSuit": "en/thumbs/helios_prime_set.625b7c74ea5dd61b1840f8094df7648a.128x128.png",
  "/Lotus/Upgrades/Mods/Warframe/Expert/AvatarPowerMaxModExpert": "ru/thumbs/primed_flow.2dc03b7a44d324d135ae8e44f89885a0.128x128.png",
  "/Lotus/Upgrades/Mods/Warframe/DualStat/CorruptedPowerEfficiencyWarframe": "ru/thumbs/blind_rage.75fc70337700cc463e3ee46f142b9e63.128x128.png",
  "/Lotus/Upgrades/Mods/Warframe/AvatarAbilityStrengthMod": "ru/thumbs/intensify.4e7e94644bc42c23665fd0dcc2fc519f.128x128.png",
  "/Lotus/Upgrades/Mods/Warframe/AvatarAbilityDurationMod": "ru/thumbs/continuity.d3175cd01c144a47114e37f29bf432f6.128x128.png",
  "/Lotus/Upgrades/CosmeticEnhancers/Utility/GolemArcaneRadialEnergyOnEnergyPickup": "en/thumbs/arcane_energize.e5f85590ceb29db89c6753dfdf823485.128x128.png",
  "/Lotus/Upgrades/CosmeticEnhancers/Defensive/ArmourOnDamage": "en/thumbs/arcane_guardian.5d8874dfd06eaaf9aaceef59d99213b3.128x128.png",
  "/Lotus/Upgrades/Mods/Shotgun/WeaponDamageAmountMod": "ru/thumbs/point_blank.ea7390ac3cafdbed38bef397132446f3.128x128.png",
  "/Lotus/Upgrades/Mods/Shotgun/WeaponFireIterationsMod": "ru/thumbs/hells_chamber.105da5a7db7426096ba39ce96fa1c55d.128x128.png",
  "/Lotus/Upgrades/Mods/Pistol/WeaponDamageAmountMod": "ru/thumbs/hornet_strike.1c3197bb17bf720985347754d12b7e5e.128x128.png",
  "/Lotus/Upgrades/Mods/Pistol/WeaponFireIterationsMod": "ru/thumbs/barrel_diffusion.b96103b17a46dbc974cd61632ff2716b.128x128.png",
  "/Lotus/Upgrades/Mods/Melee/WeaponMeleeDamageMod": "ru/thumbs/pressure_point.c784c3586801b301b8bdf5fa2f425179.128x128.png",
  "/Lotus/Upgrades/Mods/Melee/Event/ComboStatusChanceMod": "ru/thumbs/weeping_wounds.c8164175deacef952964d64fac426559.128x128.png",
  "/Lotus/Upgrades/Mods/Sets/Synth/SentinelSynthDeconstructMod": "ru/thumbs/synth_deconstruct.544f667f6d8adf4aa38dd69dc626680d.128x128.png",
};
const part = (path: string, name: string, nameEn: string, kind = "mod", rank: number | null = null): SquadPart => ({
  path, name, nameEn, kind, rank,
  imageUrl: images[path] ? `https://warframe.market/static/assets/items/images/${images[path]}` : null,
});
type StoredBuild = Omit<SavedBuild, "equipment"> & { equipment: SquadEquipment | SquadEquipment[] };
const normalizeBuild = (build: StoredBuild): SavedBuild => ({ ...build, equipment: Array.isArray(build.equipment) ? build.equipment : [build.equipment] });
const gear: SquadEquipment = {
  key: "NORMAL:0", category: "Варфрейм", item: part("/Lotus/Powersuits/Wisp/WispPrime", "Висп Прайм", "Wisp Prime", "equipment"), level: 30, forma: 4,
  upgrades: [part("/Lotus/Upgrades/Mods/Warframe/Expert/AvatarPowerMaxModExpert", "Поток Прайм", "Primed Flow"), part("/Lotus/Upgrades/Mods/Warframe/DualStat/CorruptedPowerEfficiencyWarframe", "Слепая Ярость", "Blind Rage"), part("/Lotus/Upgrades/Mods/Warframe/AvatarAbilityStrengthMod", "Усиление", "Intensify"), part("/Lotus/Upgrades/Mods/Warframe/AvatarAbilityDurationMod", "Непрерывность", "Continuity"), part("/Lotus/Upgrades/CosmeticEnhancers/Utility/GolemArcaneRadialEnergyOnEnergyPickup", "Мистическая Зарядка", "Arcane Energize", "arcane"), part("/Lotus/Unknown/LongUnresolvedUpgradeNameForVerification", "", "", "unknown")],
  modularParts: [], unreadableUpgrades: 1, shards: null, abilityOverride: null, source: "squad",
  context: { focus: part("/Lotus/Upgrades/Focus/Power/PowerFocusAbility", "Зенурик", "Zenurik", "focus"), relic: part("/Lotus/TestRelic", "Лит G14", "Lith G14", "relic"), refinement: 3 },
};
export function makeSquadMock() {
  const scenario = new URLSearchParams(location.search).get("squadScenario");
  const showcase = scenario === "showcase";
  // Демонстрационная коллекция изолирована: сохранения обычного mock не меняются.
  const savedKey = showcase ? "platscope.mock.squad.builds.showcase" : "platscope.mock.squad.builds";
  const stored = localStorage.getItem(savedKey);
  let saved: SavedBuild[] = (JSON.parse(stored ?? "[]") as StoredBuild[]).map(normalizeBuild);
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
  const showcaseEquipment: SquadEquipment[] = [
    { ...structuredClone(gear), upgrades: [
      ...gear.upgrades.slice(0, 4).map((upgrade, index) => ({ ...upgrade, rank: index === 2 ? null : [10, 8, 0, 5][index] })),
      { ...gear.upgrades[4], rank: 5 },
      part("/Lotus/Upgrades/CosmeticEnhancers/Defensive/ArmourOnDamage", "Мистический Страж", "Arcane Guardian", "arcane", 3),
      part("/Lotus/Demo/LongModName", "Улучшение с длинным названием для проверки переноса на карточке", "A long upgrade name for checking card wrapping", "mod", 0),
      { ...part("/Lotus/Demo/MissingImage", "Улучшение без доступной картинки", "Upgrade with an unavailable image"), imageUrl: "/__platscope_mock_missing_image__.png" },
      structuredClone(gear.upgrades[5]),
    ] },
    { ...structuredClone(gear), key: "NORMAL:2", category: "Основное оружие", forma: 6, context: undefined, unreadableUpgrades: 0,
      item: part("/Lotus/Weapons/Tenno/LongGuns/PrimePhantasma/PhantasmaPrimeShotgun", "Фантазма Прайм", "Phantasma Prime", "equipment"),
      upgrades: [part("/Lotus/Upgrades/Mods/Shotgun/WeaponDamageAmountMod", "В упор", "Point Blank", "mod", 5), part("/Lotus/Upgrades/Mods/Shotgun/WeaponFireIterationsMod", "Адский Патронник", "Hell’s Chamber", "mod", 5)] },
    { ...structuredClone(gear), key: "NORMAL:1", category: "Вторичное оружие", forma: 2, context: undefined, unreadableUpgrades: 0,
      item: part("/Lotus/Weapons/Tenno/Pistols/PrimeLex/PrimeLex", "Лекс Прайм", "Lex Prime", "equipment"),
      upgrades: [part("/Lotus/Upgrades/Mods/Pistol/WeaponDamageAmountMod", "Удар Осы", "Hornet Strike", "mod", 10), part("/Lotus/Upgrades/Mods/Pistol/WeaponFireIterationsMod", "Двойной Ствол", "Barrel Diffusion")] },
    { ...structuredClone(gear), key: "NORMAL:3", category: "Ближний бой", forma: 3, context: undefined, unreadableUpgrades: 0,
      item: part("/Lotus/Weapons/Tenno/Melee/Polearms/PrimeGuandao/PrimeGuandaoWeapon", "Гуаньдао Прайм", "Guandao Prime", "equipment"),
      upgrades: [part("/Lotus/Upgrades/Mods/Melee/WeaponMeleeDamageMod", "Болевая Точка", "Pressure Point", "mod", 5), part("/Lotus/Upgrades/Mods/Melee/Event/ComboStatusChanceMod", "Стенающие Раны", "Weeping Wounds", "mod", 5), structuredClone(riven)] },
    { ...structuredClone(gear), key: "SENTINEL:0", category: "Спутник", level: null, forma: null, context: undefined, unreadableUpgrades: 0,
      item: part("/Lotus/Types/Sentinels/SentinelPowersuits/PrimeHeliosPowerSuit", "Гелиос Прайм", "Helios Prime", "equipment"),
      upgrades: [part("/Lotus/Upgrades/Mods/Sets/Synth/SentinelSynthDeconstructMod", "Синт-Расщепление", "Synth Deconstruct", "mod", 3)] },
  ];
  if (showcase && stored === null) {
    const common = { player: "LotusGuide", platform: "PC", capturedAt: "2026-09-18T02:00:00Z", savedAt: "2026-09-18T02:10:00Z" };
    const demoBuilds: StoredBuild[] = [
      { ...common, id: "showcase-full-loadout", title: "Поддержка отряда — весь комплект для долгих миссий", note: "Сохранил все пять предметов. Перед применением сверить неизвестные ранги в Арсенале.\nЛекс оставлен для одиночных целей.", equipment: structuredClone(showcaseEquipment) },
      { ...common, id: "showcase-legacy-single", title: "Старое сохранение: Фантазма Прайм", savedAt: "2026-09-12T02:10:00Z", note: "Запись старого формата с одним предметом и без изображения.", equipment: { ...structuredClone(showcaseEquipment[1]), item: { ...showcaseEquipment[1].item, imageUrl: undefined } } },
    ];
    saved = demoBuilds.map(normalizeBuild);
  }
  const view: SquadView = { enabled: true, running: scenario !== "offline", scanning: false, error: scenario === "error" ? "Не удалось прочитать процесс Warframe. Повторите попытку." : null,
    members: scenario === "empty" || scenario === "offline" ? [] : [
      { name: "LotusGuide", platform: "PC", isHost: true, status: "matched", capturedAt: "2026-09-12T02:00:00Z", masteryRank: 30, equipment: [gear, { ...gear, key: "NORMAL:2", category: "Основное оружие", item: part("/Lotus/Weapons/Tenno/Shotgun/PrimePhantasma", "Фантазма Прайм", "Phantasma Prime", "equipment"), upgrades: [part("/Lotus/Upgrades/Mods/Shotgun/WeaponDamageAmountMod", "В упор", "Point Blank")] }] },
      { name: "Tenno_With_A_Long_Name", platform: "PS", isHost: false, status: "ambiguous", capturedAt: null, masteryRank: null, equipment: [] },
    ], saved, configurations: [], configurationsAt: null, readingConfigurations: false };
  if (showcase) {
    configurations.push({ ...structuredClone(showcaseEquipment[3]), key: "configuration:4", source: "memoryConfiguration", configuration: 1,
      item: part("/Lotus/Demo/ModularWeapon", "Модульное оружие с длинным собственным названием", "Custom modular weapon with a long name", "equipment"),
      modularParts: [part("/Lotus/Demo/Strike", "Ударная часть", "Strike", "part"), part("/Lotus/Demo/Grip", "Рукоять", "Grip", "part"), part("/Lotus/Demo/Link", "Связка", "Link", "part")],
    });
    view.members = [
      { name: "LotusGuide", platform: "PC", isHost: true, status: "matched", capturedAt: "2026-09-18T02:00:00Z", masteryRank: 30, equipment: structuredClone(showcaseEquipment) },
      { name: "Tenno_With_A_Very_Long_Name", platform: "PS", isHost: false, status: "matched", capturedAt: "2026-09-18T02:01:00Z", masteryRank: 18,
        equipment: [{ ...structuredClone(gear), item: part("/Lotus/Powersuits/Jade/NyxPrime", "Никс Прайм", "Nyx Prime", "equipment"), level: null, forma: null, upgrades: [], unreadableUpgrades: 0, shards: [], context: undefined }] },
      { name: "WaitingForLoadout", platform: "Xbox", isHost: false, status: "pending", capturedAt: null, masteryRank: null, equipment: [] },
      { name: "UnconfirmedTenno", platform: "PC", isHost: false, status: "ambiguous", capturedAt: null, masteryRank: 12, equipment: [] },
    ];
  }
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
      const equipment = fromConfiguration ? member?.equipment.filter(e => e.key === args.equipmentKey) : member?.equipment;
      if (!member || !equipment?.length) throw "Экипировка уже недоступна.";
      if (member.capturedAt !== args.capturedAt) throw "Снимок экипировки изменился. Просмотрите его и сохраните снова.";
      if (!saved.some(build => build.player === member.name && build.platform === member.platform && build.capturedAt === member.capturedAt && JSON.stringify(build.equipment.map(e => e.key)) === JSON.stringify(equipment.map(e => e.key)))) {
        saved.unshift({ id: crypto.randomUUID(), title: fromConfiguration ? equipment[0].item.name : `Экипировка ${member.name}`, player: member.name, platform: member.platform, capturedAt: member.capturedAt!, savedAt: new Date().toISOString(), note: "", equipment: structuredClone(equipment) });
      }
    }
    if (command === "squad_edit_build") { const b = saved.find(b => b.id === args.id); if (b) { b.title = String(args.title); b.note = String(args.note); } }
    if (command === "squad_delete_build") saved = saved.filter(b => b.id !== args.id);
    if (["squad_save_build", "squad_edit_build", "squad_delete_build"].includes(command)) localStorage.setItem(savedKey, JSON.stringify(saved));
    view.saved = saved;
    return structuredClone(view);
  };
}
