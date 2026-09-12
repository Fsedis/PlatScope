import { matchesSearch } from "./searchText";

export interface RivenFingerprint {
  weaponPath: string | null; weaponName: string | null; weaponNameEn: string | null;
  masteryRank: number | null; rerolls: number | null; polarity: string | null;
  buffs: { tag: string; value: number | null }[]; curses: { tag: string; value: number | null }[];
}
export interface SquadPart { path: string; name: string; nameEn: string; kind: string; rank: number | null; slotIndex?: number | null; fingerprint?: RivenFingerprint | null }
export interface SquadEquipment {
  key: string; category: string; item: SquadPart; level: number | null; forma: number | null;
  upgrades: SquadPart[]; modularParts: SquadPart[]; unreadableUpgrades: number;
  shards?: { color: string; effect: SquadPart }[] | null;
  abilityOverride?: { ability: SquadPart; slot: number | null } | null;
  configuration?: number | null;
  source?: string;
  inventoryResolved?: boolean;
  upgradeSlots?: { index: number; status: "empty" | "resolved" | "unresolved"; part: SquadPart | null }[] | null;
  context?: { focus: SquadPart | null; relic: SquadPart | null; refinement: number | null };
}
export interface SquadMember {
  name: string; platform: string; isHost: boolean; status: string; capturedAt: string | null;
  equipment: SquadEquipment[]; masteryRank: number | null;
}
export interface SavedBuild {
  id: string; title: string; player: string; platform: string; capturedAt: string;
  savedAt: string; note: string; equipment: SquadEquipment;
}
export interface SquadView {
  enabled: boolean; running: boolean; scanning: boolean; error: string | null;
  members: SquadMember[]; saved: SavedBuild[];
  configurations: SquadEquipment[]; configurationsAt: string | null; readingConfigurations: boolean;
}
export function refinementName(level: number | null | undefined): string { return ["Нетронутая", "Необычная", "Безупречная", "Сияющая"][level ?? -1] ?? "Улучшение неизвестно"; }
export function shardColor(color: string): string {
  const names: Record<string,string> = { ACC_RED: "Багровый", ACC_BLUE: "Лазурный", ACC_YELLOW: "Янтарный", ACC_GREEN: "Изумрудный", ACC_ORANGE: "Топазовый", ACC_PURPLE: "Аметистовый" };
  const base = names[color.replace(/_MYTHIC$/, "")];
  return base ? `${base}${color.endsWith("_MYTHIC") ? " · усиленный" : ""}` : "Неизвестный цвет";
}
export function partName(part: SquadPart): string { return part.name || part.nameEn || "Неизвестный предмет"; }
export function equipmentMatchesSearch(equipment: SquadEquipment, query: string, extra: string[] = []): boolean {
  const parts = [equipment.item, ...equipment.upgrades, ...equipment.modularParts,
    ...(equipment.upgradeSlots ?? []).flatMap(slot => slot.part ? [slot.part] : []),
    ...(equipment.shards ?? []).map(shard => shard.effect),
    ...[equipment.abilityOverride?.ability, equipment.context?.focus, equipment.context?.relic].filter((part): part is SquadPart => Boolean(part))];
  return matchesSearch(query, [equipment.category, ...extra,
    ...(equipment.shards ?? []).map(shard => shardColor(shard.color)),
    ...parts.flatMap(part => [part.name, part.nameEn, part.path, part.fingerprint?.weaponName, part.fingerprint?.weaponNameEn,
      ...(part.fingerprint ? [...part.fingerprint.buffs, ...part.fingerprint.curses].map(stat => rivenStatName(stat.tag)) : [])])]);
}
export function rivenStatName(tag: string): string {
  const names: Record<string,string> = { ComboDurationMod:"Длительность комбо", SlideAttackCritChanceMod:"Шанс крита при атаке в скольжении", WeaponCritChanceMod:"Шанс критического удара", WeaponCritDamageMod:"Критический урон", WeaponDamageAmountMod:"Урон", WeaponElectricityDamageMod:"Урон электричеством", WeaponFactionDamageCorpus:"Урон по Корпусу", WeaponFactionDamageGrineer:"Урон по Гринир", WeaponFactionDamageInfested:"Урон по заражённым", WeaponMeleeFactionDamageInfested:"Урон по заражённым", WeaponFireIterationsMod:"Мультивыстрел", WeaponFireRateMod:"Скорость стрельбы / атаки", WeaponImpactDamageMod:"Ударный урон", WeaponMeleeComboBonusOnHitMod:"Вероятность дополнительного счётчика комбо", WeaponReloadSpeedMod:"Скорость перезарядки", WeaponSlashDamageMod:"Режущий урон", WeaponStunChanceMod:"Шанс статуса" };
  return names[tag] ?? tag;
}
export function rivenText(part: SquadPart): string[] {
  const f = part.fingerprint;
  return f ? [`Мод разлома: ${partName(part)}; ранг ${part.rank ?? "неизвестен"}`, `Оружие: ${f.weaponName || f.weaponNameEn || f.weaponPath || "не определено"}`, `Мастерство: ${f.masteryRank ?? "неизвестно"}; преобразований: ${f.rerolls ?? "неизвестно"}`,
    ...f.buffs.map(s => `Плюс: ${rivenStatName(s.tag)}; исходное значение ${s.value ?? "не получено"}`), ...f.curses.map(s => `Минус: ${rivenStatName(s.tag)}; исходное значение ${s.value ?? "не получено"}`), "Исходные значения свойств не являются процентами бонуса."] : [];
}
export function buildText(equipment: SquadEquipment, player: string, title: string, note = ""): string {
  const describe = (p: SquadPart) => `${partName(p)}${p.nameEn && p.nameEn !== p.name ? ` / ${p.nameEn}` : ""}${p.name || p.nameEn ? "" : ` (${p.path})`} — ${p.rank === null ? "ранг неизвестен" : `ранг ${p.rank}`}`;
  return [title, `Игрок: ${player}`, `Предмет: ${partName(equipment.item)}`, `Уровень предмета: ${equipment.level ?? "неизвестен"}; форм: ${equipment.forma ?? "неизвестно"}`,
    equipment.source === "memoryConfiguration" ? `Источник: ${equipment.configuration ? `конфигурация ${equipment.configuration} из памяти` : "запись инвентаря из памяти"}. Владелец и актуальность не подтверждены.` : "Источник: снимок экипировки; принадлежность сопоставлена по размеру данных.",
    equipment.inventoryResolved ? "Моды сопоставлены внутри одного инвентаря. Позиции — порядок в конфигурации, не сетка Арсенала. Владелец и активный вариант не подтверждены." : "Неполный билд: неизвестные ранги, порядок и параметры нельзя восстановить из этой записи.", "",
    ...(equipment.context?.focus ? [`Фокус: ${partName(equipment.context.focus)}`] : []),
    ...(equipment.context?.relic ? [`Реликвия: ${partName(equipment.context.relic)} — ${refinementName(equipment.context.refinement)}`] : []),
    ...(equipment.category === "Варфрейм" ? [equipment.shards == null ? "Осколки: данные не получены." : `Осколки: ${equipment.shards.length}`, ...(equipment.shards ?? []).map((s, i) => `${i+1}. ${shardColor(s.color)} — ${partName(s.effect)} (${s.effect.path})`), equipment.abilityOverride ? `Заменённая способность: ${partName(equipment.abilityOverride.ability)}; слот ${equipment.abilityOverride.slot ?? "неизвестен"} (${equipment.abilityOverride.ability.path})` : "Заменённая способность: не указана в полученных данных."] : []),
    ...(equipment.upgradeSlots ? ["Порядок в конфигурации:", ...equipment.upgradeSlots.map(s => `${s.index + 1}. ${s.status === "empty" ? "Пусто" : s.part ? describe(s.part) : "Не удалось определить улучшение"}`)] : ["Моды:", ...equipment.upgrades.filter(p => p.kind === "mod").map(describe),
      "Мистификаторы:", ...equipment.upgrades.filter(p => p.kind === "arcane").map(describe),
      "Неопределённые улучшения:", ...equipment.upgrades.filter(p => p.kind === "unknown").map(describe)]),
    ...(equipment.modularParts.length ? ["Части предмета:", ...equipment.modularParts.map(describe)] : []),
    ...(equipment.unreadableUpgrades ? [`Не разобрано элементов: ${equipment.unreadableUpgrades}`] : []),
    ...rivenText(equipment.item), ...equipment.upgrades.flatMap(rivenText),
    ...(note ? ["", `Заметка: ${note}`] : []),
  ].join("\n");
}
