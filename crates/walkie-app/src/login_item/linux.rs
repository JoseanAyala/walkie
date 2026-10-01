//! Launch at login via an XDG autostart `.desktop` file under
//! `$XDG_CONFIG_HOME/autostart` (default `~/.config/autostart`) — the
//! portable Linux equivalent of `SMAppService`. The file's presence is the
//! only state; nothing is cached, same as macOS.

use super::Status;
use std::fs;
use std::path::{Path, PathBuf};

const DESKTOP_FILE: &str = "walkie.desktop";
/// Appended to the `Exec=` line so a launch from the desktop session's
/// autostart can tell itself apart from one started by hand.
const AUTOSTARTED_ARG: &str = "--autostarted";

/// An XDG autostart entry works from anywhere — unlike `SMAppService`,
/// there's no app-bundle requirement — so this is always true; kept only
/// for symmetry with macOS's gate in `mod.rs`.
pub fn bundled() -> bool {
    std::env::current_exe().is_ok_and(|p| !in_cargo_target(&p))
}

/// A dev build (`make run`, from `target/debug`): registering it would
/// launch a stale build at every login, so it counts as unbundled like on
/// macOS.
fn in_cargo_target(exe: &Path) -> bool {
    let parts: Vec<_> = exe.components().map(|c| c.as_os_str()).collect();
    parts
        .windows(2)
        .any(|w| w[0] == "target" && (w[1] == "debug" || w[1] == "release"))
}

pub fn status() -> Status {
    match autostart_dir() {
        Some(dir) => status_in(&dir),
        None => Status::NotFound,
    }
}

pub fn set(enabled: bool) -> Result<(), String> {
    let dir = autostart_dir().ok_or("no $HOME to write an autostart entry under")?;
    set_in(&dir, enabled, &exec_path())
}

/// Whether argv carries the flag the autostart entry's `Exec=` line adds —
/// i.e. whether the desktop session started us, not a person opening the
/// app. Unlike macOS's Apple-event check (good only during startup), argv
/// stays readable for the life of the process, so this is always a
/// definite answer.
pub fn launched_at_login() -> Option<bool> {
    Some(std::env::args().any(|a| a == AUTOSTARTED_ARG))
}

/// `status()`/`set()`'s file logic, with the autostart directory as a
/// parameter instead of read from the environment — so tests can pass a
/// temp dir instead of touching the real `$XDG_CONFIG_HOME`.
fn status_in(dir: &Path) -> Status {
    if dir.join(DESKTOP_FILE).is_file() {
        Status::Enabled
    } else {
        Status::Disabled
    }
}

fn set_in(dir: &Path, enabled: bool, exec: &Path) -> Result<(), String> {
    let path = dir.join(DESKTOP_FILE);
    if enabled {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        fs::write(&path, desktop_entry(exec)).map_err(|e| e.to_string())
    } else {
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn desktop_entry(exec: &Path) -> String {
    let exec = exec.display().to_string();
    // No embedded spaces or %-field codes expected in practice (a Nix
    // store path or a profile's bin dir has none); quote defensively
    // anyway, per the Desktop Entry spec's Exec key.
    let exec = if exec.contains(' ') {
        format!("\"{exec}\"")
    } else {
        exec
    };
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Walkie\n\
         Comment=Dictation hotkey and tray icon\n\
         Exec={exec} {AUTOSTARTED_ARG}\n\
         Terminal=false\n"
    )
}

/// Where the autostart entry goes: `$XDG_CONFIG_HOME/autostart`, or
/// `~/.config/autostart` per the XDG base directory spec's default.
fn autostart_dir() -> Option<PathBuf> {
    autostart_dir_from(
        std::env::var("XDG_CONFIG_HOME").ok(),
        std::env::var("HOME").ok(),
    )
}

/// `autostart_dir()`'s logic, with the two env vars as parameters instead
/// of read directly — so tests can pin down the XDG fallback without
/// mutating process-wide env (which other tests may be reading at the same
/// time).
fn autostart_dir_from(xdg_config_home: Option<String>, home: Option<String>) -> Option<PathBuf> {
    let base = xdg_config_home
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("autostart"))
}

/// The path to put in the `Exec=` line.
fn exec_path() -> PathBuf {
    let current = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("walkie"));
    let path_dirs = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    stable_exe(&current, &path_dirs, |p| p.is_file())
}

/// `current_exe()` is usually the right answer, but on NixOS it's a
/// `/nix/store/...` path that stops existing after the next rebuild or
/// garbage collection — not something to hard-code into a file that's
/// meant to outlive this process. When that's where we're running from,
/// prefer a stable copy of `walkie` elsewhere on `PATH` (typically
/// `~/.nix-profile/bin`, `/run/current-system/sw/bin`, or
/// `/etc/profiles/per-user/$USER/bin` — all commonly on `PATH` already on a
/// NixOS/home-manager system), falling back to `current` if none turns up.
fn stable_exe(current: &Path, candidates: &[PathBuf], exists: impl Fn(&Path) -> bool) -> PathBuf {
    if !in_nix_store(current) {
        return current.to_path_buf();
    }
    candidates
        .iter()
        .map(|dir| dir.join("walkie"))
        .find(|p| !in_nix_store(p) && exists(p))
        .unwrap_or_else(|| current.to_path_buf())
}

