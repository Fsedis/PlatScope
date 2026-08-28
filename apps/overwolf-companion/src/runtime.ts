import {
  analyzeInventoryValue,
  createCompanionEnvelope,
  extractInventoryValue,
  isAbsoluteJsonPath,
  isWarframeRunning,
  serializeEnvelope,
  SnapshotError,
  type CompanionEnvelope,
  type InventoryAnalysis,
  type SnapshotErrorCode,
} from "./model";
import type { OverwolfApi, OverwolfResult } from "./overwolf";

const SETTINGS_KEY = "platscope.overwolf-companion.settings.v1";
const MAX_GEP_ATTEMPTS = 3;

export type RuntimeStatus = "ready" | "unavailable";
export type GameStatus = "checking" | "running" | "not_running";
export type GepStatus = "idle" | "registering" | "available" | "degraded";
export type ExportStatus = "disabled" | "ready" | "writing" | "written" | "error";

export interface CompanionSettings {
  exportEnabled: boolean;
  destination: string;
}

export interface CoherentSnapshot {
  envelope: CompanionEnvelope;
  analysis: InventoryAnalysis;
}

export type NoticeCode =
  | "runtime_unavailable"
  | "game_not_running"
  | "gep_ready"
  | "gep_failed"
  | "snapshot_ready"
  | "snapshot_rejected"
  | "settings_saved"
  | "path_required"
  | "export_disabled"
  | "nothing_to_export"
  | "export_written"
  | "export_failed";

export interface CompanionNotice {
  kind: "status" | "error";
  code: NoticeCode;
  detail?: string;
  snapshotError?: SnapshotErrorCode;
}

export interface CompanionState {
  runtime: RuntimeStatus;
  game: GameStatus;
  gep: GepStatus;
  exportStatus: ExportStatus;
  busy: boolean;
  settings: CompanionSettings;
  snapshot: CoherentSnapshot | null;
  notice: CompanionNotice | null;
}

type StateListener = (state: Readonly<CompanionState>) => void;

function sanitizeDetail(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const normalized = value.replace(/[\r\n\t]+/gu, " ").trim().slice(0, 180);
  return normalized.length > 0 ? normalized : undefined;
}

function loadSettings(storage: Storage): CompanionSettings {
  const fallback: CompanionSettings = { exportEnabled: false, destination: "" };
  try {
    const raw = storage.getItem(SETTINGS_KEY);
    if (raw === null) return fallback;
    const parsed = JSON.parse(raw) as unknown;
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return fallback;
    const record = parsed as Record<string, unknown>;
    if (typeof record.exportEnabled !== "boolean" || typeof record.destination !== "string") {
      return fallback;
    }
    return {
      exportEnabled: record.exportEnabled,
      destination: record.destination.trim().slice(0, 1024),
    };
  } catch {
    return fallback;
  }
}

function wait(milliseconds: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}

export class CompanionRuntime {
  private stateValue: CompanionState;
  private readonly infoListener = (event: unknown): void => {
    void this.acceptUpdate(event);
  };
  private readonly errorListener = (event: unknown): void => this.acceptGepError(event);

  constructor(
    private readonly api: OverwolfApi | undefined,
    private readonly storage: Storage,
    private readonly listener: StateListener,
    private readonly now: () => Date = () => new Date(),
    private readonly retryDelay: (milliseconds: number) => Promise<void> = wait,
  ) {
    const settings = loadSettings(storage);
    this.stateValue = {
      runtime: api ? "ready" : "unavailable",
      game: "checking",
      gep: "idle",
      exportStatus: settings.exportEnabled ? "ready" : "disabled",
      busy: false,
      settings,
      snapshot: null,
      notice: api ? null : { kind: "error", code: "runtime_unavailable" },
    };
  }

  get state(): Readonly<CompanionState> {
    return this.stateValue;
  }

  async start(): Promise<void> {
    this.emit();
    if (!this.api) return;
    this.api.games.events.onInfoUpdates2.removeListener(this.infoListener);
    this.api.games.events.onError.removeListener(this.errorListener);
    this.api.games.events.onInfoUpdates2.addListener(this.infoListener);
    this.api.games.events.onError.addListener(this.errorListener);
    await this.refreshConnection(false);
  }

  destroy(): void {
    if (!this.api) return;
    this.api.games.events.onInfoUpdates2.removeListener(this.infoListener);
    this.api.games.events.onError.removeListener(this.errorListener);
  }

  async saveSettings(settings: CompanionSettings): Promise<boolean> {
    const normalized: CompanionSettings = {
      exportEnabled: settings.exportEnabled,
      destination: settings.destination.trim(),
    };
    if (normalized.exportEnabled && !isAbsoluteJsonPath(normalized.destination)) {
      this.patch({
        notice: { kind: "error", code: "path_required" },
        exportStatus: "error",
      });
      return false;
    }

    this.storage.setItem(SETTINGS_KEY, JSON.stringify(normalized));
    this.patch({
      settings: normalized,
      exportStatus: normalized.exportEnabled ? "ready" : "disabled",
      notice: { kind: "status", code: "settings_saved" },
    });
    if (normalized.exportEnabled && this.stateValue.snapshot) {
      await this.writeSnapshot(this.stateValue.snapshot);
    }
    return true;
  }

