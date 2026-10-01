//! "Is there anywhere to paste?" — asks the Accessibility API (macOS) what
//! has keyboard focus before injecting, so dictating at the desktop or a
//! Finder window leaves the text on the clipboard instead of silently
//! losing it. Linux has no such query yet (see `linux.rs`), so `probe`
//! always answers `Failed` there, which `classify` below treats the same
//! as "can't tell" — paste anyway.
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
/// thread: when walkie itself is frontmost, the query is answered by our own
/// main thread, which would deadlock waiting on itself.
pub fn current() -> Focus {
    classify(&probe())
}

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
mod platform;
pub use platform::{frontmost, probe, selected_text, wants_shift_paste};

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
