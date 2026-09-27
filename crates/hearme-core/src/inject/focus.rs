//! "Is there anywhere to paste?" — asks the Accessibility API what has
//! keyboard focus before injecting, so dictating at the desktop or a Finder
//! window leaves the text on the clipboard instead of silently losing it.
//!
//! Deliberately biased towards pasting: a wrong "no field" makes a working
//! paste fail, while a wrong "editable" just behaves like before. Browsers,
//! Electron apps and custom-drawn terminals report focus unreliably, so
//! anything short of a clear "nothing to type into" is `Unknown` → paste.

/// What the focused UI element looks like, as far as `classify` cares.
#[derive(Debug, Clone, PartialEq)]
pub enum Probe {
    /// The AX query itself failed (no permission, app not answering, …).
    Failed,
    /// The frontmost app answered: nothing inside it has keyboard focus.
    NoElement { app: String },
    Element {
        /// Executable name of the frontmost app, e.g. "Finder".
        app: String,
        role: String,
        subrole: String,
        /// Whether AXValue is settable (true for every native text input).
        value_settable: bool,
        /// Inside an AXWebArea — web content, where paste handlers are the page's business.
        in_web: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Editable,
    NoField,
    Unknown,
}

const TEXT_ROLES: &[&str] = &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];
const TEXT_SUBROLES: &[&str] = &["AXSearchField", "AXSecureTextField"];
/// File/table views: nothing a ⌘V of text can land in, in any app.
const NON_TEXT_ROLES: &[&str] = &["AXList", "AXOutline", "AXTable", "AXBrowser"];
/// Stock AppKit apps whose focus reports are trustworthy and which have no
/// text input outside real text fields — here anything else is "no field",
/// including no focused element at all (the desktop belongs to Finder).
const QUIET_APPS: &[&str] = &["Finder", "QuickTime Player", "Preview", "Photos"];

pub fn classify(p: &Probe) -> Focus {
    match p {
        Probe::Failed => Focus::Unknown,
        Probe::NoElement { app } if QUIET_APPS.contains(&app.as_str()) => Focus::NoField,
        // Chromium/Electron and GL terminals often report no focus while typing works.
        Probe::NoElement { .. } => Focus::Unknown,
        Probe::Element {
            app,
            role,
            subrole,
            value_settable,
            in_web,
        } => {
            if TEXT_ROLES.contains(&role.as_str())
                || TEXT_SUBROLES.contains(&subrole.as_str())
                || *value_settable
            {
                Focus::Editable
            } else if *in_web || role == "AXWebArea" {
                Focus::Unknown
            } else if QUIET_APPS.contains(&app.as_str()) || NON_TEXT_ROLES.contains(&role.as_str())
            {
                Focus::NoField
            } else {
                Focus::Unknown
            }
        }
    }
}

/// Probes and classifies the current keyboard focus. Call off the main
/// thread: when hearme itself is frontmost, the query is answered by our own
/// main thread, which would deadlock waiting on itself.
pub fn current() -> Focus {
    classify(&probe())
}

#[cfg(not(target_os = "macos"))]
pub fn probe() -> Probe {
    Probe::Failed
}

#[cfg(target_os = "macos")]
pub use ax::probe;

#[cfg(target_os = "macos")]
mod ax {
    use super::Probe;
    use core_foundation::base::{CFType, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    use std::ffi::c_void;

    type AXUIElementRef = *const c_void;
    type AXError = i32;
    const SUCCESS: AXError = 0;
    const NO_VALUE: AXError = -25212;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, secs: f32) -> AXError;
        fn AXUIElementCopyAttributeValue(
            el: AXUIElementRef,
            attr: CFStringRef,
            out: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementIsAttributeSettable(
            el: AXUIElementRef,
            attr: CFStringRef,
            out: *mut u8,
        ) -> AXError;
        fn AXUIElementGetPid(el: AXUIElementRef, pid: *mut i32) -> AXError;
    }

    /// An owned AXUIElement/CF value (released on drop).
    struct Owned(CFType);

    impl Owned {
        fn ptr(&self) -> AXUIElementRef {
            self.0.as_CFTypeRef()
        }

        fn attr(&self, name: &'static str) -> Result<Owned, AXError> {
            let mut out: CFTypeRef = std::ptr::null();
            let key = CFString::from_static_string(name);
            let err = unsafe {
                AXUIElementCopyAttributeValue(self.ptr(), key.as_concrete_TypeRef(), &mut out)
            };
            match err {
                SUCCESS if out.is_null() => Err(NO_VALUE),
                SUCCESS => Ok(Owned(unsafe { CFType::wrap_under_create_rule(out) })),
                e => Err(e),
            }
        }

        fn string(&self, name: &'static str) -> String {
            self.attr(name)
                .ok()
                .and_then(|v| v.0.downcast::<CFString>())
                .map(|s| s.to_string())
                .unwrap_or_default()
        }

        fn settable(&self, name: &'static str) -> bool {
            let mut out = 0u8;
            let key = CFString::from_static_string(name);
            let err = unsafe {
                AXUIElementIsAttributeSettable(self.ptr(), key.as_concrete_TypeRef(), &mut out)
            };
            err == SUCCESS && out != 0
        }
    }

    fn app_name(app: &Owned) -> String {
        let mut pid = 0;
        if unsafe { AXUIElementGetPid(app.ptr(), &mut pid) } != SUCCESS {
            return String::new();
        }
        let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
        if n <= 0 {
            return String::new();
        }
        let path = String::from_utf8_lossy(&buf[..n as usize]).to_string();
        path.rsplit('/').next().unwrap_or_default().to_string()
    }

