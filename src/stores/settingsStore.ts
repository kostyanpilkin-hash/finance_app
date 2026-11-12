import { create } from "zustand";
import i18n, { isLanguage, loadLanguage, type Language } from "@/i18n";
import { api } from "@/lib/api";
import { applyTheme, DEFAULT_THEME, isTheme, type Theme } from "@/lib/themes";

// Theme/language live in the per-device `settings` table; localStorage is
// just a cache so the right theme applies before the DB answers.
const CACHE_KEY = "finance.ui";

function readCache(): { theme?: string; language?: string } {
  try {
    return JSON.parse(localStorage.getItem(CACHE_KEY) ?? "{}");
  } catch {
    return {};
  }
}

function applyLanguage(language: Language) {
  document.documentElement.lang = language;
  void loadLanguage(language).then(() => {
    void i18n.changeLanguage(language);
    const t = i18n.getFixedT(language);
    void api.setTrayLabels(t("tray.open"), t("tray.quit"), t("app.title")).catch(() => undefined);
  });
}

interface SettingsState {
  theme: Theme;
  language: Language;
  loaded: boolean;
  init: () => Promise<void>;
  setTheme: (theme: Theme) => void;
  setLanguage: (language: Language) => void;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  theme: DEFAULT_THEME,
  language: "ru",
  loaded: false,
  init: async () => {
    const cache = readCache();
    const cachedTheme = isTheme(cache.theme) ? cache.theme : DEFAULT_THEME;
    const cachedLang = isLanguage(cache.language) ? cache.language : "ru";
    applyTheme(cachedTheme);
    applyLanguage(cachedLang);
    set({ theme: cachedTheme, language: cachedLang });

    try {
      const s = await api.getSettings();
      const theme = isTheme(s.theme) ? (s.theme as Theme) : get().theme;
      const language = isLanguage(s.language) ? (s.language as Language) : get().language;
      applyTheme(theme);
      applyLanguage(language);
      set({ theme, language, loaded: true });
    } catch {
      set({ loaded: true });
    }
  },
  setTheme: (theme) => {
    applyTheme(theme);
    localStorage.setItem(CACHE_KEY, JSON.stringify({ ...readCache(), theme }));
    void api.setSetting("theme", theme).catch(() => undefined);
    set({ theme });
  },
  setLanguage: (language) => {
    applyLanguage(language);
    localStorage.setItem(CACHE_KEY, JSON.stringify({ ...readCache(), language }));
    void api.setSetting("language", language).catch(() => undefined);
    set({ language });
  },
}));
