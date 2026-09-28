import { describe, expect, it } from "vitest";
import { COLUMNS, DEAF_MS, deaf, heard, loudness, push, REACH, reach } from "./meter";

describe("loudness", () => {
  it("puts silence and room noise at the floor", () => {
    expect(loudness(0)).toBe(0);
    expect(loudness(-1)).toBe(0);
    expect(loudness(0.0005)).toBe(0); // -66 dB
  });
  it("lets quiet speech move it and loud speech top out", () => {
    const quiet = loudness(0.01); // -40 dB
    expect(quiet).toBeGreaterThan(0.4);
    expect(quiet).toBeLessThan(0.5);
    expect(loudness(0.2)).toBe(1);
  });
});

describe("heard", () => {
  it("is more than a quiet room", () => {
    expect(heard(0)).toBe(false);
    expect(heard(0.003)).toBe(false); // -50 dB
    expect(heard(0.01)).toBe(true);
  });
});

describe("push", () => {
  it("adds on the right and keeps the last n", () => {
    expect(push([1, 2], 3, 3)).toEqual([1, 2, 3]);
    expect(push([1, 2, 3], 4, 3)).toEqual([2, 3, 4]);
  });
  it("defaults to the wave's width", () => {
    let w: number[] = [];
    for (let i = 0; i < COLUMNS + 5; i++) w = push(w, i);
    expect(w).toHaveLength(COLUMNS);
    expect(w.at(-1)).toBe(COLUMNS + 4);
  });
});

describe("reach", () => {
  it("is flat for silence and full for loud, clamped", () => {
    expect(reach(0)).toBe(0);
    expect(reach(1)).toBe(REACH);
    expect(reach(2)).toBe(REACH);
    expect(reach(-1)).toBe(0);
  });
});

describe("deaf", () => {
  it("waits DEAF_MS of hearing nothing since the start", () => {
    expect(deaf(0, false, DEAF_MS - 1)).toBe(false);
    expect(deaf(0, false, DEAF_MS)).toBe(true);
  });
  it("never fires once something was heard, even in a long pause", () => {
    expect(deaf(0, true, 60_000)).toBe(false);
  });
});
