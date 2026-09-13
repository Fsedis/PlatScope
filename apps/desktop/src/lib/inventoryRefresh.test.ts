import { describe, expect, it } from "vitest";
import { inventoryScanErrorMessage } from "./inventoryRefresh";

describe("автообновление инвентаря", () => {
  it("не выводит произвольную техническую ошибку или ответ сервера", () => {
    expect(inventoryScanErrorMessage("private server response", "Безопасная ошибка", true)).toBe("Безопасная ошибка");
  });
});
