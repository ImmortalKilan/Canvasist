import { useEffect, useState } from "react";

/** How pressing an open assignment is (lighthouse G5 colours). */
export type Urgency = "overdue" | "urgent" | "soon" | "later";

const HOUR_MS = 60 * 60 * 1000;
const DAY_MS = 24 * HOUR_MS;

/**
 * Overdue work is grey, work due within 24 hours is red with a countdown,
 * work due within 7 days keeps the accent colour, and anything later is neutral.
 */
export function urgencyOf(dueAt: string, now: Date, overdue: boolean): Urgency {
  const left = Date.parse(dueAt) - now.getTime();
  if (overdue || left < 0) return "overdue";
  if (left < DAY_MS) return "urgent";
  if (left < 7 * DAY_MS) return "soon";
  return "later";
}

/** Whole hours and minutes left until `dueAt`, rounded down; null once due. */
export function timeLeft(dueAt: string, now: Date): { hours: number; minutes: number } | null {
  const left = Date.parse(dueAt) - now.getTime();
  if (left <= 0) return null;
  const totalMinutes = Math.floor(left / 60_000);
  return { hours: Math.floor(totalMinutes / 60), minutes: totalMinutes % 60 };
}

/** The current time, refreshed every `intervalMs` so countdowns and groups stay live. */
export function useNow(intervalMs: number): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}
