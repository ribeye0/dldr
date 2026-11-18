import { DEFAULT_LANGUAGE_PRIORITY } from "$lib/model/constants";
import type { UiLocale } from "$lib/i18n";

export const TITLE_LANGUAGES = ["ja_JP", "zh_TW", "zh_CN", "en_US"] as const;

export function defaultLanguagePriority(locale: UiLocale): string[] {
  switch (locale) {
    case "zh-TW": return [...DEFAULT_LANGUAGE_PRIORITY];
    case "zh-CN": return ["zh_CN", "zh_TW", "ja_JP", "en_US"];
    case "ja": return ["ja_JP", "zh_TW", "zh_CN", "en_US"];
    case "en": return ["en_US", "ja_JP", "zh_TW", "zh_CN"];
  }
}

// Only an untouched locale preset follows a locale change. User reordering stays explicit.
export function priorityForLocaleChange(draft: readonly string[], current: UiLocale, next: UiLocale) {
  return sameLanguagePriority(draft, defaultLanguagePriority(current))
    ? defaultLanguagePriority(next)
    : [...draft];
}

export function sameLanguagePriority(a: readonly string[], b: readonly string[]) {
  return a.length === b.length && a.every((code, index) => code === b[index]);
}

// Other cached languages remain backend fallbacks, never extra settings rows.
export function buildLanguagePriority(saved: readonly string[], locale: UiLocale = "zh-TW") {
  const supported = [...new Set(saved)]
    .filter((code) => TITLE_LANGUAGES.some((language) => language === code));
  if (!supported.length) return defaultLanguagePriority(locale);
  return [...new Set([...supported, ...DEFAULT_LANGUAGE_PRIORITY])];
}

export function moveLanguagePriority(codes: readonly string[], code: string, direction: -1 | 1) {
  const index = codes.indexOf(code), target = index + direction;
  const result = [...codes];
  if (index >= 0 && target >= 0 && target < codes.length) {
    [result[index], result[target]] = [result[target], result[index]];
  }
  return result;
}

export function languagePriorityPatch(draft: readonly string[], baseline: readonly string[]): { languagePriority?: string[] } {
  return sameLanguagePriority(draft, baseline) ? {} : { languagePriority: [...draft] };
}
