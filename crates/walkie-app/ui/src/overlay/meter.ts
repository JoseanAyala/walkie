// The overlay's scrolling wave and "can't hear you" hint. Pure, so the
// scaling and timing are unit-tested (meter.test.ts).

/** Columns of history in the wave; one per TICK_MS, so about 2s. */
export const COLUMNS = 24;
export const TICK_MS = 80;
/** A column reaches up to this many pixels each side of the center line. */
export const REACH = 5;

// Loudness range the wave spans: room noise sits near the floor, speech
// roughly -40 to -20 dB, so quiet speech still moves it.
const FLOOR_DB = -60;
const CEIL_DB = -15;
/** Louder than this counts as hearing something (above a quiet room). */
const HEARD_DB = -45;
/** No sound at all for this long after recording starts: say so. */
export const DEAF_MS = 2000;

const db = (rms: number) => (rms > 0 ? 20 * Math.log10(rms) : -Infinity);

/** An input level (RMS) on a loudness scale, 0 (floor) to 1 (loud). */
export function loudness(rms: number): number {
  const t = (db(rms) - FLOOR_DB) / (CEIL_DB - FLOOR_DB);
  return Math.min(1, Math.max(0, t));
}

/** Whether an input level is more than a quiet room. */
export const heard = (rms: number) => db(rms) > HEARD_DB;

/** The wave with `v` (0–1) added on the right, the oldest column dropped. */
export function push(wave: number[], v: number, n = COLUMNS): number[] {
  const out = [...wave, v];
  return out.length > n ? out.slice(out.length - n) : out;
}

/** How far a column reaches each side of the center line, 0 (flat) to REACH. */
export const reach = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * REACH);

/**
 * Recording but nothing heard since it started, for DEAF_MS: a muted or
 * disconnected mic, or a headset that isn't sending audio. Pausing mid-
 * dictation doesn't count; once something was heard, it's fine.
 */
export const deaf = (startedAt: number, heardAny: boolean, now: number) =>
  !heardAny && now - startedAt >= DEAF_MS;
