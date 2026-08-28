import "./style.css";
import { createMockOverwolfApi } from "./mockOverwolf";
import {
  englishPlural,
  isAbsoluteJsonPath,
  russianPlural,
  type SnapshotErrorCode,
} from "./model";
import {
  CompanionRuntime,
  type CompanionNotice,
  type CompanionState,
  type ExportStatus,
  type GameStatus,
  type GepStatus,
  type RuntimeStatus,
} from "./runtime";

type Locale = "ru" | "en";

const copy = {
  ru: {
    skip: "Перейти к содержимому",
    brandNote: "Локальный снимок Warframe",
    language: "Язык",
    eyebrow: "Состояние companion",
    heading: "Снимок инвентаря",
    lede: "Companion читает только поддерживаемое событие Overwolf и по вашему согласию сохраняет JSON локально.",
    statusRegion: "Состояние источника",
    runtime: "Overwolf",
    warframe: "Warframe",
    inventoryEvent: "Inventory event",
    lastSnapshot: "Последний снимок",
    localExport: "Локальный export",
    exportHeading: "Передача в PlatScope",
    enableExport: "Сохранять coherent snapshot",
    enableExportHint: "Новые данные записываются только после строгой локальной проверки.",
    destination: "Полный путь к JSON",
    destinationHint: "Используйте абсолютный Windows-путь с расширением .json.",
    saveSettings: "Сохранить настройки",
    syncNow: "Синхронизировать сейчас",
    coherentData: "Проверенные данные",
    summaryHeading: "Сводка снимка",
    recognizedRows: "Распознанные строки",
    distinctItems: "Уникальные предметы",
    totalQuantity: "Суммарное количество",
    categories: "Категории",
    noSnapshot: "Запустите Warframe и синхронизируйте данные, чтобы увидеть сводку.",
    privacyLabel: "Privacy",
    privacyHeading: "Только локальная обработка",
    privacyLocal: "Inventory не отправляется на сервер.",
    privacyExcluded: "Username, chat, account ID, nonce и WFM credentials не читаются и не записываются.",
    privacyTradeability: "Снимок подтверждает владение, но не tradeability; PlatScope не предложит такие вещи к продаже автоматически.",
  },
  en: {
    skip: "Skip to content",
    brandNote: "Local Warframe snapshot",
    language: "Language",
    eyebrow: "Companion status",
    heading: "Inventory snapshot",
    lede: "The companion reads only the supported Overwolf event and saves JSON locally after you opt in.",
    statusRegion: "Source status",
    runtime: "Overwolf",
    warframe: "Warframe",
    inventoryEvent: "Inventory event",
    lastSnapshot: "Latest snapshot",
    localExport: "Local export",
    exportHeading: "Send to PlatScope",
    enableExport: "Save coherent snapshots",
    enableExportHint: "New data is written only after strict local validation.",
    destination: "Full JSON path",
    destinationHint: "Use an absolute Windows path ending in .json.",
    saveSettings: "Save settings",
    syncNow: "Sync now",
    coherentData: "Validated data",
    summaryHeading: "Snapshot summary",
    recognizedRows: "Recognized rows",
    distinctItems: "Distinct items",
    totalQuantity: "Total quantity",
    categories: "Categories",
    noSnapshot: "Start Warframe and sync to see the inventory summary.",
    privacyLabel: "Privacy",
    privacyHeading: "Local processing only",
    privacyLocal: "Inventory is never uploaded to a server.",
    privacyExcluded: "Username, chat, account ID, nonce, and WFM credentials are neither read nor written.",
    privacyTradeability: "The snapshot proves ownership, not tradeability; PlatScope will not automatically recommend these items for sale.",
  },
} as const;

