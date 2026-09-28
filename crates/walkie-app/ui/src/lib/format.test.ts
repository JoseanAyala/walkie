import { describe, expect, it } from "vitest";
import { byDay, chord, clock, day, glyph, megabytes, sameKeys, words } from "./format";

describe("glyph", () => {
  it("shows modifiers as symbols and sides in words", () => {
    expect(glyph("Cmd")).toBe("⌘");
    expect(glyph("RightOpt")).toBe("right ⌥");
    expect(glyph("LeftShift")).toBe("left ⇧");
    expect(glyph("Fn")).toBe("fn");
  });
  it("leaves plain keys alone", () => {
    expect(glyph("V")).toBe("V");
    expect(glyph("F5")).toBe("F5");
  });
});

describe("chord", () => {
  it("joins keys, and an empty one is off", () => {
    expect(chord(["Ctrl", "Cmd", "V"])).toBe("⌃ ⌘ V");
    expect(chord([])).toBe("off");
  });
});

describe("words", () => {
  it("splits on commas and drops blanks", () => {
    expect(words(" um, uh ,, you know ")).toEqual(["um", "uh", "you know"]);
    expect(words("")).toEqual([]);
  });
});

describe("day", () => {
  const now = new Date(2026, 8, 27, 21, 0);
  it("says Today and Yesterday", () => {
    expect(day("2026-09-27 20:10:07", now)).toBe("Today");
    expect(day("2026-09-26 23:59:00", now)).toBe("Yesterday");
  });
  it("knows yesterday across a month and a year", () => {
    expect(day("2026-08-31 10:00:00", new Date(2026, 8, 1))).toBe("Yesterday");
    expect(day("2025-12-31 10:00:00", new Date(2026, 0, 1))).toBe("Yesterday");
  });
  it("shows month and day otherwise, and the year if it isn't this one", () => {
    expect(day("2026-09-25 09:15:00", now)).toBe("Sep 25");
    expect(day("2025-09-25 09:15:00", now)).toBe("Sep 25 2025");
  });
  it("passes through what it can't parse", () => {
    expect(day("yesterday", now)).toBe("yesterday");
  });
});

describe("clock", () => {
  it("is the time of day", () => {
    expect(clock("2026-09-27 20:10:07")).toBe("20:10");
    expect(clock("garbage")).toBe("");
  });
});

describe("byDay", () => {
  it("groups consecutive rows by day, keeping their order", () => {
    const now = new Date(2026, 8, 27, 21, 0);
    const rows = ["2026-09-27 20:00:00", "2026-09-27 08:00:00", "2026-09-25 09:00:00"].map(
      (created_at, id) => ({ id, created_at }),
    );
    expect(byDay(rows, now).map((g) => [g.day, g.rows.map((r) => r.id)])).toEqual([
      ["Today", [0, 1]],
      ["Sep 25", [2]],
    ]);
  });
});

describe("sameKeys", () => {
  it("ignores order", () => {
    expect(sameKeys(["Fn", "Space"], ["Space", "Fn"])).toBe(true);
    expect(sameKeys(["Fn"], ["Fn", "Space"])).toBe(false);
  });
});

describe("megabytes", () => {
  it("uses MB below a gigabyte and GB from there", () => {
    expect(megabytes(148)).toBe("148 MB");
    expect(megabytes(1000)).toBe("1 GB");
    expect(megabytes(1540)).toBe("1.5 GB");
  });
});
