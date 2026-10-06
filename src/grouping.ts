import type { Assignment, AssignmentStatus, Marks } from "./ipc";

/** Display groups from the lighthouse (G5), in on-screen order. */
export type GroupKey =
  "overdue" | "today" | "tomorrow" | "nextSevenDays" | "later" | "completed" | "dismissed";

export const GROUP_ORDER: readonly GroupKey[] = [
  "overdue",
  "today",
  "tomorrow",
  "nextSevenDays",
  "later",
  "completed",
  "dismissed",
];

/** Groups shown folded at the bottom of the list. */
export const FOLDED_GROUPS: ReadonlySet<GroupKey> = new Set(["completed", "dismissed"]);

const DONE: ReadonlySet<AssignmentStatus> = new Set(["submitted", "late", "graded", "excused"]);
const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000;

export const NO_MARKS: Marks = { done: {}, dismissed: {} };

export function isDone(status: AssignmentStatus): boolean {
  return DONE.has(status);
}

/** True when the user marked it done and Canvas/Gradescope do not say so yet. */
export function isManuallyDone(assignment: Assignment, marks: Marks): boolean {
  return !isDone(assignment.status) && assignment.id in marks.done;
}

function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

function addDays(d: Date, days: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + days);
}

/** Assigns one assignment to its display group, using local calendar days. */
export function groupOf(assignment: Assignment, now: Date, marks: Marks = NO_MARKS): GroupKey {
  // Real submission data wins over the user's own marks.
  if (isDone(assignment.status)) return "completed";
  if (assignment.id in marks.dismissed) return "dismissed";
  if (assignment.id in marks.done) return "completed";

  const due = new Date(assignment.dueAt);
  if (assignment.status === "missing" || due < now) return "overdue";

  const today = startOfDay(now);
  if (due < addDays(today, 1)) return "today";
  if (due < addDays(today, 2)) return "tomorrow";
  if (due.getTime() - now.getTime() < SEVEN_DAYS_MS) return "nextSevenDays";
  return "later";
}

/**
 * Groups assignments for display. Open work is sorted soonest first; completed
 * and dismissed work most recent first, since that is what a student looks back at.
 */
export function groupAssignments(
  assignments: readonly Assignment[],
  now: Date,
  marks: Marks = NO_MARKS,
): Map<GroupKey, Assignment[]> {
  const groups = new Map<GroupKey, Assignment[]>(GROUP_ORDER.map((k) => [k, []]));
  for (const a of assignments) groups.get(groupOf(a, now, marks))?.push(a);

  const byDue = (a: Assignment, b: Assignment) => Date.parse(a.dueAt) - Date.parse(b.dueAt);
  for (const [key, list] of groups) {
    list.sort(FOLDED_GROUPS.has(key) ? (a, b) => byDue(b, a) : byDue);
  }
  return groups;
}
