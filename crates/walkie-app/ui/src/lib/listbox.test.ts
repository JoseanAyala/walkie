import { describe, expect, it } from "vitest";
import { move, type Option, typeahead } from "./listbox";

describe("move", () => {
  it("steps and stops at the ends", () => {
    expect(move(0, "ArrowDown", 3)).toBe(1);
    expect(move(2, "ArrowDown", 3)).toBe(2);
    expect(move(0, "ArrowUp", 3)).toBe(0);
  });
  it("jumps to the ends", () => {
    expect(move(1, "Home", 3)).toBe(0);
    expect(move(1, "End", 3)).toBe(2);
  });
  it("ignores other keys and empty lists", () => {
    expect(move(1, "a", 3)).toBeNull();
    expect(move(0, "ArrowDown", 0)).toBeNull();
  });
});

describe("typeahead", () => {
  const opts: Option[] = ["English", "Español", "Auto-detect"].map((text) => ({
    value: text,
    text,
  }));
  it("finds the next option starting with the letter, wrapping", () => {
    expect(typeahead(opts, 0, "e")).toBe(1);
    expect(typeahead(opts, 1, "E")).toBe(0);
    expect(typeahead(opts, 0, "a")).toBe(2);
  });
  it("is -1 for no match or a non-letter key", () => {
    expect(typeahead(opts, 0, "z")).toBe(-1);
    expect(typeahead(opts, 0, "Enter")).toBe(-1);
    expect(typeahead(opts, 0, " ")).toBe(-1);
  });
});
