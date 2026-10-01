import { describe, expect, it } from "vitest";
import type { Check } from "@/lib/api";
import { fixLabel, problems } from "./problems";

const check = (id: string, ok: boolean, detail = ""): Check => ({
  id,
  label: id,
  ok,
  detail,
  fix: ok ? null : id,
});
const ids = (cs: Check[]) => cs.map((c) => c.id);

describe("problems", () => {
  it("lists only what's failing", () => {
    expect(ids(problems([check("mic", true), check("device", false)]))).toEqual(["device"]);
  });

  it("leaves the model and Apple Intelligence to their own pages", () => {
    expect(problems([check("model", false), check("apple-ai", false)])).toEqual([]);
  });

  it("folds the hook and Input Monitoring into Accessibility", () => {
    const cs = [check("accessibility", false), check("input", false), check("hook", false)];
    expect(ids(problems(cs))).toEqual(["accessibility"]);
  });

  it("keeps a broken hook when Accessibility is granted", () => {
    const cs = [check("accessibility", true), check("hook", false)];
    expect(ids(problems(cs))).toEqual(["hook"]);
  });

  it("lists Linux's keyboard-access check like any other failing row", () => {
    // No "accessibility" row on Linux, so keyboard-access (and a broken
    // hook alongside it) isn't folded away.
    const cs = [check("keyboard-access", false), check("hook", false)];
    expect(ids(problems(cs))).toEqual(["keyboard-access", "hook"]);
  });
});

describe("fixLabel", () => {
  it("asks for a microphone macOS hasn't asked about", () => {
    expect(fixLabel(check("mic", false, "not asked yet — Fix asks now"))).toBe("Allow");
    expect(fixLabel(check("mic", false, "denied"))).toBe("Fix");
    expect(fixLabel(check("accessibility", false))).toBe("Fix");
  });

  it("falls back to Fix for Linux's keyboard-access check", () => {
    expect(fixLabel(check("keyboard-access", false, "can't open /dev/input/event*"))).toBe("Fix");
  });
});
