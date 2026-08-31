import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("../app.css", import.meta.url), "utf8");

describe("компоновка рынка", () => {
  it("оставляет текст поиска контрастным на светлом поле", () => {
    expect(css).toMatch(/\.search-control input\s*\{[^}]*color:\s*var\(--text\)/s);
    expect(css).toMatch(
      /\.search-control input::placeholder\s*\{[^}]*color:\s*var\(--text-subtle\)[^}]*opacity:\s*1/s,
    );
  });

  it("выравнивает фильтр цены по верхнему краю поиска", () => {
    expect(css).toMatch(/\.market-toolbar\s*\{[^}]*align-items:\s*start/s);
  });

  it("прокручивает правую карточку рынка и инвентаря независимо", () => {
    expect(css).toMatch(
      /\.market-detail,\s*\.sell-detail\s*\{[^}]*max-height:\s*calc\(100dvh - 2rem\)[^}]*overflow-y:\s*auto/s,
    );
  });

  it("возвращает карточки в общий поток в одноколоночном режиме", () => {
    expect(css).toMatch(
      /@media \(max-width: 65rem\)\s*\{[\s\S]*?\.sell-detail\s*\{[^}]*max-height:\s*none[^}]*overflow:\s*visible/s,
    );
    expect(css).toMatch(/\.market-detail\s*\{[^}]*max-height:\s*none[^}]*overflow:\s*visible/s);
  });
});
