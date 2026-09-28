// Themes: three colors in, every token theme.css uses out. Pure, so the
// shades and their contrast are unit-tested (palette.test.ts).

export type Mode = "light" | "dark";

export const TOKENS = [
  "desk",
  "desk-dot",
  "panel",
  "paper",
  "fg",
  "muted",
  "line",
  "bar",
  "bar-fg",
  "chip",
  "chip-fg",
  "tag",
  "accent",
  "accent-fg",
  "warn",
] as const;
export type Tokens = Record<(typeof TOKENS)[number], string>;

/** What a theme is made of; the same shape as a saved imported theme. */
export interface Palette {
  /** the dark color: text and lines in light mode, the desk in dark */
  base: string;
  /** the desk in light mode, title bars and labels in dark */
  main: string;
  /** hover, focus and warnings */
  accent: string;
}

export interface Preset extends Palette {
  key: string;
  name: string;
  feel: string;
  /** hand-tuned tokens instead of derived ones (Classic keeps its look) */
  fixed?: Record<Mode, Tokens>;
}

// Classic is theme.css's own values, so the first paint (before the config
// loads) and the default theme look the same.
const CLASSIC: Record<Mode, Tokens> = {
  light: {
    desk: "#e8899a",
    "desk-dot": "#1c1a1b",
    panel: "#dcdcdc",
    paper: "#f3f3f3",
    fg: "#1c1a1b",
    muted: "#5b5557",
    line: "#1c1a1b",
    bar: "#1c1a1b",
    "bar-fg": "#f3f3f3",
    chip: "#1c1a1b",
    "chip-fg": "#f3f3f3",
    tag: "#c4c4c4",
    accent: "#d4586f",
    "accent-fg": "#f3f3f3",
    warn: "#d4586f",
  },
  dark: {
    desk: "#141213",
    "desk-dot": "#e8899a",
    panel: "#221f21",
    paper: "#1a1819",
    fg: "#ebe6e8",
    muted: "#a0979b",
    line: "#ebe6e8",
    bar: "#e8899a",
    "bar-fg": "#141213",
    chip: "#e8899a",
    "chip-fg": "#141213",
    tag: "#3a3538",
    accent: "#e8899a",
    "accent-fg": "#141213",
    warn: "#e8899a",
  },
};

export const DEFAULT_THEME = "classic";

export const PRESETS: Preset[] = [
  {
    key: "classic",
    name: "Classic",
    feel: "the original pink",
    base: "#1c1a1b",
    main: "#e8899a",
    accent: "#d4586f",
    fixed: CLASSIC,
  },
  {
    key: "pantone",
    name: "Pantone",
    feel: "black sea, blueberry, cherry tomato",
    base: "#2b2a30",
    main: "#7479d8",
    accent: "#e94b3c",
  },
  {
    key: "riso",
    name: "Riso",
    feel: "risograph print, loud",
    base: "#1d1b2e",
    main: "#ff5fa2",
    accent: "#ffe14d",
  },
  {
    key: "terminal",
    name: "Terminal",
    feel: "old CRT",
    base: "#101410",
    main: "#4af626",
    accent: "#ffb000",
  },
  {
    key: "matcha",
    name: "Matcha",
    feel: "calm, easy on the eyes",
    base: "#1f2a24",
    main: "#9cb89a",
    accent: "#e8632b",
  },
  {
    key: "klein",
    name: "Klein",
    feel: "Bauhaus poster",
    base: "#222226",
    main: "#2b3fd6",
    accent: "#f6d743",
  },
  {
    key: "mocha",
    name: "Mocha",
    feel: "warm, 70s",
    base: "#2a211d",
    main: "#d9b99b",
    accent: "#2a9d8f",
  },
  {
    key: "lilac",
    name: "Lilac",
    feel: "soft pastel",
    base: "#2b1f33",
    main: "#b9a6e0",
    accent: "#7fe0b5",
  },
];

// ---- color math ----

const WHITE = "#ffffff";
const BLACK = "#000000";
const HEX = /^#[0-9a-f]{6}$/;

export const isHex = (s: string) => HEX.test(s);

