use super::super::engine::{Engine, Signal};
use super::TapStatus;
use std::sync::{Arc, Mutex};

/// The factory default for `hotkeys.dictate` (see `config::Hotkeys`).
// TODO(linux): once a real keyboard hook exists (see `spawn` below),
// reconsider this default against whatever key is comfortable to hold on a
// typical Linux keyboard layout.
pub const DEFAULT_DICTATE_KEY: &str = "RightCtrl";

/// No keyboard hook on this platform yet (see AGENTS.md: Linux is
/// build-only for now). Reports it through `status` the same way a missing
/// Accessibility grant does on macOS, so Settings' "Keyboard hook" check
/// surfaces it instead of just doing nothing.
// TODO(linux): a real hook needs an input backend — e.g. evdev/libinput (X11
// and Wayland both allow reading raw keys that way with the right
// permissions), or a compositor-specific global-shortcut protocol.
pub fn spawn(
    _engine: Arc<Mutex<Engine>>,
    status: Arc<TapStatus>,
    _on_signal: impl Fn(Signal) + Send + Sync + 'static,
) {
    let msg = "no keyboard hook on this platform yet";
    eprintln!("walkie: {msg}");
    *status.error.lock().unwrap() = Some(msg.into());
}
