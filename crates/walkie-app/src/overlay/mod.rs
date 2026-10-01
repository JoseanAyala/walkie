//! The overlay pill as a panel: the webview stays, but its NSWindow becomes a
//! non-activating NSPanel — never key, on every Space, over full-screen apps,
//! click-through — and is shown without Tauri's `makeKeyAndOrderFront`.
//!
//! Only an NSPanel with the non-activating style mask may sit on another
//! app's full-screen Space; a plain NSWindow with the same collection
//! behavior stays hidden there. Tauri can't make panels, so the window's
//! class is swapped for an NSPanel subclass after it's built (as the
//! tauri-nspanel plugin does). See `macos`/`linux` for the platform specifics.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
mod platform;
pub use platform::{setup, show};
