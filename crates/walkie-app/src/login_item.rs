//! Launch at login via SMAppService.mainApp (macOS 13+), so walkie shows up
//! in System Settings → General → Login Items. macOS owns the on/off state;
//! nothing is kept in the config, it's read live every time.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Disabled,
    Enabled,
    /// Registered, but the user has to allow it in Login Items first.
    RequiresApproval,
    NotFound,
    /// Not running from an .app bundle (`cargo tauri dev`): there is nothing
    /// macOS could launch.
    Unbundled,
}

impl Status {
    /// SMAppServiceStatus: 0 notRegistered, 1 enabled, 2 requiresApproval, 3 notFound.
    pub fn from_raw(v: isize) -> Status {
        match v {
            0 => Status::Disabled,
            1 => Status::Enabled,
            2 => Status::RequiresApproval,
            _ => Status::NotFound,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Status::Disabled => "disabled",
            Status::Enabled => "enabled",
            Status::RequiresApproval => "requires_approval",
            Status::NotFound => "not_found",
            Status::Unbundled => "unbundled",
        }
    }

    /// Whether walkie is registered — the checkbox state. A registration
    /// waiting for approval counts: unchecking it is how you withdraw it.
    pub fn on(self) -> bool {
        matches!(self, Status::Enabled | Status::RequiresApproval)
    }

    pub fn hint(self) -> Option<&'static str> {
        match self {
            Status::RequiresApproval => {
                Some("macOS needs your approval: allow walkie in System Settings → General → Login Items.")
            }
            Status::NotFound => Some("macOS can't find walkie.app — reinstall it in /Applications."),
            Status::Unbundled => Some(UNBUNDLED),
            Status::Disabled | Status::Enabled => None,
        }
    }
}

const UNBUNDLED: &str = "walkie isn't running from walkie.app (e.g. cargo tauri dev), \
                         so it can't launch at login.";

/// What the UI gets back.
#[derive(Serialize, Clone, Debug)]
pub struct LoginItem {
    pub status: &'static str,
    pub on: bool,
    pub hint: Option<&'static str>,
}

impl From<Status> for LoginItem {
    fn from(s: Status) -> Self {
        LoginItem {
            status: s.name(),
            on: s.on(),
            hint: s.hint(),
        }
    }
}

fn in_app_bundle(exe: &std::path::Path) -> bool {
    exe.to_string_lossy().contains(".app/Contents/MacOS/")
}

fn bundled() -> bool {
    std::env::current_exe().is_ok_and(|p| in_app_bundle(&p))
}

/// Whether `set(enabled)` has anything to do from `current`.
fn needs_change(current: Status, enabled: bool) -> bool {
    current.on() != enabled
}

pub fn status() -> Status {
    if !bundled() {
        return Status::Unbundled;
    }
    mac::status()
}

/// Registers or unregisters, then returns the state macOS reports afterwards
/// (which may be RequiresApproval rather than Enabled).
/// Whether macOS started walkie at login (vs. someone opening it). Only
/// answerable during launch; None when it can't tell.
pub fn launched_at_login() -> Option<bool> {
    mac::launched_at_login()
}

pub fn set(enabled: bool) -> Result<Status, String> {
    if !bundled() {
        return Err(UNBUNDLED.into());
    }
    if needs_change(mac::status(), enabled) {
        mac::set(enabled).map_err(|e| {
            let verb = if enabled { "turn on" } else { "turn off" };
            format!("couldn't {verb} launch at login: {e}")
        })?;
    }
    let s = mac::status();
    eprintln!("walkie: login item {}", s.name());
    Ok(s)
}

/// `walkie --login-item status|on|off`: prints the resulting status and
/// exits without starting the app. The OS e2e suite uses it to check and
/// restore the real login-item state from outside.
pub fn cli() -> Option<i32> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--login-item") {
        return None;
    }
    let r = match args.next().as_deref() {
        Some("status") => Ok(status()),
        Some("on") => set(true),
        Some("off") => set(false),
        _ => Err("usage: walkie --login-item status|on|off".into()),
    };
    Some(match r {
        Ok(s) => {
            println!("{}", s.name());
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    })
}

#[cfg(target_os = "macos")]
mod mac {
    use super::Status;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2::{msg_send, rc::autoreleasepool};
    use std::ffi::{c_char, CStr};

    #[link(name = "ServiceManagement", kind = "framework")]
    extern "C" {}

    /// `SMAppService.mainApp`, or None before macOS 13.
    fn main_app() -> Option<*mut AnyObject> {
        let cls = AnyClass::get(c"SMAppService")?;
        let svc: *mut AnyObject = unsafe { msg_send![cls, mainAppService] };
        (!svc.is_null()).then_some(svc)
    }