const statusLabels: Record<Locale, {
  runtime: Record<RuntimeStatus, [string, string]>;
  game: Record<GameStatus, [string, string]>;
  gep: Record<GepStatus, [string, string]>;
  export: Record<ExportStatus, string>;
}> = {
  ru: {
    runtime: {
      ready: ["Доступен", "Overwolf API подключён"],
      unavailable: ["Недоступен", "Откройте собранный package в Overwolf"],
    },
    game: {
      checking: ["Проверка…", "Определяем состояние игры"],
      running: ["Запущен", "Game ID 8954 подтверждён"],
      not_running: ["Не запущен", "Запустите Warframe и повторите синхронизацию"],
    },
    gep: {
      idle: ["Ожидание", "Событие ещё не зарегистрировано"],
      registering: ["Подключение…", "Регистрируем match_info"],
      available: ["Доступно", "Принимается только inventory"],
      degraded: ["Недоступно", "Предыдущий coherent snapshot сохранён"],
    },
    export: { disabled: "Выключено", ready: "Готово", writing: "Запись…", written: "Записано", error: "Нужна проверка" },
  },
  en: {
    runtime: {
      ready: ["Available", "Overwolf API connected"],
      unavailable: ["Unavailable", "Open the built package in Overwolf"],
    },
    game: {
      checking: ["Checking…", "Detecting game state"],
      running: ["Running", "Game ID 8954 verified"],
      not_running: ["Not running", "Start Warframe and sync again"],
    },
    gep: {
      idle: ["Waiting", "The event is not registered yet"],
      registering: ["Connecting…", "Registering match_info"],
      available: ["Available", "Only inventory is accepted"],
      degraded: ["Unavailable", "The previous coherent snapshot is preserved"],
    },
    export: { disabled: "Off", ready: "Ready", writing: "Writing…", written: "Written", error: "Needs attention" },
  },
};

const noticeText: Record<Locale, Record<CompanionNotice["code"], string>> = {
  ru: {
    runtime_unavailable: "Overwolf runtime недоступен. Для реальных событий загрузите папку dist как unpacked extension.",
    game_not_running: "Warframe не запущен. Запустите игру и повторите синхронизацию.",
    gep_ready: "Событие match_info зарегистрировано.",
    gep_failed: "Не удалось получить inventory event. Проверьте состояние GEP и повторите попытку.",
    snapshot_ready: "Получен новый coherent snapshot.",
    snapshot_rejected: "Новый payload отклонён. Предыдущий coherent snapshot не изменён.",
    settings_saved: "Настройки сохранены локально.",
    path_required: "Укажите абсолютный Windows-путь к файлу .json.",
    export_disabled: "Локальный export выключен.",
    nothing_to_export: "Coherent snapshot ещё не получен.",
    export_written: "Coherent snapshot записан локально.",
    export_failed: "Не удалось записать файл. Проверьте путь и доступ к каталогу.",
  },
  en: {
    runtime_unavailable: "The Overwolf runtime is unavailable. Load the dist folder as an unpacked extension for real events.",
    game_not_running: "Warframe is not running. Start the game and sync again.",
    gep_ready: "The match_info event is registered.",
    gep_failed: "Unable to read the inventory event. Check GEP health and try again.",
    snapshot_ready: "A new coherent snapshot is ready.",
    snapshot_rejected: "The new payload was rejected. The previous coherent snapshot was preserved.",
    settings_saved: "Settings saved locally.",
    path_required: "Enter an absolute Windows path to a .json file.",
    export_disabled: "Local export is off.",
    nothing_to_export: "No coherent snapshot is available yet.",
    export_written: "The coherent snapshot was written locally.",
    export_failed: "Unable to write the file. Check the path and folder access.",
  },
};

const snapshotErrors: Record<Locale, Record<SnapshotErrorCode, string>> = {
  ru: {
    invalid_json: "Inventory value не является корректным JSON.", invalid_shape: "Inventory value имеет неподдерживаемую структуру.", payload_too_large: "Payload превышает лимит 8 МиБ.", nesting_too_deep: "Payload превышает лимит вложенности.", node_limit: "Payload превышает лимит JSON nodes.", row_limit: "Payload превышает лимит строк inventory.", invalid_item: "В inventory обнаружена некорректная строка предмета.", items_missing: "В payload нет распознаваемых ItemType-строк.",
  },
  en: {
    invalid_json: "The inventory value is not valid JSON.", invalid_shape: "The inventory value has an unsupported shape.", payload_too_large: "The payload exceeds the 8 MiB limit.", nesting_too_deep: "The payload exceeds the nesting limit.", node_limit: "The payload exceeds the JSON node limit.", row_limit: "The payload exceeds the inventory row limit.", invalid_item: "The inventory contains an invalid item row.", items_missing: "The payload contains no recognizable ItemType rows.",
  },
};

