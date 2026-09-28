// Every Tauri command and event the UI uses, typed in one place. The shapes
// mirror crates/walkie-app/src/commands.rs and walkie_core::config.
import { invoke } from "@tauri-apps/api/core";
import { type EventCallback, listen } from "@tauri-apps/api/event";

export type HotkeyName = "dictate" | "polish" | "paste_last";

export interface Config {
  first_run: boolean;
  language: string;
  model: string;
  hotkeys: Record<HotkeyName, string[]>;
  polish: Polish;
  inject: { strategy: string; restore_clipboard_ms: number };
  history: { enabled: boolean };
  audio: { input_device: string; duck_while_recording: boolean; duck_percent: number };
  theme: ThemeCfg;
}

export type PolishProvider = "apple" | "command";

/** How Apple's model writes; the prompts behind the tones are built in. */
export type Tone = "formal" | "casual" | "very_casual" | "excited";

export interface Polish {
  provider: PolishProvider;
  tone: Tone;
  command: string;
  timeout_secs: number;
}

/** Whether Apple's on-device model can polish: a walkie-ai status word
 * ("available", "off", "not-ready", …) and what it means. */
export interface AppleAi {
  status: string;
  detail: string;
}

export type Appearance = "system" | "light" | "dark";

export interface CustomTheme {
  name: string;
  base: string;
  main: string;
  accent: string;
}

export interface ThemeCfg {
  /** a preset key or a custom theme's name */
  name: string;
  appearance: Appearance;
  custom: CustomTheme[];
}

export interface Check {
  id: string;
  label: string;
  ok: boolean;
  detail: string;
  /** a pane for open_settings_pane, or "restart" */
  fix: string | null;
}

export interface HistoryRecord {
  id: number;
  created_at: string;
  raw: string;
  cleaned: string;
  polished: string | null;
  lang: string | null;
  duration_ms: number;
}

export interface ModelChoice {
  key: string;
  note: string;
  size_mb: number;
  memory_mb: number;
  downloaded: boolean;
}

export interface LoginItem {
  status: string;
  on: boolean;
  hint: string | null;
}

export interface InputDevices {
  default: string | null;
  devices: string[];
}

export type Pane = "mic" | "accessibility" | "input" | "keyboard" | "sound" | "loginitems" | "ai";

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (cfg: Config) => invoke<void>("save_config", { cfg }),
  getStatus: () => invoke<Check[]>("get_status"),
  listModels: () => invoke<ModelChoice[]>("list_models"),
  listInputDevices: () => invoke<InputDevices>("list_input_devices"),
  recordShortcut: () => invoke<void>("record_shortcut"),
  restartApp: () => invoke<void>("restart_app"),
  historyRecent: (limit: number) => invoke<HistoryRecord[]>("history_recent", { limit }),
  historySearch: (q: string, limit: number) =>
    invoke<HistoryRecord[]>("history_search", { q, limit }),
  historyDelete: (id: number) => invoke<boolean>("history_delete", { id }),
  historyClear: () => invoke<number>("history_clear"),
  testPolish: (polish: Polish) => invoke<string>("test_polish", { polish }),
  appleAiStatus: () => invoke<AppleAi>("apple_ai_status"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  openSettingsPane: (pane: Pane | string) => invoke<void>("open_settings_pane", { pane }),
  getLaunchAtLogin: () => invoke<LoginItem>("get_launch_at_login"),
  setLaunchAtLogin: (enabled: boolean) => invoke<LoginItem>("set_launch_at_login", { enabled }),
  finishOnboarding: (launchAtLogin: boolean) =>
    invoke<void>("finish_onboarding", { launchAtLogin }),
};

export type SessionState = "idle" | "recording" | "transcribing" | "polishing" | "injecting";

/** Events the Rust side emits, and their payloads. */
export interface Events {
  state: SessionState;
  level: number;
  "app-error": string;
  "app-notice": string;
  /** a dictation finished; its final text */
  transcribed: string;
  "model-status": string;
  "download-progress": number;
  "shortcut-recorded": string[];
  "shortcut-error": string;
  "shortcut-cancelled": null;
  "show-tab": string;
  /** the saved theme changed (sent to every window) */
  theme: ThemeCfg;
}

export function on<K extends keyof Events>(name: K, f: (payload: Events[K]) => void) {
  const cb: EventCallback<Events[K]> = (e) => f(e.payload);
  return listen(name, cb);
}

/** Error messages from commands come back as plain strings. */
export const message = (e: unknown) => (e instanceof Error ? e.message : String(e));
