import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  applyDocumentLang,
  loadStoredLocale,
  saveLocale,
  type AppLocale,
} from "./locale";
import { t as translate, tf as translateFormat } from "./messages";

type TranslateFn = (key: string) => string;
type TranslateFormatFn = (
  key: string,
  vars: Record<string, string | number>
) => string;

interface I18nValue {
  locale: AppLocale;
  setLocale: (locale: AppLocale) => void;
  t: TranslateFn;
  tf: TranslateFormatFn;
}

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<AppLocale>(() => loadStoredLocale());

  const setLocale = useCallback((next: AppLocale) => {
    setLocaleState(next);
    saveLocale(next);
    applyDocumentLang(next);
  }, []);

  useEffect(() => {
    applyDocumentLang(locale);
  }, [locale]);

  const value = useMemo<I18nValue>(
    () => ({
      locale,
      setLocale,
      t: (key) => translate(locale, key),
      tf: (key, vars) => translateFormat(locale, key, vars),
    }),
    [locale, setLocale]
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    return {
      locale: "th",
      setLocale: () => undefined,
      t: (key) => key,
      tf: (key) => key,
    };
  }
  return ctx;
}

export function useLocale(): [AppLocale, (locale: AppLocale) => void] {
  const { locale, setLocale } = useI18n();
  return [locale, setLocale];
}

export function useT(): TranslateFn {
  return useI18n().t;
}

export function useTf(): TranslateFormatFn {
  return useI18n().tf;
}
