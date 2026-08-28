import { describe, expect, it } from "vitest";
import type { OverwolfApi, OverwolfEvent, OverwolfResult } from "./overwolf";
import { CompanionRuntime, type CompanionState } from "./runtime";

class MemoryStorage implements Storage {
  private readonly values = new Map<string, string>();
  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  removeItem(key: string): void { this.values.delete(key); }
  setItem(key: string, value: string): void { this.values.set(key, value); }
}

class TestEvent implements OverwolfEvent<unknown> {
  private readonly listeners = new Set<(event: unknown) => void>();
  addListener(listener: (event: unknown) => void): void { this.listeners.add(listener); }
  removeListener(listener: (event: unknown) => void): void { this.listeners.delete(listener); }
  emit(event: unknown): void { for (const listener of this.listeners) listener(event); }
}

function makeApi(): OverwolfApi & { writes: Array<{ path: string; content: string }>; updates: TestEvent } {
  const updates = new TestEvent();
  const writes: Array<{ path: string; content: string }> = [];
  const success = (callback: (result: OverwolfResult) => void): void => callback({ success: true });
  return {
    writes,
    updates,
    games: {
      getRunningGameInfo(callback): void { callback({ isRunning: true, id: 8954 }); },
      events: {
        setRequiredFeatures(features, callback): void {
          callback({ success: features[0] === "match_info", supportedFeatures: ["match_info"] });
        },
        getInfo(callback): void {
          callback({
            info: {
              match_info: {
                inventory: { Inventory: { MiscItems: [{ ItemType: "/Lotus/Test/Part", ItemCount: 2 }] } },
              },
            },
          });
        },
        onInfoUpdates2: updates,
        onError: new TestEvent(),
      },
    },
    io: {
      writeFileContents(path, content, _encoding, _triggerUac, callback): void {
        writes.push({ path, content });
        success(callback);
      },
    },
  };
}

describe("CompanionRuntime", () => {
  it("registers only match_info and keeps export disabled by default", async () => {
    const api = makeApi();
    const states: Readonly<CompanionState>[] = [];
    const runtime = new CompanionRuntime(api, new MemoryStorage(), (state) => states.push(state));
    await runtime.start();
    expect(runtime.state.game).toBe("running");
    expect(runtime.state.gep).toBe("available");
    expect(runtime.state.snapshot?.analysis.rowCount).toBe(1);
    expect(runtime.state.exportStatus).toBe("disabled");
    expect(api.writes).toHaveLength(0);
    expect(states.length).toBeGreaterThan(2);
  });

  it("writes an exact v1 envelope only after explicit opt-in", async () => {
    const api = makeApi();
    const runtime = new CompanionRuntime(api, new MemoryStorage(), () => undefined, () => new Date("2026-08-27T10:15:30Z"));
    await runtime.start();
    const saved = await runtime.saveSettings({
      exportEnabled: true,
      destination: "C:\\Users\\Dmitrii\\PlatScope Companion\\inventory.json",
    });
    expect(saved).toBe(true);
    expect(api.writes).toHaveLength(1);
    const envelope = JSON.parse(api.writes[0].content) as Record<string, unknown>;
    expect(api.writes[0].path).toBe("C:\\Users\\Dmitrii\\PlatScope Companion\\inventory.json");
    expect(envelope).toMatchObject({
      schemaVersion: 1,
      producer: "platscope-overwolf-companion",
      observedAt: "2026-08-27T10:15:30.000Z",
      gameId: 8954,
      feature: "match_info",
      key: "inventory",
      complete: true,
    });
  });

  it("rejects an invalid path without persisting opt-in", async () => {
    const storage = new MemoryStorage();
    const runtime = new CompanionRuntime(makeApi(), storage, () => undefined);
    await runtime.start();
    expect(await runtime.saveSettings({ exportEnabled: true, destination: "inventory.json" })).toBe(false);
    expect(runtime.state.notice?.code).toBe("path_required");
    expect(storage.length).toBe(0);
  });

  it("does not replace a coherent snapshot after a malformed event", async () => {
    const api = makeApi();
    const runtime = new CompanionRuntime(api, new MemoryStorage(), () => undefined);
    await runtime.start();
    const firstObservedAt = runtime.state.snapshot?.envelope.observedAt;
    api.updates.emit({ feature: "match_info", info: { match_info: { inventory: "{" } } });
    await Promise.resolve();
    expect(runtime.state.snapshot?.envelope.observedAt).toBe(firstObservedAt);
    expect(runtime.state.notice).toMatchObject({ code: "snapshot_rejected", snapshotError: "invalid_json" });
  });
});
