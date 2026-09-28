import type { Check } from "@/lib/api";

export type Verdict = "ok" | "pending" | "missing" | "absent";

/** The permission rows onboarding shows, in order. */
export const PERMS = ["mic", "accessibility", "globe"] as const;

/**
 * One onboarding row's state. The microphone is only asked for on the
 * first dictation, so "not asked yet" is pending, not missing; the globe
 * check only exists when a shortcut uses fn.
 */
export function verdict(checks: Check[], id: string): Verdict {
  const c = checks.find((x) => x.id === id);
  if (!c) return "absent";
  if (c.ok) return "ok";
  if (c.id === "mic" && /not asked/.test(c.detail)) return "pending";
  return "missing";
}
