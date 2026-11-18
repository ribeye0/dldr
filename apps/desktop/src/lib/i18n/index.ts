import { derived, writable } from "svelte/store";
import en from "./en.json";
import zhTW from "./zh-TW.json";
import zhCN from "./zh-CN.json";
import ja from "./ja.json";

export const locales = ["zh-TW", "zh-CN", "ja", "en"] as const;
export type UiLocale = typeof locales[number];
export const localeNames: Record<UiLocale, string> = {
  "zh-TW": "繁體中文", "zh-CN": "简体中文", ja: "日本語", en: "English",
};
export function normalizeLocale(value: unknown): UiLocale {
  return locales.includes(value as UiLocale) ? value as UiLocale : "zh-TW";
}
export const locale = writable<UiLocale>("zh-TW");
export const messages: Record<UiLocale, Record<string, string>> = { en, "zh-TW": zhTW, "zh-CN": zhCN, ja };
export function translator(language: UiLocale) {
  return (key: string, ...values: unknown[]): string =>
    (messages[language][key] ?? messages.en[key] ?? key)
      .replace(/\{(\d+)\}/g, (match, index) => Number(index) < values.length ? String(values[Number(index)]) : match);
}
// Svelte's store subscription makes both labels and parameterized messages reactive.
export const t = derived(locale, translator);
