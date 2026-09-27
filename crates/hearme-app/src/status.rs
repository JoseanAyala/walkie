//! The Status tab: everything that has silently broken dictation before,
//! checked directly instead of inferred from "nothing happened".

use hearme_core::audio;
use hearme_core::hotkey::engine::{Bindings, Engine};
use hearme_core::hotkey::keys::Key;
use hearme_core::hotkey::tap::TapStatus;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

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

#[cfg(target_os = "macos")]
mod mac {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOHIDCheckAccess(request: u32) -> u32;
    }
    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {
        static AVMediaTypeAudio: &'static objc2::runtime::AnyObject;
    }

    pub fn accessibility() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    /// kIOHIDRequestTypeListenEvent = 1; kIOHIDAccessTypeGranted = 0.
    pub fn input_monitoring() -> bool {
        unsafe { IOHIDCheckAccess(1) == 0 }
    }

    /// AVAuthorizationStatus: 0 not determined, 1 restricted, 2 denied, 3 authorized.
    pub fn microphone() -> isize {
        unsafe {
            objc2::msg_send![
                objc2::class!(AVCaptureDevice),
                authorizationStatusForMediaType: AVMediaTypeAudio
            ]
        }
    }

    /// System Settings → Keyboard → "Press 🌐 key to": 0 = Do Nothing.
    pub fn globe_does_nothing() -> bool {
        std::process::Command::new("defaults")
            .args(["read", "com.apple.HIToolbox", "AppleFnUsageType"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim() == "0")
            .unwrap_or(false)
    }
}

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

    #[cfg(target_os = "macos")]
    {
        let mic = mac::microphone();
        if let Some(p) = std::env::var_os("HEARME_TEST_AUDIO") {
            out.push(Check {
                id: "mic",
                label: "Microphone",
                ok: true,
                detail: format!("test mode — playing {}", std::path::Path::new(&p).display()),
                fix: None,
            });
        } else {
            out.push(Check {
                id: "mic",
                label: "Microphone",
                ok: mic == 3,
                detail: match mic {
                    3 => "granted".into(),
                    0 => "not asked yet — it's requested on your first dictation".into(),
                    _ => "denied — hearme records silence".into(),
                },
                fix: (mic != 3).then_some("mic"),
            });
        }
        let ax = mac::accessibility();
        out.push(Check {
            id: "accessibility",
            label: "Accessibility",
            ok: ax,
            detail: if ax {
                "granted".into()
            } else {
                "needed for the keyboard shortcut and pasting text".into()
            },
            fix: (!ax).then_some("accessibility"),
        });
        let im = mac::input_monitoring();
        out.push(Check {
            id: "input",
            label: "Input Monitoring",
            ok: im || tap_running,
            detail: if im {
                "granted".into()
            } else if tap_running {
                "not granted, but not needed while the keyboard hook runs".into()
            } else {
                "not granted".into()
            },
            fix: (!im && !tap_running).then_some("input"),
        });
    }

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
            tap_error.unwrap_or_else(|| "not running — restart hearme".into())
        },
        fix: (!tap_running).then_some("restart"),
    });

    let bindings: Bindings = hk.engine.lock().unwrap().bindings().clone();
    let errors = hk.errors.lock().unwrap().clone();
    out.push(Check {
        id: "shortcuts",
        label: "Shortcuts",
        ok: errors.is_empty() && !bindings.dictate.is_empty(),
        detail: if errors.is_empty() {
            format!(
                "Dictate: {} · Polish: {} · Hands-free: {}",
                pretty(&bindings.dictate),
                pretty(&bindings.polish),
                pretty(&bindings.hands_free)
            )
        } else {
            errors.join("; ")
        },
        fix: None,
    });

    #[cfg(target_os = "macos")]
    {
        let uses_fn = [&bindings.dictate, &bindings.polish, &bindings.hands_free]
            .iter()
            .any(|b| b.contains(&Key::Fn));
        if uses_fn {
            let ok = mac::globe_does_nothing();
            out.push(Check {
                id: "globe",
                label: "Globe (Fn) key",
                ok,
                detail: if ok {
                    "\"Press 🌐 key to\" is Do Nothing".into()
                } else {
                    "set Keyboard → \"Press 🌐 key to\" → Do Nothing, or Fn also opens emoji / switches input".into()
                },
                fix: (!ok).then_some("keyboard"),
            });
        }
    }

    let device = audio::default_input_name();
    let silent = audio::LAST_CAPTURE_SILENT.load(Ordering::Relaxed);
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

    let m = model.0.lock().unwrap().clone();
    out.push(Check {
        id: "model",
        label: "Speech model",
        ok: m == "ready",
        detail: m,
        fix: None,
    });

    out
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
    fn pretty_bindings() {
        assert_eq!(pretty(&[]), "off");
        assert_eq!(pretty(&[Key::Fn, Key::Code(49)]), "Fn + Space");
    }
}
