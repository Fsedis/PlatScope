import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { writable } from "svelte/store";
import type { UiLocale } from "./i18n";

export type MasteryStatus = "mastered" | "progress" | "unknown";
export interface MasteryItemView {
  gameRef: string;
  displayName: string;
  displayNameEn: string;
  category: string;
  imageUrl: string | null;
  maxRank: number | null;
  xp: number | null;
  masteryRank?: number | null;
  status: MasteryStatus;
  reason: "equipment_confirmed" | "history_confirmed" | "history_partial" | "no_record" | "unsupported";
  setSlugs: string[];
}
export interface MasteryView {
  observedAt: string | null;
  source: "inventory_xp_info" | null;
  refreshFailed: boolean;
  catalogAvailable: boolean;
  items: MasteryItemView[];
}
export interface MasteryState {
  view: MasteryView | null;
  byGameRef: ReadonlyMap<string, MasteryItemView>;
  loading: boolean;
  error: boolean;
}

export function masteryStatusLabel(status: MasteryStatus, locale: UiLocale = "ru"): string {
  return locale === "ru"
    ? { mastered: "Освоено", progress: "Не освоено", unknown: "Нет данных" }[status]
    : { mastered: "Mastered", progress: "Not mastered", unknown: "Unknown" }[status];
}

/** Отсутствие подтверждения не подменяем нулевым прогрессом. */
export function masteryExplanation(item: MasteryItemView, locale: UiLocale = "ru"): string {
  const t = (ru: string, en: string) => locale === "ru" ? ru : en;
  if (item.reason === "unsupported") return t("Для этого снаряжения особые правила освоения. Автоматический статус пока не определяем.", "This equipment has special mastery rules. An automatic status is not available yet.");
  if (item.reason === "no_record") return t("В полученной истории нет записи об этом предмете. Его освоение пока неизвестно, а не равно нулю.", "The received history has no entry for this item. Its mastery is unknown, not zero.");
  if (item.status === "progress") return t("Максимальный ранг ещё не достигнут. Показано освоение готового предмета за всё время, а не текущий ранг отдельной копии.", "The maximum rank has not been reached. This is lifetime mastery of the built item, not the current rank of an individual copy.");
  return t("Готовый предмет уже освоен на аккаунте. Продажа и применение Формы не стирают эту отметку.", "The built item is already mastered on this account. Selling and Forma do not remove this record.");
}

export function masteryAnnotation(
  item: MasteryItemView | undefined,
  options: { loading?: boolean; error?: boolean; stale?: boolean; historyAvailable?: boolean } = {},
  locale: UiLocale = "ru",
) {
  const t = (ru: string, en: string) => locale === "ru" ? ru : en;
  const tone = item?.status ?? "unknown";
  let text = item && item.status !== "unknown"
    ? masteryStatusLabel(item.status, locale)
    : t("Освоение: нет данных", "Mastery: unknown");
  let title = item ? masteryExplanation(item, locale)
    : t("Для этого предмета нет данных об освоении.", "Mastery data is unavailable for this item.");
  if (item?.status === "progress" && item.masteryRank != null && item.maxRank != null) {
    text += ` · ${item.masteryRank}/${item.maxRank}`;
    title += t(` Освоено рангов: ${item.masteryRank} из ${item.maxRank}.`, ` Mastery ranks: ${item.masteryRank} of ${item.maxRank}.`);
  }
  if (!item && options.loading) {
    text = t("Освоение: загружаем…", "Mastery: loading…");
    title = t("Получаем историю освоения аккаунта.", "Loading account mastery history.");
  } else if (!item && options.error) {
    text = t("Освоение недоступно", "Mastery unavailable");
    title = t("Не удалось загрузить историю. Повторите обновление в «Мои предметы → Освоение».", "History could not be loaded. Retry in My items → Mastery.");
  } else if ((!item || item.status === "unknown") && !options.historyAvailable) {
    title += t(" Обновите данные из Warframe в «Мои предметы → Освоение».", "Update from Warframe in My items → Mastery.");
  }
  if (item && options.stale) {
    text += t(" · сохранено", " · saved");
    title += t(" Показана последняя сохранённая история: обновить её не удалось.", " Showing the last saved history: refresh failed.");
  }
  return { text, title, tone, mastered: item?.status === "mastered" };
}

