// Keyboard rules for Select.svelte's list, as in a native popup menu. Pure,
// so they're unit-tested (listbox.test.ts).

export interface Option {
  value: string;
  text: string;
}

/** Where the highlight goes for a navigation key; null if it isn't one. */
export function move(active: number, key: string, count: number): number | null {
  if (!count) return null;
  switch (key) {
    case "ArrowDown":
      return Math.min(active + 1, count - 1);
    case "ArrowUp":
      return Math.max(active - 1, 0);
    case "Home":
    case "PageUp":
      return 0;
    case "End":
    case "PageDown":
      return count - 1;
    default:
      return null;
  }
}

/**
 * Typing a letter jumps to the next option starting with it, wrapping
 * around, like a native popup; `from` itself is checked last. -1 if none.
 */
export function typeahead(options: Option[], from: number, key: string): number {
  if (key.length !== 1 || !key.trim()) return -1;
  const k = key.toLowerCase();
  const n = options.length;
  for (let i = 1; i <= n; i++) {
    const j = (from + i) % n;
    if (options[j]?.text.toLowerCase().startsWith(k)) return j;
  }
  return -1;
}
