import { describe, expect, it } from "vitest";
import { chord, glyph, megabytes, sameKeys, when, words } from "./format";

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

describe("when", () => {
  const now = new Date(2026, 8, 27, 21, 0);
  it("says today for today's rows", () => {
    expect(when("2026-09-27 20:10:07", now)).toBe("today 20:10");
  });
  it("shows month and day otherwise", () => {
    expect(when("2026-09-25 09:15:00", now)).toBe("Sep 25 09:15");
  });
  it("passes through what it can't parse", () => {
    expect(when("yesterday", now)).toBe("yesterday");
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