const categoryNames: Record<string, readonly [string, string]> = {
  warframe: ["Варфреймы", "Warframes"], primary: ["Основное оружие", "Primary weapons"],
  secondary: ["Вторичное оружие", "Secondary weapons"], melee: ["Ближний бой", "Melee"],
  companion: ["Компаньоны", "Companions"], sentinel: ["Стражи", "Sentinels"],
  sentinel_weapon: ["Оружие стражей", "Sentinel weapons"], archwing: ["Арчвинги", "Archwings"],
  companion_weapon: ["Оружие компаньонов", "Companion weapons"],
  archgun: ["Арч-пушки", "Archguns"], archmelee: ["Арч-ближний бой", "Arch-melee"],
  necramech: ["Некрамехи", "Necramechs"], amp: ["Усилители", "Amps"],
  plexus: ["Плексус", "Plexus"],
  modular: ["Модульное снаряжение", "Modular equipment"], kdrive: ["К-Драйвы", "K-Drives"],
  weapon: ["Оружие", "Weapons"], other: ["Другое", "Other"],
};
export function masteryCategoryLabel(category: string, locale: UiLocale = "ru"): string {
  return (categoryNames[category] ?? categoryNames.other)[locale === "ru" ? 0 : 1];
}

function normalized(value: string): string {
  return value.toLocaleLowerCase("ru").replaceAll("ё", "е").trim();
}
export function filterMasteryItems(
  items: readonly MasteryItemView[],
  query: string,
  category: string,
  status: MasteryStatus | "all",
  locale: UiLocale = "ru",
): MasteryItemView[] {
  const terms = normalized(query).split(/\s+/).filter(Boolean);
  return items.filter(item => (category === "all" || category === item.category)
    && (status === "all" || status === item.status)
    && terms.every(term => normalized(`${item.displayName} ${item.displayNameEn}`).includes(term)))
    .sort((a, b) => (locale === "ru" ? a.displayName : a.displayNameEn)
      .localeCompare(locale === "ru" ? b.displayName : b.displayNameEn, locale) || a.gameRef.localeCompare(b.gameRef));
}

/** Деталь не осваивается сама: показываем историю точно связанного готового предмета. */
export function masteryTargetsForReward(
  reward: { rewardGameRef: string; rewardSlug: string | null },
  sets: readonly { definition: { setGameRef: string }; components: readonly { definition: { gameRef: string; slug: string } }[] }[],
): string[] {
  // Игровой blueprint и component могут иметь разные пути; канонический рыночный
  // идентификатор связывает их только при подтверждённом точном сопоставлении.
  return [...new Set(sets.filter(set => set.components.some(part =>
    (Boolean(reward.rewardGameRef) && part.definition.gameRef === reward.rewardGameRef)
      || (Boolean(reward.rewardSlug) && part.definition.slug === reward.rewardSlug)))
    .map(set => set.definition.setGameRef).filter(Boolean))];
}

interface MasteryDependencies {
  load: () => Promise<MasteryView>;
  listen: (event: string, handler: () => void) => Promise<UnlistenFn>;
}

/** Один снимок и одна подписка на события для всех мест интерфейса. */
export function createMasteryStore(dependencies: MasteryDependencies) {
  let state: MasteryState = { view: null, byGameRef: new Map(), loading: false, error: false };
  let inFlight: Promise<void> | null = null;
  let queued = false;
  let inventoryRevision = 0;
  const store = writable<MasteryState>(state, () => {
    let disposed = false;
    const cleanups: UnlistenFn[] = [];
    // Пока экранов не было, событие смены аккаунта могло пройти мимо подписки.
    inventoryRevision++;
    publish({ view: null, byGameRef: new Map(), error: false });
    for (const event of ["inventory-updated", "game-metadata-updated"]) {
      void dependencies.listen(event, () => {
        if (event === "inventory-updated") {
          // Снимок может принадлежать другому аккаунту. До ответа backend прежние
          // подтверждения нельзя ни показывать, ни восстанавливать при ошибке.
          inventoryRevision++;
          publish({ view: null, byGameRef: new Map(), error: false });
        }
        void refresh();
      }).then(cleanup => {
        if (disposed) cleanup(); else cleanups.push(cleanup);
      }).catch(() => { /* Без событий остаются загрузка экрана и ручное обновление. */ });
    }
    void refresh();
    return () => { disposed = true; cleanups.forEach(cleanup => cleanup()); };
  });

  function publish(update: Partial<MasteryState>) {
    state = { ...state, ...update };
    store.set(state);
  }
  function refresh(): Promise<void> {
    if (inFlight) { queued = true; return inFlight; }
    inFlight = (async () => {
      do {
        queued = false;
        const revision = inventoryRevision;
        publish({ loading: true, error: false });
        try {
          const view = await dependencies.load();
          if (revision === inventoryRevision) {
            publish({ view, byGameRef: new Map(view.items.map(item => [item.gameRef, item])), error: false });
          }
        } catch {
          // Ошибка чтения не превращает сохранённую историю в пустой аккаунт.
          if (revision === inventoryRevision) publish({ error: true });
        }
      } while (queued);
      publish({ loading: false });
    })().finally(() => { inFlight = null; });
    return inFlight;
  }
  return { subscribe: store.subscribe, refresh };
}

export const masteryStore = createMasteryStore({
  load: () => invoke<MasteryView>("load_mastery"),
  listen: (event, handler) => listen(event, handler),
});
