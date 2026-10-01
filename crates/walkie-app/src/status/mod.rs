//! The checks behind Settings (what walkie needs, up top; all of them under
//! Advanced): everything that has silently broken dictation before,
//! checked directly instead of inferred from "nothing happened".

use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use walkie_core::audio;
use walkie_core::config::{Config, PolishProvider};
use walkie_core::hotkey::engine::{Bindings, Engine};
use walkie_core::hotkey::keys::Key;
use walkie_core::hotkey::tap::TapStatus;
use walkie_core::pipeline::polish;

/// Shared between the keyboard hook, the commands and the status page.
pub struct HotkeyState {
    pub engine: Arc<Mutex<Engine>>,
    pub tap: Arc<TapStatus>,
    pub errors: Mutex<Vec<String>>,
}

pub struct ModelStatus(pub Mutex<String>);

#[derive(Serialize, Clone, Debug)]
pub struct Check {
    pub id: &'static str,
    pub label: &'static str,
    pub ok: bool,
    pub detail: String,
    /// What the "Fix" button does: a settings pane for `open_settings_pane`,
    /// or "restart".
    pub fix: Option<&'static str>,
}

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
mod platform;
pub use platform::{ask_for_microphone, globe_does_nothing};

pub fn pretty(keys: &[Key]) -> String {
    if keys.is_empty() {
        "off".into()
    } else {
        keys.iter()
            .map(|k| k.name())
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

fn looks_virtual(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "virtual",
        "aggregate",
        "blackhole",
        "loopback",
        "soundflower",
    ]
    .iter()
    .any(|v| n.contains(v))
}

pub fn collect(hk: &HotkeyState, model: &ModelStatus) -> Vec<Check> {
    let mut out = Vec::new();
    let tap_running = hk.tap.running.load(Ordering::SeqCst);

    out.extend(platform::permission_checks(tap_running));

    let tap_error = hk.tap.error.lock().unwrap().clone();
    let reenabled = hk.tap.reenabled.load(Ordering::SeqCst);
    out.push(Check {
        id: "hook",
        label: "Keyboard hook",
        ok: tap_running,
        detail: if tap_running {
            match reenabled {
                0 => "running".into(),
                n => format!("running (macOS paused it {n}× — recovered)"),
            }
        } else {
            tap_error.unwrap_or_else(|| "not running".into())
        },
        fix: (!tap_running).then_some("accessibility"),
    });

    let bindings: Bindings = hk.engine.lock().unwrap().bindings().clone();
    let errors = hk.errors.lock().unwrap().clone();
    out.push(Check {
        id: "shortcuts",
        label: "Shortcuts",
        ok: errors.is_empty() && !bindings.dictate.is_empty(),
        detail: if errors.is_empty() {
            format!(
                "Dictate: {} · Polish: {} · Paste last: {}",
                pretty(&bindings.dictate),
                pretty(&bindings.polish),
                pretty(&bindings.paste_last)
            )
        } else {
            errors.join("; ")
        },
        fix: None,
    });

    let (device, missing) = audio::current_input();
    let silent = audio::LAST_CAPTURE_SILENT.load(Ordering::Relaxed);
    if let Some(m) = missing {
        // Not a broken setup (the desk mic is just unplugged), so it gets
        // its own id: startup_check doesn't pop Settings open for it.
        let using = device.unwrap_or_else(|| "no input device".into());
        out.push(Check {
            id: "device-missing",
            label: "Input device",
            ok: false,
            detail: format!("{m} is not connected — using the system default, {using}"),
            fix: None,
        });
    } else {
        let (ok, detail) = match &device {
            None => (false, "no input device".to_string()),
            Some(d) if silent => (false, format!("{d} — last recording was silent")),
            Some(d) if looks_virtual(d) => {
                (false, format!("{d} — a virtual device, may record silence"))
            }
            Some(d) => (true, d.clone()),
        };
        out.push(Check {
            id: "device",
            label: "Input device",
            ok,
            detail,
            fix: (!ok).then_some("sound"),
        });
    }

    let m = model.0.lock().unwrap().clone();
    out.push(Check {
        id: "model",
        label: "Speech model",
        ok: m == "ready",
        detail: m,
        fix: None,
    });

    let cfg = Config::load().unwrap_or_default();
    if cfg.polish.provider == PolishProvider::Apple && !bindings.polish.is_empty() {
        out.push(apple_check(
            &polish::apple_status(&crate::glue::ai_helper()),
        ));
    }

    out
}

/// Only listed when dictate + polish uses Apple's model. Not a broken setup:
/// plain dictation works without it, so it never pops Settings open.
fn apple_check(status: &str) -> Check {
    Check {
        id: "apple-ai",
        label: "Apple Intelligence",
        ok: status == "available",
        detail: format!("for polish: {}", polish::describe_apple_status(status)),
        fix: (status == "off").then_some("ai"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_device_names_are_flagged() {
        assert!(looks_virtual("MOTIV Mix Virtual"));
        assert!(looks_virtual("BlackHole 2ch"));
        assert!(!looks_virtual("MacBook Pro Microphone"));
    }

    #[test]
    fn apple_check_offers_a_fix_only_when_it_is_off() {
        let off = apple_check("off");
        assert!(!off.ok);
        assert_eq!(off.fix, Some("ai"));
        let ready = apple_check("available");
        assert!(ready.ok);
        assert_eq!(ready.detail, "for polish: ready");
        assert_eq!(apple_check("not-eligible").fix, None);
    }

    #[test]
    fn pretty_bindings() {
        assert_eq!(pretty(&[]), "off");
        assert_eq!(pretty(&[Key::Fn, Key::Code(49)]), "Fn + Space");
    }
}
