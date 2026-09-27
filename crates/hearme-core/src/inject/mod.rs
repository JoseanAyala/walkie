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
fn on_main<T: Send + 'static>(main: &Option<MainThread>, f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    match main {
        None => f(),
        Some(run) => {
            let (tx, rx) = mpsc::sync_channel(1);
            run(Box::new(move || {
                let _ = tx.send(f());
            }));
            rx.recv().context("main thread never ran the keystroke job")?
        }
    }
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

        let paste = || -> Result<()> {
            let mut enigo = Enigo::new(&Settings::default()).context("enigo init — check Accessibility permission")?;
            let modk = if cfg!(target_os = "macos") { Key::Meta } else { Key::Control };
            enigo.key(modk, Direction::Press)?;
            let v = enigo.key(Key::Unicode('v'), Direction::Click);
            enigo.key(modk, Direction::Release)?; // never leave Cmd stuck down
            Ok(v?)
        };
        if let Err(e) = on_main(&self.main, paste) {
            // Leave the transcript on the clipboard so no words are lost.
            anyhow::bail!("paste blocked ({e}); text left on clipboard — press ⌘V manually");
        }

        std::thread::sleep(std::time::Duration::from_millis(self.restore_ms));
        if let Some(old) = saved {
            let _ = cb.set_text(old);
        }
        Ok(Injected::Typed)
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
            let mut enigo = Enigo::new(&Settings::default()).context("enigo init — check Accessibility permission")?;
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
            anyhow::bail!("typing blocked ({e}) and clipboard fallback also failed; text lost: {text}");
        }
        Ok(Injected::Typed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        let ran_on = on_main(&Some(runner), move || Ok(std::thread::current().id() != main_id)).unwrap();
        assert!(ran_on, "job should run on the runner's thread");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn on_main_without_a_runner_runs_inline() {
        assert_eq!(on_main(&None, || Ok(7)).unwrap(), 7);
    }

    #[test]
    fn a_runner_that_drops_the_job_is_an_error_not_a_panic() {
        let runner: MainThread = Arc::new(|job| drop(job));
        assert!(on_main(&Some(runner), || Ok(())).is_err());
    }
}
