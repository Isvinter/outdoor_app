import { de } from "./de";

export type MessageKey = keyof typeof de;
type Messages = Record<MessageKey, string>;

/** Available languages. Add a file next to `de.ts` and list it here. */
const languages: Record<string, Messages> = { de };
let current: Messages = de;
let locale = "de";

export function setLanguage(code: string): void {
  current = languages[code] ?? de;
  locale = code in languages ? code : "de";
  document.documentElement.lang = locale;
}

export function currentLocale(): string {
  return locale;
}

/** Translated text; `{name}` placeholders are replaced from `values`. */
export function t(key: MessageKey, values: Record<string, string | number> = {}): string {
  return current[key].replace(/\{(\w+)\}/g, (_, name: string) => String(values[name] ?? ""));
}

/** Fills every element carrying `data-i18n` with its text. */
export function translateDocument(): void {
  for (const el of document.querySelectorAll<HTMLElement>("[data-i18n]")) {
    el.textContent = t(el.dataset.i18n as MessageKey);
  }
}
