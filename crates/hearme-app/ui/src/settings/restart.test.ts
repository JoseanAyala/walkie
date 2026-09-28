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
  cleanup: { enabled: true, fillers_en: ["um"], fillers_es: ["eh"] },
  polish: { command: "", timeout_secs: 60 },
  inject: { strategy: "paste", restore_clipboard_ms: 300 },
  history: { enabled: true },
  audio: { input_device: "", duck_while_recording: true, duck_percent: 30 },
});

describe("pendingRestart", () => {
  it("is empty when nothing changed", () => {
    expect(pendingRestart(cfg(), cfg())).toEqual([]);
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
    now.cleanup.fillers_en = ["um", "uh"];
    expect(pendingRestart(cfg(), now)).toEqual(["model", "ducking", "cleanup"]);
  });
});
