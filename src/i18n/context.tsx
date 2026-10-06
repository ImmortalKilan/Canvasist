import { createContext, useContext, useMemo, type ReactNode } from "react";
import type { MessageKey } from "./en";
import { translate, type Locale, type TranslateParams } from "./translate";

type TranslateFn = (key: MessageKey, params?: TranslateParams) => string;

const I18nContext = createContext<TranslateFn | null>(null);

export function I18nProvider({ locale, children }: { locale: Locale; children: ReactNode }) {
  const t = useMemo<TranslateFn>(() => (key, params) => translate(locale, key, params), [locale]);
  return <I18nContext.Provider value={t}>{children}</I18nContext.Provider>;
}

export function useT(): TranslateFn {
  const t = useContext(I18nContext);
  if (!t) throw new Error("useT must be used inside <I18nProvider>");
  return t;
}
