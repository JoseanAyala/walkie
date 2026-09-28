# hearme 🎙️

Local-only dictation for any agent. Hold a key, speak (English or Spanish),
and the transcription is typed into whatever window has focus — Claude Code,
Cursor, a browser, anything. Whisper runs on-device; audio never leaves your
machine.

## Use

Default shortcuts (Wispr-style; rebind any of them in Settings → General,
to any key or combo):

- **Hold fn** — speak, release → text appears (< 1s). Double-tap fn to lock.
- **fn + space** — hands-free: keeps recording after you let go; press fn to stop.
  Holding fn and then tapping space switches a running recording to hands-free.
- **Hold fn + shift** — dictate + polish: the transcript is piped
  through your configured command (`claude -p`, `codex exec`, ollama, …).
- **ctrl + cmd + V** — paste last transcript again, for when it landed in the
  wrong window (also in the tray menu).
- Tray icon: gray idle · red recording · amber working.
- **Launch at login**: Settings → General (on by default from onboarding).
  It's a real macOS login item (System Settings → General → Login Items),
  read live rather than stored in the config; it needs the installed
  `hearme.app`, not `cargo tauri dev`. `hearme --login-item status|on|off`
  does the same from a terminal.
- Microphone: Settings → General (applies to the next dictation). If the
  chosen mic is unplugged, hearme records from the system default and the
  Status tab says so.

## Build

```sh
mise install    # pinned Rust, tauri-cli, actionlint (mise.toml)
make cert       # once per machine: local signing identity
make run        # cargo tauri dev
make dev        # build + install + relaunch + follow log
make help       # every other command
```

`bundle.macOS.minimumSystemVersion` in `tauri.conf.json` is pinned to `10.15`
— don't lower it. `cargo tauri build` propagates it into
`MACOSX_DEPLOYMENT_TARGET` for the whole build graph, and whisper.cpp's use
of `std::filesystem` doesn't link below 10.15.

`cargo tauri build` signs with the self-signed "hearme local signing"
identity, so macOS permission grants survive rebuilds. Without it the bundle
is only ad-hoc signed and every rebuild silently invalidates Microphone,
Accessibility and Input Monitoring — the toggles still show on, but no
longer apply. `hearme.entitlements` carries `audio-input`; the hardened
runtime otherwise mutes the mic.

First launch downloads the Whisper model (~570MB) to `~/.cache/hearme/models`.
macOS permissions needed: Microphone, Accessibility, Input Monitoring
(the onboarding window has deep links).

## Release

CI (`.github/workflows/ci.yml`) runs fmt, clippy and unit tests on every PR
and push to `main`; pushes to `main` also build `hearme.app` and upload it as
a workflow artifact. To release:

```sh
make release VERSION=0.2.0
```

That bumps the version, runs `make check`, commits, tags `v0.2.0` and (after
a confirmation) pushes.

The release workflow builds an ad-hoc-signed zip (`scripts/package.sh`) and
attaches it to a GitHub Release. Ad-hoc means no Gatekeeper trust: first
launch needs right-click → Open, and macOS permissions must be re-granted
after each update.

## Config

`~/.config/hearme/config.toml` — created on first run. Polish example:

```toml
[polish]
command = "claude -p 'Clean up this dictated text. Output only the cleaned text.'"
timeout_secs = 60

[audio]
input_device = ""   # a device name from Settings; empty = system default
```

While you dictate, other audio is lowered (not muted) to `duck_percent` of
its volume and put back when the recording ends — unless you changed the
volume yourself meanwhile. Settings → General, or:

```toml
[audio]
duck_while_recording = true
duck_percent = 30
```

History lives in `~/.local/share/hearme/history.sqlite3` (off switch in
settings). Failed transcriptions keep their audio in `~/.cache/hearme/spool/`.

## Tests

```sh
make check      # fmt-check + clippy + unit tests (what CI runs)
make test-stt   # real STT on en/es fixtures (fetches the base model)
make test-e2e   # keystrokes → engine → session → Whisper, in-process
make test-app   # the real app: key events, tray, windows (~30s)
```

`e2e/run-app-tests.sh` builds and installs the app, then drives it the way
a person would: real key events through macOS, the tray menu, the Settings
and onboarding windows, dictating into a bare test window. It plays a WAV
instead of the mic and uses temp config/history, so your setup isn't
touched. The WAV swap only exists in `test-hooks` builds, which the script
makes; `make install` puts a normal build back. Needs Accessibility for hearme and your terminal, and
"Press 🌐 key to" set to Do Nothing. Each test must finish within 10s; a
stuck one aborts the run and says where it stopped. Don't type while it runs.
