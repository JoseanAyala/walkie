/**
 * How many of `n` meter blocks to light for an input level (RMS, roughly
 * 0–0.125 for speech). Lit blocks fall back one per update, so the meter
 * reads as a level instead of flickering with every audio buffer.
 */
export function litBlocks(level: number, prev: number, n: number): number {
  const want = Math.round(Math.min(1, Math.max(0, level) * 8) * n);
  return Math.max(want, prev - 1);
}
