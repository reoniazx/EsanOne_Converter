export type AppLocale = "en" | "th";

export const DEFAULT_LOCALE: AppLocale = "th";
export const LOCALE_STORAGE_KEY = "esanone_stem_locale";

export function normalizeLocale(value?: string | null): AppLocale {
  if (!value) return DEFAULT_LOCALE;
  const lower = value.toLowerCase();
  if (lower.startsWith("th")) return "th";
  if (lower.startsWith("en")) return "en";
  return DEFAULT_LOCALE;
}

export function loadStoredLocale(): AppLocale {
  if (typeof window === "undefined") return DEFAULT_LOCALE;
  try {
    const stored = localStorage.getItem(LOCALE_STORAGE_KEY);
    if (stored) return normalizeLocale(stored);
    // Thai is the product default. The user can still switch to English and
    // that explicit choice is kept in localStorage for future launches.
    return DEFAULT_LOCALE;
  } catch {
    return DEFAULT_LOCALE;
  }
}

export function saveLocale(locale: AppLocale): void {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  } catch {
    // ignore
  }
}

export function applyDocumentLang(
  locale: AppLocale,
  root: HTMLElement = document.documentElement
): void {
  root.lang = locale;
}
