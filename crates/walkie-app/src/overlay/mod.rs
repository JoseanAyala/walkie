//! The overlay pill as a panel: it must never take keyboard focus (the
//! transcript is typed into whatever's focused, not the pill), must stay on
//! top of everything else including another app's full-screen window, and
//! must be click-through (nothing in it is interactive).
//!
//! - macOS: the webview stays, but its NSWindow's class is swapped for a
//!   non-activating NSPanel subclass after it's built (as the tauri-nspanel
//!   plugin does) — only such a panel may sit on another app's full-screen
//!   Space — and it's shown without Tauri's `makeKeyAndOrderFront`. See
//!   `macos` for the NSWindowCollectionBehavior/level details.
//! - Linux: a real layer-shell surface (wlr-layer-shell, e.g. Hyprland) on
//!   the Overlay layer with keyboard interactivity off gives the same
//!   guarantees, enforced by the compositor; elsewhere it falls back to a
//!   plain always-on-top, non-focusable window. See `linux`.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
mod platform;
pub use platform::{setup, show};
