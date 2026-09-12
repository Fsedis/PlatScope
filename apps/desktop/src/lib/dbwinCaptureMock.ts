import type { DbwinCaptureStatus, DbwinEntry } from "./dbwinCapture";

export function makeDbwinCaptureMock(scenario: string | null) {
  let sequence = 0;
  let started = 0;
  let state: DbwinCaptureStatus = {
    connected: scenario !== "unavailable", supported: true, sharedListener: scenario === "shared",
    active: false, path: null, startedAt: null, stoppedAt: null,
    messages: 0, markers: 0, bytes: 0, error: null, stopReason: null, entries: [],
  };
  const append = (kind: DbwinEntry["kind"], message: string) => {
    state.entries.push({ sequence: ++sequence, receivedAt: new Date().toISOString(), elapsedMs: Date.now() - started, kind, processId: kind === "message" ? 12345 : null, message });
    state.entries = state.entries.slice(-100);
    state.bytes += message.length * 2 + 160;
    if (kind === "message") state.messages++;
    if (kind === "marker") state.markers++;
  };
  return (command: string, args: Record<string, unknown> | undefined): DbwinCaptureStatus | null => {
    if (command === "open_dbwin_capture_folder") return null;
    if (command === "start_dbwin_capture") {
      if (scenario === "error") throw new Error("Не удалось создать журнал: нет доступа к папке.");
      started = Date.now(); sequence = 0;
      state = { ...state, active: true, path: "C:\\Users\\Demo\\AppData\\Local\\PlatScope\\diagnostics\\dbwin\\warframe-dbwin-20260912T120000.jsonl", startedAt: new Date().toISOString(), stoppedAt: null, messages: 0, markers: 0, bytes: 0, error: null, stopReason: null, entries: [] };
      append("session_start", "Начало записи сообщений Warframe (демонстрационные данные).");
      if (scenario !== "empty") {
        append("message", "ProjectionRewardChoice.lua: Got rewards");
        append("message", "Тестовое неизвестное сообщение: проверка сохранения полного текста\nВторая строка с кириллицей и длинным путём /Lotus/Types/Game/Projections/ExampleResourceForLayoutVerification");
      }
    }
    if (command === "mark_dbwin_capture") append("marker", String(args?.message ?? ""));
    if (command === "stop_dbwin_capture") {
      state.active = false; state.stoppedAt = new Date().toISOString(); state.stopReason = "Запись остановлена пользователем.";
      append("session_stop", state.stopReason);
    }
    return structuredClone(state);
  };
}
