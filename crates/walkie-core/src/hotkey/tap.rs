//! The macOS keyboard hook: an active CGEventTap that feeds `Engine` and
//! drops the events it says to swallow. Kept thin on purpose — all decisions
//! live in the engine, which is unit-tested.
//!
//! Needs Accessibility (an active tap can modify the event stream). Without
//! it `CGEventTapCreate` returns NULL: `spawn` reports that through
//! `TapStatus` and keeps trying, so granting it takes effect without a
//! restart.

use super::engine::{Engine, Signal};
use super::keys::Key;
use crate::inject::SYNTHETIC_EVENT_MARKER;
use core_foundation::base::TCFType;
use core_foundation::mach_port::CFMachPortRef;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult, EventField,
};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Default)]
pub struct TapStatus {
    pub running: AtomicBool,
    /// Times macOS disabled the tap (callback too slow) and we re-enabled it.
    pub reenabled: AtomicU32,
    pub error: Mutex<Option<String>>,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

/// The live tap's port, so the callback can re-enable it after a timeout.
static TAP_PORT: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

/// How often a tap that couldn't start (no Accessibility yet) tries again.
const RETRY: std::time::Duration = std::time::Duration::from_secs(1);

/// Starts the tap on its own thread (it needs a run loop). Signals are handed
/// to `on_signal` synchronously on the tap thread, so it must not block —
/// forward them over a channel.
pub fn spawn(
    engine: Arc<Mutex<Engine>>,
    status: Arc<TapStatus>,
    on_signal: impl Fn(Signal) + Send + Sync + 'static,
) {
    let debug = std::env::var_os("WALKIE_DEBUG_EVENTS").is_some();
    let start = Instant::now();

    {
        let engine = engine.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let t = start.elapsed().as_millis();
            engine.lock().unwrap_or_else(|e| e.into_inner()).poll(t);
        });
    }

    let on_signal = Arc::new(on_signal);
    std::thread::spawn(move || loop {
        let cb_status = status.clone();
        let engine = engine.clone();
        let on_signal = on_signal.clone();
        let tap = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
            ],
            move |_proxy, ty, ev| {
                let (key, down) = match ty {
                    CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                        let port = TAP_PORT.load(Ordering::SeqCst);
                        if !port.is_null() {
                            unsafe { CGEventTapEnable(port as CFMachPortRef, true) };
                        }
                        cb_status.reenabled.fetch_add(1, Ordering::SeqCst);
                        eprintln!(
                            "walkie: keyboard hook was disabled by macOS ({ty:?}) — re-enabled"
                        );
                        return CallbackResult::Keep;
                    }
                    // Our own typing and pasting: never a shortcut, and its
                    // keyDowns have no keyUps (see SYNTHETIC_EVENT_MARKER).
                    _ if ev.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA)
                        == SYNTHETIC_EVENT_MARKER =>
                    {
                        return CallbackResult::Keep;
                    }
                    CGEventType::KeyDown | CGEventType::KeyUp => {
                        let code = ev.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                        (
                            Key::from_keycode(code as u16),
                            matches!(ty, CGEventType::KeyDown),
                        )
                    }
                    CGEventType::FlagsChanged => {
                        let code = ev.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                        let key = Key::from_keycode(code as u16);
                        // Caps Lock also arrives here, but its flag is lock
                        // state, not key state — not a usable hold key.
                        let Some(bit) = key.flag_bit() else {
                            return CallbackResult::Keep;
                        };
                        (key, ev.get_flags().bits() & bit != 0)
                    }
                    _ => return CallbackResult::Keep,
                };
                let t = start.elapsed().as_millis();
                let v = engine
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_key(key, down, t);
                if debug {
                    eprintln!(
                        "walkie: key @{t}ms {} {} → {:?}{}",
                        key.name(),
                        if down { "down" } else { "up" },
                        v.signal,
                        if v.swallow { " (swallowed)" } else { "" }
                    );
                }
                if let Some(s) = v.signal {
                    on_signal(s);
                }
                if v.swallow {
                    CallbackResult::Drop
                } else {
                    CallbackResult::Keep
                }
            },
        );
        let tap = match tap {
            Ok(t) => t,
            Err(()) => {
                let msg = "waiting for Accessibility — walkie starts listening once it's granted";
                let mut error = status.error.lock().unwrap();
                if error.is_none() {
                    eprintln!("walkie: keyboard hook couldn't start: {msg}");
                }
                *error = Some(msg.into());
                drop(error);
                std::thread::sleep(RETRY);
                continue;
            }
        };
        *status.error.lock().unwrap() = None;
        TAP_PORT.store(
            tap.mach_port().as_concrete_TypeRef() as *mut _,
            Ordering::SeqCst,
        );
        let source = tap
            .mach_port()
            .create_runloop_source(0)
            .expect("runloop source");
        CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
        tap.enable();
        status.running.store(true, Ordering::SeqCst);
        eprintln!("walkie: keyboard hook running");
        CFRunLoop::run_current();
        status.running.store(false, Ordering::SeqCst);
        return;
    });
}