fn in_nix_store(p: &Path) -> bool {
    p.starts_with("/nix/store")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cargo_build_is_not_bundled() {
        assert!(in_cargo_target(Path::new(
            "/home/me/dev/walkie/target/debug/walkie"
        )));
        assert!(!in_cargo_target(Path::new(
            "/etc/profiles/per-user/me/bin/walkie"
        )));
    }
    use tempfile::tempdir;

    #[test]
    fn a_test_run_is_not_autostarted() {
        // cargo test's argv never carries --autostarted.
        assert_eq!(launched_at_login(), Some(false));
    }

    #[test]
    fn keeps_a_non_store_exe_as_is() {
        let p = Path::new("/home/me/dev/walkie/target/debug/walkie");
        assert_eq!(stable_exe(p, &[], |_| true), p);
    }

    #[test]
    fn prefers_the_first_existing_path_entry_outside_the_store() {
        let store = Path::new("/nix/store/abc123-walkie-0.7.0/bin/walkie");
        let candidates = vec![
            PathBuf::from("/run/current-system/sw/bin"),
            PathBuf::from("/home/me/.nix-profile/bin"),
        ];
        let want = PathBuf::from("/home/me/.nix-profile/bin/walkie");
        let got = stable_exe(store, &candidates, |p| p == want);
        assert_eq!(got, want);
    }

    #[test]
    fn skips_a_path_entry_that_is_itself_in_the_store() {
        let store = Path::new("/nix/store/abc123-walkie-0.7.0/bin/walkie");
        // e.g. a profile dir that's itself a symlink resolving into the
        // store: still inside /nix/store, so no more stable than `current`.
        let candidates = vec![PathBuf::from("/nix/store/other-profile/bin")];
        let got = stable_exe(store, &candidates, |_| true);
        assert_eq!(got, store);
    }

    #[test]
    fn falls_back_to_current_exe_when_nothing_is_found() {
        let store = Path::new("/nix/store/abc123-walkie-0.7.0/bin/walkie");
        let candidates = vec![PathBuf::from("/run/current-system/sw/bin")];
        let got = stable_exe(store, &candidates, |_| false);
        assert_eq!(got, store);
    }

    #[test]
    fn quotes_an_exec_path_containing_spaces() {
        let entry = desktop_entry(Path::new("/home/me/My Apps/walkie"));
        assert!(entry.contains("Exec=\"/home/me/My Apps/walkie\" --autostarted"));
    }

    #[test]
    fn a_fresh_directory_reads_as_disabled() {
        let dir = tempdir().unwrap();
        assert_eq!(status_in(dir.path()), Status::Disabled);
    }

    #[test]
    fn enabling_writes_a_desktop_entry_and_reads_back_as_enabled() {
        let dir = tempdir().unwrap();
        let exec = Path::new("/home/me/.nix-profile/bin/walkie");

        set_in(dir.path(), true, exec).unwrap();

        assert_eq!(status_in(dir.path()), Status::Enabled);
        let contents = fs::read_to_string(dir.path().join(DESKTOP_FILE)).unwrap();
        assert!(contents.contains("[Desktop Entry]"));
        assert!(contents.contains("Exec=/home/me/.nix-profile/bin/walkie --autostarted"));
    }

    #[test]
    fn disabling_removes_the_desktop_entry() {
        let dir = tempdir().unwrap();
        let exec = Path::new("/usr/bin/walkie");

        set_in(dir.path(), true, exec).unwrap();
        assert_eq!(status_in(dir.path()), Status::Enabled);

        set_in(dir.path(), false, exec).unwrap();
        assert_eq!(status_in(dir.path()), Status::Disabled);
    }

    #[test]
    fn disabling_when_already_disabled_is_not_an_error() {
        let dir = tempdir().unwrap();
        assert!(set_in(dir.path(), false, Path::new("/usr/bin/walkie")).is_ok());
    }

    #[test]
    fn prefers_xdg_config_home_over_the_home_fallback() {
        let got = autostart_dir_from(Some("/custom/config".into()), Some("/home/me".into()));
        assert_eq!(got, Some(PathBuf::from("/custom/config/autostart")));
    }

    #[test]
    fn falls_back_to_home_dot_config_when_xdg_config_home_is_unset_or_empty() {
        let via_home = "/home/me".to_string();
        assert_eq!(
            autostart_dir_from(None, Some(via_home.clone())),
            Some(PathBuf::from("/home/me/.config/autostart"))
        );
        assert_eq!(
            autostart_dir_from(Some(String::new()), Some(via_home)),
            Some(PathBuf::from("/home/me/.config/autostart"))
        );
    }

    #[test]
    fn no_home_and_no_xdg_config_home_is_unresolvable() {
        assert_eq!(autostart_dir_from(None, None), None);
    }
}