    /// Reads the "open application" Apple event macOS launched us with;
    /// it carries `keyAELaunchedAsLogInItem` when we're a login item. Only
    /// current while the app is finishing launching (i.e. during setup).
    /// None if there's no such event to read.
    pub fn launched_at_login() -> Option<bool> {
        const OPEN_APP: u32 = u32::from_be_bytes(*b"oapp"); // kAEOpenApplication
        const PROP_DATA: u32 = u32::from_be_bytes(*b"prdt"); // keyAEPropData
        const LOGIN_ITEM: u32 = u32::from_be_bytes(*b"lgit"); // keyAELaunchedAsLogInItem
        autoreleasepool(|_| unsafe {
            let mgr: *mut AnyObject = msg_send![
                AnyClass::get(c"NSAppleEventManager")?,
                sharedAppleEventManager
            ];
            let ev: *mut AnyObject = msg_send![mgr.as_ref()?, currentAppleEvent];
            let ev = ev.as_ref()?;
            let id: u32 = msg_send![ev, eventID];
            if id != OPEN_APP {
                return None;
            }
            let prop: *mut AnyObject = msg_send![ev, paramDescriptorForKeyword: PROP_DATA];
            Some(match prop.as_ref() {
                Some(p) => {
                    let code: u32 = msg_send![p, enumCodeValue];
                    code == LOGIN_ITEM
                }
                None => false,
            })
        })
    }

    pub fn status() -> Status {
        autoreleasepool(|_| match main_app() {
            Some(svc) => Status::from_raw(unsafe { msg_send![svc, status] }),
            None => Status::NotFound,
        })
    }

    pub fn set(enabled: bool) -> Result<(), String> {
        autoreleasepool(|_| {
            let svc = main_app().ok_or("launch at login needs macOS 13 or later")?;
            let mut err: *mut AnyObject = std::ptr::null_mut();
            let out = &mut err as *mut *mut AnyObject;
            let ok: Bool = unsafe {
                if enabled {
                    msg_send![svc, registerAndReturnError: out]
                } else {
                    msg_send![svc, unregisterAndReturnError: out]
                }
            };
            if ok.as_bool() {
                return Ok(());
            }
            if err.is_null() {
                return Err("unknown error".into());
            }
            unsafe {
                let desc: *mut AnyObject = msg_send![err, localizedDescription];
                let s: *const c_char = msg_send![desc, UTF8String];
                Err(CStr::from_ptr(s).to_string_lossy().into_owned())
            }
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod mac {
    use super::Status;
    pub fn status() -> Status {
        Status::NotFound
    }
    pub fn launched_at_login() -> Option<bool> {
        None
    }
    pub fn set(_: bool) -> Result<(), String> {
        Err("launch at login is only supported on macOS".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_test_run_is_not_a_login_launch() {
        // No launch Apple event here: must never read as "at login", or a
        // hand launch would stay silent.
        assert_ne!(launched_at_login(), Some(true));
    }

    #[test]
    fn raw_status_maps_to_smappservice_values() {
        assert_eq!(Status::from_raw(0), Status::Disabled);
        assert_eq!(Status::from_raw(1), Status::Enabled);
        assert_eq!(Status::from_raw(2), Status::RequiresApproval);
        assert_eq!(Status::from_raw(3), Status::NotFound);
        assert_eq!(Status::from_raw(42), Status::NotFound);
    }

    #[test]
    fn requires_approval_is_checked_and_explained() {
        let item = LoginItem::from(Status::RequiresApproval);
        assert!(item.on);
        assert_eq!(item.status, "requires_approval");
        assert!(item.hint.unwrap().contains("Login Items"));
    }

    #[test]
    fn enabled_and_disabled_need_no_hint() {
        assert!(LoginItem::from(Status::Enabled).on);
        assert!(!LoginItem::from(Status::Disabled).on);
        assert!(Status::Enabled.hint().is_none() && Status::Disabled.hint().is_none());
        for s in [Status::NotFound, Status::Unbundled] {
            assert!(!s.on() && s.hint().is_some(), "{s:?}");
        }
    }

    #[test]
    fn only_registers_or_unregisters_on_a_real_change() {
        assert!(needs_change(Status::Disabled, true));
        assert!(needs_change(Status::NotFound, true));
        assert!(!needs_change(Status::Enabled, true));
        // already registered, just waiting for approval: nothing to redo
        assert!(!needs_change(Status::RequiresApproval, true));
        assert!(needs_change(Status::RequiresApproval, false));
        assert!(needs_change(Status::Enabled, false));
        assert!(!needs_change(Status::Disabled, false));
    }

    #[test]
    fn detects_running_from_an_app_bundle() {
        assert!(in_app_bundle(Path::new(
            "/Applications/walkie.app/Contents/MacOS/walkie"
        )));
        assert!(!in_app_bundle(Path::new(
            "/Users/me/dev/walkie/target/debug/walkie"
        )));
    }
}
