import { describe, expect, it } from "vitest";

import { describeFoundationStatus, type FoundationStatus } from "./foundation";

describe("describeFoundationStatus", () => {
  it("сообщает версию готовой локальной схемы", () => {
    const status: FoundationStatus = {
      appName: "PlatScope",
      appVersion: "0.1.0",
      databasePath: "C:/Data/PlatScope/platscope.db",
      schemaVersion: 1,
      offlineReady: true,
      marketSnapshot: null,
      catalogItemCount: null,
      historyCoverage: { oldestDate: null, newestDate: null, dayCount: 0 },
      inventoryItemCount: null,
    };

    expect(describeFoundationStatus(status)).toBe(
      "Сохранённые данные готовы · формат 1",
    );
  });

  it("не выдаёт неготовое хранилище за рабочее", () => {
    const status: FoundationStatus = {
      appName: "PlatScope",
      appVersion: "0.1.0",
      databasePath: "",
      schemaVersion: 0,
      offlineReady: false,
      marketSnapshot: null,
      catalogItemCount: null,
      historyCoverage: { oldestDate: null, newestDate: null, dayCount: 0 },
      inventoryItemCount: null,
    };

    expect(describeFoundationStatus(status)).toBe(
      "Сохранённые данные не готовы",
    );
  });
});
