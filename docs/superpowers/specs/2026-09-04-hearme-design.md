# hearme — local dictation for any agent

**Date:** 2026-09-04
**Status:** Approved

## Purpose

A local-only, Wispr Flow-style dictation app. Hold a hotkey anywhere, speak, and the transcription is typed into whatever window has focus — a terminal running Claude Code, Cursor, a browser, anything. No cloud STT, no per-agent integration. Speed is the top priority: release-to-text under 1 second for typical utterances.

## Decisions

| Question | Decision |
|---|---|
| Delivery to agents | Type into the focused app (system-wide). MCP is designed for, built later. |
| Platforms | Cross-platform Rust foundation. macOS first (daily driver, M1 Pro 16GB), Linux/NixOS fast-follow, Windows "compiles but untested". |
| Languages | English + Spanish. Whisper auto-detects per utterance; pinnable in config. |
| Activation | Both from day one: hold-to-talk and double-tap toggle-lock. |
| Cleanup | Fast rule-based cleanup built in. Optional "polish" pipes the transcript through any user-configured CLI (`claude -p`, `codex exec`, an Ollama call, …). |
| Architecture | Single binary: `hearme-core` library crate + thin Tauri v2 shell. No IPC in v1. |
| STT engine | whisper.cpp via `whisper-rs` (Metal), model `large-v3-turbo` quantized. Behind an `SttEngine` trait so Parakeet v3 can drop in later. |

## Architecture

```
hearme/
├── crates/
│   ├── hearme-core/        # library — all logic, zero UI dependencies
│   │   ├── audio/          # mic capture (cpal), resample to 16kHz mono
│   │   ├── stt/            # SttEngine trait + WhisperEngine (whisper-rs, Metal)
│   │   ├── hotkey/         # global listener; hold vs double-tap-lock state machine
│   │   ├── pipeline/       # session orchestration, rule cleanup, polish pipe
│   │   ├── inject/         # paste-based injection + keystroke fallback
│   │   ├── config/         # TOML config + model download/cache
│   │   └── history/        # SQLite transcript history
│   └── hearme-app/         # Tauri v2 shell: tray, overlay, settings window
```

**The rule that keeps future options cheap:** `hearme-core` exposes plain Rust APIs (`start_session`, `stop_session`, an event stream out) and never imports UI. The v2 MCP server is a second small binary over the same crate; a daemon split, if ever wanted, cuts along the same seam.

## Core flows

**Hold-to-talk.** Press and hold the hotkey → overlay pill appears, mic capture starts → release → Whisper transcribes the whole utterance → rule cleanup → inject → overlay fades.

**Toggle-lock.** Double-tap the hotkey → recording locks with a visible indicator → single tap stops → same pipeline. V1 transcribes at stop (a 60s dictation takes ~2–3s on the M1 Pro). Incremental silence-based chunking is a v1.x follow-up if long dictations feel slow.

**Polish.** A second hotkey means "dictate + polish": the cleaned transcript is piped to the configured command's stdin; stdout is injected. The overlay shows a "polishing…" state while the command runs. On error or timeout (default 60s, configurable), fall back to injecting the raw transcript — words are never lost.

**Default hotkeys:** hold Right Option (⌥) for dictation; hold Right Option + Shift for dictate-with-polish. Both configurable, and both support hold and double-tap toggle-lock identically — the state machine doesn't care which hotkey triggered it. (Wispr-style Fn/Globe needs special IOKit handling on macOS — v1.x, not v1.)

## Components

**audio** — `cpal` capture from the default input device into a ring buffer; resample to 16kHz mono f32 (what Whisper expects). Device hot-swap (e.g., AirPods connect) picked up between sessions.

**stt** — trait:

```rust
trait SttEngine {
    fn transcribe(&self, samples: &[f32], lang: LangHint) -> Result<Transcript>;
}
```

`WhisperEngine` implements it with `whisper-rs` (Metal on macOS, CPU/Vulkan elsewhere). Model `ggml-large-v3-turbo` quantized (~1GB on disk). `LangHint` is `Auto | En | Es | Other(code)`. Engine loads once at startup and stays resident (~1.5GB RAM) — that is the price of instant transcription and is acceptable on 16GB.

