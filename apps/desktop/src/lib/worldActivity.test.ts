import { describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import { alertStates, countdown, markWorldAlertsDelivered, nextReset, nextState, nextWorldRefresh, offerCost,
  parseWorldPreferences, periodState, worldAlertCandidates, type WorldAlertRule, type WorldPreferences } from "./worldActivity";
import { makeWorldActivityMock } from "./worldActivityMock";
import { createWorldActivityStore } from "./worldActivityStore";

const now = Date.parse("2026-09-05T09:45:00Z");
const fixture = () => makeWorldActivityMock(null, now);
const rule = (extra: Partial<WorldAlertRule> = {}): WorldAlertRule => ({ id: "night", key: "cetus", state: "night", leadMinutes: 0,
  repeat: true, enabled: true, createdAt: now - 600_000, ...extra });
const preferences = (rules = [rule()]): WorldPreferences => ({ startHere: false, rules, sent: [] });

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
    expect(parseWorldPreferences(null)).toEqual({ startHere: false, rules: [], sent: [] });
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
    time += 46_000; await store.refresh(true); expect(get(store).error).toBe(false);
  });
});
