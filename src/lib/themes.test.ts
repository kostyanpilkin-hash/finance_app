import { describe, expect, it } from "vitest";
import ru from "@/i18n/locales/ru";
import be from "@/i18n/locales/be";
import en from "@/i18n/locales/en";
import { DEFAULT_THEME, THEMES, isTheme } from "./themes";

describe("themes", () => {
  it("validates theme ids", () => {
    expect(isTheme(DEFAULT_THEME)).toBe(true);
    expect(isTheme("nope")).toBe(false);
    expect(isTheme(42)).toBe(false);
  });

  it("has a translated name for every theme in every language", () => {
    for (const locale of [ru, be, en]) {
      for (const th of THEMES) {
        expect(locale.settings.themes[th.id], `${th.id}`).toBeTruthy();
      }
    }
  });
});
