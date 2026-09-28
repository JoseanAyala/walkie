// Puts a window in light or dark: theme.css holds both sets of colors, keyed
// by <html data-theme>. Before the config loads, macOS's own mode applies.
import { type Appearance, api, on, type ThemeCfg } from "@/lib/api";
import { onSystemFlip, system } from "@/lib/system.svelte";

type Mode = "light" | "dark";
let current: ThemeCfg | null = null;

/** The mode a theme shows in right now (reactive in components). */
export const modeOf = (a: Appearance): Mode =>
  a === "system" ? (system.dark ? "dark" : "light") : a;

export function applyTheme(t: ThemeCfg) {
  current = t;
  const mode = modeOf(t.appearance);
  const root = document.documentElement;
  root.style.colorScheme = mode;
  root.dataset.theme = mode;
}

/** Follows the saved theme: now, when it's saved, and when macOS flips. */
export function followTheme() {
  api.getConfig().then(
    (c) => applyTheme(c.theme),
    () => {}, // keep macOS's mode
  );
  on("theme", applyTheme);
  onSystemFlip(() => {
    if (current?.appearance === "system") applyTheme(current);
  });
}
