import { describe, expect, it } from "vitest";
import { formatDue, formatTime, hostOf } from "./format";

describe("format", () => {
  const iso = "2026-10-10T06:59:00Z";

  it("formats due dates in both locales", () => {
    expect(formatDue(iso, "en")).toMatch(/\d/);
    expect(formatDue(iso, "zh-CN")).toMatch(/\d/);
    expect(formatDue(iso, "en")).not.toEqual(formatDue(iso, "zh-CN"));
  });

  it("formats times", () => {
    expect(formatTime(iso, "en")).toMatch(/\d{1,2}:\d{2}/);
  });

  it("extracts hosts from origins", () => {
    expect(hostOf("https://example.instructure.com")).toBe("example.instructure.com");
    expect(hostOf("not a url")).toBe("not a url");
  });
});
