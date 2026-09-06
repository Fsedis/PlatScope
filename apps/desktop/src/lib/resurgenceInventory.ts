import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { writable } from "svelte/store";
import type { InventoryView } from "./inventory";

export interface ResurgenceInventoryState { view: InventoryView | null; loading: boolean; error: boolean }
interface Dependencies {
  load: () => Promise<InventoryView | null>;
  listen: (event: string, handler: () => void) => Promise<UnlistenFn>;
}

export function createResurgenceInventory(dependencies: Dependencies) {
  const store = writable<ResurgenceInventoryState>({ view: null, loading: true, error: false }, () => {
    let disposed = false;
    let revision = 0;
    let running = false;
    let queued = false;
    const cleanups: UnlistenFn[] = [];
    const refresh = async () => {
      if (running) { queued = true; return; }
      running = true;
      do {
        queued = false;
        const request = revision;
        store.set({ view: null, loading: true, error: false });
        try {
          const view = await dependencies.load();
          if (!disposed && request === revision) store.set({ view, loading: false, error: false });
        } catch {
          if (!disposed && request === revision) store.set({ view: null, loading: false, error: true });
        }
      } while (queued && !disposed);
      running = false;
    };
    const invalidate = () => {
      revision++;
      // Новый снимок может принадлежать другому аккаунту: старые количества убираем сразу.
      store.set({ view: null, loading: true, error: false });
      void refresh();
    };
    for (const event of ["inventory-updated", "game-metadata-updated"]) {
      void dependencies.listen(event, invalidate).then(cleanup => {
        if (disposed) cleanup(); else cleanups.push(cleanup);
      }).catch(() => { /* Фоновая проверка остаётся доступной при сбое подписки. */ });
    }
    const timer = setInterval(() => { if (typeof document === "undefined" || !document.hidden) void refresh(); }, 60_000);
    void refresh();
    return () => { disposed = true; clearInterval(timer); cleanups.forEach(cleanup => cleanup()); };
  });
  return { subscribe: store.subscribe };
}

async function loadInventory(): Promise<InventoryView | null> {
  let timer: ReturnType<typeof setTimeout>;
  try {
    return await Promise.race([invoke<InventoryView | null>("load_inventory"),
      new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("inventory deadline exceeded")), 20_000); })]);
  } finally { clearTimeout(timer!); }
}
export const resurgenceInventory = typeof window === "undefined"
  ? writable<ResurgenceInventoryState>({ view: null, loading: false, error: false })
  : createResurgenceInventory({ load: loadInventory, listen });
