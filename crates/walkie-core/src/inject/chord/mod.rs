//! The modifier + letter chords walkie presses in the focused app (paste,
//! copy, select all), where the platform has a better way than enigo.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
mod platform;
pub use platform::send;
