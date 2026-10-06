import { describe, expect, it } from "vitest";
import { groupAssignments, groupOf } from "./grouping";
import type { Assignment, AssignmentStatus } from "./ipc";

// Dates are built in local time so the tests pass in any time zone.
// 2026-10-07 is a Wednesday.
const NOW = new Date(2026, 9, 7, 12, 0);

function assignment(id: string, due: Date, status: AssignmentStatus = "notSubmitted"): Assignment {
  return {
    id,
    courseId: "c",
    title: id,
    dueAt: due.toISOString(),
    lateDueAt: null,
    url: null,
    kind: "online",
    status,
    source: "canvas",
    linksToGradescope: false,
  };
}

describe("groupOf", () => {
  it("puts submitted, late, graded and excused work in completed", () => {
    for (const status of ["submitted", "late", "graded", "excused"] as const) {
      expect(groupOf(assignment("a", new Date(2026, 9, 8), status), NOW)).toBe("completed");
    }
  });

  it("puts missing or past-due open work in overdue", () => {
    expect(groupOf(assignment("a", new Date(2026, 9, 1), "missing"), NOW)).toBe("overdue");
    expect(groupOf(assignment("a", new Date(2026, 9, 7, 9, 0)), NOW)).toBe("overdue");
  });

  it("uses local calendar days for today and tomorrow", () => {
    expect(groupOf(assignment("a", new Date(2026, 9, 7, 23, 59)), NOW)).toBe("today");
    expect(groupOf(assignment("a", new Date(2026, 9, 8, 0, 0)), NOW)).toBe("tomorrow");
    expect(groupOf(assignment("a", new Date(2026, 9, 8, 23, 59)), NOW)).toBe("tomorrow");
  });

  it("treats Monday through Sunday as this week", () => {
    expect(groupOf(assignment("a", new Date(2026, 9, 9, 10, 0)), NOW)).toBe("thisWeek");
    expect(groupOf(assignment("a", new Date(2026, 9, 11, 23, 59)), NOW)).toBe("thisWeek");
    expect(groupOf(assignment("a", new Date(2026, 9, 12, 0, 0)), NOW)).toBe("later");
  });

  it("handles Sunday as the last day of the week", () => {
    const sunday = new Date(2026, 9, 11, 9, 0);
    expect(groupOf(assignment("a", new Date(2026, 9, 11, 20, 0)), sunday)).toBe("today");
    expect(groupOf(assignment("a", new Date(2026, 9, 12, 20, 0)), sunday)).toBe("tomorrow");
    expect(groupOf(assignment("a", new Date(2026, 9, 13, 20, 0)), sunday)).toBe("later");
  });
});

describe("groupAssignments", () => {
  it("sorts open work soonest first and completed work most recent first", () => {
    const groups = groupAssignments(
      [
        assignment("later-2", new Date(2026, 9, 20)),
        assignment("later-1", new Date(2026, 9, 15)),
        assignment("done-old", new Date(2026, 9, 1), "graded"),
        assignment("done-new", new Date(2026, 9, 6), "submitted"),
      ],
      NOW,
    );
    expect(groups.get("later")?.map((a) => a.id)).toEqual(["later-1", "later-2"]);
    expect(groups.get("completed")?.map((a) => a.id)).toEqual(["done-new", "done-old"]);
    expect(groups.get("today")).toEqual([]);
  });
});
