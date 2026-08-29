import { describe, expect, it } from "vitest";

import {
  providerCondition,
  providerDiagnosticRows,
  type DiagnosticsStatus,
  type ProviderHealth,
} from "./diagnostics";

function health(overrides: Partial<ProviderHealth> = {}): ProviderHealth {
  return {
    provider: "relics_run",
    lastAttempt: "2026-08-27T09:00:00Z",
    lastSuccess: "2026-08-27T09:00:00Z",
    lastErrorCode: null,
    lastErrorMessage: null,
    latencyMs: 42,
    consecutiveFailures: 0,
    ...overrides,
  };
}

const foundation = {
  appName: "PlatScope",
  appVersion: "0.1.0",
  databasePath: "C:\\PlatScope\\platscope.db",
  schemaVersion: 10,
  offlineReady: true,
  marketSnapshot: null,
  catalogItemCount: null,
  historyCoverage: { oldestDate: null, newestDate: null, dayCount: 0 },
  inventoryItemCount: null,
};

describe("provider diagnostics", () => {
  it("distinguishes healthy, degraded, failed and unchecked providers", () => {
    expect(providerCondition(health())).toBe("ok");
    expect(
      providerCondition(
        health({ lastErrorCode: "Timeout", lastErrorMessage: "таймаут", consecutiveFailures: 1 }),
      ),
    ).toBe("degraded");
    expect(
      providerCondition(
        health({ lastSuccess: null, lastErrorCode: "Timeout", consecutiveFailures: 2 }),
      ),
    ).toBe("error");
    expect(providerCondition(health({ lastAttempt: null, lastSuccess: null }))).toBe("unchecked");
  });

  it("always exposes the three product providers without inventing an OK state", () => {
    const status: DiagnosticsStatus = {
      generatedAt: "2026-08-27T09:00:00Z",
      foundation,
      providers: [health()],
    };

    const rows = providerDiagnosticRows(status);
    expect(rows.map((row) => row.provider)).toEqual([
      "relics_run",
      "frame_forge_mirror",
      "warframe_market",
    ]);
    expect(rows.map((row) => row.condition)).toEqual(["ok", "unchecked", "unchecked"]);
  });
});
