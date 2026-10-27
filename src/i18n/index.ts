import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./en.json";
import es from "./es.json";

export const resources = {
  es: { translation: es },
  en: { translation: en },
} as const;

export type Language = keyof typeof resources;

export function detectLanguage(locale: string | undefined): Language {
  return locale?.toLowerCase().startsWith("es") ? "es" : "en";
}

void i18n.use(initReactI18next).init({
  resources,
  lng: detectLanguage(globalThis.navigator?.language),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
