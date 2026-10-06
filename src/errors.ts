import type { TranslateFn } from "./i18n/context";
import type { AppError } from "./ipc";

/** Maps a backend error to a localized, user-facing sentence. */
export function describeError(t: TranslateFn, error: AppError): string {
  switch (error.kind) {
    case "network":
      return t("error.network");
    case "invalidUrl":
      return t("error.invalidUrl");
    case "notCanvas":
      return t("error.notCanvas");
    case "sessionExpired":
    case "notSignedIn":
      return t("error.sessionExpired");
    case "canvas":
      return t("error.canvas", { message: error.message });
    case "busy":
      return t("error.busy");
    default:
      return t("error.generic", { message: error.message });
  }
}
