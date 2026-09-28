import { describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import { alertStates, countdown, markWorldAlertsDelivered, nextReset, nextState, nextWorldRefresh, offerCost,
  parseWorldPreferences, periodState, worldAlertCandidates, DEFAULT_FISSURE_RULE, type WorldAlertRule, type WorldFissure, type WorldPreferences } from "./worldActivity";
import { makeWorldActivityMock } from "./worldActivityMock";
import { createWorldActivityStore } from "./worldActivityStore";
import { selectFissureMatches } from "./worldFissures";
import type { InsightsView, RelicInsightRow } from "./insights";
import type { PersonalGoalsView } from "./personalGoals";
import type { PriceRecommendation } from "./market";

const now = Date.parse("2026-09-05T09:45:00Z");
const fixture = () => makeWorldActivityMock(null, now);
const rule = (extra: Partial<WorldAlertRule> = {}): WorldAlertRule => ({ id: "night", key: "cetus", state: "night", leadMinutes: 0,
  repeat: true, enabled: true, createdAt: now - 600_000, ...extra });
const preferences = (rules = [rule()]): WorldPreferences => ({ startHere: false, rules, sent: [], fissureRules: [{ ...DEFAULT_FISSURE_RULE }] });

describe("события и расписание игры", () => {
  it("считает таймер по абсолютному времени, не показывает отрицательные часы", () => {
    expect(countdown(now + 65_000, now)).toBe("1:05");
    expect(countdown(now + 3_600_000, now)).toBe("1 ч 00 мин");
    expect(countdown(now, now)).toBe("Уточняем…");
    expect(countdown("bad", now)).toBe("—");
    expect(countdown(null, now)).toBe("—");
  });
  it("не называет истёкший визит активным и не продлевает цикл арифметически", () => {
    expect(periodState(fixture().baro, now)).toBe("active");
    expect(periodState(makeWorldActivityMock("upcoming", now).baro, now)).toBe("upcoming");
    expect(periodState(makeWorldActivityMock("expired", now).baro, now)).toBe("expired");
    expect(periodState({ activation: "bad", expiry: "bad" }, now)).toBe("missing");
  });
  it("правильно определяет следующий этап всех пяти циклов", () => {
    expect(fixture().cycles.map(nextState)).toEqual(["night", "warm", "vome", "corpus", "joy"]);
    expect(nextState({ ...fixture().cycles[4], state: "envy" })).toBe("sorrow");
    expect(nextState({ ...fixture().cycles[0], state: "unknown" })).toBeNull();
  });
  it("не связывает смену ежедневных и недельных лимитов с часовым поясом Windows", () => {
    expect(new Date(nextReset(now)).toISOString()).toBe("2026-09-06T00:00:00.000Z");
    expect(new Date(nextReset(now, true)).toISOString()).toBe("2026-09-07T00:00:00.000Z");
    expect(new Date(nextReset(Date.parse("2026-09-07T00:00:00Z"), true)).toISOString()).toBe("2026-09-14T00:00:00.000Z");
  });
  it("ждёт ближайшую смену и не опрашивает источник каждую секунду", () => {
    expect(nextWorldRefresh(fixture(), now)).toBe(now + 8 * 60_000 + 3_000);
    expect(nextWorldRefresh(makeWorldActivityMock("expired", now), now)).toBe(now + 45_000);
    expect(nextWorldRefresh(makeWorldActivityMock("stale", now), now)).toBe(now + 45_000);
    expect(nextWorldRefresh(makeWorldActivityMock("catalog", now), now)).toBe(now + 15_000);
  });
  it("различает Ая, Королевскую Ая, дукаты и кредиты; null не превращается в ноль", () => {
    const item = fixture().resurgenceOffers[0];
    expect(offerCost(item, true)).toBe("3 Королевской Ая");
    expect(offerCost({ ...item, ducats: null, credits: 1 }, true)).toBe("1 Ая");
    expect(offerCost({ ...item, ducats: 350, credits: 110000 }, false)).toBe("350 дукатов + 110 000 кредитов");
    expect(offerCost({ ...item, ducats: null, credits: null }, false)).toBe("Цена не указана");
  });
});

describe("напоминания без повторов и лишних уведомлений", () => {
  it("все выключены по умолчанию; повреждённые настройки не мешают запуску", () => {
    expect(parseWorldPreferences(null)).toEqual({ startHere: false, rules: [], sent: [], fissureRules: [DEFAULT_FISSURE_RULE] });
    expect(parseWorldPreferences("{").rules).toEqual([]);
    expect(alertStates("baro")).toEqual(["arrival", "departure"]);
  });
  it("проверяет типы, события и дубликаты сохранённых правил", () => {
    const saved = parseWorldPreferences(JSON.stringify({ startHere: true,
      rules: [rule(), rule({ id: "same-target" }), rule({ id: "bad", state: "space" }), { key: "__proto__" }] }));
    expect(saved.startHere).toBe(true);
    expect(saved.rules).toEqual([rule()]);
  });
  it("не уведомляет о событии, которое было активно до добавления правила", () => {
    const view = fixture(); view.cycles[0].state = "night";
    expect(worldAlertCandidates(view, preferences(), now, now - 1000)).toEqual([]);
  });
  it("уведомляет об актуальной подтверждённой фазе только один раз", () => {
    const view = fixture(); view.cycles[0].state = "night"; view.cycles[0].activation = new Date(now - 10_000).toISOString();
    const first = worldAlertCandidates(view, preferences(), now, now - 1000);
    expect(first).toHaveLength(1); expect(first[0].body).toContain("ночь");
    expect(worldAlertCandidates(view, { ...preferences(), sent: [first[0].id] }, now + 1000, now)).toEqual([]);
    expect(worldAlertCandidates(view, preferences([rule({ enabled: false })]), now, now - 1000)).toEqual([]);
  });
  it("за пять минут напоминает о следующей фазе, а не о текущей", () => {
    const view = fixture(); view.cycles[0].expiry = new Date(now + 5 * 60_000).toISOString();
    const first = worldAlertCandidates(view, preferences([rule({ leadMinutes: 5 })]), now, now - 1000);
    expect(first).toHaveLength(1); expect(first[0].body).toBe("Через 5 минут: ночь.");
    expect(worldAlertCandidates(view, preferences([rule({ state: "day", leadMinutes: 5 })]), now, now - 1000)).toEqual([]);
    expect(worldAlertCandidates(view, preferences([rule({ leadMinutes: 5 })]), now - 1000, now - 2000)).toEqual([]);
  });
  it("при сбое источника не рассылает прогноз как свершившееся событие", () => {
    const view = fixture(); view.cycles[0].state = "night"; view.cycles[0].activation = new Date(now - 5000).toISOString();
    view.refreshFailed = true;
    expect(worldAlertCandidates(view, preferences(), now, now - 1000)).toEqual([]);
    view.refreshFailed = false; view.unavailableSections.push("cetus");
    expect(worldAlertCandidates(view, preferences(), now, now - 1000)).toEqual([]);
  });
  it("не догоняет пропущенные события после сна и перевода часов назад", () => {
    const view = fixture(); view.cycles[0].state = "night"; view.cycles[0].activation = new Date(now - 5000).toISOString();
    expect(worldAlertCandidates(view, preferences(), now, now - 600_000)).toEqual([]);
    expect(worldAlertCandidates(view, preferences(), now, now + 1000)).toEqual([]);
    expect(worldAlertCandidates(view, preferences(), now + 1000, now, now)).toEqual([]);
  });
  it("ежедневный сброс может напоминать без публичного источника", () => {
    const midnight = Date.parse("2026-09-06T00:00:00Z");
    expect(worldAlertCandidates(null, preferences([rule({ key: "daily", state: "any" })]), midnight, midnight - 1000)).toHaveLength(1);
    expect(worldAlertCandidates(null, preferences([rule({ key: "daily", state: "any" })]), midnight + 30_000, midnight - 1000)).toHaveLength(1);
  });
  it("одноразовое правило выключается, повторяемое остаётся на следующий цикл", () => {
    const configured = preferences([rule({ id: "once", repeat: false }), rule({ id: "repeat", state: "day" })]);
    const saved = markWorldAlertsDelivered(configured, [
      { id: "event-one", ruleId: "once", title: "", body: "" }, { id: "event-repeat", ruleId: "repeat", title: "", body: "" },
    ]);
    expect(saved.rules.map(rule => rule.enabled)).toEqual([false, true]);
    expect(saved.sent).toEqual(["event-one", "event-repeat"]);
    expect(configured.rules.every(rule => rule.enabled)).toBe(true);
  });
});

describe("общий запрос экрана и уведомлений", () => {
  it("пересчитывает локальные названия после загрузки справочника без принудительного запроса в сеть", async () => {
    let time = now;
    const load = vi.fn().mockResolvedValue(fixture());
    const store = createWorldActivityStore(load, () => time);
    await store.refresh();
    store.invalidate();
    expect(get(store).nextRefreshAt).toBe(0);
    await store.refresh(true);
    expect(load).toHaveBeenCalledTimes(1);
    time += 15_000;
    await store.refresh(true);
    expect(load).toHaveBeenLastCalledWith(false);
    expect(get(store).nextRefreshAt).toBeGreaterThan(time);
  });
  it("не теряет обновление справочника, пришедшее во время запроса", async () => {
    let finish!: (view: ReturnType<typeof fixture>) => void;
    let time = now;
    const load = vi.fn(() => new Promise<ReturnType<typeof fixture>>(resolve => finish = resolve));
    const store = createWorldActivityStore(load, () => time);
    const first = store.refresh();
    store.invalidate(); finish(fixture()); await first;
    expect(get(store).nextRefreshAt).toBe(0);
    time += 15_000;
    const retry = store.refresh(true);
    expect(load).toHaveBeenLastCalledWith(false);
    finish(fixture()); await retry;
    expect(get(store).nextRefreshAt).toBeGreaterThan(time);
  });
  it("объединяет одновременные загрузки и ограничивает ручные повторы", async () => {
    let finish!: (view: ReturnType<typeof fixture>) => void;
    const load = vi.fn(() => new Promise<ReturnType<typeof fixture>>(resolve => finish = resolve));
    const store = createWorldActivityStore(load, () => now);
    const first = store.refresh(); const second = store.refresh(true);
    expect(load).toHaveBeenCalledTimes(1); expect(get(store).loading).toBe(true);
    finish(fixture()); await Promise.all([first, second]);
    await store.refresh(true); expect(load).toHaveBeenCalledTimes(1);
    expect(get(store).loading).toBe(false); expect(get(store).view).not.toBeNull();
  });
  it("оставляет последний ответ видимым при ошибке и даёт повторить после паузы", async () => {
    let time = now;
    const load = vi.fn().mockResolvedValueOnce(fixture()).mockRejectedValueOnce(new Error("offline")).mockResolvedValue(fixture());
    const store = createWorldActivityStore(load, () => time);
    await store.refresh(); time += 60_000; await store.refresh(true);
    expect(get(store).view).not.toBeNull(); expect(get(store).error).toBe(true); expect(get(store).loading).toBe(false);
    await store.refresh(true); expect(load).toHaveBeenCalledTimes(2);
    time += 16_000;
    expect(get(store).nextRefreshAt).toBeGreaterThan(time);
    await store.refresh(true); expect(get(store).error).toBe(false);
  });
});

function fissure(id: string, tier: string, missionTypeKey: string, mode: "normal" | "steel" | "storm" = "normal"): WorldFissure {
  return { id, tier, missionType: "Локализованный тип", missionTypeKey, node: "Тестовый узел", tierNum: 4,
    isHard: mode === "steel", isStorm: mode === "storm",
    activation: new Date(now - 60_000).toISOString(), expiry: new Date(now + 30 * 60_000).toISOString() };
}

function reliablePrice(slug: string, fairPrice: number): PriceRecommendation {
  return { key: { slug, platform: "pc", rank: null, charges: null, subtype: null, amberStars: null, cyanStars: null },
    provider: "relics_run", sourceDate: "2026-09-05", fairPrice, listPrice: fairPrice, quickSell: null,
    lowestAsk: fairPrice, depthThree: fairPrice, depthPrice: fairPrice, closedVolume: 20,
    liveSellOrderCount: 5, liveBuyOrderCount: 0, confidence: "high", freshness: "fresh", reasons: [] };
}

function ownedRelic(slug: string, rareReward: string, rarePrice: number): RelicInsightRow {
  const chances = [25.33, 25.33, 25.33, 11, 11, 2];
  const rewards = chances.map((chancePercent, index) => {
    const rewardSlug = index === 5 ? rareReward : `${slug}_reward_${index}`;
    const definition = { rewardSlug, rewardGameRef: `/test/${rewardSlug}`, displayNameEn: rewardSlug, chancePercent };
    return { definition, displayName: rewardSlug, recommendation: reliablePrice(rewardSlug, index === 5 ? rarePrice : 1) };
  });
  return { definition: { relicSlug: slug, relicGameRef: `/test/${slug}`, displayNameEn: slug,
      refinement: "intact", vaultStatus: "available", rewards: rewards.map(reward => reward.definition) },
    displayName: slug, ownedQuantity: 2, sellableQuantity: 2, relicRecommendation: reliablePrice(slug, 2),
    expectedValue: { pricedExpectedValue: null, pricedChancePercent: 100, totalChancePercent: 100,
      missingRewardCount: 0, coverage: "complete", reasons: [] }, rewards };
}

function insightView(relics: RelicInsightRow[], voidTraces = 0): InsightsView {
  return { metadata: { source: "wfcd_warframe_items", fetchedAt: new Date(now).toISOString(), schemaVersion: 1,
      setCount: 0, relicCount: relics.length, primePartCount: 0, rivenDispositionCount: 0,
      itemDefinitionCount: 0, checksumSha256: "test" },
    inventoryAvailable: true, sets: [], relics, ducats: [], voidTraces };
}

function personalView(relic: RelicInsightRow): PersonalGoalsView {
  return { inventoryAvailable: true, metadataAvailable: true, observedAt: new Date(now).toISOString(), catalog: [],
    goals: [{ setSlug: "goal", displayName: "Моя цель", displayNameEn: "My goal", imageUrl: null,
      completedAt: null, completionPending: false,
      parts: [{ slug: "blade", displayName: "Лезвие", displayNameEn: "Blade", imageUrl: null,
        requiredQuantity: 1, allocatedQuantity: 0 }] }],
    relics: [{ definition: relic.definition, displayName: relic.displayName, ownedQuantity: relic.ownedQuantity }] };
}

describe("мои реликвии и активные разломы", () => {
  it("применяет правила эра × миссия × режим и допускает имеющуюся Акси в Омнию", () => {
    const world = fixture();
    world.fissures = [fissure("defense", "Axi", "Defense", "steel"),
      fissure("capture", "Axi", "Capture", "steel"), fissure("lith", "Lith", "Defense"),
      fissure("omnia", "Omnia", "Survival", "storm"), fissure("requiem", "Requiem", "Defense")];
    const insights = insightView([ownedRelic("axi_goal_relic", "blade", 100), ownedRelic("requiem_i_relic", "requiem_reward", 5)]);
    const rules = [{ id: "axi-defense", tier: "Axi", missionType: "Defense", mode: "steel" as const },
      { id: "omnia-storm", tier: "Omnia", missionType: "any", mode: "storm" as const },
      { id: "requiem", tier: "Requiem", missionType: "Defense", mode: "normal" as const }];
    expect(selectFissureMatches(world, insights, null, rules, now).map(match => match.fissure.id)).toEqual(["defense", "omnia", "requiem"]);
    expect(selectFissureMatches(world, insights, null, rules, now).find(match => match.fissure.id === "omnia")?.relic.relicSlug).toBe("axi_goal_relic");
    world.fissures[3].isHard = true;
    expect(selectFissureMatches(world, insights, null, [{ id: "steel-storm", tier: "Omnia", missionType: "any", mode: "steel" }], now)
      .map(match => match.fissure.id)).toEqual(["omnia"]);
    expect(selectFissureMatches(world, insights, null, [], now)).toEqual([]);
  });

  it("ставит недостающую деталь цели выше платины и учитывает улучшение реликвии", () => {
    const world = fixture(); world.fissures = [fissure("axi", "Axi", "Defense")];
    const goalRelic = ownedRelic("axi_goal_relic", "blade", 100);
    const cashRelic = ownedRelic("axi_cash_relic", "other", 1000);
    const insights = insightView([goalRelic, cashRelic]);
    const rules = [DEFAULT_FISSURE_RULE];
    const goals = personalView(goalRelic);
    expect(selectFissureMatches(world, insights, goals, rules, now)[0]).toMatchObject({
      relic: { relicSlug: "axi_goal_relic" }, goalChancePercent: 2, goalNames: ["Моя цель"] });
    expect(selectFissureMatches(world, insights, null, rules, now)[0].relic.relicSlug).toBe("axi_cash_relic");
    const refined = selectFissureMatches(world, insightView([goalRelic], 100), goals, rules, now)[0];
    expect(refined.relic.recommendedRefinement).toBe("radiant");
    expect(refined.goalChancePercent).toBe(10);
  });

  it("скрывает истёкшие разломы, недоступный источник и реликвии без копий", () => {
    const world = fixture(); world.fissures = [fissure("axi", "Axi", "Defense")];
    const insights = insightView([ownedRelic("axi_goal_relic", "blade", 100)]);
    const rules = [DEFAULT_FISSURE_RULE];
    world.fissures[0].expiry = new Date(now - 1).toISOString();
    expect(selectFissureMatches(world, insights, null, rules, now)).toEqual([]);
    world.fissures[0].expiry = new Date(now + 60_000).toISOString();
    world.unavailableSections.push("fissures");
    expect(selectFissureMatches(world, insights, null, rules, now)).toEqual([]);
    world.unavailableSections = [];
    insights.relics[0].ownedQuantity = 0;
    expect(selectFissureMatches(world, insights, null, rules, now)).toEqual([]);
  });
});
