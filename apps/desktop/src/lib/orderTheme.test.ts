import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("../app.css", import.meta.url), "utf8");
const orderCss = css.slice(css.indexOf(".wfm-order-panel"), css.indexOf(".sell-price-grid .price-grid__primary"));

describe("светлая тема формы ордера", () => {
  it("использует контрастные семантические цвета для основного текста", () => {
    expect(css).toMatch(/\.wfm-order-panel\s*\{[^}]*color:\s*var\(--text\)/s);
    expect(css).toMatch(/\.wfm-order-heading h3,[\s\S]*?\.wfm-order-confirmation h3\s*\{[^}]*color:\s*var\(--text\)/s);
    expect(css).toMatch(/\.wfm-order-visible,[\s\S]*?\.wfm-confirm-check\s*\{[^}]*color:\s*var\(--text\)/s);
  });

  it("не возвращает цвета текста от старой тёмной темы", () => {
    for (const legacyColor of ["#edf4f7", "#c8d7dd", "#9fdcc7", "#e4edf1"]) {
      expect(orderCss.toLowerCase()).not.toContain(legacyColor);
    }
  });
});
