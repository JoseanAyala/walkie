//! Sends stderr to `~/Library/Logs/walkie/walkie.log` (Linux:
//! `$XDG_STATE_HOME/walkie/walkie.log`) when there's no
//! terminal to see it. Every diagnostic in the app is an `eprintln!`, and a
//! bundle launched from Finder or `open` would otherwise drop them all.

use std::fs::OpenOptions;
use std::io::{IsTerminal, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

/// Past this size the log is started over on the next launch.
const MAX_BYTES: u64 = 5 * 1024 * 1024;

/// `WALKIE_LOG` overrides the location (the OS e2e suite uses a temp file).
pub fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("WALKIE_LOG") {
        return Some(PathBuf::from(p));
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    if cfg!(target_os = "macos") {
        return Some(home.join("Library/Logs/walkie/walkie.log"));
    }
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/state"));
    Some(state.join("walkie/walkie.log"))
}

/// Redirects stderr to the log file unless stderr is a terminal
/// (`cargo tauri dev`), where the output is already visible.
pub fn init() {
    if std::io::stderr().is_terminal() {
        return;
    }
    let Some(path) = path() else { return };
    if let Err(e) = redirect(&path) {
        eprintln!("walkie: could not open log file {}: {e}", path.display());
    }
}

fn redirect(path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    if needs_rotation(file.metadata()?.len()) {
        file.set_len(0)?;
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    writeln!(
        file,
        "--- walkie {} started (unix {secs}) ---",
        env!("CARGO_PKG_VERSION")
    )?;
    // SAFETY: both fds are valid; dup2 atomically replaces fd 2, and `file`
    // can be dropped afterwards because fd 2 holds its own reference.
    if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

fn needs_rotation(len: u64) -> bool {
    len > MAX_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_only_past_the_cap() {
        assert!(!needs_rotation(0));
        assert!(!needs_rotation(MAX_BYTES));
        assert!(needs_rotation(MAX_BYTES + 1));
    }

    #[test]
    fn log_lives_under_library_logs() {
        if std::env::var_os("WALKIE_LOG").is_some() {
            return;
        }
        let p = path().unwrap();
        if cfg!(target_os = "macos") {
            assert!(p.ends_with("Library/Logs/walkie/walkie.log"));
        } else {
            assert!(p.ends_with("walkie/walkie.log"));
        }
    }
}
