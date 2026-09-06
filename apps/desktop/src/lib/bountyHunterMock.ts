import { bountyEstimate, type BountyHunterView, type BountyJobView, type BountyRewardView } from "./bountyHunter";

// Данные для проверки интерфейса. Цены и вероятности вымышлены, не являются таблицей выпадения.
// Изображения из публичного каталога https://api.warframe.market/v2/items (06.09.2026).
const images: Record<string, string> = {
  augur_reach: "augur_reach.f7ec6b1dd7f4636d502d4a463883e027",
  gladiator_aegis: "gladiator_aegis.b090c523158aaeb9625bdca8df150372",
  boreals_hatred: "boreals_hatred.de83eb54e55037ec026c099b9ad00d89",
  vazarin_lens: "vazarin_lens.3a08aee24f8a10e3bf4a0d1e28c34528",
  synth_deconstruct: "synth_deconstruct.3491a192eb4347689d3ba93fe03d429c",
  synth_reflex: "synth_reflex.72b529bc038f95a3c119c4817fcaa7be",
  targeting_subsystem: "targeting_subsystem.bab012feb316116d1c4c66888e73a6e8",
  ayatan_amber_star: "ayatan_amber_star.9657dbb79cc82338d5b3caa62da3e934",
  weeping_wounds: "weeping_wounds.b1b82c8b21d3fb09861b8f004f4278cf",
  necramech_continuity: "necramech_continuity.9d2574a325182ce7a355a1cf652e8b22",
};
function reward(name: string, slug: string | null, chance: number, quantity: number, price: number | null): BountyRewardView {
  return {
    trackingKey: (slug ? "market:" : "worldstate:") + (slug ?? name), displayName: name,
    imageUrl: slug && images[slug] ? "https://warframe.market/static/assets/items/images/en/thumbs/" + images[slug] + ".128x128.png" : null,
    slug, marketKey: slug ? { slug, platform: "pc", rank: null, charges: null, subtype: null, amberStars: null, cyanStars: null } : null,
    ownedQuantity: name === "Айя" ? 4 : slug ? 2 : null,
    rarity: chance < 25 ? "Редкая" : "Необычная", expectedQuantity: quantity, chancePercent: chance,
    unitPrice: price, expectedPlatinum: price === null ? null : price * quantity,
  };
}

export function makeBountyHunterMock(scenario: string | null = null, now = Date.now()): BountyHunterView {
  const aya = (chance: number) => reward("Айя", null, chance, chance / 85, null);
  const endo = reward("400 эндо", null, 43.5, .52, null);
  const spec: Array<[string, string, string[], BountyRewardView[]]> = [
    ["cetus", "Цетус", ["Поимка шпиона", "Проредить ряды врага", "Верните их домой (Нармер)", "Захватить агента Гринир", "Ослабить позиции Гринир"], [
      reward("Охват Предвестника", "augur_reach", 40.8, .56, 25),
      reward("Щит Гладиатора", "gladiator_aegis", 51.4, .6, 8),
      reward("Ненависть Бореаля", "boreals_hatred", 40.5, .45, 10),
    ]],
    ["fortuna", "Фортуна", ["Проявление долга", "Коллапс сети", "Голос мастера (Нармер)", "Засада на курьера", "Награда без рыночной оценки"], [
      reward("Линза: Вазарин", "vazarin_lens", 64.4, .8, 9),
      reward("Синт-Расщепление", "synth_deconstruct", 25, .35, 17),
      reward("Синт-Рефлекс", "synth_reflex", 29.1, .3, 10),
    ]],
    ["necralisk", "Некралиск", ["Очистить землю", "Охотник за артефактами (бесконечный)", "Получение аномалии", "Изоляционные хранилища A", "Изоляционные хранилища C"], [
      reward("Подсистема Прицеливания", "targeting_subsystem", 29.1, .4, 29),
      reward("Звезда Аятан: Янтарь", "ayatan_amber_star", 89.9, 1.2, 5),
      reward("Стенающие Раны", "weeping_wounds", 22.8, .3, 29),
    ]],
  ];
  return {
    fetchedAt: new Date(now).toISOString(), marketSourceDate: "2026-09-05",
    regions: scenario === "empty" ? [] : spec.map(([key, displayName, titles, pool], regionIndex) => ({
      key, displayName,
      expiry: new Date(scenario === "expired" || scenario === "partial-expired" && regionIndex === 0 ? now - 1000 : now + (scenario === "rotation" || scenario === "refresh-error" ? 8000 : (46 + regionIndex * 11) * 60000)).toISOString(),
      jobs: titles.map((title, index) => {
        let rewards = [pool[index % pool.length]!, aya(23.4 + index * 8 + regionIndex * 4), endo];
        if (index === 3) rewards = [...rewards, reward("Непрерывность Некрамеха", "necramech_continuity", 7.5, .08, null)];
        if (index === 4 && regionIndex === 1) rewards = [reward("Синт-Рефлекс", "synth_reflex", 18.5, .2, null), aya(51)];
        if (index === 4 && regionIndex === 0) rewards = [aya(78.2), endo];
        if (scenario === "unpriced") rewards = rewards.map(item => ({ ...item, unitPrice: null, expectedPlatinum: null }));
        const job: BountyJobView = { id: "tier-" + index, title, minLevel: 15 + index * 5, maxLevel: 30 + index * 10,
          minMasteryRank: index === 3 ? 5 : 0, stageCount: index === 1 ? 3 : 5, totalStanding: 2400 + index * 350,
          timeBound: index === 3 ? "Доступен ночью" : null,
          expectedPlatinum: 0, marketRewardCount: 0, pricedRewardCount: 0, priceCoveragePercent: 0, rewards };
        const estimate = bountyEstimate(job);
        return { ...job, expectedPlatinum: estimate.value ?? 0, marketRewardCount: estimate.total, pricedRewardCount: estimate.priced, priceCoveragePercent: estimate.total ? estimate.priced / estimate.total * 100 : 0 };
      }),
    })),
  };
}
