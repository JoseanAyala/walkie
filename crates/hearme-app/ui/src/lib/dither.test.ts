import { describe, expect, it } from "vitest";
import { density, rng } from "./dither";

describe("rng", () => {
  it("is deterministic per seed", () => {
    const a = rng(11);
    const b = rng(11);
    const c = rng(12);
    const xs = [a(), a(), a()];
    expect([b(), b(), b()]).toEqual(xs);
    expect(c()).not.toBe(xs[0]);
    for (const x of xs) expect(x >= 0 && x < 1).toBe(true);
  });
});

describe("density", () => {
  const clouds: [number, number, number][] = [[0.5, 0.5, 0.25]];
  it("peaks at a cloud's centre and is zero outside", () => {
    expect(density(50, 50, 100, 100, clouds)).toBe(1);
    expect(density(0, 0, 100, 100, clouds)).toBe(0);
    const mid = density(60, 50, 100, 100, clouds);
    expect(mid > 0 && mid < 1).toBe(true);
  });
});
