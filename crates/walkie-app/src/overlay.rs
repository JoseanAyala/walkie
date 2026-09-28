//! The overlay pill as a panel: the webview stays, but its NSWindow is made to
//! behave like a non-activating HUD — never key, on every Space, over
//! full-screen apps, click-through — and is shown without Tauri's
//! `makeKeyAndOrderFront`.

use objc2::msg_send;
use objc2::runtime::{AnyObject, Bool};
use tauri::WebviewWindow;

// NSWindowCollectionBehavior bits.
const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
const STATIONARY: usize = 1 << 4; // stays put during Exposé / Mission Control
const IGNORES_CYCLE: usize = 1 << 6; // not in ⌘` window cycling
const FULL_SCREEN_AUXILIARY: usize = 1 << 8; // may sit on another app's full-screen Space

/// NSStatusWindowLevel: above other apps' floating panels (alwaysOnTop is
/// only NSFloatingWindowLevel).
const STATUS_WINDOW_LEVEL: isize = 25;

/// Once, at startup.
pub fn setup(w: &WebviewWindow) {
    // Never key: the focused app must keep keyboard focus, since that's where
    // the transcript is typed.
    let _ = w.set_focusable(false);
    with_ns_window(w, |ns| unsafe {
        let behavior: usize = msg_send![ns, collectionBehavior];
        let behavior =
            behavior | CAN_JOIN_ALL_SPACES | STATIONARY | IGNORES_CYCLE | FULL_SCREEN_AUXILIARY;
        let _: () = msg_send![ns, setCollectionBehavior: behavior];
        let _: () = msg_send![ns, setLevel: STATUS_WINDOW_LEVEL];
        // Nothing in the pill is clickable; don't block what's under it.
        let _: () = msg_send![ns, setIgnoresMouseEvents: Bool::YES];
        let _: () = msg_send![ns, setHidesOnDeactivate: Bool::NO];
    });
}

/// Brings the pill up without activating walkie or making it key.
pub fn show(w: &WebviewWindow) {
    with_ns_window(w, |ns| unsafe {
        let _: () = msg_send![ns, orderFrontRegardless];
    });
}

/// Runs `f` on the main thread (AppKit requires it) with the NSWindow.
fn with_ns_window(w: &WebviewWindow, f: impl FnOnce(&AnyObject) + Send + 'static) {
    let Ok(ptr) = w.ns_window() else { return };
    let ptr = ptr as usize; // raw pointers aren't Send; the window outlives the app
    let _ = w.run_on_main_thread(move || {
        if let Some(ns) = unsafe { (ptr as *const AnyObject).as_ref() } {
            f(ns);
        }
    });
}
