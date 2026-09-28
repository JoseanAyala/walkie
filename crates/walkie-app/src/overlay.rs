//! The overlay pill as a panel: the webview stays, but its NSWindow becomes a
//! non-activating NSPanel — never key, on every Space, over full-screen apps,
//! click-through — and is shown without Tauri's `makeKeyAndOrderFront`.
//!
//! Only an NSPanel with the non-activating style mask may sit on another
//! app's full-screen Space; a plain NSWindow with the same collection
//! behavior stays hidden there. Tauri can't make panels, so the window's
//! class is swapped for an NSPanel subclass after it's built (as the
//! tauri-nspanel plugin does).

use std::sync::OnceLock;

use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{class, msg_send, sel};
use tauri::WebviewWindow;

// NSWindowCollectionBehavior bits.
const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
const STATIONARY: usize = 1 << 4; // stays put during Exposé / Mission Control
const IGNORES_CYCLE: usize = 1 << 6; // not in ⌘` window cycling
const FULL_SCREEN_AUXILIARY: usize = 1 << 8; // may sit on another app's full-screen Space

/// NSWindowStyleMaskNonactivatingPanel.
const NONACTIVATING_PANEL: usize = 1 << 7;

/// NSStatusWindowLevel: above other apps' floating panels (alwaysOnTop is
/// only NSFloatingWindowLevel).
const STATUS_WINDOW_LEVEL: isize = 25;

/// Once, at startup.
pub fn setup(w: &WebviewWindow) {
    with_ns_window(w, |ns| unsafe {
        into_panel(ns);
        let mask: usize = msg_send![ns, styleMask];
        let _: () = msg_send![ns, setStyleMask: mask | NONACTIVATING_PANEL];
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

/// Swaps Tauri's window class (an NSWindow subclass) for an NSPanel subclass
/// that can never be key or main, so the focused app keeps keyboard focus —
/// that's where the transcript is typed. Tauri's class only adds a
/// `focusable` ivar, read solely by the two methods overridden here, so
/// nothing reads it after the swap. Don't call `set_focusable` on this window.
unsafe fn into_panel(ns: &AnyObject) {
    let panel = panel_class();
    let old = ns.class();
    // The object was allocated for `old`; the panel's ivars must fit in it.
    if panel.instance_size() > old.instance_size() {
        eprintln!(
            "walkie: overlay stays a window: {} is larger than {}",
            panel.name().to_string_lossy(),
            old.name().to_string_lossy()
        );
        return;
    }
    AnyObject::set_class(ns, panel);
}

fn panel_class() -> &'static AnyClass {
    static CLASS: OnceLock<usize> = OnceLock::new();
    let ptr = *CLASS.get_or_init(|| {
        extern "C-unwind" fn no(_: &AnyObject, _: Sel) -> Bool {
            Bool::NO
        }
        let mut b = ClassBuilder::new(c"WalkieOverlayPanel", class!(NSPanel))
            .expect("WalkieOverlayPanel registered twice");
        unsafe {
            b.add_method(
                sel!(canBecomeKeyWindow),
                no as extern "C-unwind" fn(_, _) -> _,
            );
            b.add_method(
                sel!(canBecomeMainWindow),
                no as extern "C-unwind" fn(_, _) -> _,
            );
        }
        b.register() as *const AnyClass as usize
    });
    unsafe { &*(ptr as *const AnyClass) }
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
