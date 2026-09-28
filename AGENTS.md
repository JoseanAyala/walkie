# AGENTS.md

This file provides guidance agents when working with code in this repository.

walkie is a macOS menu-bar dictation app (Rust + Tauri 2): hold a hotkey, speak, and on-device Whisper transcribes into the focused window. See README.md for user-facing behavior and config.

## Commands

Toolchain is pinned via `mise install` (Rust, tauri-cli, actionlint). `make help` lists everything.

```sh
make check        # fmt-check + clippy (-D warnings) + unit tests — what CI runs
make test-stt     # real Whisper on en/es fixtures (cargo test -p walkie-core --features stt-tests)
make test-ai      # Apple's real on-device model (cargo test -p walkie-app --features ai-tests)
make test-e2e     # in-process e2e with real Whisper (cargo test -p walkie-e2e)
make test-app     # OS-level e2e against installed /Applications/Walkie.app (~30s, don't type)
make run          # cargo tauri dev
make dev          # build + install + relaunch + follow log (make debug adds key-event logging)
make model        # download base Whisper model (needed by test-stt / test-e2e)
make hooks        # enable the pre-commit hook (.githooks: make fmt, then make lint)
```

Single tests:
- Unit: `cargo test -p walkie-core <name>`
- In-process e2e: `cargo test -p walkie-e2e --test dictation <name>`
- OS e2e: `e2e/run-app-tests.sh <name-filter>`

Plain `cargo test` only covers `walkie-core` and `walkie-app` (workspace `default-members`); the `e2e` crate runs only when asked for with `-p walkie-e2e`.

## Architecture

Workspace crates:
- **`crates/walkie-core`** — all dictation logic, no UI imports (enforced by convention, see `lib.rs`). Modules: `hotkey` (key → signal), `audio` (cpal capture, DSP, volume ducking), `stt` (whisper-rs, Metal), `pipeline` (session state machine, polish via walkie-ai or an external command, polish tones), `inject` (typing/pasting into the focused app), `config` (TOML + model registry), `history` (SQLite).
- **`crates/walkie-app`** — Tauri shell: tray, windows (a Svelte UI in `ui/`, built by Vite into `dist/`; import across folders with `@/` = `ui/src/`, since oxlint rejects `../` imports), `commands.rs` (Tauri IPC), `glue.rs` (wires core into the app: builds the `Session`, forwards its `Event`s to the tray/overlay), `login_item.rs` (also a `walkie --login-item` CLI).
- **`crates/walkie-app/swift/walkie-ai.swift`** — a Swift CLI for Apple's on-device model (FoundationModels), built by `walkie-app/build.rs` and bundled as a Tauri `externalBin`. `pipeline::polish` runs it (`status`, `respond <prompt>` with text on stdin) for the opt-in Apple polish provider; a separate process so the timeout can kill it. `make fmt`/`lint` run swift-format on it.
- **`e2e`** — two test layers (see below).

Data flow: macOS event tap (`hotkey/tap.rs`) → `hotkey::engine::Engine` (pure, no OS calls; turns key events into `Signal`s, decides what to swallow) → `Command::from_signal` → `pipeline::session::Session` worker thread (Idle → Recording → Transcribing → [Polishing] → Injecting) → emits `Event`s (state, level, done, error, notice) back to the app.

Key design points:
- The hotkey `Engine`/`HotkeyMachine` are deliberately pure so gestures (hold, double-tap lock, stray-key cancel, paste-last) are unit-testable. Timing constants (`HOLD_MIN_MS`, `TAP_WINDOW_MS`, `STRAY_CANCEL_MS`) carry comments explaining measured macOS behavior — keep them in sync.
- The session takes its OS edges as trait objects via `Deps` (`Capture`, `Injector`, `SttEngine`, `Ducker`/volume). Tests substitute `FileCapture` (WAV), a fake `Injector`, `MemVolume`.
- Keystroke synthesis must run on the main thread (`inject::MainThread`); off-main enigo calls crash after an input-source change.
- Tauri windows are declared once in `tauri.conf.json`; close requests hide instead of destroy.
- `WALKIE_TEST_AUDIO` (play a WAV instead of the mic) is only honored in builds with the `test-hooks` feature. `e2e/run-app-tests.sh` installs such a build; `make install` restores a normal one.

## Tests

- `e2e/src/lib.rs` `Rig`: in-process harness — real `Engine`, `Session`, Whisper; only mic (WAV fixture) and focused app (Vec) are faked.
- `e2e/tests/app*.rs` (feature `os-tests`): drives the real installed app via real key events, tray menu, windows, and a `typing-target` window. Runs with `--test-threads=1`. Each test has a 10s watchdog (`TEST_BUDGET` in `e2e/src/os.rs`) that aborts and reports where it got stuck. Needs Accessibility for walkie and the terminal, and "Press 🌐 key to" = Do Nothing.
- Features ship with unit tests plus both e2e layers; keep every test under 10s.

## Build gotchas

- `bundle.macOS.minimumSystemVersion` in `tauri.conf.json` must stay `10.15` (whisper.cpp's `std::filesystem` doesn't link below it).
- walkie-ai must never fail the build: older SDKs compile it via `#if canImport(FoundationModels)` to report "unsupported", and if Swift is missing or fails, `build.rs` warns and bundles a shell stub that does the same. CI runs on `macos-26` so releases include the real model.
- `.cargo/config.toml` sets `GGML_NATIVE=OFF` (no `-mcpu=native`; CI's clang rejects it).
- Local builds sign with the "walkie local signing" identity (`make cert`, once per machine) so macOS permission grants survive rebuilds. `walkie.entitlements` must keep `audio-input`.
- Release: `make release VERSION=x.y.z` bumps, checks, commits, tags `v*`; the release workflow ships an ad-hoc-signed zip (`scripts/package.sh`).