**hotkey** — global key listener (`rdev`/event-tap based). A pure state machine turns raw key events into session commands: `HoldStart`, `HoldEnd`, `ToggleStart`, `ToggleStop`, with double-tap detection (two presses within 300ms) and accidental-tap suppression (holds under 150ms are ignored). The state machine is pure logic, unit-tested against simulated event streams; the OS listener is a thin adapter.

**pipeline** — owns the session lifecycle: idle → recording → transcribing → (polishing) → injecting → idle. Applies rule cleanup: strip filler words (en: "um", "uh", "you know"; es: "este", "eh", "o sea" — configurable list), collapse repeated words from false starts, normalize spacing around punctuation. Runs the polish pipe when requested. Emits events the UI subscribes to.

**inject** — primary strategy: clipboard-paste (save current clipboard, set transcript, synthesize paste keystroke, restore clipboard after a short delay). Fast and accent-safe for Spanish. Fallback strategy (per-app configurable): synthesized keystrokes for apps that block paste. If both fail (e.g., macOS secure input), leave text on the clipboard and notify.

**config** — TOML at `~/.config/hearme/config.toml` (XDG paths everywhere; home-manager friendly). Covers: hotkeys, language pin, filler-word lists, polish commands, injection strategy overrides, history on/off, model choice. Models auto-download to the XDG cache dir on first run with progress shown in settings.

**history** — SQLite (`rusqlite`) storing timestamp, raw transcript, cleaned transcript, polished output (if any), language, duration. Browsable/searchable from the settings window. Off switch in config. Local only, never leaves the machine.

**hearme-app (Tauri v2)** — menu bar/tray icon with states (idle / listening / transcribing / polishing); a small always-on-top transparent overlay pill showing recording state and audio level; a settings window (hotkeys, model, language, cleanup, polish commands, history browser); first-run onboarding that walks through OS permissions.

## Platform specifics

**macOS (v1).** Permissions: Microphone, Accessibility (injection), Input Monitoring (global hotkey). Onboarding deep-links each System Settings pane and detects when granted. Ad-hoc code signing for personal use; hardened runtime + notarization only if distributed later. Known edge: secure-input mode blocks synthetic events → clipboard + notification fallback.

**Linux/NixOS (v1.x, same codebase).** Flake ships in v1: dev shell, package, optional home-manager module. X11: same crates work as-is. Wayland tiering: wlroots compositors (Hyprland, Sway) get injection via the virtual-keyboard protocol; GNOME/KDE get clipboard + notification until their portals allow more. Hotkeys via evdev (user in `input` group — one line in a NixOS module).

**Windows (v2).** All chosen crates support it; kept compiling, untested until it matters.

## Error handling

Every failure degrades toward "words are never lost":

| Failure | Behavior |
|---|---|
| Mic permission missing | Tray alert + deep link to the settings pane |
| Model missing/corrupt | Re-download prompt in settings; dictation disabled until resolved |
| STT error | Notification; raw audio kept in a temp spool for retry |
| Polish command error/timeout | Inject the raw transcript; subtle notification |
| Injection blocked | Text left on clipboard + notification |
| Hotkey conflict | Configurable hotkeys; conflicts surfaced in settings |

## Testing

TDD for the logic-heavy parts (per superpowers):

- **Unit:** hotkey state machine (simulated event streams: hold, double-tap, accidental taps), cleanup rules (en + es fixtures), config parsing/defaults, history queries.
- **Integration:** `SttEngine` against fixture WAVs in English and Spanish, asserting expected phrases; runs with a tiny model (`base`) so CI stays fast, gated behind a feature flag.
- **Adapters:** audio/hotkey/injection OS layers are thin traits, mocked in tests; verified by a short manual smoke checklist per platform (dictate into: terminal, browser, native app; toggle mode; polish mode; secure-input fallback).

## Scope

**V1 (daily-drive on macOS):** hold + toggle dictation, Whisper large-v3-turbo, rule cleanup, polish pipe, tray + overlay + settings + onboarding, history, TOML config, flake skeleton.

**V1.x:** NixOS/Wayland hardening, incremental transcription for long dictations, Fn/Globe hotkey support.

**V2:** MCP server binary (`listen`, `transcribe_file`, `get_recent_dictations`), Parakeet v3 engine option, Windows as a supported tier.
