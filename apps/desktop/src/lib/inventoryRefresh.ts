export interface InventoryRefreshStatus {
  enabled: boolean;
  running: "inventory" | "nightwave" | null;
  waitingForGame: boolean;
  inventoryError: boolean;
  nightwaveError: boolean;
}

export function inventoryRefreshHint(status: InventoryRefreshStatus, ru: boolean): string {
  if (status.running === "inventory") return ru ? "Обновляем инвентарь…" : "Updating inventory…";
  if (status.running === "nightwave") return ru ? "Обновляем магазин Норы…" : "Updating Nora’s shop…";
  if (!status.enabled) return ru ? "Только по кнопке" : "Manual updates only";
  if (status.waitingForGame) return ru ? "Продолжим, когда запустите Warframe" : "Resumes when Warframe starts";
  if (status.inventoryError) return ru ? "Не удалось обновить. Сохранён прошлый инвентарь" : "Update failed. Previous inventory kept";
  if (status.nightwaveError) return ru ? "Магазин Норы пока недоступен. Повторим позже" : "Nora’s shop is unavailable. Will retry later";
  return ru ? "После миссий и переходов между локациями" : "After missions and location changes";
}

export function inventoryScanErrorMessage(error: unknown, fallback: string, ru: boolean): string {
  switch (error) {
    case "inventory_scan_busy": return ru ? "Чтение игры уже выполняется. Дождитесь завершения." : "Game data is already being read. Please wait.";
    case "inventory_game_not_running": return ru ? "Запустите Warframe и войдите в игру." : "Start Warframe and log in.";
    case "inventory_game_loading": return ru ? "Дождитесь загрузки локации и повторите обновление." : "Wait for the location to load, then try again.";
    case "inventory_session_changed": return ru ? "Во время чтения изменилась игровая сессия. Повторите обновление после загрузки." : "The game session changed during the scan. Retry after loading.";
    default: return fallback;
  }
}
