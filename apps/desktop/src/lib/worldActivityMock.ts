import { CYCLES, type ActivityOffer, type WorldActivityView } from "./worldActivity";
import realResurgenceOffers from "../../../../fixtures/world-activity/resurgence-offers.json";
import directSourceViews from "../../../../fixtures/worldstate/views-2026-09-07.json";

/** Только демонстрационные данные фонового браузера, не настройки игрока. */
export function makeWorldActivityMock(scenario: string | null, now = Date.now()): WorldActivityView {
  // Зафиксированный результат настоящего резервного загрузчика, без данных аккаунта.
  if (scenario === "direct") return { fissures: [], ...structuredClone(directSourceViews.world) } as WorldActivityView;
  const date = (minutes: number) => new Date(now + minutes * 60_000).toISOString();
  const gear = (name: string, nameEn: string, ref: string, cost: number): ActivityOffer => ({
    gameRef: ref, displayName: name, displayNameEn: nameEn, kind: "equipment",
    ducats: cost, credits: null, masteryRef: ref, setSlug: null, relicSlug: null,
    equipmentCategory: /Banshee|Mirage/.test(ref) ? "warframe" : "secondary", imageUrl: null, rewards: [],
  });
  const offers: ActivityOffer[] = [
    gear("Банши Прайм", "Banshee Prime", "/Lotus/Demo/World/Banshee", 3),
    gear("Мираж Прайм", "Mirage Prime", "/Lotus/Demo/World/Mirage", 3),
    gear("Акболто Прайм", "Akbolto Prime", "/Lotus/Demo/World/Akbolto", 2),
    gear("Гелиос Прайм", "Helios Prime", "/Lotus/Demo/World/Helios", 2),
    gear("Когаке Прайм", "Kogake Prime", "/Lotus/Demo/World/Kogake", 2),
    gear("Юфона Прайм", "Euphona Prime", "/Lotus/Demo/World/Euphona", 2),
    ...["Лит K5", "Лит M7", "Мезо E5", "Нео B6", "Акси H5", "Акси A12"].map((name, index): ActivityOffer => ({
      gameRef: `/Lotus/Demo/Relic/${index}`, displayName: `Реликвия ${name}`, displayNameEn: `Relic ${index}`,
      kind: "relic", ducats: null, credits: 1, masteryRef: null, setSlug: null,
      relicSlug: ["lith_k5_relic", "lith_m7_relic", "meso_e5_relic", "neo_b6_relic", "axi_h5_relic", "axi_a12_relic"][index],
      equipmentCategory: null, imageUrl: null,
      rewards: [
        { gameRef: `/Lotus/Demo/Reward/${index}/frame`, displayName: `${index % 2 ? "Мираж" : "Банши"} Прайм: ${["Каркас", "Нейрооптика", "Система"][index % 3]}`,
          chancePercent: 11, equipmentRefs: [`/Lotus/Demo/World/${index % 2 ? "Mirage" : "Banshee"}`] },
        { gameRef: `/Lotus/Demo/Reward/${index}/weapon`, displayName: "Акболто Прайм: Ствол", chancePercent: 2, equipmentRefs: ["/Lotus/Demo/World/Akbolto"] },
        { gameRef: `/Lotus/Demo/Reward/${index}/other`, displayName: "Когаке Прайм: Чертёж", chancePercent: 11, equipmentRefs: ["/Lotus/Demo/World/Kogake"] },
        ...["Чертёж: Форма", "Лекс Прайм: Ствол", "Бронко Прайм: Приёмник"].map((displayName, rewardIndex) => ({
          gameRef: `/Lotus/Demo/Reward/${index}/${rewardIndex}`, displayName,
          chancePercent: rewardIndex === 2 ? 25.34 : 25.33, equipmentRefs: [],
        })),
      ],
    })),
  ];
  const view: WorldActivityView = {
    fetchedAt: date(0), sourceAt: date(0), refreshFailed: false, catalogAvailable: true,
    unavailableSections: [],
    cycles: Object.keys(CYCLES).map((key, index) => ({ key: key as keyof typeof CYCLES,
      state: ["day", "cold", "fass", "grineer", "fear"][index],
      activation: date(-20), expiry: date([37, 8, 37, 41, 72][index]) })),
    baro: { activation: date(-1500), expiry: date(1727), location: "Strata Relay (Earth)", inventoryIncomplete: false },
    baroOffers: Array.from({ length: 36 }, (_, index) => ({
      ...gear(["Поток Прайм", "Накалённый заряд Прайм", "Горгона Призма", "Мод с длинным названием для проверки переноса текста"][index % 4], "Primed Flow", `/Lotus/Demo/Baro/${index}`, 350 + index * 5),
      kind: index === 2 ? "equipment" : "other", masteryRef: index === 2 ? "/Lotus/Demo/World/Gorgon" : null, credits: 110000 + index * 5000,
    })),
    resurgence: { activation: date(-2400), expiry: date(37000), location: "Maroo's Bazaar (Mars)", inventoryIncomplete: false },
    resurgenceOffers: offers,
    steelPath: { activation: date(-6000), expiry: date(2400), reward: "Rifle Riven Mod", cost: 75 },
    sortie: { activation: date(-300), expiry: date(1100) },
    events: [{ id: "dog-days", name: "Дог Дэйз", activation: date(-600), expiry: date(4500) }],
    fissures: [
      { id: "axi-defense", activation: date(-15), expiry: date(45), node: "Ио (Юпитер)", missionType: "Defense", missionTypeKey: "Defense", tier: "Axi", tierNum: 4, isStorm: false, isHard: false },
      { id: "axi-survival", activation: date(-20), expiry: date(75), node: "Нимус (Эрида)", missionType: "Survival", missionTypeKey: "Survival", tier: "Axi", tierNum: 4, isStorm: false, isHard: true },
      { id: "meso-capture", activation: date(-10), expiry: date(55), node: "Кассини (Сатурн)", missionType: "Capture", missionTypeKey: "Capture", tier: "Meso", tierNum: 2, isStorm: false, isHard: false },
      { id: "lith-defense", activation: date(-5), expiry: date(35), node: "Литий (Земля)", missionType: "Defense", missionTypeKey: "Defense", tier: "Lith", tierNum: 1, isStorm: false, isHard: false },
      { id: "omnia-alchemy", activation: date(-7), expiry: date(50), node: "Камбия (Деймос)", missionType: "Alchemy", missionTypeKey: "Alchemy", tier: "Omnia", tierNum: 6, isStorm: false, isHard: false },
    ],
  };
  // Публичный ассортимент Варзии, обогащённый настоящим справочником в тесте ядра.
  if (scenario === "real") view.resurgenceOffers = structuredClone(realResurgenceOffers) as ActivityOffer[];
  if (scenario === "upcoming") { view.baro!.activation = date(4500); view.baro!.expiry = date(7380); view.baroOffers = []; }
  if (scenario === "expired") {
    view.cycles.forEach(cycle => { cycle.expiry = date(-1); });
    view.baro!.expiry = date(-1); view.resurgence!.expiry = date(-1); view.steelPath!.expiry = date(-1);
    view.fissures.forEach(fissure => { fissure.expiry = date(-1); });
  }
  if (scenario === "stale") { view.refreshFailed = true; view.sourceAt = date(-90); }
  if (scenario === "partial") { view.unavailableSections = ["vallis", "resurgence"]; view.cycles = view.cycles.filter(cycle => cycle.key !== "vallis"); view.resurgence = null; }
  if (scenario === "catalog") { view.catalogAvailable = false; view.resurgenceOffers = offers.filter(offer => offer.kind === "relic").map(offer => ({ ...offer, rewards: [] })); }
  if (scenario === "empty") { view.baroOffers = []; view.resurgenceOffers = []; view.events = []; }
  return view;
}
