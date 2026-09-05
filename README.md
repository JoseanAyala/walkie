# hearme 🎙️

Local-only dictation for any agent. Hold a key, speak (English or Spanish),
and the transcription is typed into whatever window has focus — Claude Code,
Cursor, a browser, anything. Whisper runs on-device; audio never leaves your
machine.

## Use

- **Hold Right Option** — speak, release → text appears (< 1s).
- **Double-tap Right Option** — locked recording for long dictation; tap to stop.
- **Hold Shift + Right Option** — dictate + polish: the transcript is piped
  through your configured command (`claude -p`, `codex exec`, ollama, …).
- Tray icon: gray idle · red recording · amber working.

## Build

```sh
cargo install tauri-cli --locked
cargo run -p hearme-app --example gen_icons   # once
cd crates/hearme-app && cargo tauri dev        # dev
cargo tauri build                              # bundles hearme.app
```

First launch downloads the Whisper model (~570MB) to `~/.cache/hearme/models`.
macOS permissions needed: Microphone, Accessibility, Input Monitoring
(the onboarding window has deep links).

## Config

`~/.config/hearme/config.toml` — created on first run. Polish example:

```toml
[polish]
command = "claude -p 'Clean up this dictated text. Output only the cleaned text.'"
timeout_secs = 60
```

History lives in `~/.local/share/hearme/history.sqlite3` (off switch in
settings). Failed transcriptions keep their audio in `~/.cache/hearme/spool/`.

## Tests

```sh
cargo test -p hearme-core                       # fast unit tests
cargo run -p hearme-core --example fetch_model -- base
cargo test -p hearme-core --features stt-tests  # real STT on en/es fixtures
```
