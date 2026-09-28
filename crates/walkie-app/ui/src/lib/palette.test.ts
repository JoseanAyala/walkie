import { describe, expect, it } from "vitest";
import {
  badName,
  contrast,
  derive,
  find,
  isHex,
  type Mode,
  type Palette,
  PRESETS,
  parseColors,
  readable,
  suggest,
  TOKENS,
  tokensFor,
} from "@/lib/palette";

const MODES: Mode[] = ["light", "dark"];

/** Every text/background pair the UI draws, and the contrast it needs. */
function pairs(t: ReturnType<typeof derive>): [string, string, string, number][] {
  return [
    ["fg on paper", t.fg, t.paper, 7],
    ["fg on panel", t.fg, t.panel, 7],
    ["fg on tag", t.fg, t.tag, 4.5],
    ["muted on paper", t.muted, t.paper, 4.5],
    ["muted on panel", t.muted, t.panel, 4.5],
    ["bar-fg on bar", t["bar-fg"], t.bar, 4.5],
    ["chip-fg on chip", t["chip-fg"], t.chip, 4.5],
    ["accent-fg on accent", t["accent-fg"], t.accent, 4.5],
  ];
}

describe("derive", () => {
  it("fills every token with a hex color", () => {
    for (const p of PRESETS)
      for (const m of MODES) {
        const t = tokensFor(p, m);
        for (const k of TOKENS) expect(isHex(t[k]), `${p.key} ${m} ${k}=${t[k]}`).toBe(true);
      }
  });

  it("keeps every derived preset's text readable", () => {
    for (const p of PRESETS.filter((x) => !x.fixed))
      for (const m of MODES)
        for (const [what, fg, bg, min] of pairs(derive(p, m)))
          expect(contrast(fg, bg), `${p.key} ${m}: ${what}`).toBeGreaterThanOrEqual(min);
  });

  it("keeps imported palettes readable, however odd", () => {
    // a fixed LCG, so a failure reproduces
    let seed = 3;
    const color = () => {
      seed = (seed * 1103515245 + 12345) % 2 ** 31;
      return `#${(seed % 0x1000000).toString(16).padStart(6, "0")}`;
    };
    const odd: Palette[] = [
      { base: "#ffffff", main: "#ffffff", accent: "#ffffff" },
      { base: "#000000", main: "#000000", accent: "#000000" },
      { base: "#808080", main: "#777777", accent: "#888888" },
    ];
    for (let i = 0; i < 200; i++) odd.push({ base: color(), main: color(), accent: color() });
    // 7:1 isn't always possible (mid-grey backgrounds); 4.5:1 is
    for (const p of odd)
      for (const m of MODES)
        for (const [what, fg, bg] of pairs(derive(p, m)))
          expect(contrast(fg, bg), `${JSON.stringify(p)} ${m}: ${what}`).toBeGreaterThanOrEqual(
            4.5,
          );
  });

  it("uses the palette's own colors where it can", () => {
    const p = PRESETS.find((x) => x.key === "pantone") as Palette;
    expect(derive(p, "light").desk).toBe(p.main);
    expect(derive(p, "light").accent).toBe(p.accent);
    expect(derive(p, "dark").bar).toBe(p.main);
    expect(derive(p, "dark").panel).toBe(p.base);
  });

  it("gives Classic theme.css's own values", () => {
    const classic = find({ name: "classic", custom: [] });
    expect(tokensFor(classic, "light").desk).toBe("#e8899a");
    expect(tokensFor(classic, "dark").desk).toBe("#141213");
  });
});

describe("readable", () => {
  it("leaves a color that's already readable alone", () => {
    expect(readable("#000000", ["#ffffff"], 7)).toBe("#000000");
  });
  it("nudges a color just enough", () => {
    const c = readable("#777777", ["#ffffff"], 7);
    expect(contrast(c, "#ffffff")).toBeGreaterThanOrEqual(7);
    expect(c).not.toBe("#000000");
  });
});

describe("find", () => {
  const mine = { name: "mine", base: "#101010", main: "#abcdef", accent: "#ff0000" };
  it("finds presets and imported themes", () => {
    expect(find({ name: "klein", custom: [] }).name).toBe("Klein");
    expect(find({ name: "mine", custom: [mine] }).main).toBe("#abcdef");
  });
  it("falls back to Classic for unknown or broken themes", () => {
    expect(find({ name: "gone", custom: [] }).key).toBe("classic");
    expect(find({ name: "mine", custom: [{ ...mine, main: "blue" }] }).key).toBe("classic");
  });
});

describe("badName", () => {
  const custom = [{ name: "Mine" }];
  it("needs a new, non-preset name", () => {
    expect(badName("  ", custom)).toBe("give it a name");
    expect(badName("klein", custom)).toBe("that's a preset's name");
    expect(badName("KLEIN", custom)).toBe("that's a preset's name");
    expect(badName("mine", custom)).toBe("already saved");
    expect(badName("mine", custom, "Mine")).toBe("");
    expect(badName("Sunset", custom)).toBe("");
  });
});

describe("parseColors", () => {
  it("reads a Coolors link", () => {
    expect(parseColors("https://coolors.co/2b2a30-7479d8-e94b3c")).toEqual([
      "#2b2a30",
      "#7479d8",
      "#e94b3c",
    ]);
  });
  it("reads a Lospec .hex list and hex codes in prose", () => {
    expect(parseColors("FF5FA2\r\nffe14d\n1d1b2e\n")).toEqual(["#ff5fa2", "#ffe14d", "#1d1b2e"]);
    expect(parseColors("bg #FFF, text #1c1a1b; accent: #d4586f.")).toEqual([
      "#ffffff",
      "#1c1a1b",
      "#d4586f",
    ]);
  });
  it("skips duplicates and things that aren't colors", () => {
    expect(parseColors("#abc #aabbcc abc 1234567 #12345 hello")).toEqual(["#aabbcc"]);
  });
});

describe("suggest", () => {
  it("picks base, main and accent", () => {
    expect(suggest(["#e94b3c", "#7479d8", "#2b2a30"])).toEqual({
      base: "#2b2a30",
      main: "#7479d8",
      accent: "#e94b3c",
    });
  });
  it("needs three colors", () => {
    expect(suggest(["#000000", "#ffffff"])).toBeNull();
  });
});
