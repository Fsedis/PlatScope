import { render } from "svelte/server";
import { describe, expect, it, vi } from "vitest";
import { writable } from "svelte/store";
import { makeWorldActivityMock } from "./worldActivityMock";
import type { WorldActivityState } from "./worldActivityStore";
import WorldActivityScreen from "./WorldActivityScreen.svelte";

vi.mock("./i18n", async importOriginal => ({ ...await importOriginal<typeof import("./i18n")>(), useLocale: () => writable("ru") }));
vi.mock("./worldActivityStore", async () => {
  const { writable } = await import("svelte/store");
  return { worldActivityStore: Object.assign(writable({ view: null, loading: false, error: false, nextRefreshAt: 0, manualRefreshAt: 0 }), { refresh: vi.fn() }),
    worldNow: writable(Date.parse("2026-09-05T09:45:00Z")), worldPreferences: writable({ startHere: false, rules: [], sent: [] }),
    retainWorldActivityScreen: () => () => undefined, saveWorldPreferences: vi.fn(), requestWorldNotificationPermission: vi.fn() };
});
import { worldActivityStore } from "./worldActivityStore";
const stateStore = worldActivityStore as unknown as ReturnType<typeof writable<WorldActivityState>>;
const now = Date.parse("2026-09-05T09:45:00Z");
function show(scenario: string | null, extra: Partial<WorldActivityState> = {}): string {
  stateStore.set({ view: makeWorldActivityMock(scenario, now), loading: false, error: false, nextRefreshAt: 0, manualRefreshAt: 0, ...extra });
  return render(WorldActivityScreen, { props: { onOpenBounties: vi.fn(), onOpenInsights: vi.fn(), onOpenSettings: vi.fn() } }).body;
}
describe("экран «Сейчас в игре»", () => {
  it("показывает решения и пять циклов, а не только сырой список данных", () => {
    const body = show(null);
    for (const label of ["Баро Ки’Тиир", "Возрождение Прайм", "Равнины Эйдолона", "Долина Сфер", "Камбионский Дрейф", "Зариман", "Дувири", "Напоминания", "Открыть мои реликвии"]) expect(body).toContain(label);
    expect(body).not.toContain("Автообновление через");
    expect(body).toContain("Королевской Ая");
  });
  it("не продолжает показывать старый ассортимент после окончания визита", () => {
    const body = show("expired");
    expect(body).not.toContain("Поток Прайм");
    expect(body).not.toContain("Банши Прайм");
    expect(body).toContain("Предыдущий визит закончился");
    expect(body).toContain("Уточняем смену…");
  });
  it("показывает будущий визит без выдуманного ассортимента", () => {
    const body = show("upcoming");
    expect(body).toContain("Прибудет");
    expect(body).toContain("Ассортимент появится после прибытия");
    expect(body).not.toContain("Товары Баро");
  });
  it("оставляет здоровые секции видимыми при частичном ответе", () => {
    const body = show("partial");
    expect(body).toContain("Часть данных задерживается");
    expect(body).toContain("Реле Страта");
    expect(body).toContain("Нет данных");
    expect(body).not.toContain("Банши Прайм");
  });
  it("отличает первую загрузку, ошибку и сохранённые данные", () => {
    expect(show(null, { view: null, loading: true })).toContain("Получаем события игры…");
    expect(show(null, { view: null, error: true })).toContain("Не удалось получить события");
    const cached = show("stale");
    expect(cached).toContain("Показываем сохранённые данные");
    expect(cached).toContain("Банши Прайм");
  });
});
