// Paints the chosen theme into a window: every token becomes an inline CSS
// variable on <html>, overriding theme.css (whose values are Classic, so
// the first paint before the config loads already looks right).
import { api, on, type ThemeCfg } from "@/lib/api";
import { find, type Mode, TOKENS, tokensFor } from "@/lib/palette";
import { onSystemFlip, system } from "@/lib/system.svelte";

let current: ThemeCfg | null = null;

/** The mode a theme shows in right now (reactive in components). */
export const modeOf = (t: ThemeCfg): Mode =>
  t.appearance === "system" ? (system.dark ? "dark" : "light") : t.appearance;

export function applyTheme(t: ThemeCfg) {
  current = t;
  const mode = modeOf(t);
  const tokens = tokensFor(find(t), mode);
  const root = document.documentElement;
  for (const k of TOKENS) root.style.setProperty(`--${k}`, tokens[k]);
  root.style.colorScheme = mode;
  root.dataset.theme = mode;
}

/** Follows the saved theme: now, when it's saved, and when macOS flips. */
export function followTheme() {
  api.getConfig().then(
    (c) => applyTheme(c.theme),
    () => {}, // keep theme.css's colors
  );
  on("theme", applyTheme);
  onSystemFlip(() => {
    if (current?.appearance === "system") applyTheme(current);
  });
}
