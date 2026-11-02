import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import ru from "./locales/ru";
import be from "./locales/be";
import en from "./locales/en";

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
  resources: {
    ru: { translation: ru },
    be: { translation: be },
    en: { translation: en },
  },
  lng: "ru",
  fallbackLng: "ru",
  interpolation: { escapeValue: false },
});

export default i18n;
