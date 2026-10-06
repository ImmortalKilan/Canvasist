import type { Locale } from "./i18n/translate";

/** Formats a due date such as "Fri, Oct 10, 11:59 PM" / "10月10日周五 23:59". */
export function formatDue(iso: string, locale: Locale): string {
  return new Intl.DateTimeFormat(locale, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(iso));
}

/** Formats a time of day, e.g. "3:05 PM" / "15:05". */
export function formatTime(iso: string, locale: Locale): string {
  return new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).format(
    new Date(iso),
  );
}

/** Returns the host of an origin such as "https://canvas.school.edu". */
export function hostOf(origin: string): string {
  try {
    return new URL(origin).host;
  } catch {
    return origin;
  }
}
