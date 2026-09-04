import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("../app.css", import.meta.url), "utf8");

function luminance(token: string): number {
  const match = css.match(new RegExp(`--${token}: oklch\\(([\\d.]+) ([\\d.]+) ([\\d.]+)\\)`));
  if (!match) throw new Error(`Не найден цвет ${token}`);
  const [lightness, chroma, hue] = match.slice(1).map(Number);
  const a = chroma * Math.cos(hue * Math.PI / 180);
  const b = chroma * Math.sin(hue * Math.PI / 180);
  const l = (lightness + .3963377774 * a + .2158037573 * b) ** 3;
  const m = (lightness - .1055613458 * a - .0638541728 * b) ** 3;
  const s = (lightness - .0894841775 * a - 1.291485548 * b) ** 3;
  const rgb = [
    4.0767416621 * l - 3.3077115913 * m + .2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - .3413193965 * s,
    -.0041960863 * l - .7034186147 * m + 1.707614701 * s,
  ].map((value) => Math.max(0, Math.min(1, value)));
  return rgb[0] * .2126 + rgb[1] * .7152 + rgb[2] * .0722;
}

describe("читаемость рабочего интерфейса", () => {
  it("сохраняет контраст текста не ниже 4.5:1 на основных поверхностях", () => {
    const pairs = [
      ["text", "surface-1"], ["text-muted", "surface-2"], ["text-subtle", "app-bg"],
      ["text-subtle", "sidebar-bg"], ["surface-1", "accent"], ["surface-1", "accent-strong"],
      ["success", "success-soft"], ["danger", "danger-soft"],
    ];
    for (const [foreground, background] of pairs) {
      const a = luminance(foreground), b = luminance(background);
      expect((Math.max(a, b) + .05) / (Math.min(a, b) + .05), `${foreground} / ${background}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("не уменьшает подписи ордеров и заказов ниже 12 px при базовых 16 px", () => {
    for (const filename of ["MarketTradingShift.svelte", "BountyHunterScreen.svelte"]) {
      const source = readFileSync(new URL(filename, import.meta.url), "utf8");
      for (const match of source.matchAll(/font-size:\s*([\d.]+)rem/g)) {
        expect(Number(match[1]), `${filename}: ${match[0]}`).toBeGreaterThanOrEqual(.75);
      }
    }
  });
});
