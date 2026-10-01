pub mod focus;

use anyhow::{Context, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::sync::{mpsc, Arc};

/// Runs a closure on the app's main thread. Keystroke synthesis must: enigo
/// resolves characters through the keyboard layout (TSMGetInputSourceProperty),
/// which macOS asserts is main-thread-only once the input source has changed
/// (e.g. after a 🌐 tap) — off the main thread that's a SIGTRAP, and the
/// dictation is lost with the app.
pub type MainThread = Arc<dyn Fn(Box<dyn FnOnce() + Send>) + Send + Sync>;

/// Runs `f` via `main` (if given) and waits for its result.
fn on_main<T: Send + 'static>(
    main: &Option<MainThread>,
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    match main {
        None => f(),
        Some(run) => {
            let (tx, rx) = mpsc::sync_channel(1);
            run(Box::new(move || {
                let _ = tx.send(f());
            }));
            rx.recv()
                .context("main thread never ran the keystroke job")?
        }
    }
}

/// Stamped on every key event walkie synthesizes (EVENT_SOURCE_USER_DATA), so
/// the keyboard hook can ignore them. It must: enigo types text as a keyDown
/// with no keyUp, which otherwise reads as a key held forever, and no
/// shortcut matches again until that key is pressed for real.
pub const SYNTHETIC_EVENT_MARKER: i64 = 0x7761_6c6b_6965; // "walkie"

fn enigo_settings() -> Settings {
    Settings {
        event_source_user_data: Some(SYNTHETIC_EVENT_MARKER),
        x11_display: disable_x11_fallback(),
        ..Settings::default()
    }
}

/// On Linux, enigo is built with both its Wayland and X11 backends (see
/// `walkie-core/Cargo.toml`); if both connect it fires every keystroke
/// through both of them, which on Hyprland means typing twice — XWayland
/// leaves `$DISPLAY` set on a pure-Wayland session, so the X11 backend
/// connects too unless stopped. When `$WAYLAND_DISPLAY` is set, hand enigo
/// a syntactically invalid X11 display name: `x11rb::connect` rejects it in
/// `parse_display`, before any socket I/O, so the X11 connection reliably
/// fails and only the Wayland virtual-keyboard path runs. Settings has no
/// "don't even try X11" switch, so this is the only lever available.
#[cfg(target_os = "linux")]
fn disable_x11_fallback() -> Option<String> {
    x11_display_override(std::env::var_os("WAYLAND_DISPLAY").is_some())
}

/// The invalid-on-purpose display name `disable_x11_fallback` passes
/// through, split out so it's testable without touching the real
/// environment (tests run in parallel and share it).
#[cfg(target_os = "linux")]
fn x11_display_override(wayland_session: bool) -> Option<String> {
    wayland_session.then(|| "walkie-no-x11".to_string())
}

#[cfg(not(target_os = "linux"))]
fn disable_x11_fallback() -> Option<String> {
    None
}

fn enigo() -> Result<Enigo> {
    Enigo::new(&enigo_settings()).context("enigo init — check Accessibility permission")
}

/// How the text was delivered — both are success; neither loses words.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Injected {
    Typed,
    /// Nothing editable had focus: the text was left on the clipboard instead.
    CopiedNoField,
}

pub trait Injector {
    fn inject(&mut self, text: &str) -> Result<Injected>;
    /// The focused field's text, for polish: its selection or, with nothing
    /// selected, all of it. Either way it's left selected, so the next
    /// `inject` replaces it. None when there's no text to be had. The
    /// clipboard is back to what it was on return.
    fn grab(&mut self) -> Result<Option<String>>;
}

/// How long a ⌘C gets to reach the clipboard: the app answers it on its own
/// main thread, so it isn't instant.
const COPY_WAIT_MS: u64 = 500;

/// Presses ⌘ (Ctrl elsewhere) + `ch` in the focused app. For a paste ('v')
/// on Linux, adds Shift when the focused window is a terminal that binds
/// plain Ctrl+V to something else of its own and uses Ctrl+Shift+V instead
/// (see `focus::wants_shift_paste`).
fn shortcut(main: &Option<MainThread>, ch: char) -> Result<()> {
    on_main(main, move || {
        let mut enigo = enigo()?;
        let modk = if cfg!(target_os = "macos") {
            Key::Meta
        } else {
            Key::Control
        };
        let shift = ch == 'v' && focus::wants_shift_paste();
        enigo.key(modk, Direction::Press)?;
        if shift {
            enigo.key(Key::Shift, Direction::Press)?;
        }
        let r = enigo.key(Key::Unicode(ch), Direction::Click);
        if shift {
            enigo.key(Key::Shift, Direction::Release)?;
        }
        enigo.key(modk, Direction::Release)?; // never leave Cmd stuck down
        Ok(r?)
    })
}

