import { getContext, setContext } from "svelte";
import { writable, type Writable } from "svelte/store";

export type UiLocale = "ru" | "en";
export type PersistedLanguage = "russian" | "english";

export interface AppSettings {
  language: PersistedLanguage;
  platform: "pc" | "playstation" | "xbox" | "switch" | "mobile";
  crossplay: boolean;
  bulk_refresh_hours: number;
  live_quote_ttl_seconds: number;
  keep_inventory_copies: number;
  reward_overlay_scale_percent: number;
  reward_overlay_offset_x_percent: number;
  reward_overlay_offset_y_percent: number;
}

export const DEFAULT_APP_SETTINGS: AppSettings = {
  language: "russian",
  platform: "pc",
  crossplay: true,
  bulk_refresh_hours: 4,
  live_quote_ttl_seconds: 90,
  keep_inventory_copies: 1,
  reward_overlay_scale_percent: 100,
  reward_overlay_offset_x_percent: 0,
  reward_overlay_offset_y_percent: 0,
};

const UI_LOCALE = Symbol("platscope-ui-locale");

export function localeFromLanguage(language: PersistedLanguage): UiLocale {
  return language === "english" ? "en" : "ru";
}

export function languageFromLocale(locale: UiLocale): PersistedLanguage {
  return locale === "en" ? "english" : "russian";
}

export function installLocale(initial: UiLocale = "ru"): Writable<UiLocale> {
  const store = writable<UiLocale>(initial);
  setContext(UI_LOCALE, store);
  return store;
}

export function useLocale(): Writable<UiLocale> {
  const store = getContext<Writable<UiLocale> | undefined>(UI_LOCALE);
  if (!store) throw new Error("PlatScope locale context is unavailable");
  return store;
}

export function localeCode(locale: UiLocale): "ru-RU" | "en-US" {
  return locale === "en" ? "en-US" : "ru-RU";
}
