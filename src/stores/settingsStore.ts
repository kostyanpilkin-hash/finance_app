import { create } from "zustand";
import i18n, { isLanguage, type Language } from "@/i18n";
import { api } from "@/lib/api";
import { applyTheme, DEFAULT_THEME, isTheme, type Theme } from "@/lib/themes";

// Local UI preferences. Stage 1 moves them into the per-device `settings` table.
const STORAGE_KEY = "finance.ui";

interface Stored {
  theme?: string;
  language?: string;
}

function readStored(): Stored {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as Stored;
  } catch {
    return {};
  }
}

function persist(patch: Stored) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...readStored(), ...patch }));
}

function applyLanguage(language: Language) {
  void i18n.changeLanguage(language);
  document.documentElement.lang = language;
  const t = i18n.getFixedT(language);
  void api.setTrayLabels(t("tray.open"), t("tray.quit"), t("app.title")).catch(() => undefined);
}

interface SettingsState {
  theme: Theme;
  language: Language;
  init: () => void;
  setTheme: (theme: Theme) => void;
  setLanguage: (language: Language) => void;
}

export const useSettingsStore = create<SettingsState>((set) => ({
  theme: DEFAULT_THEME,
  language: "ru",
  init: () => {
    const s = readStored();
    const theme = isTheme(s.theme) ? s.theme : DEFAULT_THEME;
    const language = isLanguage(s.language) ? s.language : "ru";
    applyTheme(theme);
    applyLanguage(language);
    set({ theme, language });
  },
  setTheme: (theme) => {
    applyTheme(theme);
    persist({ theme });
    set({ theme });
  },
  setLanguage: (language) => {
    applyLanguage(language);
    persist({ language });
    set({ language });
  },
}));
