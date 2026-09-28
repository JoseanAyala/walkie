// Settings-window state shared by every tab: the config being edited, the
// autosave and the error banner.
import { api, type Config, message } from "@/lib/api";

class Settings {
  cfg = $state<Config | null>(null);
  error = $state("");
  saved = $state(false);
  /** counts saves, so each one replays the `saved ✓` blink */
  saves = $state(0);
  #savedTimer: ReturnType<typeof setTimeout> | undefined;
  #typing: ReturnType<typeof setTimeout> | undefined;

  async load() {
    try {
      this.cfg = await api.getConfig();
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
      this.saves++;
      clearTimeout(this.#savedTimer);
      this.#savedTimer = setTimeout(() => {
        this.saved = false;
      }, 1200);
    } catch (e) {
      this.fail(e);
    }
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
