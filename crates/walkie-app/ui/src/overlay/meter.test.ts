import { describe, expect, it } from "vitest";
import { litBlocks } from "./meter";

describe("litBlocks", () => {
  it("scales the level to the blocks, capped at all of them", () => {
    expect(litBlocks(0, 0, 10)).toBe(0);
    expect(litBlocks(0.0625, 0, 10)).toBe(5);
    expect(litBlocks(1, 0, 10)).toBe(10);
  });

  it("falls back one block at a time", () => {
    expect(litBlocks(0, 10, 10)).toBe(9);
    expect(litBlocks(0, 1, 10)).toBe(0);
  });

  it("treats a negative level as silence", () => {
    expect(litBlocks(-1, 0, 10)).toBe(0);
  });
});
