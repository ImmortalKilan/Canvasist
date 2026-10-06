import type { Assignment, AssignmentStatus } from "./ipc";

/** Display groups from the lighthouse (G5), in on-screen order. */
export type GroupKey = "overdue" | "today" | "tomorrow" | "thisWeek" | "later" | "completed";

export const GROUP_ORDER: readonly GroupKey[] = [
  "overdue",
  "today",
  "tomorrow",
  "thisWeek",
  "later",
  "completed",
];

const DONE: ReadonlySet<AssignmentStatus> = new Set(["submitted", "late", "graded", "excused"]);

export function isDone(status: AssignmentStatus): boolean {
  return DONE.has(status);
}

function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

function addDays(d: Date, days: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + days);
}

/**
 * Start of next Monday (local time): "this week" runs Monday through Sunday,
 * so Sunday-night deadlines still count as this week.
 */
function startOfNextWeek(today: Date): Date {
  const daysSinceMonday = (today.getDay() + 6) % 7;
  return addDays(today, 7 - daysSinceMonday);
}

/** Assigns one assignment to its display group, using local calendar days. */
export function groupOf(assignment: Assignment, now: Date): GroupKey {
  if (isDone(assignment.status)) return "completed";
  const due = new Date(assignment.dueAt);
  if (assignment.status === "missing" || due < now) return "overdue";

  const today = startOfDay(now);
  const tomorrow = addDays(today, 1);
  const dayAfter = addDays(today, 2);
  if (due < tomorrow) return "today";
  if (due < dayAfter) return "tomorrow";
  if (due < startOfNextWeek(today)) return "thisWeek";
  return "later";
}

/**
 * Groups assignments for display. Open work is sorted soonest first; completed
 * work is sorted most recent first, since that is what a student looks back at.
 */
export function groupAssignments(
  assignments: readonly Assignment[],
  now: Date,
): Map<GroupKey, Assignment[]> {
  const groups = new Map<GroupKey, Assignment[]>(GROUP_ORDER.map((k) => [k, []]));
  for (const a of assignments) groups.get(groupOf(a, now))?.push(a);

  const byDue = (a: Assignment, b: Assignment) => Date.parse(a.dueAt) - Date.parse(b.dueAt);
  for (const [key, list] of groups) {
    list.sort(key === "completed" ? (a, b) => byDue(b, a) : byDue);
  }
  return groups;
}
