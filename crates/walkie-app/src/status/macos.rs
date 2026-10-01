use super::Check;

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

fn accessibility() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// kIOHIDRequestTypeListenEvent = 1; kIOHIDAccessTypeGranted = 0.
fn input_monitoring() -> bool {
    unsafe { IOHIDCheckAccess(1) == 0 }
}

/// AVAuthorizationStatus: 0 not determined, 1 restricted, 2 denied, 3 authorized.
fn microphone() -> isize {
    unsafe {
        objc2::msg_send![
            objc2::class!(AVCaptureDevice),
            authorizationStatusForMediaType: AVMediaTypeAudio
        ]
    }
}

/// Shows macOS's microphone prompt. Only does anything while the answer
/// is "not determined"; after that, only System Settings can change it.
fn request_microphone() {
    let done = block2::RcBlock::new(|_granted: objc2::runtime::Bool| {});
    unsafe {
        let _: () = objc2::msg_send![
            objc2::class!(AVCaptureDevice),
            requestAccessForMediaType: AVMediaTypeAudio,
            completionHandler: &*done
        ];
    }
}

/// Whether "Press 🌐 key to" is Do Nothing, so Fn only reaches walkie.
pub fn globe_does_nothing() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "com.apple.HIToolbox", "AppleFnUsageType"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "0")
        .unwrap_or(false)
}

/// Asks for the microphone the way a first dictation would, if macOS hasn't
/// asked yet. False when it already has: then only System Settings helps.
pub fn ask_for_microphone() -> bool {
    if microphone() == 0 {
        request_microphone();
        return true;
    }
    false
}

/// The mic/Accessibility/Input Monitoring checks at the top of Settings.
pub fn permission_checks(tap_running: bool) -> Vec<Check> {
    let mut out = Vec::new();
    let mic = microphone();
    if let Some(p) = crate::glue::test_audio() {
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
                0 => "not asked yet — Fix asks now, or your first dictation will".into(),
                _ => "denied — walkie records silence".into(),
            },
            fix: (mic != 3).then_some("mic"),
        });
    }
    let ax = accessibility();
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
    let im = input_monitoring();
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
    out
}
