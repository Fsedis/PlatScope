import { render } from "svelte/server";
import { describe, expect, it, vi } from "vitest";
import { writable } from "svelte/store";
import MasteryStatus from "./MasteryStatus.svelte";
import { makeMasteryMock } from "./masteryMock";

vi.mock("./i18n", async importOriginal => ({
  ...await importOriginal<typeof import("./i18n")>(),
  useLocale: () => writable("ru"),
}));

describe("разметка отметки освоения", () => {
  it("сохраняет видимые подписи во всех состояниях, венок — только при освоении", () => {
    for (const item of makeMasteryMock(null).items) {
      const { body } = render(MasteryStatus, { props: { item, historyAvailable: true } });
      expect(body).toContain(item.status === "mastered" ? "Освоено" : item.status === "progress" ? "Не освоено" : "Освоение: нет данных");
      expect(body.includes('class="mastered-icon')).toBe(item.status === "mastered");
    }
  });
  it("подписывает готовый предмет у награды, а не объявляет деталь освоенной", () => {
    const item = makeMasteryMock(null).items[0];
    const { body } = render(MasteryStatus, { props: { item, showName: true } });
    expect(body).toContain("Никс Прайм · Освоено");
    expect(body).toContain('aria-hidden="true"');
  });
  it("не оставляет карточку без информации при отсутствии данных или ошибке", () => {
    expect(render(MasteryStatus).body).toContain("Освоение: нет данных");
    expect(render(MasteryStatus, { props: { loading: true } }).body).toContain("Освоение: загружаем…");
    expect(render(MasteryStatus, { props: { error: true } }).body).toContain("Освоение недоступно");
  });
});