    /// A handful of AX round-trips (~1ms total); a hung app costs at most
    /// the messaging timeout per call before we give up and paste anyway.
    pub fn probe() -> Probe {
        let sys = unsafe { AXUIElementCreateSystemWide() };
        if sys.is_null() {
            return Probe::Failed;
        }
        let sys = Owned(unsafe { CFType::wrap_under_create_rule(sys) });
        unsafe { AXUIElementSetMessagingTimeout(sys.ptr(), 0.25) };
        // Needs a WindowServer connection (any GUI app has one; a bare CLI
        // process gets kAXErrorCannotComplete → Failed → paste as before).
        let Ok(app) = sys.attr("AXFocusedApplication") else {
            return Probe::Failed;
        };
        unsafe { AXUIElementSetMessagingTimeout(app.ptr(), 0.25) };
        let name = app_name(&app);
        if name.is_empty() {
            return Probe::Failed;
        }
        let el = match app.attr("AXFocusedUIElement") {
            Ok(el) => el,
            Err(NO_VALUE) => return Probe::NoElement { app: name },
            Err(_) => return Probe::Failed,
        };
        let role = el.string("AXRole");
        if role.is_empty() {
            return Probe::Failed;
        }
        let mut in_web = false;
        let mut cur = el.attr("AXParent").ok();
        for _ in 0..32 {
            let Some(p) = cur else { break };
            if p.string("AXRole") == "AXWebArea" {
                in_web = true;
                break;
            }
            cur = p.attr("AXParent").ok();
        }
        Probe::Element {
            app: name,
            subrole: el.string("AXSubrole"),
            value_settable: el.settable("AXValue"),
            role,
            in_web,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(app: &str, role: &str, subrole: &str, settable: bool, in_web: bool) -> Probe {
        Probe::Element {
            app: app.into(),
            role: role.into(),
            subrole: subrole.into(),
            value_settable: settable,
            in_web,
        }
    }

    #[test]
    fn text_inputs_are_editable_everywhere() {
        for role in ["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"] {
            assert_eq!(
                classify(&el("TextEdit", role, "", false, false)),
                Focus::Editable,
                "{role}"
            );
            assert_eq!(
                classify(&el("Finder", role, "", false, false)),
                Focus::Editable,
                "Finder rename/search: {role}"
            );
        }
        assert_eq!(
            classify(&el(
                "Safari",
                "AXTextField",
                "AXSecureTextField",
                false,
                true
            )),
            Focus::Editable
        );
    }

    #[test]
    fn a_settable_value_counts_as_editable() {
        assert_eq!(
            classify(&el("Warp", "AXGroup", "", true, false)),
            Focus::Editable
        );
    }

    #[test]
    fn terminals_paste() {
        // Terminal / iTerm2 / Ghostty expose their buffer as a text area.
        for app in ["Terminal", "iTerm2", "ghostty"] {
            assert_eq!(
                classify(&el(app, "AXTextArea", "", false, false)),
                Focus::Editable,
                "{app}"
            );
        }
        // Custom-drawn ones report whatever they like, or nothing.
        assert_eq!(
            classify(&el("Warp", "AXGroup", "", false, false)),
            Focus::Unknown
        );
        assert_eq!(
            classify(&el(
                "alacritty",
                "AXWindow",
                "AXStandardWindow",
                false,
                false
            )),
            Focus::Unknown
        );
        assert_eq!(
            classify(&Probe::NoElement {
                app: "kitty".into()
            }),
            Focus::Unknown
        );
    }

    #[test]
    fn web_content_is_never_no_field() {
        // Chrome/Safari pages and Electron apps (Slack, VS Code): lists and
        // plain groups in a page may still route a paste to an editor.
        assert_eq!(
            classify(&el("Google Chrome", "AXWebArea", "", false, false)),
            Focus::Unknown
        );
        assert_eq!(
            classify(&el("Slack", "AXList", "", false, true)),
            Focus::Unknown
        );
        assert_eq!(
            classify(&el("Code", "AXGroup", "", false, true)),
            Focus::Unknown
        );
        assert_eq!(
            classify(&Probe::NoElement {
                app: "Electron".into()
            }),
            Focus::Unknown
        );
    }

    #[test]
    fn finder_and_the_desktop_are_no_field() {
        assert_eq!(
            classify(&Probe::NoElement {
                app: "Finder".into()
            }),
            Focus::NoField,
            "desktop"
        );
        assert_eq!(
            classify(&el("Finder", "AXScrollArea", "", false, false)),
            Focus::NoField
        );
        assert_eq!(
            classify(&el("Finder", "AXOutline", "", false, false)),
            Focus::NoField
        );
        assert_eq!(
            classify(&el("Finder", "AXList", "", false, false)),
            Focus::NoField
        );
        assert_eq!(
            classify(&el(
                "QuickTime Player",
                "AXWindow",
                "AXStandardWindow",
                false,
                false
            )),
            Focus::NoField
        );
    }

    #[test]
    fn native_file_and_table_views_are_no_field() {
        assert_eq!(
            classify(&el("Mail", "AXTable", "", false, false)),
            Focus::NoField
        );
        assert_eq!(
            classify(&el("Xcode", "AXOutline", "", false, false)),
            Focus::NoField
        );
    }

    #[test]
    fn anything_unclear_pastes() {
        assert_eq!(classify(&Probe::Failed), Focus::Unknown);
        assert_eq!(
            classify(&el("Calculator", "AXButton", "", false, false)),
            Focus::Unknown
        );
        assert_eq!(
            classify(&el("Figma", "AXGroup", "", false, false)),
            Focus::Unknown
        );
    }
}