function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (!found) throw new Error(`Missing element: ${id}`);
  return found as T;
}

const language = element<HTMLSelectElement>("language");
const exportForm = element<HTMLFormElement>("export-form");
const exportEnabled = element<HTMLInputElement>("export-enabled");
const destination = element<HTMLInputElement>("destination");
const destinationError = element<HTMLParagraphElement>("destination-error");
const saveSettings = element<HTMLButtonElement>("save-settings");
const syncNow = element<HTMLButtonElement>("sync-now");
const liveStatus = element<HTMLParagraphElement>("live-status");
const errorStatus = element<HTMLParagraphElement>("error-status");
const exportResult = element<HTMLParagraphElement>("export-result");
const main = element<HTMLElement>("main-content");

const localeKey = "platscope.overwolf-companion.locale.v1";
const storedLocale = localStorage.getItem(localeKey);
let locale: Locale = storedLocale === "en" || storedLocale === "ru"
  ? storedLocale
  : navigator.language.toLowerCase().startsWith("ru") ? "ru" : "en";
let latestState: Readonly<CompanionState> | null = null;
let settingsSignature = "";
let formSubmitting = false;

const mockMode = new URLSearchParams(window.location.search).get("mock") === "1";
if (mockMode && !window.overwolf) window.overwolf = createMockOverwolfApi();

function applyCopy(): void {
  document.documentElement.lang = locale;
  document.title = locale === "ru" ? "PlatScope Companion · Снимок инвентаря" : "PlatScope Companion · Inventory snapshot";
  language.value = locale;
  for (const node of document.querySelectorAll<HTMLElement>("[data-copy]")) {
    const key = node.dataset.copy as keyof typeof copy.ru;
    node.textContent = copy[locale][key];
  }
  for (const node of document.querySelectorAll<HTMLElement>("[data-copy-aria]")) {
    const key = node.dataset.copyAria as keyof typeof copy.ru;
    node.setAttribute("aria-label", copy[locale][key]);
  }
}

function formatNotice(notice: CompanionNotice): string {
  const base = noticeText[locale][notice.code];
  const snapshot = notice.snapshotError ? ` ${snapshotErrors[locale][notice.snapshotError]}` : "";
  const detail = notice.detail ? ` ${notice.detail}` : "";
  return `${base}${snapshot}${detail}`;
}

function setStatusPair(prefix: string, pair: [string, string]): void {
  element<HTMLElement>(`${prefix}-value`).textContent = pair[0];
  element<HTMLElement>(`${prefix}-note`).textContent = pair[1];
}

