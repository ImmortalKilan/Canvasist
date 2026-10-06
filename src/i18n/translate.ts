import { en, type MessageKey } from "./en";
import { zhCN } from "./zh-CN";

export type Locale = "en" | "zh-CN";
export type TranslateParams = Readonly<Record<string, string | number>>;

const catalogs: Readonly<Record<Locale, Readonly<Record<MessageKey, string>>>> = {
  en,
  "zh-CN": zhCN,
};

/** Looks up `key` for `locale` and fills `{name}` placeholders from `params`. */
export function translate(locale: Locale, key: MessageKey, params?: TranslateParams): string {
  const template = catalogs[locale][key];
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (placeholder, name: string) => {
    const value = params[name];
    return value === undefined ? placeholder : String(value);
  });
}
