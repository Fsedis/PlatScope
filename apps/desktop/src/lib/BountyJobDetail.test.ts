import { render } from "svelte/server";
import { describe, expect, it, vi } from "vitest";
import BountyJobDetail from "./BountyJobDetail.svelte";
import { makeBountyHunterMock } from "./bountyHunterMock";

function show(regionIndex = 0, jobIndex = 0, query = "", scenario: string | null = null): string {
  const region = makeBountyHunterMock(scenario).regions[regionIndex]!;
  return render(BountyJobDetail, { props: {
    row: { regionKey: region.key, regionName: region.displayName, expiry: region.expiry, job: region.jobs[jobIndex]! },
    query, watchedKeys: new Set(["worldstate:Айя"]), remaining: "45:00",
    onCheck: vi.fn(), onMarket: vi.fn(), onWatch: vi.fn(), onFind: vi.fn(), onBack: vi.fn(),
  } }).body;
}

describe("понятные условия и оценки заказа", () => {
  it("отличает неизвестные цены от непродаваемых наград и показывает полноту оценки", () => {
    expect(show(0, 3)).toContain("Неполная оценка: учтено цен 1 из 2");
    expect(show(0, 3)).toContain("Цена неизвестна");
    expect(show(0, 4)).toContain("Не для продажи");
    expect(show(0, 4)).not.toContain("Проверить цены на рынке");
    expect(show(1, 4)).toContain("Нет оценки");
    expect(show(1, 4)).toContain("Проверить цены на рынке");
  });
  it("показывает условия получения и различает цикл бесконечного заказа", () => {
    expect(show(0, 3)).toContain("Нужен ранг мастерства 5");
    expect(show(0, 3)).toContain("Доступен ночью");
    expect(show(2, 1)).toContain("за цикл из 3 этапов");
    expect(show(2, 1)).toContain("в Некралиске");
  });
  it("выделяет именно искомую награду и сохраняет её отметку", () => {
    const body = show(0, 4, "Айя");
    expect(body).toContain("Искомая награда и другая добыча");
    expect(body).toContain("Не отслеживать: Айя");
    expect(body).toContain("78,2");
    expect(show(0, 0, "Поимка")).not.toContain("Искомая награда и другая добыча");
  });
});