function rgb(h: string): [number, number, number] {
  return [1, 3, 5].map((i) => Number.parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
}
const hex = (c: number[]) =>
  `#${c
    .map((v) =>
      Math.round(Math.min(255, Math.max(0, v)))
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`;

/** `a` moved `t` (0–1) of the way to `b`. */
export function mix(a: string, b: string, t: number) {
  const x = rgb(a);
  const y = rgb(b);
  return hex(x.map((v, i) => v + ((y[i] as number) - v) * t));
}

export function luminance(h: string) {
  const [r, g, b] = rgb(h).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG contrast ratio, 1–21. */
export function contrast(a: string, b: string) {
  const x = luminance(a);
  const y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

const chroma = (h: string) => {
  const c = rgb(h);
  return (Math.max(...c) - Math.min(...c)) / 255;
};

/**
 * `fg`, darkened or lightened just enough to read at `min` contrast on every
 * one of `bgs` — or as close as black or white get.
 */
export function readable(fg: string, bgs: string[], min: number) {
  const worst = (c: string) => Math.min(...bgs.map((b) => contrast(c, b)));
  if (worst(fg) >= min) return fg;
  // the smaller nudge, towards black or towards white
  let best: [number, string] | null = null;
  for (const to of [BLACK, WHITE]) {
    for (let i = 1; i <= 20; i++) {
      const c = mix(fg, to, i / 20);
      if (worst(c) >= min) {
        if (!best || i < best[0]) best = [i, c];
        break;
      }
    }
  }
  if (best) return best[1];
  return worst(BLACK) >= worst(WHITE) ? BLACK : WHITE;
}

/** Dark or light text for a solid `bg`, always at least 4.5:1. */
const textOn = (bg: string, ink: string, paper: string) =>
  readable(contrast(ink, bg) >= contrast(paper, bg) ? ink : paper, [bg], 4.5);

// ---- themes ----

/** Every token for a palette in one mode. */
export function derive(p: Palette, mode: Mode): Tokens {
  const ink = mix(p.base, BLACK, 0.5);
  const light = "#f4f4f7";
  if (mode === "light") {
    const paper = mix(light, p.main, 0.06);
    const panel = mix(light, p.main, 0.14);
    const tag = mix(light, p.main, 0.3);
    const fg = readable(p.base, [paper, panel, tag], 7);
    return {
      desk: p.main,
      "desk-dot": contrast(p.base, p.main) >= contrast(light, p.main) ? p.base : light,
      panel,
      paper,
      fg,
      muted: readable(mix(fg, light, 0.35), [paper, panel], 4.5),
      line: fg,
      bar: fg,
      "bar-fg": textOn(fg, ink, paper),
      chip: fg,
      "chip-fg": textOn(fg, ink, paper),
      tag,
      accent: p.accent,
      "accent-fg": textOn(p.accent, ink, paper),
      warn: p.accent,
    };
  }
  // a light or mid-grey base is darkened: dark mode needs dark surfaces
  const panel = readable(p.base, [light], 11);
  const desk = mix(panel, BLACK, 0.25);
  const paper = mix(panel, BLACK, 0.15);
  const tag = mix(panel, p.main, 0.18);
  const fg = readable(mix(light, p.main, 0.1), [paper, panel, tag], 7);
  return {
    desk,
    "desk-dot": readable(p.main, [desk], 3),
    panel,
    paper,
    fg,
    muted: readable(mix(fg, panel, 0.4), [paper, panel], 4.5),
    line: fg,
    bar: p.main,
    "bar-fg": textOn(p.main, ink, light),
    chip: p.main,
    "chip-fg": textOn(p.main, ink, light),
    tag,
    accent: p.accent,
    "accent-fg": textOn(p.accent, ink, light),
    warn: p.accent,
  };
}

export interface ThemeChoice {
  name: string;
  custom: (Palette & { name: string })[];
}

/** The preset or imported theme `name` refers to; the default if neither. */
export function find(t: ThemeChoice): Preset {
  const preset = PRESETS.find((p) => p.key === t.name);
  if (preset) return preset;
  const mine = t.custom.find((c) => c.name === t.name);
  if (mine && [mine.base, mine.main, mine.accent].every(isHex)) {
    return { ...mine, key: mine.name, feel: "imported" };
  }
  return PRESETS[0] as Preset;
}

export const tokensFor = (p: Preset, mode: Mode): Tokens => p.fixed?.[mode] ?? derive(p, mode);

/** Why `name` can't be used for an imported theme, or "" if it can. */
export function badName(name: string, custom: { name: string }[], keep = "") {
  const n = name.trim().toLowerCase();
  if (!n) return "give it a name";
  if (PRESETS.some((p) => p.key === n || p.name.toLowerCase() === n))
    return "that's a preset's name";
  if (custom.some((c) => c.name.toLowerCase() === n && c.name !== keep)) return "already saved";
  return "";
}

// ---- import ----

/**
 * The colors in pasted text: `#rrggbb` / `#rgb`, or bare `rrggbb` as in a
 * Coolors link (coolors.co/2b2a30-7479d8-e94b3c) or a Lospec .hex list.
 * Lowercased, deduplicated, in order.
 */
export function parseColors(text: string): string[] {
  const out: string[] = [];
  for (const word of text.toLowerCase().split(/[^0-9a-z#]+/)) {
    let m = /^#?([0-9a-f]{6})$/.exec(word)?.[1];
    const short = /^#([0-9a-f]{3})$/.exec(word)?.[1];
    if (short) m = [...short].map((c) => c + c).join("");
    if (m && !out.includes(`#${m}`)) out.push(`#${m}`);
  }
  return out;
}

/**
 * Roles for imported colors: the darkest is the base, the most colorful of
 * the rest the accent, and of what's left the one that stands out most from
 * the base the main color. Null with fewer than three colors.
 */
export function suggest(colors: string[]): Palette | null {
  if (colors.length < 3) return null;
  const base = colors.reduce((a, b) => (luminance(b) < luminance(a) ? b : a));
  const rest = colors.filter((c) => c !== base);
  const accent = rest.reduce((a, b) => (chroma(b) > chroma(a) ? b : a));
  const others = rest.filter((c) => c !== accent);
  const main = others.reduce((a, b) => (contrast(b, base) > contrast(a, base) ? b : a));
  return { base, main, accent };
}
