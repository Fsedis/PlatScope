import { describe, expect, it, vi } from "vitest";
import { createMasteryStore, filterMasteryItems, masteryStatusLabel, masteryTargetsForReward, type MasteryItemView, type MasteryState, type MasteryView } from "./mastery";
import { makeMasteryMock } from "./masteryMock";

const source = () => makeMasteryMock(null);
const flush = async () => { for (let count = 0; count < 8; count++) await Promise.resolve(); };

describe("освоение аккаунта", () => {
  it("не привязывает весь каталог и проданное снаряжение к торговому инвентарю", () => {
    const result = filterMasteryItems(source().items, "ранее продан", "all", "mastered");
    expect(result).toHaveLength(1);
    expect(result[0].setSlugs).toEqual([]);
  });
  it("ищет по русскому и английскому названию, словам и ё/е", () => {
    const items = source().items;
    for (const query of ["Никс ПРАЙМ", "nyx prime", "Прайм Никс"]) {
      expect(filterMasteryItems(items, query, "all", "all")[0].displayNameEn).toBe("Nyx Prime");
    }
    const item = { ...items[0], displayName: "Ружьё" };
    expect(filterMasteryItems([item], "РУЖЬЕ", "all", "all")).toEqual([item]);
  });
  it("сочетает фильтры и не переводит отсутствующую запись в неосвоенное", () => {
    const items = source().items;
    const filtered = filterMasteryItems(items, "", "primary", "unknown");
    expect(filtered.map(item => item.displayNameEn)).toEqual(["Boar Prime"]);
    expect(masteryStatusLabel(filtered[0].status)).toBe("Нет данных");
    expect(filterMasteryItems(items, "Никс", "primary", "all")).toEqual([]);
  });
  it("показывает подтверждённое историей освоение и отдельный прогресс до ранга 40", () => {
    const highXp = source().items.find(item => item.displayNameEn === "Boltor Prime")!;
    expect(highXp.xp).toBeGreaterThan(20_000_000);
    expect(filterMasteryItems([highXp], "", "all", "mastered")).toEqual([highXp]);
    expect(masteryStatusLabel(highXp.status)).toBe("Освоено");
    const coda = source().items.find(item => item.displayNameEn === "Coda Hema")!;
    expect(coda.masteryRank).toBe(30);
    expect(coda.maxRank).toBe(40);
    expect(masteryStatusLabel(coda.status)).toBe("Не до конца");
  });
  it("использует точную связь детали с готовым предметом, без догадок по имени", () => {
    const sets = [
      { definition: { setGameRef: "/frame" }, components: [{ definition: { gameRef: "/part", slug: "part" } }] },
      { definition: { setGameRef: "/frame" }, components: [{ definition: { gameRef: "/part", slug: "part" } }] },
      { definition: { setGameRef: "/weapon" }, components: [{ definition: { gameRef: "/part", slug: "part" } }] },
      { definition: { setGameRef: "/other" }, components: [{ definition: { gameRef: "/Part", slug: "other" } }] },
    ];
    expect(masteryTargetsForReward({ rewardGameRef: "/part", rewardSlug: null }, sets)).toEqual(["/frame", "/weapon"]);
    expect(masteryTargetsForReward({ rewardGameRef: "part", rewardSlug: null }, sets)).toEqual([]);
    expect(masteryTargetsForReward({ rewardGameRef: "", rewardSlug: "" }, sets)).toEqual([]);
  });
  it("связывает чертежи нейрооптики, каркаса и системы через точный канонический идентификатор", () => {
    const parts = ["neuroptics", "chassis", "systems"];
    const sets = [{ definition: { setGameRef: "/NyxPrime" }, components: parts.map(part => ({ definition: { gameRef: `/NyxPrime${part}Component`, slug: `nyx_prime_${part}` } })) }];
    for (const part of parts) {
      expect(masteryTargetsForReward({ rewardGameRef: `/NyxPrime${part}Blueprint`, rewardSlug: `nyx_prime_${part}` }, sets)).toEqual(["/NyxPrime"]);
    }
    expect(masteryTargetsForReward({ rewardGameRef: "/NyxPrimechassisBlueprint", rewardSlug: "nyx_prime" }, sets)).toEqual([]);
  });
});

