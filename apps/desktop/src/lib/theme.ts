import { writable } from "svelte/store";

export const THEME_STORAGE_KEY = "dldr.color-theme";
export const themePreferences = ["light", "dark", "system"] as const;
export type ThemePreference = typeof themePreferences[number];
export const theme = writable<ThemePreference>("light");

function normalizeTheme(value: unknown): ThemePreference {
  return themePreferences.includes(value as ThemePreference) ? value as ThemePreference : "light";
}

function savedTheme(): ThemePreference {
  try { return normalizeTheme(localStorage.getItem(THEME_STORAGE_KEY)); }
  catch { return "light"; }
}

export function setTheme(value: ThemePreference) {
  const next = normalizeTheme(value);
  theme.set(next);
  // A denied/full browser store must not prevent changing the current session.
  try { localStorage.setItem(THEME_STORAGE_KEY, next); } catch { /* Session only. */ }
}

/** UI preference only: no library/database mutation, and no dependency on login. */
export function initializeTheme(onApplied?: (dark: boolean) => void) {
  const system = matchMedia("(prefers-color-scheme: dark)");
  let current = savedTheme();
  const apply = () => {
    const resolved = current === "dark" || (current === "system" && system.matches) ? "dark" : "light";
    document.documentElement.dataset.theme = resolved;
    document.documentElement.style.colorScheme = resolved;
    onApplied?.(resolved === "dark");
  };
  theme.set(current);
  const unsubscribe = theme.subscribe(value => { current = value; apply(); });
  const storageChanged = (event: StorageEvent) => {
    if (event.storageArea === localStorage && (event.key === THEME_STORAGE_KEY || event.key === null)) {
      theme.set(normalizeTheme(event.newValue));
    }
  };
  system.addEventListener("change", apply);
  window.addEventListener("storage", storageChanged);
  return () => {
    unsubscribe();
    system.removeEventListener("change", apply);
    window.removeEventListener("storage", storageChanged);
  };
}
