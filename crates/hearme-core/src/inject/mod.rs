use anyhow::{Context, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

pub trait Injector {
    fn inject(&mut self, text: &str) -> Result<()>;
}

/// Save clipboard → set text → paste keystroke → restore clipboard.
/// Fast and accent-safe (Spanish text arrives as one paste, not keystrokes).
pub struct PasteInjector {
    pub restore_ms: u64,
}

impl Injector for PasteInjector {
    fn inject(&mut self, text: &str) -> Result<()> {
        let mut cb = arboard::Clipboard::new().context("clipboard unavailable")?;
        let saved = cb.get_text().ok();
        cb.set_text(text.to_string()).context("setting clipboard")?;

        let paste = || -> Result<()> {
            let mut enigo = Enigo::new(&Settings::default()).context("enigo init — check Accessibility permission")?;
            let modk = if cfg!(target_os = "macos") { Key::Meta } else { Key::Control };
            enigo.key(modk, Direction::Press)?;
            enigo.key(Key::Unicode('v'), Direction::Click)?;
            enigo.key(modk, Direction::Release)?;
            Ok(())
        };
        if let Err(e) = paste() {
            // Leave the transcript on the clipboard so no words are lost.
            anyhow::bail!("paste blocked ({e}); text left on clipboard — press ⌘V manually");
        }

        std::thread::sleep(std::time::Duration::from_millis(self.restore_ms));
        if let Some(old) = saved {
            let _ = cb.set_text(old);
        }
        Ok(())
    }
}

/// Per-app fallback for paste-hostile targets: type the text as keystrokes.
pub struct TypeInjector;

impl Injector for TypeInjector {
    fn inject(&mut self, text: &str) -> Result<()> {
        let type_it = || -> Result<()> {
            let mut enigo = Enigo::new(&Settings::default()).context("enigo init — check Accessibility permission")?;
            enigo.text(text)?;
            Ok(())
        };
        if let Err(e) = type_it() {
            let saved_to_clipboard = arboard::Clipboard::new()
                .and_then(|mut cb| cb.set_text(text.to_string()))
                .is_ok();
            if saved_to_clipboard {
                anyhow::bail!("typing blocked ({e}); text left on clipboard — press ⌘V manually");
            }
            anyhow::bail!("typing blocked ({e}) and clipboard fallback also failed; text lost: {text}");
        }
        Ok(())
    }
}