/// Copies from the focused app: the text that lands on the (emptied)
/// clipboard, or None if nothing does in time — nothing was selected.
fn copy(cb: &mut arboard::Clipboard, main: &Option<MainThread>) -> Result<Option<String>> {
    cb.clear().context("clearing clipboard")?;
    shortcut(main, 'c')?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(COPY_WAIT_MS);
    loop {
        if let Ok(t) = cb.get_text() {
            if !t.is_empty() {
                return Ok(Some(t));
            }
        }
        if std::time::Instant::now() > deadline {
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// `Injector::grab` for the real focused app. Asks Accessibility for the
/// selection first; where it can't tell, a ⌘C does (it copies only when
/// something is selected). Nothing selected → ⌘A, ⌘C.
fn grab_focused(main: &Option<MainThread>) -> Result<Option<String>> {
    if focus::current() == focus::Focus::NoField {
        return Ok(None);
    }
    let selected = focus::selected_text();
    if let Some(sel) = selected.as_ref().filter(|s| !s.is_empty()) {
        return Ok(Some(sel.clone()));
    }
    let mut cb = arboard::Clipboard::new().context("clipboard unavailable")?;
    let saved = cb.get_text().ok();
    let mut grab = || -> Result<Option<String>> {
        if selected.is_none() {
            if let Some(t) = copy(&mut cb, main)? {
                return Ok(Some(t));
            }
        }
        shortcut(main, 'a')?;
        copy(&mut cb, main)
    };
    let grabbed = grab();
    let _ = match saved {
        Some(old) => cb.set_text(old),
        None => cb.clear(),
    };
    grabbed
}

/// If focus clearly isn't a text field, puts `text` on the clipboard (not
/// restored later) and says so; otherwise None and the caller injects.
fn copy_if_no_field(text: &str) -> Result<Option<Injected>> {
    if focus::current() != focus::Focus::NoField {
        return Ok(None);
    }
    let mut cb = arboard::Clipboard::new().context("clipboard unavailable")?;
    cb.set_text(text.to_string()).context("setting clipboard")?;
    Ok(Some(Injected::CopiedNoField))
}

/// Save clipboard → set text → paste keystroke → restore clipboard.
/// Fast and accent-safe (Spanish text arrives as one paste, not keystrokes).
///
/// On Linux/Wayland (the `wayland-data-control` feature), `arboard`'s
/// `set_text` hands the content to `wl-clipboard-rs`, which by default
/// forks a short-lived background process to keep serving it to whoever
/// asks — the same thing the `wl-copy` CLI does — so the text (both the
/// transcript and, moments later, the restored original) stays on the
/// clipboard after `cb` is dropped at the end of this function, without
/// blocking this thread. That's `arboard::SetExtLinux`'s *default*
/// behavior; its `.wait()`/`.wait_until()` opt-in does the opposite —
/// blocks the caller in the foreground until the clipboard is next
/// overwritten — which would hang the session's worker thread, so this
/// code must never call them.
pub struct PasteInjector {
    pub restore_ms: u64,
    pub main: Option<MainThread>,
}

impl Injector for PasteInjector {
    fn inject(&mut self, text: &str) -> Result<Injected> {
        if let Some(copied) = copy_if_no_field(text)? {
            return Ok(copied);
        }
        let mut cb = arboard::Clipboard::new().context("clipboard unavailable")?;
        let saved = cb.get_text().ok();
        cb.set_text(text.to_string()).context("setting clipboard")?;

        if let Err(e) = shortcut(&self.main, 'v') {
            // Leave the transcript on the clipboard so no words are lost.
            anyhow::bail!("paste blocked ({e}); text left on clipboard — press ⌘V manually");
        }

        std::thread::sleep(std::time::Duration::from_millis(self.restore_ms));
        if let Some(old) = saved {
            let _ = cb.set_text(old);
        }
        Ok(Injected::Typed)
    }

    fn grab(&mut self) -> Result<Option<String>> {
        grab_focused(&self.main)
    }
}

/// Per-app fallback for paste-hostile targets: type the text as keystrokes.
pub struct TypeInjector {
    pub main: Option<MainThread>,
}

impl Injector for TypeInjector {
    fn inject(&mut self, text: &str) -> Result<Injected> {
        if let Some(copied) = copy_if_no_field(text)? {
            return Ok(copied);
        }
        let owned = text.to_string();
        let type_it = move || -> Result<()> {
            let mut enigo = enigo()?;
            enigo.text(&owned)?;
            Ok(())
        };
        if let Err(e) = on_main(&self.main, type_it) {
            let saved_to_clipboard = arboard::Clipboard::new()
                .and_then(|mut cb| cb.set_text(text.to_string()))
                .is_ok();
            if saved_to_clipboard {
                anyhow::bail!("typing blocked ({e}); text left on clipboard — press ⌘V manually");
            }
            anyhow::bail!(
                "typing blocked ({e}) and clipboard fallback also failed; text lost: {text}"
            );
        }
        Ok(Injected::Typed)
    }

    fn grab(&mut self) -> Result<Option<String>> {
        grab_focused(&self.main)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn synthesized_events_carry_the_marker() {
        assert_eq!(
            enigo_settings().event_source_user_data,
            Some(SYNTHETIC_EVENT_MARKER)
        );
    }

    #[test]
    fn on_main_runs_the_job_through_the_runner_and_returns_its_result() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        // Stands in for Tauri's run_on_main_thread: runs the job elsewhere.
        let runner: MainThread = Arc::new(move |job| {
            c.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(job);
        });
        let main_id = std::thread::current().id();
        let ran_on = on_main(&Some(runner), move || {
            Ok(std::thread::current().id() != main_id)
        })
        .unwrap();
        assert!(ran_on, "job should run on the runner's thread");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn on_main_without_a_runner_runs_inline() {
        assert_eq!(on_main(&None, || Ok(7)).unwrap(), 7);
    }

    #[test]
    fn a_runner_that_drops_the_job_is_an_error_not_a_panic() {
        let runner: MainThread = Arc::new(drop);
        assert!(on_main(&Some(runner), || Ok(())).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn x11_fallback_is_disabled_only_on_a_wayland_session() {
        assert_eq!(x11_display_override(true), Some("walkie-no-x11".into()));
        assert_eq!(x11_display_override(false), None);
    }
}
