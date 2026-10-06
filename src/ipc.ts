// Typed wrappers around the Rust commands. Names and payloads must match
// src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";
import type { Locale } from "./i18n/translate";

export type LanguagePreference = "system" | Locale;

export interface SettingsView {
  language: LanguagePreference;
  effectiveLocale: Locale;
}

export interface AppInfo {
  version: string;
}

export const api = {
  getSettings: () => invoke<SettingsView>("get_settings"),
  setLanguage: (language: LanguagePreference) => invoke<SettingsView>("set_language", { language }),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<boolean>("set_autostart", { enabled }),
  getAppInfo: () => invoke<AppInfo>("get_app_info"),
};

/** Rust errors arrive as plain strings; anything else is stringified defensively. */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
