// Whether macOS is in dark mode, as reactive state.
const query = matchMedia("(prefers-color-scheme: dark)");
const after: (() => void)[] = [];

export const system = $state({ dark: query.matches });
query.addEventListener("change", () => {
  system.dark = query.matches;
  for (const f of after) f();
});

/** Runs `f` whenever macOS flips, once `system.dark` is up to date. */
export const onSystemFlip = (f: () => void) => after.push(f);
