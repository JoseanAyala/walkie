import { describe, expect, it } from "vitest";
import type { Check } from "@/lib/api";
import { verdict } from "./checks";

const check = (id: string, ok: boolean, detail = ""): Check => ({
  id,
  label: id,
  ok,
  detail,
  fix: null,
});

describe("verdict", () => {
  const checks = [
    check("mic", false, "not asked yet — it's requested on your first dictation"),
    check("accessibility", true, "granted"),
  ];

  it("reads ok and absent", () => {
    expect(verdict(checks, "accessibility")).toBe("ok");
    expect(verdict(checks, "globe")).toBe("absent");
  });

  it("treats a microphone that was never asked for as pending", () => {
    expect(verdict(checks, "mic")).toBe("pending");
  });

  it("treats a denied microphone as missing", () => {
    expect(verdict([check("mic", false, "denied — walkie records silence")], "mic")).toBe(
      "missing",
    );
  });
});
