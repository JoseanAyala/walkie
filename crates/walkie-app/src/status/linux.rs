use super::Check;
use std::fs;

/// No "Press 🌐 key to" setting on Linux; assume it's fine.
pub fn globe_does_nothing() -> bool {
    true
}

/// No mic-permission prompt to trigger on this platform yet.
pub fn ask_for_microphone() -> bool {
    false
}

/// True if at least one `/dev/input/event*` node can be opened for reading
/// — the access the keyboard hook needs (see `hotkey/tap/linux.rs`).
/// Usually gated by membership in the `input` group.
fn can_read_input_events() -> bool {
    let Ok(entries) = fs::read_dir("/dev/input") else {
        return false;
    };
    entries.filter_map(Result::ok).any(|e| {
        e.file_name()
            .to_str()
            .is_some_and(|n| n.starts_with("event"))
            && fs::File::open(e.path()).is_ok()
    })
}

/// The keyboard-access/mic checks at the top of Settings. There's no macOS-
/// style permission dialog here, and no System Settings pane to send
/// "Fix" to, so these never offer one — the detail text carries the hint
/// instead (see AGENTS.md).
pub fn permission_checks(tap_running: bool) -> Vec<Check> {
    let mut out = Vec::new();

    let readable = can_read_input_events();
    out.push(Check {
        id: "keyboard-access",
        label: "Keyboard access",
        ok: readable || tap_running,
        detail: if readable {
            "granted".into()
        } else if tap_running {
            "not granted, but not needed while the keyboard hook runs".into()
        } else {
            "can't open /dev/input/event* — add your user to the `input` group \
             (sudo usermod -aG input $USER), then log out and back in"
                .into()
        },
        fix: None,
    });

    if let Some(p) = crate::glue::test_audio() {
        out.push(Check {
            id: "mic",
            label: "Microphone",
            ok: true,
            detail: format!("test mode — playing {}", std::path::Path::new(&p).display()),
            fix: None,
        });
    } else if let Some(name) = walkie_core::audio::default_input_name() {
        out.push(Check {
            id: "mic",
            label: "Microphone",
            ok: true,
            detail: name,
            fix: None,
        });
    }
    // No default input device at all: skip the row rather than guess at
    // a cause. The "device" check in `collect` already reports "no input
    // device" generically.

    out
}
