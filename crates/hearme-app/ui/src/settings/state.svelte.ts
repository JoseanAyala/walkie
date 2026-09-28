// Settings-window state shared by every tab: the config being edited, the
// autosave, the restart hint and the error banner.
import { api, type Config, message } from "@/lib/api";
import { pendingRestart } from "./restart";

class Settings {
  cfg = $state<Config | null>(null);
  error = $state("");
  saved = $state(false);
  pending = $state<string[]>([]);
  // What the running app launched with. This page loads at launch too (the
  // window is hidden, never destroyed), so the first read is that config.
  #boot: Config | null = null;
  #savedTimer: ReturnType<typeof setTimeout> | undefined;
  #typing: ReturnType<typeof setTimeout> | undefined;

  async load() {
    try {
      const c = await api.getConfig();
      this.#boot = structuredClone(c);
      this.cfg = c;
    } catch (e) {
      this.fail(e);
    }
  }

  async save() {
    clearTimeout(this.#typing);
    const cfg = this.cfg;
    if (!cfg) return;
    const snapshot = $state.snapshot(cfg) as Config;
    try {
      // The rest of the config is saved even when a shortcut is rejected.
      await api.saveConfig(snapshot);
      this.saved = true;
      clearTimeout(this.#savedTimer);
      this.#savedTimer = setTimeout(() => {
        this.saved = false;
      }, 1200);
    } catch (e) {
      this.fail(e);
    }
    if (this.#boot) this.pending = pendingRestart(this.#boot, snapshot);
  }

  /** For typing and dragging: save once the person pauses. */
  saveSoon() {
    clearTimeout(this.#typing);
    this.#typing = setTimeout(() => this.save(), 500);
  }

  fail(e: unknown) {
    this.error = message(e);
  }
}

export const settings = new Settings();