function render(state: Readonly<CompanionState>): void {
  latestState = state;
  setStatusPair("runtime", statusLabels[locale].runtime[state.runtime]);
  setStatusPair("game", statusLabels[locale].game[state.game]);
  setStatusPair("gep", statusLabels[locale].gep[state.gep]);

  const signature = JSON.stringify(state.settings);
  if (signature !== settingsSignature) {
    settingsSignature = signature;
    exportEnabled.checked = state.settings.exportEnabled;
    destination.value = state.settings.destination;
  }

  const snapshot = state.snapshot;
  const snapshotDate = snapshot
    ? new Intl.DateTimeFormat(locale === "ru" ? "ru-RU" : "en-US", { dateStyle: "medium", timeStyle: "medium" }).format(new Date(snapshot.envelope.observedAt))
    : "—";
  element("snapshot-value").textContent = snapshotDate;
  element("snapshot-note").textContent = snapshot
    ? locale === "ru"
      ? `${snapshot.analysis.rowCount} ${russianPlural(snapshot.analysis.rowCount, "строка", "строки", "строк")} · coherent`
      : `${snapshot.analysis.rowCount} ${englishPlural(snapshot.analysis.rowCount, "row", "rows")} · coherent`
    : locale === "ru" ? "Ожидаем валидный inventory event" : "Waiting for a valid inventory event";

  element("export-badge").textContent = statusLabels[locale].export[state.exportStatus];
  element("export-badge").dataset.status = state.exportStatus;
  const numberFormat = new Intl.NumberFormat(locale === "ru" ? "ru-RU" : "en-US");
  element("row-count").textContent = numberFormat.format(snapshot?.analysis.rowCount ?? 0);
  element("recognized-rows").textContent = numberFormat.format(snapshot?.analysis.rowCount ?? 0);
  element("distinct-items").textContent = numberFormat.format(snapshot?.analysis.distinctItemCount ?? 0);
  element("total-quantity").textContent = numberFormat.format(snapshot?.analysis.totalQuantity ?? 0);

  const categoryList = element<HTMLUListElement>("category-list");
  categoryList.replaceChildren();
  for (const category of snapshot?.analysis.categories ?? []) {
    const item = document.createElement("li");
    const name = document.createElement("span");
    const value = document.createElement("strong");
    name.textContent = category.name;
    value.textContent = locale === "ru"
      ? `${numberFormat.format(category.rows)} ${russianPlural(category.rows, "строка", "строки", "строк")} · ${numberFormat.format(category.quantity)} шт.`
      : `${numberFormat.format(category.rows)} ${englishPlural(category.rows, "row", "rows")} · ${numberFormat.format(category.quantity)} total`;
    item.append(name, value);
    categoryList.append(item);
  }
  element("category-empty").hidden = snapshot !== null;

  destination.disabled = !exportEnabled.checked || state.busy || formSubmitting;
  saveSettings.disabled = formSubmitting;
  saveSettings.classList.toggle("is-busy", formSubmitting);
  syncNow.disabled = state.runtime === "unavailable" || state.busy;
  syncNow.classList.toggle("is-busy", state.busy);
  main.setAttribute("aria-busy", state.busy ? "true" : "false");

  const pathInvalid = state.notice?.code === "path_required";
  destination.setAttribute("aria-invalid", pathInvalid ? "true" : "false");
  destinationError.textContent = pathInvalid ? noticeText[locale].path_required : "";

  const message = state.notice ? formatNotice(state.notice) : "";
  errorStatus.hidden = state.notice?.kind !== "error";
  errorStatus.textContent = state.notice?.kind === "error" ? message : "";
  exportResult.textContent = state.notice && ["settings_saved", "export_written", "export_failed"].includes(state.notice.code)
    ? message
    : "";
  if (state.notice?.kind === "status") liveStatus.textContent = message;
}

applyCopy();
const runtime = new CompanionRuntime(window.overwolf, localStorage, render);

language.addEventListener("change", () => {
  locale = language.value === "en" ? "en" : "ru";
  localStorage.setItem(localeKey, locale);
  applyCopy();
  if (latestState) render(latestState);
});

exportEnabled.addEventListener("change", () => {
  destination.disabled = !exportEnabled.checked;
  if (exportEnabled.checked) destination.focus();
});

destination.addEventListener("input", () => {
  if (destination.getAttribute("aria-invalid") === "true" && isAbsoluteJsonPath(destination.value)) {
    destination.setAttribute("aria-invalid", "false");
    destinationError.textContent = "";
  }
});

exportForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (formSubmitting) return;
  formSubmitting = true;
  if (latestState) render(latestState);
  const saved = await runtime.saveSettings({
    exportEnabled: exportEnabled.checked,
    destination: destination.value,
  });
  formSubmitting = false;
  if (latestState) render(latestState);
  if (!saved) destination.focus();
});

syncNow.addEventListener("click", () => {
  void runtime.syncNow();
});

window.addEventListener("beforeunload", () => runtime.destroy(), { once: true });
void runtime.start();
