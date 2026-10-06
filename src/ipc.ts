// Typed wrappers around the Rust commands and events. Names and payloads must
// match src-tauri/src/commands.rs and src-tauri/src/account.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Locale } from "./i18n/translate";

export type LanguagePreference = "system" | Locale;

export interface SettingsView {
  language: LanguagePreference;
  effectiveLocale: Locale;
}

export interface AppInfo {
  version: string;
}

export interface School {
  name: string;
  url: string;
}

export type AuthStatus =
  | { state: "signedOut" }
  | { state: "signedIn"; origin: string }
  | { state: "expired"; origin: string };

export type AssignmentStatus =
  "notSubmitted" | "submitted" | "late" | "graded" | "missing" | "excused";

export type SubmissionKind = "online" | "onPaper" | "noSubmission" | "notGraded";

export type Source = "canvas" | "gradescope";

/** Outcome of the Gradescope part of the last refresh. */
export type GradescopeState = "ok" | "notLinked" | "needsCanvasLogin" | "unavailable";

export interface Course {
  id: string;
  code: string;
  name: string;
  source: Source;
}

export interface Assignment {
  id: string;
  courseId: string;
  title: string;
  dueAt: string;
  lateDueAt: string | null;
  url: string | null;
  kind: SubmissionKind;
  status: AssignmentStatus;
  source: Source;
  linksToGradescope: boolean;
}

export interface Snapshot {
  fetchedAt: string;
  courses: Course[];
  assignments: Assignment[];
  gradescope: GradescopeState;
}

export type ErrorKind =
  | "io"
  | "data"
  | "autostart"
  | "internal"
  | "network"
  | "crypto"
  | "invalidUrl"
  | "notCanvas"
  | "notSignedIn"
  | "sessionExpired"
  | "canvas"
  | "busy"
  | "gradescopeAuth"
  | "gradescope";

export interface AppError {
  kind: ErrorKind;
  message: string;
}

export const api = {
  getSettings: () => invoke<SettingsView>("get_settings"),
  setLanguage: (language: LanguagePreference) => invoke<SettingsView>("set_language", { language }),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<boolean>("set_autostart", { enabled }),
  getAppInfo: () => invoke<AppInfo>("get_app_info"),

  searchSchools: (query: string) => invoke<School[]>("search_schools", { query }),
  checkCanvasUrl: (input: string) => invoke<string>("check_canvas_url", { input }),
  startCanvasLogin: (origin: string) => invoke<null>("start_canvas_login", { origin }),
  cancelCanvasLogin: () => invoke<null>("cancel_canvas_login"),
  getAuthStatus: () => invoke<AuthStatus>("get_auth_status"),
  signOut: () => invoke<null>("sign_out"),

  getSnapshot: () => invoke<Snapshot | null>("get_snapshot"),
  refresh: () => invoke<Snapshot>("refresh"),
  openExternal: (url: string) => invoke<null>("open_external", { url }),
};

export const events = {
  onAuthChanged: (handler: (status: AuthStatus) => void): Promise<UnlistenFn> =>
    listen<AuthStatus>("auth-changed", (e) => handler(e.payload)),
  onLoginCancelled: (handler: () => void): Promise<UnlistenFn> =>
    listen("canvas-login-cancelled", () => handler()),
};

function isAppError(e: unknown): e is AppError {
  return (
    typeof e === "object" &&
    e !== null &&
    typeof (e as AppError).kind === "string" &&
    typeof (e as AppError).message === "string"
  );
}

/** Normalizes anything thrown by `invoke` into an `AppError`. */
export function toAppError(e: unknown): AppError {
  if (isAppError(e)) return e;
  if (typeof e === "string") return { kind: "internal", message: e };
  if (e instanceof Error) return { kind: "internal", message: e.message };
  return { kind: "internal", message: String(e) };
}

/** Human-readable detail for errors whose kind has no dedicated message. */
export function errorMessage(e: unknown): string {
  return toAppError(e).message;
}