describe("единая история освоения в интерфейсе", () => {
  it("загружает один снимок для многих подписчиков и снимает слушателей", async () => {
    const load = vi.fn(async () => source());
    const cleanups = [vi.fn(), vi.fn()];
    const listen = vi.fn(async (_event: string, _handler: () => void) => cleanups[listen.mock.calls.length - 1]);
    const store = createMasteryStore({ load, listen });
    let state: MasteryState | undefined;
    const stopA = store.subscribe(value => { state = value; });
    const stopB = store.subscribe(() => {});
    await flush();
    expect(load).toHaveBeenCalledTimes(1);
    expect(listen.mock.calls.map(call => call[0])).toEqual(["inventory-updated", "game-metadata-updated"]);
    expect(state?.byGameRef.get(source().items[0].gameRef)?.status).toBe("mastered");
    stopA();
    expect(cleanups[0]).not.toHaveBeenCalled();
    stopB();
    cleanups.forEach(cleanup => expect(cleanup).toHaveBeenCalledTimes(1));
  });
  it("сохраняет успешный снимок при ошибке и очищает ошибку после восстановления", async () => {
    const view = source();
    const load = vi.fn<() => Promise<MasteryView>>().mockResolvedValueOnce(view).mockRejectedValueOnce(new Error("offline")).mockResolvedValue(view);
    const store = createMasteryStore({ load, listen: async () => () => {} });
    let state: MasteryState | undefined;
    const stop = store.subscribe(value => { state = value; });
    await flush();
    await store.refresh();
    expect(state?.error).toBe(true);
    expect(state?.view).toBe(view);
    expect(state?.loading).toBe(false);
    await store.refresh();
    expect(state?.error).toBe(false);
    stop();
  });
  it("обновляет снимок после события, полученного во время предыдущего чтения", async () => {
    let resolveFirst!: (view: MasteryView) => void;
    const newer = { ...source(), items: [] as MasteryItemView[] };
    const load = vi.fn<() => Promise<MasteryView>>().mockImplementationOnce(() => new Promise(resolve => { resolveFirst = resolve; })).mockResolvedValue(newer);
    const handlers = new Map<string, () => void>();
    const store = createMasteryStore({ load, listen: async (event, handler) => { handlers.set(event, handler); return () => {}; } });
    let state: MasteryState | undefined;
    const stop = store.subscribe(value => { state = value; });
    handlers.get("inventory-updated")!();
    resolveFirst(source());
    await flush();
    expect(load).toHaveBeenCalledTimes(2);
    expect(state?.view).toBe(newer);
    expect(state?.byGameRef.size).toBe(0);
    stop();
  });
  it("не оставляет слушателей при закрытии экрана до завершения подписки", async () => {
    const cleanup = vi.fn();
    const store = createMasteryStore({ load: async () => source(), listen: async () => cleanup });
    const stop = store.subscribe(() => {});
    stop();
    await flush();
    expect(cleanup).toHaveBeenCalledTimes(2);
  });
  it("при новом снимке инвентаря очищает аккаунт A даже при ошибке чтения аккаунта B", async () => {
    const handlers = new Map<string, () => void>();
    const load = vi.fn<() => Promise<MasteryView>>().mockResolvedValueOnce(source()).mockRejectedValue(new Error("unavailable"));
    const store = createMasteryStore({ load, listen: async (event, handler) => { handlers.set(event, handler); return () => {}; } });
    let state: MasteryState | undefined;
    const stop = store.subscribe(value => { state = value; });
    await flush();
    expect(state?.byGameRef.size).toBeGreaterThan(0);
    handlers.get("inventory-updated")!();
    expect(state?.byGameRef.size).toBe(0);
    await flush();
    expect(state?.view).toBeNull();
    expect(state?.error).toBe(true);
    expect(state?.byGameRef.size).toBe(0);
    stop();
  });
  it("не восстанавливает прежний аккаунт запоздалым ответом после смены снимка", async () => {
    const handlers = new Map<string, () => void>();
    let resolveOld!: (view: MasteryView) => void;
    const load = vi.fn<() => Promise<MasteryView>>().mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve; })).mockRejectedValue(new Error("unavailable"));
    const store = createMasteryStore({ load, listen: async (event, handler) => { handlers.set(event, handler); return () => {}; } });
    const exposed: string[] = [];
    let state: MasteryState | undefined;
    const stop = store.subscribe(value => { state = value; exposed.push(...value.byGameRef.keys()); });
    handlers.get("inventory-updated")!();
    resolveOld(source());
    await flush();
    expect(exposed).toEqual([]);
    expect(state?.error).toBe(true);
    stop();
  });
  it("не показывает старый аккаунт при открытии после периода без подписчиков", async () => {
    const load = vi.fn<() => Promise<MasteryView>>().mockResolvedValueOnce(source()).mockRejectedValue(new Error("unavailable"));
    const store = createMasteryStore({ load, listen: async () => () => {} });
    const stopFirst = store.subscribe(() => {});
    await flush();
    stopFirst();
    const exposed: string[] = [];
    const stopSecond = store.subscribe(value => { exposed.push(...value.byGameRef.keys()); });
    await flush();
    expect(exposed).toEqual([]);
    stopSecond();
  });
});
