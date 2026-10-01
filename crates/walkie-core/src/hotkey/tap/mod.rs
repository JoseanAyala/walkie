//! The OS keyboard hook that feeds `Engine`: an active tap/hook that turns
//! raw key events into `Signal`s and drops the ones the engine says to
//! swallow. Kept thin on purpose — all decisions live in the engine, which
//! is unit-tested. See `macos`/`linux` for how each platform gets its events.

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Mutex;

#[derive(Default)]
pub struct TapStatus {
    pub running: AtomicBool,
    /// Times macOS disabled the tap (callback too slow) and we re-enabled it.
    pub reenabled: AtomicU32,
    pub error: Mutex<Option<String>>,
}

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
mod platform;
pub use platform::{spawn, DEFAULT_DICTATE_KEY, DEFAULT_PASTE_LAST};
