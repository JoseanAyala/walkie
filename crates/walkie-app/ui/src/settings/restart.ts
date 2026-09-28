import type { Config } from "@/lib/api";

// Only the shortcuts and the microphone are applied live; everything else
// is read once at launch. Each entry names a group of such settings.
const RESTART: Record<string, (c: Config) => unknown> = {
  language: (c) => c.language,
  model: (c) => c.model,
  injection: (c) => c.inject.strategy,
  ducking: (c) => [c.audio.duck_while_recording, c.audio.duck_percent],
  polish: (c) => c.polish,
  history: (c) => c.history.enabled,
};

/** The groups that differ between what the app launched with and `now`. */
export function pendingRestart(boot: Config, now: Config): string[] {
  return Object.entries(RESTART)
    .filter(([, f]) => JSON.stringify(f(boot)) !== JSON.stringify(f(now)))
    .map(([k]) => k);
}
