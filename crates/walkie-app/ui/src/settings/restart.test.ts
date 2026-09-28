import { describe, expect, it } from "vitest";
import type { Config } from "@/lib/api";
import { pendingRestart } from "./restart";

const cfg = (): Config => ({
  first_run: false,
  language: "auto",
  model: "base",
  hotkeys: {
    dictate: ["Fn"],
    polish: ["Fn", "Shift"],
    hands_free: ["Fn", "Space"],
    paste_last: [],
  },
  polish: {
    provider: "command",
    tone: "formal",
    command: "",
    timeout_secs: 60,
  },
  inject: { strategy: "paste", restore_clipboard_ms: 300 },
  history: { enabled: true },
  audio: { input_device: "", duck_while_recording: true, duck_percent: 30 },
  theme: { name: "classic", appearance: "system", custom: [] },
});

describe("pendingRestart", () => {
  it("is empty when nothing changed", () => {
    expect(pendingRestart(cfg(), cfg())).toEqual([]);
  });

  it("applies a theme change live", () => {
    const now = cfg();
    now.theme = { name: "klein", appearance: "dark", custom: [] };
    expect(pendingRestart(cfg(), now)).toEqual([]);
  });

  it("ignores settings that apply live", () => {
    const now = cfg();
    now.hotkeys.dictate = ["RightOpt"];
    now.audio.input_device = "Shure MV7";
    expect(pendingRestart(cfg(), now)).toEqual([]);
  });

  it("names each changed group once", () => {
    const now = cfg();
    now.model = "large-v3-turbo-q5_0";
    now.audio.duck_percent = 50;
    now.audio.duck_while_recording = false;
    expect(pendingRestart(cfg(), now)).toEqual(["model", "ducking"]);
  });

  it("needs a restart for the polish provider and tone", () => {
    const now = cfg();
    now.polish.provider = "apple";
    expect(pendingRestart(cfg(), now)).toEqual(["polish"]);
    const tone = cfg();
    tone.polish.tone = "excited";
    expect(pendingRestart(cfg(), tone)).toEqual(["polish"]);
  });
});
