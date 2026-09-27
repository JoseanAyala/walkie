# Manual Testing Checklist

hearme v1 is implemented, unit-tested (61 tests), and reviewed, but the parts
that need a human with real microphone/keyboard/permission access haven't
been exercised yet. This is that checklist.

**Run everything against the installed bundle:**

```sh
scripts/dev.sh           # build, sign, install to /Applications, relaunch, follow log
scripts/dev.sh --debug   # same, plus HEARME_DEBUG_EVENTS=1 (dumps key events)
```

The log is always at `~/Library/Logs/hearme/hearme.log`. Grant Microphone,
Accessibility and Input Monitoring once — the bundle is signed with a stable
local identity, so the grants survive rebuilds.

> **Two environment gotchas cost a full debugging session. Check both first:**
>
> 1. **Default input device.** macOS keeps whatever was last selected as the
>    system default input. A virtual device (e.g. "MOTIV Mix Virtual") stays
>    selected long after its hardware is unplugged, opens cleanly, and returns
>    pure silence. Whisper does not report silence — it hallucinates "Thank
>    you." / "Thanks for watching!", which reads like a transcription bug.
>    Check System Settings → Sound → Input. `hearme: recording from <device>`
>    is now logged on every capture.
> 2. **Input Monitoring is per-binary.** Without it, macOS hands the event tap
>    mouse events but *silently withholds key events* — `rdev::listen` returns
>    `Ok`, no prompt, no error, the hotkey just never fires. The grant is tied
>    to the responsible process, so a terminal that works for `hotkey_demo`
>    does not cover the app launched from a different parent process. Run
>    `HEARME_DEBUG_EVENTS=1` to dump tap events: mouse events but no
>    `KeyPress` = Input Monitoring is not granted.

## 1. Mic quality - FAILED NOTHING REGISTERED

**Resolved:** the default input device was a disconnected virtual device
returning silence, not a capture bug. See the note above.

```sh
cargo run -p hearme-core --example record_5s
```

Speak during the 5s window, then:

```sh
afplay /tmp/hearme-record-test.wav
```

Confirm it's intelligible and at the correct speed (not sped up/slowed down
— that would indicate a resampling bug).

## 2. Hotkey gestures It registered

```sh
cargo run -p hearme-core --example hotkey_demo
```

Grant Input Monitoring to your terminal if prompted (restart the terminal
after granting). Try each gesture and confirm the printed `Mode Output`
pairs match:

- Hold Right Option ≥150ms, release → `Dictate Start` then `Dictate Finish`
- Quick-tap Right Option → `Dictate Start` then `Dictate CancelDiscard`
- Double-tap, wait, then tap → locks (`Start`), then stops (`Finish`)
- Hold Shift + Right Option → `Polish Start` (not `Dictate`)

## 3. Full live dictation - PASSED in `cargo tauri dev`

```sh
cd crates/hearme-app
cargo tauri dev
```

Grant Microphone, Accessibility, and Input Monitoring when prompted.

- Click into a text field, hold Right Option, speak, release → text appears
  in under ~1s
- Try Spanish — accents should come through correctly
- Double-tap to lock, speak a longer sentence, tap once to stop
- Configure a polish command in Settings (e.g. `claude -p "clean this up"`),
  hold Shift + Right Option, speak, confirm the polished text appears

## 4. Tray menu

Click the tray icon. Confirm "Settings…" opens the settings window and
"Quit hearme" exits the app.

## 5. Settings window

- Confirm all four tabs render: General, Cleanup, Polish, History
- Change a setting, click Save, confirm it persists after restart
- **Close the settings window via the red button, then reopen it from the
  tray** — this exercises a bug (windows being permanently destroyed on
  close) that was found and fixed in the final review. Confirm it reopens
  correctly instead of doing nothing.
- Trigger an error (e.g. a broken polish command) and confirm the red error
  banner appears at the top of the settings window, and that it persists
  until dismissed.

## 6. Onboarding

Simulate first run (back up and remove `~/.config/hearme/config.toml`),
launch the app, and confirm the onboarding window appears. Click each "Open
Settings" button and confirm it opens the correct System Settings pane
(Microphone / Accessibility / Input Monitoring). Click "finish" and confirm
onboarding closes and Settings opens. Restart and confirm onboarding does
NOT reappear.

## 7. Bundled app

The bundled `.app` is a separately-signed binary from the dev build — its
permission grants are independent. Repeat the permission grants and the
live dictation flow (section 3) against the installed `/Applications/hearme.app`.

## 8. Edge cases worth a look

- A long (>60s) locked dictation
- Dictating into a password/secure-input field (paste may silently fail —
  confirm the transcript is still recoverable from history or clipboard)
- Pasting into a slow-to-respond app (does the 150ms clipboard-restore
  window hold up, or does it restore the old clipboard before the paste
  lands?)
- Multi-monitor: does the overlay pill position correctly, and does it show
  at all when dictating into a fullscreen app on another Space?
