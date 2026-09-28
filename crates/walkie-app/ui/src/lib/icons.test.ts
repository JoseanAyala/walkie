import { describe, expect, it } from "vitest";
import { ICONS, pixels } from "./icons";

describe("ICONS", () => {
  it("are all 11×11 bitmaps, on the font's grid", () => {
    for (const [name, rows] of Object.entries(ICONS)) {
      expect(rows, name).toHaveLength(11);
      for (const row of rows) expect(row, name).toMatch(/^[.#]{11}$/);
    }
  });
});

describe("pixels", () => {
  it("draws one rectangle per run of lit pixels", () => {
    expect(pixels(["#.##", "...."])).toBe("M0 0h1v1H0zM2 0h2v1H2z");
  });
  it("is empty for a blank bitmap", () => {
    expect(pixels(["...", "..."])).toBe("");
  });
});
