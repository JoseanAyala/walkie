use tauri::WebviewWindow;

use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

/// How far the pill's bottom edge sits above the screen's bottom edge.
/// Matches the cross-platform bottom-center math in `glue::position_overlay`
/// (`s.height + 120` against the monitor height), so a compositor without
/// layer-shell (which falls back to that `set_position` call) and one with
/// it (this margin) land the pill in the same spot.
const BOTTOM_MARGIN: i32 = 120;

/// Reported to the compositor for this layer-shell surface — shows up in
/// layer-aware tools (e.g. `hyprctl layers`) when tracing down a stray pill.
const NAMESPACE: &str = "walkie-overlay";

/// Once, at startup: make the overlay behave like the macOS NSPanel (see
/// `macos.rs`) — never takes keyboard focus, stays on top of everything
/// including another app's full-screen window, and is click-through.
///
/// On a Wayland compositor with the wlr layer-shell protocol (Hyprland and
/// other wlroots compositors — this project's dev target), the window
/// becomes a real layer-shell surface on the `Overlay` layer with keyboard
/// interactivity turned off: the same guarantee the NSPanel gives, enforced
/// by the compositor rather than GTK's own (advisory) focus hints.
///
/// `gtk_layer_shell::init_layer_shell` must run before the GTK window is
/// realized/mapped, which this does: the overlay window is declared
/// `"visible": false` in tauri.conf.json, so tao's window constructor calls
/// `window.hide()` rather than `window.show_all()` (tao 0.35.3
/// `src/platform_impl/linux/window.rs:181,208-212`) and the window is never
/// realized until something shows it. Nothing does before `setup` runs:
/// `setup` is called from `main.rs`'s `.setup(|app| ...)` closure, which
/// `tauri` invokes synchronously on `RuntimeRunEvent::Ready` (tauri 2.11.5
/// `src/app.rs:1423-1424`) — before `main.rs` shows the *settings* window
/// and before `glue::start` (called right after) spins up the event-pump
/// thread that's the only thing that ever shows the overlay.
///
/// Elsewhere (X11, or a Wayland compositor without layer-shell), this
/// leaves the window a plain always-on-top, non-focusable, taskbar-skipped
/// window. `tauri.conf.json` additionally sets `"focusable": false` for the
/// overlay so tao's one-shot "restore accept-focus after first paint" dance
/// (`window.rs:216-227`, armed whenever `focusable && !focused`) never
/// arms in the first place and silently re-enables focus later.
pub fn setup(w: &WebviewWindow) {
    let _ = w.set_focusable(false);
    let _ = w.set_always_on_top(true);
    let _ = w.set_skip_taskbar(true);

    with_gtk_window(w, |gtk_win| {
        // WALKIE_OVERLAY_PLAIN: skip layer-shell, to tell its problems
        // apart from the webview's.
        if std::env::var_os("WALKIE_OVERLAY_PLAIN").is_some() {
            eprintln!("walkie: overlay: WALKIE_OVERLAY_PLAIN set, plain window");
            return;
        }
        if !gtk_layer_shell::is_supported() {
            eprintln!(
                "walkie: overlay: no wlr-layer-shell here, falling back to a plain always-on-top window"
            );
            return;
        }
        gtk_win.init_layer_shell();
        gtk_win.set_namespace(NAMESPACE);
        gtk_win.set_layer(Layer::Overlay);
        gtk_win.set_keyboard_mode(KeyboardMode::None);
        // Anchored only to the bottom: the compositor centers the surface
        // horizontally, matching `position_overlay`'s bottom-center math.
        gtk_win.set_anchor(Edge::Bottom, true);
        gtk_win.set_layer_shell_margin(Edge::Bottom, BOTTOM_MARGIN);
        // The pill shouldn't reserve space, nor get pushed around by other
        // surfaces' exclusive zones (a bar reserving space at the edges).
        gtk_win.set_exclusive_zone(-1);
    });
}

/// Brings the pill up without stealing focus. `w.show()` alone is most of
/// it — the window is never focusable to begin with (see `setup`) — but
/// click-through (`set_ignore_cursor_events`) needs the window's GDK window,
/// which only exists once it's realized; tao's handler for it unconditionally
/// unwraps that (tao 0.35.3 `src/platform_impl/linux/event_loop.rs:453-457`),
/// so it's applied here, after the first `show()` has realized the window,
/// rather than in `setup` where the window is deliberately still unrealized.
/// Reapplying it on every call is harmless (and cheap).
pub fn show(w: &WebviewWindow) {
    let _ = w.show();
    let _ = w.set_ignore_cursor_events(true);
}

/// Runs `f` on the main thread (GTK requires it) with the window's
/// `gtk::ApplicationWindow`, mirroring macOS's `with_ns_window`: the only
/// caller of `setup`/`show` that matters here is the event-pump thread
/// (`glue::start` spawns it; it isn't the main thread), so every call into
/// real GTK objects — as opposed to Tauri's own dispatcher methods like
/// `set_focusable`, which already hop threads on their own — needs this.
fn with_gtk_window(w: &WebviewWindow, f: impl FnOnce(&gtk::ApplicationWindow) + Send + 'static) {
    let w2 = w.clone();
    let _ = w.run_on_main_thread(move || {
        if let Ok(gtk_win) = w2.gtk_window() {
            f(&gtk_win);
        }
    });
}
