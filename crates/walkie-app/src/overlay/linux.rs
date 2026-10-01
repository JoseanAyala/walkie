use tauri::WebviewWindow;

/// No AppKit here yet: the overlay stays a plain Tauri window (steals focus,
/// unlike macOS's NSPanel above) until this gets a Linux implementation.
// TODO(linux): gtk-layer-shell gives a layer-shell surface (Wayland) the
// same never-focused, always-on-top, every-workspace behavior the macOS
// NSPanel has; X11 would need its own window-manager hints instead.
pub fn setup(_w: &WebviewWindow) {}

pub fn show(w: &WebviewWindow) {
    let _ = w.show();
}
