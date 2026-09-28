import type { Check } from "@/lib/api";

// Shown where they matter instead: the speech model by its picker, Apple
// Intelligence on the Polish page.
const ELSEWHERE = ["model", "apple-ai"];
// Without Accessibility the keyboard hook can't start, and Input Monitoring
// reads as missing too: granting Accessibility fixes all three.
const FOLLOW_ACCESSIBILITY = ["hook", "input"];

/** The failing checks worth a row at the top of Settings, in order. */
export function problems(checks: Check[]): Check[] {
  const failing = checks.filter((c) => !c.ok && !ELSEWHERE.includes(c.id));
  if (!failing.some((c) => c.id === "accessibility")) return failing;
  return failing.filter((c) => !FOLLOW_ACCESSIBILITY.includes(c.id));
}

/** macOS hasn't asked for the microphone yet: the button asks, not fixes. */
export const fixLabel = (c: Check) =>
  c.id === "mic" && /not asked/.test(c.detail) ? "Allow" : "Fix";
