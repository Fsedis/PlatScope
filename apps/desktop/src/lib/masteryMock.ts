import type { MasteryItemView, MasteryView } from "./mastery";

/** Только данные фонового браузера: не связаны с игровым аккаунтом. */
export function makeMasteryMock(scenario: string | null): MasteryView {
  const seeds: MasteryItemView[] = [
    { gameRef: "/Lotus/Powersuits/Jade/JadePrime", displayName: "Никс Прайм", displayNameEn: "Nyx Prime", category: "warframe", imageUrl: "https://warframe.market/static/assets/items/images/en/thumbs/nyx_prime_set.fd41c04c9e9bcc7e0e6963914f68f880.128x128.png", maxRank: 30, xp: 8_124_000, status: "mastered", reason: "history_confirmed", setSlugs: ["nyx_prime_set"] },
    { gameRef: "/Lotus/Demo/Mastery/Sold", displayName: "Брэтон — ранее продан", displayNameEn: "Braton — previously sold", category: "primary", imageUrl: null, maxRank: 30, xp: 920_000, status: "mastered", reason: "history_confirmed", setSlugs: [] },
    { gameRef: "/Lotus/Demo/Mastery/Rhino", displayName: "Рино Прайм", displayNameEn: "Rhino Prime", category: "warframe", imageUrl: null, maxRank: 30, xp: 93_000, masteryRank: 9, status: "progress", reason: "history_partial", setSlugs: ["rhino_prime_set"] },
    { gameRef: "/Lotus/Demo/Mastery/HighXp", displayName: "Болтор Прайм", displayNameEn: "Boltor Prime", category: "primary", imageUrl: null, maxRank: 30, xp: 21_456_100, masteryRank: 30, status: "mastered", reason: "history_confirmed", setSlugs: ["boltor_prime_set"] },
    { gameRef: "/Lotus/Demo/Mastery/Boar", displayName: "Боар Прайм", displayNameEn: "Boar Prime", category: "primary", imageUrl: null, maxRank: 30, xp: null, status: "unknown", reason: "no_record", setSlugs: ["boar_prime_set"] },
    { gameRef: "/Lotus/Demo/Mastery/Amp", displayName: "Усилитель с особыми условиями освоения", displayNameEn: "Amp with special mastery requirements", category: "amp", imageUrl: null, maxRank: null, xp: 10_000_000, status: "unknown", reason: "unsupported", setSlugs: [] },
    { gameRef: "/Lotus/Demo/Mastery/Companion", displayName: "Кават Адарза", displayNameEn: "Adarza Kavat", category: "companion", imageUrl: null, maxRank: 30, xp: 6_000, masteryRank: 2, status: "progress", reason: "history_partial", setSlugs: [] },
    { gameRef: "/Lotus/Demo/Mastery/Coda", displayName: "Хема Кода", displayNameEn: "Coda Hema", category: "primary", imageUrl: null, maxRank: 40, masteryRank: 30, xp: 450_000, status: "progress", reason: "history_partial", setSlugs: [] },
    { gameRef: "/Lotus/Demo/Mastery/Raplak", displayName: "Призма: Раплак", displayNameEn: "Raplak Prism", category: "amp", imageUrl: null, maxRank: 30, masteryRank: 28, xp: 406_864, status: "progress", reason: "history_partial", setSlugs: [] },
  ];
  // Те же карточки сетов и наград во всех состояниях, без изменения торговых данных.
  if (scenario === "progress") Object.assign(seeds[0], { xp: 93_000, masteryRank: 9, status: "progress", reason: "history_partial" });
  if (scenario === "not-mastered") Object.assign(seeds[0], { xp: null, masteryRank: null, status: "progress", reason: "history_absent" });
  if (scenario === "unknown") Object.assign(seeds[0], { xp: null, masteryRank: null, status: "unknown", reason: "no_record" });
  if (scenario === "unmapped") seeds.shift();
  const items = scenario === "large"
    ? Array.from({ length: 97 }, (_, index) => ({ ...seeds[index % seeds.length], gameRef: `${seeds[index % seeds.length].gameRef}/${index}`, displayName: `${seeds[index % seeds.length].displayName} ${index + 1}`, displayNameEn: `${seeds[index % seeds.length].displayNameEn} ${index + 1}` }))
    : seeds;
  return {
    source: scenario === "none" ? null : "inventory_xp_info",
    observedAt: scenario === "none" ? null : "2026-09-05T08:15:00Z",
    refreshFailed: scenario === "stale",
    catalogAvailable: scenario !== "catalog",
    items: scenario === "catalog" ? [] : scenario === "none"
      ? items.map(item => ({ ...item, xp: null, masteryRank: null, status: "unknown", reason: "no_record" })) : items,
  };
}
