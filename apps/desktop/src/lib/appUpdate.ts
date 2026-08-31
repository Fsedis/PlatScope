import { getVersion } from "@tauri-apps/api/app";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { get, writable } from "svelte/store";

export type AppUpdateStatus =
  | "idle"
  | "checking"
  | "current"
  | "available"
  | "downloading"
  | "installing"
  | "installed"
  | "error";

export interface AppUpdateState {
  status: AppUpdateStatus;
  currentVersion: string;
  availableVersion: string;
  releaseNotes: string;
  downloadedBytes: number;
  totalBytes: number | null;
  progressPercent: number | null;
  errorMessage: string;
  bannerDismissed: boolean;
}

const initialState: AppUpdateState = {
  status: "idle",
  currentVersion: "",
  availableVersion: "",
  releaseNotes: "",
  downloadedBytes: 0,
  totalBytes: null,
  progressPercent: null,
  errorMessage: "",
  bannerDismissed: false,
};

export const appUpdateState = writable<AppUpdateState>(initialState);

let pendingUpdate: Update | null = null;
let activeCheck: Promise<void> | null = null;

function isDesktopRuntime(): boolean {
  return Boolean(
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__,
  );
}

export function updateProgressPercent(
  downloadedBytes: number,
  totalBytes: number | null,
): number | null {
  if (!totalBytes || totalBytes <= 0) return null;
  return Math.min(100, Math.max(0, Math.round((downloadedBytes / totalBytes) * 100)));
}

async function loadCurrentVersion(): Promise<string> {
  const known = get(appUpdateState).currentVersion;
  if (known) return known;
  const currentVersion = await getVersion();
  appUpdateState.update((state) => ({ ...state, currentVersion }));
  return currentVersion;
}

export async function checkForAppUpdate(manual = false): Promise<void> {
  if (activeCheck) return activeCheck;
  if (!isDesktopRuntime()) {
    if (manual) {
      appUpdateState.update((state) => ({
        ...state,
        status: "error",
        errorMessage: "Проверка обновлений доступна в установленном приложении.",
      }));
    }
    return;
  }
  if (["downloading", "installing"].includes(get(appUpdateState).status)) return;

  activeCheck = (async () => {
    appUpdateState.update((state) => ({
      ...state,
      status: "checking",
      errorMessage: "",
    }));
    try {
      await loadCurrentVersion();
      const update = await check({
        timeout: 20_000,
        headers: {
          Accept: "application/json",
          "Cache-Control": "no-cache, no-store",
          Pragma: "no-cache",
        },
      });
      if (!update) {
        pendingUpdate = null;
        appUpdateState.update((state) => ({
          ...state,
          status: "current",
          availableVersion: "",
          releaseNotes: "",
          bannerDismissed: false,
        }));
        return;
      }

      pendingUpdate = update;
      appUpdateState.update((state) => ({
        ...state,
        status: "available",
        availableVersion: update.version,
        releaseNotes: update.body?.trim() ?? "",
        downloadedBytes: 0,
        totalBytes: null,
        progressPercent: null,
        errorMessage: "",
        bannerDismissed: false,
      }));
    } catch (error) {
      appUpdateState.update((state) => ({
        ...state,
        status: "error",
        errorMessage: updateCheckErrorMessage(error),
      }));
    } finally {
      activeCheck = null;
    }
  })();

  return activeCheck;
}

export function updateCheckErrorMessage(error: unknown): string {
  const detail = String(error).toLocaleLowerCase("ru");
  if (detail.includes("timed out") || detail.includes("timeout")) {
    return "GitHub не ответил вовремя. Повторите проверку через несколько секунд.";
  }
  if (
    detail.includes("network") ||
    detail.includes("connection") ||
    detail.includes("dns") ||
    detail.includes("fetch") ||
    detail.includes("http")
  ) {
    return "Не удалось связаться с GitHub. Проверьте подключение и повторите проверку.";
  }
  return "Ответ GitHub не удалось проверить. Повторите попытку; установленная версия продолжит работать.";
}

function applyDownloadEvent(event: DownloadEvent): void {
  if (event.event === "Started") {
    const totalBytes = event.data.contentLength ?? null;
    appUpdateState.update((state) => ({
      ...state,
      status: "downloading",
      downloadedBytes: 0,
      totalBytes,
      progressPercent: updateProgressPercent(0, totalBytes),
    }));
    return;
  }
  if (event.event === "Progress") {
    appUpdateState.update((state) => {
      const downloadedBytes = state.downloadedBytes + event.data.chunkLength;
      return {
        ...state,
        downloadedBytes,
        progressPercent: updateProgressPercent(downloadedBytes, state.totalBytes),
      };
    });
    return;
  }
  appUpdateState.update((state) => ({
    ...state,
    status: "installing",
    progressPercent: state.totalBytes ? 100 : null,
  }));
}

export async function installAppUpdate(): Promise<void> {
  if (!pendingUpdate) {
    await checkForAppUpdate(true);
    if (!pendingUpdate) return;
  }
  if (["downloading", "installing"].includes(get(appUpdateState).status)) return;

  appUpdateState.update((state) => ({
    ...state,
    status: "downloading",
    downloadedBytes: 0,
    totalBytes: null,
    progressPercent: null,
    errorMessage: "",
    bannerDismissed: false,
  }));
  try {
    await pendingUpdate.downloadAndInstall(applyDownloadEvent, { timeout: 120_000 });
    appUpdateState.update((state) => ({ ...state, status: "installed" }));
  } catch {
    appUpdateState.update((state) => ({
      ...state,
      status: "error",
      errorMessage: "Не удалось установить обновление. Повторите попытку или скачайте установщик с GitHub.",
    }));
  }
}

export function dismissUpdateBanner(): void {
  appUpdateState.update((state) => ({ ...state, bannerDismissed: true }));
}

export function startAutomaticUpdateChecks(): () => void {
  if (!isDesktopRuntime()) return () => undefined;
  const firstCheck = window.setTimeout(() => void checkForAppUpdate(false), 4_000);
  const repeatedCheck = window.setInterval(
    () => void checkForAppUpdate(false),
    6 * 60 * 60 * 1_000,
  );
  return () => {
    window.clearTimeout(firstCheck);
    window.clearInterval(repeatedCheck);
  };
}
