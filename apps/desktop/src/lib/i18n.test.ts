import { describe, expect, it } from "vitest";

import {
  DEFAULT_APP_SETTINGS,
  languageFromLocale,
  localeCode,
  localeFromLanguage,
} from "./i18n";

describe("i18n contract", () => {
  it("maps persisted language without touching item identity", () => {
    expect(localeFromLanguage("russian")).toBe("ru");
    expect(localeFromLanguage("english")).toBe("en");
    expect(languageFromLocale("en")).toBe("english");
  });

  it("keeps conservative settings defaults and explicit number locales", () => {
    expect(DEFAULT_APP_SETTINGS.language).toBe("russian");
    expect(DEFAULT_APP_SETTINGS.platform).toBe("pc");
    expect(localeCode("ru")).toBe("ru-RU");
    expect(localeCode("en")).toBe("en-US");
  });
});
