import { describe, expect, it } from "vitest";
import { inventoryRefreshHint, inventoryScanErrorMessage, type InventoryRefreshStatus } from "./inventoryRefresh";

const idle: InventoryRefreshStatus = { enabled: true, running: null, waitingForGame: false, inventoryError: false, nightwaveError: false };

describe("автообновление инвентаря", () => {
  it("разделяет обновление инвентаря и магазина", () => {
    expect(inventoryRefreshHint({ ...idle, running: "inventory" }, true)).toContain("инвентарь");
    expect(inventoryRefreshHint({ ...idle, running: "nightwave" }, true)).toContain("Норы");
  });
  it("не выдаёт выключенную игру за ошибку", () => {
    expect(inventoryRefreshHint({ ...idle, waitingForGame: true, inventoryError: true }, true)).toContain("запустите");
  });
  it("поясняет сохранение прошлых данных при сбое", () => {
    expect(inventoryRefreshHint({ ...idle, inventoryError: true }, true)).toContain("прошлый инвентарь");
  });
  it("выключение не обещает автоматическое обновление", () => {
    expect(inventoryRefreshHint({ ...idle, enabled: false, inventoryError: true }, true)).toBe("Только по кнопке");
  });
  it("ручное обновление во время фонового объясняет занятость, а не ошибку входа", () => {
    expect(inventoryScanErrorMessage("inventory_scan_busy", "fallback", true)).toContain("уже выполняется");
    expect(inventoryScanErrorMessage("inventory_game_loading", "fallback", true)).toContain("загрузки локации");
  });
  it("не выводит произвольную техническую ошибку или ответ сервера", () => {
    expect(inventoryScanErrorMessage("private server response", "Безопасная ошибка", true)).toBe("Безопасная ошибка");
  });
});