  async syncNow(): Promise<void> {
    if (!this.api || this.stateValue.busy) return;
    await this.refreshConnection(true);
  }

  private async refreshConnection(writeAfterRefresh: boolean): Promise<void> {
    if (!this.api) return;
    this.patch({ busy: true, game: "checking", gep: "registering", notice: null });
    try {
      const gameInfo = await new Promise<unknown>((resolve) => {
        this.api?.games.getRunningGameInfo(resolve);
      });
      if (!isWarframeRunning(gameInfo)) {
        this.patch({
          game: "not_running",
          gep: "idle",
          notice: { kind: "status", code: "game_not_running" },
        });
        return;
      }
      this.patch({ game: "running" });

      const registration = await this.registerRequiredFeatures();
      if (!registration.success) {
        this.patch({
          gep: "degraded",
          notice: {
            kind: "error",
            code: "gep_failed",
            detail: sanitizeDetail(registration.error),
          },
        });
        return;
      }
      this.patch({ gep: "available", notice: { kind: "status", code: "gep_ready" } });

      const info = await new Promise<unknown>((resolve) => this.api?.games.events.getInfo(resolve));
      const accepted = await this.acceptUpdate(info);
      if (writeAfterRefresh && !accepted && this.stateValue.snapshot) {
        await this.writeAccordingToSettings(this.stateValue.snapshot);
      }
    } finally {
      this.patch({ busy: false });
    }
  }

  private async registerRequiredFeatures(): Promise<OverwolfResult> {
    let lastResult: OverwolfResult = { success: false, error: "No registration attempt" };
    for (let attempt = 1; attempt <= MAX_GEP_ATTEMPTS; attempt += 1) {
      lastResult = await new Promise((resolve) => {
        this.api?.games.events.setRequiredFeatures(["match_info"], resolve);
      });
      if (
        lastResult.success &&
        "supportedFeatures" in lastResult &&
        Array.isArray(lastResult.supportedFeatures) &&
        lastResult.supportedFeatures.includes("match_info")
      ) {
        return lastResult;
      }
      if (lastResult.success) {
        lastResult = { success: false, error: "match_info is not supported" };
      }
      if (attempt < MAX_GEP_ATTEMPTS) await this.retryDelay(3_000);
    }
    return lastResult;
  }

  private async acceptUpdate(event: unknown): Promise<boolean> {
    const value = extractInventoryValue(event);
    if (value === null) return false;
    try {
      const analysis = analyzeInventoryValue(value);
      const snapshot: CoherentSnapshot = {
        analysis,
        envelope: createCompanionEnvelope(analysis, this.now()),
      };
      this.patch({
        gep: "available",
        snapshot,
        notice: { kind: "status", code: "snapshot_ready" },
      });
      await this.writeAccordingToSettings(snapshot);
      return true;
    } catch (error) {
      this.patch({
        gep: "degraded",
        notice: {
          kind: "error",
          code: "snapshot_rejected",
          snapshotError: error instanceof SnapshotError ? error.code : "invalid_shape",
        },
      });
      return false;
    }
  }

  private acceptGepError(event: unknown): void {
    const detail =
      typeof event === "object" && event !== null
        ? sanitizeDetail((event as Record<string, unknown>).error)
        : sanitizeDetail(event);
    this.patch({
      gep: "degraded",
      notice: { kind: "error", code: "gep_failed", detail },
    });
  }

  private async writeAccordingToSettings(snapshot: CoherentSnapshot): Promise<void> {
    if (!this.stateValue.settings.exportEnabled) {
      this.patch({ exportStatus: "disabled" });
      return;
    }
    await this.writeSnapshot(snapshot);
  }

  private async writeSnapshot(snapshot: CoherentSnapshot): Promise<void> {
    if (!this.api) return;
    const destination = this.stateValue.settings.destination;
    if (!isAbsoluteJsonPath(destination)) {
      this.patch({ exportStatus: "error", notice: { kind: "error", code: "path_required" } });
      return;
    }

    this.patch({ exportStatus: "writing" });
    const content = serializeEnvelope(snapshot.envelope);
    const result = await new Promise<OverwolfResult>((resolve) => {
      this.api?.io.writeFileContents(destination, content, "UTF8", false, resolve);
    });
    if (result.success) {
      this.patch({ exportStatus: "written", notice: { kind: "status", code: "export_written" } });
    } else {
      this.patch({
        exportStatus: "error",
        notice: {
          kind: "error",
          code: "export_failed",
          detail: sanitizeDetail(result.error),
        },
      });
    }
  }

  private patch(next: Partial<CompanionState>): void {
    this.stateValue = { ...this.stateValue, ...next };
    this.emit();
  }

  private emit(): void {
    this.listener(this.stateValue);
  }
}
