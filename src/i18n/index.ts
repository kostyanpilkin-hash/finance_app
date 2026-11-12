import i18n from "i18next";
import { initReactI18next } from "react-i18next";

export const LANGUAGES = [
  { id: "ru", label: "Русский" },
  { id: "be", label: "Беларуская" },
  { id: "en", label: "English" },
] as const;

export type Language = (typeof LANGUAGES)[number]["id"];

export function isLanguage(v: unknown): v is Language {
  return LANGUAGES.some((l) => l.id === v);
}

i18n.use(initReactI18next).init({
  lng: "ru",
  fallbackLng: "ru",
  interpolation: { escapeValue: false },
});

// Locale chunks load on demand — only the active language ships to the client.
const loaders = {
  ru: () => import("./locales/ru"),
  be: () => import("./locales/be"),
  en: () => import("./locales/en"),
} satisfies Record<Language, () => Promise<{ default: unknown }>>;

/** Loads the bundle for `lng` (no-op when already loaded). */
export async function loadLanguage(lng: Language) {
  if (i18n.hasResourceBundle(lng, "translation")) return;
  const mod = await loaders[lng]();
  i18n.addResourceBundle(lng, "translation", mod.default);
}

export default i18n;
