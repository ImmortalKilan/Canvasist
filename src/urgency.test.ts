import { describe, expect, it } from "vitest";
import { timeLeft, urgencyOf } from "./urgency";

const NOW = new Date("2026-10-07T12:00:00Z");
const at = (hoursFromNow: number) =>
  new Date(NOW.getTime() + hoursFromNow * 3_600_000).toISOString();

describe("urgencyOf", () => {
  it("classifies by time left", () => {
    expect(urgencyOf(at(-1), NOW, false)).toBe("overdue");
    expect(urgencyOf(at(0.5), NOW, false)).toBe("urgent");
    expect(urgencyOf(at(23.99), NOW, false)).toBe("urgent");
    expect(urgencyOf(at(24), NOW, false)).toBe("soon");
    expect(urgencyOf(at(24 * 7 - 0.01), NOW, false)).toBe("soon");
    expect(urgencyOf(at(24 * 7), NOW, false)).toBe("later");
  });

  it("treats work flagged overdue as overdue", () => {
    expect(urgencyOf(at(5), NOW, true)).toBe("overdue");
  });
});

describe("timeLeft", () => {
  it("rounds down to whole minutes", () => {
    expect(timeLeft(at(2 + 5 / 60 + 50 / 3600), NOW)).toEqual({ hours: 2, minutes: 5 });
    expect(timeLeft(at(0.5), NOW)).toEqual({ hours: 0, minutes: 30 });
  });

  it("returns null once due", () => {
    expect(timeLeft(at(0), NOW)).toBeNull();
    expect(timeLeft(at(-1), NOW)).toBeNull();
  });
});
