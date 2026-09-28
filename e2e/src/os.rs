//! Drives the installed /Applications/hearme.app from the outside, the way a
//! person would: real key events posted through macOS (so they pass through
//! hearme's event tap), the tray menu and windows via UI scripting, and
//! a bare window (src/bin/typing-target.rs) as the app being dictated into.
//!
//! Each `App` runs against its own temp config/history/log dirs and plays a
//! WAV fixture instead of the microphone (HEARME_TEST_AUDIO), so the user's
//! real config and mic are never touched.

use crate::fixture;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use hearme_core::config::Config;
use hearme_core::hotkey::keys::{Key, Side};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- budget

/// Every OS test must finish within this. Past it, the run aborts and says
/// where it was stuck — a hang (a modal menu, a focus thief) fails in
/// seconds instead of sitting there until someone clicks.
pub const TEST_BUDGET: Duration = Duration::from_secs(10);

static SERIAL: Mutex<()> = Mutex::new(());
static STEP: Mutex<String> = Mutex::new(String::new());
static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Records what the test is doing, for the watchdog's report.
pub fn step(s: impl Into<String>) {
    *STEP.lock().unwrap_or_else(|e| e.into_inner()) = s.into();
}

pub struct TestGuard {
    _serial: MutexGuard<'static, ()>,
    done: Arc<AtomicBool>,
}

impl Drop for TestGuard {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
    }
}

/// Call first in every OS test: runs tests one at a time (one app, one
/// keyboard) and arms the TEST_BUDGET watchdog.
pub fn begin(name: &'static str) -> TestGuard {
    let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    step("starting");
    let done = Arc::new(AtomicBool::new(false));
    let d = done.clone();
    let started = Instant::now();
    std::thread::spawn(move || {
        while started.elapsed() < TEST_BUDGET {
            if d.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let at = STEP.lock().unwrap_or_else(|e| e.into_inner()).clone();
        eprintln!(
            "\n⏱  {name} took over {}s — stuck at: {at}",
            TEST_BUDGET.as_secs()
        );
        if let Some(log) = LOG.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            let text = std::fs::read_to_string(&log).unwrap_or_default();
            let tail: Vec<&str> = text.lines().rev().take(15).collect();
            eprintln!("app log tail ({}):", log.display());
            for l in tail.into_iter().rev() {
                eprintln!("  {l}");
            }
        }
        quit();
        restore_login_item();
        std::process::exit(101);
    });
    TestGuard {
        _serial: serial,
        done,
    }
}

pub const APP: &str = "/Applications/hearme.app";

pub fn sleep(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// Runs AppleScript; Err carries osascript's stderr.
pub fn osa(script: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn wait_until(secs: u64, mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if f() {
            return true;
        }
        sleep(100);
    }
    f()
}

// ---------------------------------------------------------------- the app

static RUN: AtomicU32 = AtomicU32::new(0);

pub struct App {
    pub root: PathBuf,
    pub log: PathBuf,
}

impl Default for App {
    fn default() -> App {
        App::new()
    }
}

impl App {
    /// Fresh dirs; nothing launched yet.
    pub fn new() -> App {
        LOGIN_ITEM_BEFORE.get_or_init(|| login_item("status"));
        let root = std::env::temp_dir().join(format!(
            "hearme-os-e2e-{}-{}",
            std::process::id(),
            RUN.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let log = root.join("hearme.log");
        App { root, log }
    }

    /// A config for tests: onboarding done, small model, deterministic polish.
    pub fn test_config() -> Config {
        let mut c = Config {
            first_run: false,
            model: "base".into(),
            ..Default::default()
        };
        c.polish.command = "tr 'a-z' 'A-Z'".into();
        c
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config/hearme/config.toml")
    }

    pub fn write_config(&self, c: &Config) {
        c.save_to(&self.config_path()).unwrap();
    }

    pub fn config(&self) -> Config {
        Config::load_from(&self.config_path()).unwrap()
    }

    /// Adds a transcript to the app's history, as if dictated in an earlier run.
    pub fn seed_history(&self, text: &str) {
        let h = hearme_core::history::History::open(&self.root.join("data/hearme/history.sqlite3"))
            .unwrap();
        h.insert(text, text, None, Some("en"), 1000).unwrap();
    }

    /// The app's history (cleaned text), newest first.
    pub fn history(&self) -> Vec<String> {
        hearme_core::history::History::open(&self.root.join("data/hearme/history.sqlite3"))
            .and_then(|h| h.recent(100))
            .unwrap()
            .into_iter()
            .map(|r| r.cleaned)
            .collect()
    }

    pub fn launch(&self) {
        step("launching hearme");
        *LOG.lock().unwrap_or_else(|e| e.into_inner()) = Some(self.log.clone());
        quit();
        let _ = std::fs::remove_file(&self.log);
        let env = |k: &str, v: &PathBuf| format!("{k}={}", v.display());
        let status = Command::new("open")
            .args(["-n", "-a", APP])
            .arg("--env")
            .arg(env("XDG_CONFIG_HOME", &self.root.join("config")))
            .arg("--env")
            .arg(env("XDG_DATA_HOME", &self.root.join("data")))
            .arg("--env")
            .arg(env("HEARME_LOG", &self.log))
            .arg("--env")
            .arg(env("HEARME_TEST_AUDIO", &fixture("en.wav")))
            .arg("--env")
            .arg("HEARME_DEBUG_EVENTS=1")
            .status()
            .unwrap();
        assert!(
            status.success(),
            "couldn't launch {APP} — run e2e/run-app-tests.sh to build and install it"
        );
        step("waiting for hearme to start");
        assert!(
            self.wait_log("keyboard hook", 4),
            "hearme didn't start (no hook line in {:?}):\n{}",
            self.log,
            self.log_text()
        );
    }

    /// Opens hearme again while it runs, like clicking its Dock or Finder
    /// icon (macOS sends the running instance a reopen event).
    pub fn reopen(&self) {
        step("opening hearme again");
        let ok = Command::new("open").args(["-a", APP]).status().unwrap();
        assert!(ok.success(), "couldn't open {APP}");
    }

    pub fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    pub fn wait_log(&self, needle: &str, secs: u64) -> bool {
        step(format!("waiting for log line {needle:?}"));
        wait_until(secs, || self.log_text().contains(needle))
    }

    /// The startup status line for `label` ("Keyboard hook", …): Some(ok).
    pub fn status(&self, label: &str) -> Option<bool> {
        self.wait_log("hearme: status", 3); // logged ~1.5s after launch
        self.log_text().lines().find_map(|l| {
            let rest = l.strip_prefix("hearme: status ")?;
            let (ok, rest) = rest.split_at(4);
            rest.trim_start()
                .starts_with(&format!("{label}:"))
                .then_some(ok == "ok  ")
        })
    }

    /// For tests that press keys: the tap must be running and the model loaded.
    pub fn require_keyboard(&self) {
        require_globe_does_nothing();
        assert!(
            self.wait_log("keyboard hook running", 2),
            "hearme's keyboard hook isn't running. Grant Accessibility to {APP} \
             (System Settings → Privacy & Security → Accessibility) and rerun.\n{}",
            self.log_text()
        );
        assert!(
            self.wait_log("hearme: model ready", 4),
            "model never loaded:\n{}",
            self.log_text()
        );
    }

    pub fn windows(&self) -> Vec<String> {
        osa(
            r#"tell application "System Events" to tell application process "hearme" to get name of every window"#,
            &[],
        )
        .map(|s| s.split(", ").filter(|w| !w.is_empty()).map(String::from).collect())
        .unwrap_or_default()
    }

    pub fn wait_window(&self, name: &str, present: bool, secs: u64) -> bool {
        step(format!(
            "waiting for window {name:?} to be {}",
            if present { "shown" } else { "gone" }
        ));
        wait_until(secs, || self.windows().iter().any(|w| w == name) == present)
    }

    /// Clicks a tray menu item, e.g. "Settings…".
    pub fn tray(&self, item: &str) {
        step(format!("tray → {item}"));
        // Clicking a status item runs the menu modally, and System Events
        // doesn't answer until the menu closes (≈5s, or until focus moves).
        // So open it without waiting, then pick the item in a second call.
        osa(
            r#"ignoring application responses
                tell application "System Events" to tell application process "hearme" to click menu bar item 1 of menu bar 2
            end ignoring"#,
            &[],
        )
        .unwrap_or_else(|e| panic!("opening the tray menu: {e}"));
        let mut r = Err(String::new());
        let ok = wait_until(3, || {
            r = osa(
                r#"on run argv
                    tell application "System Events" to tell application process "hearme"
                        click menu item (item 1 of argv) of menu 1 of menu bar item 1 of menu bar 2
                    end tell
                end run"#,
                &[item],
            );
            r.is_ok()
        });
        assert!(ok, "tray item {item:?}: {r:?}");
    }

    /// Clicks the nth (1-based) button titled `title` inside a window's web content.
    pub fn click(&self, window: &str, title: &str, nth: usize) {
        step(format!("clicking {title:?} in {window:?}"));
        let mut r = Err(String::new());
        let found = wait_until(3, || {
            r = self.try_click(window, title, nth);
            r.as_deref() == Ok("ok")
        });
        assert!(found, "button {title:?} #{nth} in window {window:?}: {r:?}");
    }

    fn try_click(&self, window: &str, title: &str, nth: usize) -> Result<String, String> {
        osa(
            r#"on run argv
                set wname to item 1 of argv
                set target to item 2 of argv
                set want to (item 3 of argv) as integer
                set seen to 0
                tell application "System Events" to tell application process "hearme"
                    -- materialize the list first: iterating `entire contents`
                    -- inline yields references System Events can't resolve
                    set els to entire contents of window wname
                    repeat with e in els
                        try
                            -- a button with aria-pressed is an AXCheckBox/AXToggle
                            set isButton to role of e is "AXButton"
                            if not isButton then
                                try
                                    set isButton to subrole of e is "AXToggle"
                                end try
                            end if
                            if isButton then
                                -- web content labels buttons via AXTitle
                                set t to ""
                                try
                                    set t to title of e as text
                                end try
                                if t is target then
                                    set seen to seen + 1
                                    if seen is want then
                                        click e
                                        return "ok"
                                    end if
                                end if
                            end if
                        end try
                    end repeat
                end tell
                return "missing"
            end run"#,
            &[window, title, &nth.to_string()],
        )
    }

    /// All visible text in a window's web content.
    pub fn text(&self, window: &str) -> String {
        step(format!("reading text of {window:?}"));
        osa(
            r#"on run argv
                set out to ""
                tell application "System Events" to tell application process "hearme"
                    set els to entire contents of window (item 1 of argv)
                    repeat with e in els
                        try
                            if role of e is "AXStaticText" then set out to out & (value of e as text) & linefeed
                        end try
                    end repeat
                end tell
                return out
            end run"#,
            &[window],
        )
        .unwrap_or_default()
    }

    /// A checkbox's state (by its label) in a window's web content.
    pub fn checkbox(&self, window: &str, title: &str) -> Option<bool> {
        let r = self.checkbox_do(window, title, "get");
        r.ok().and_then(|v| match v.as_str() {
            "1" | "true" => Some(true),
            "0" | "false" => Some(false),
            _ => None,
        })
    }

    pub fn click_checkbox(&self, window: &str, title: &str) {
        step(format!("clicking checkbox {title:?} in {window:?}"));
        let mut r = Err(String::new());
        let found = wait_until(3, || {
            r = self.checkbox_do(window, title, "click");
            r.as_deref() == Ok("ok")
        });
        assert!(found, "checkbox {title:?} in window {window:?}: {r:?}");
    }

    fn checkbox_do(&self, window: &str, title: &str, what: &str) -> Result<String, String> {
        osa(
            r#"on run argv
                set target to item 2 of argv
                tell application "System Events" to tell application process "hearme"
                    set els to entire contents of window (item 1 of argv)
                    repeat with e in els
                        try
                            if role of e is "AXCheckBox" then
                                set t to ""
                                try
                                    set t to title of e as text
                                end try
                                if t is not target then
                                    try
                                        set t to description of e as text
                                    end try
                                end if
                                if t contains target then
                                    if item 3 of argv is "click" then
                                        click e
                                        return "ok"
                                    end if
                                    return value of e as text
                                end if
                            end if
                        end try
                    end repeat
                end tell
                return "missing"
            end run"#,
            &[window, title, what],
        )
    }

    /// Replaces the text of a field (found by its label) in a window's web
    /// content, as if typed: the page gets its input event.
    pub fn set_field(&self, window: &str, label: &str, text: &str) {
        step(format!("typing {text:?} into {label:?} in {window:?}"));
        let mut r = Err(String::new());
        let found = wait_until(3, || {
            r = osa(
                r#"on run argv
                    tell application "System Events" to tell application process "hearme"
                        set els to entire contents of window (item 1 of argv)
                        repeat with e in els
                            try
                                if role of e is in {"AXTextField", "AXTextArea"} then
                                    if description of e as text is (item 2 of argv) then
                                        set focused of e to true
                                        set value of e to (item 3 of argv)
                                        return "ok"
                                    end if
                                end if
                            end try
                        end repeat
                    end tell
                    return "missing"
                end run"#,
                &[window, label, text],
            );
            r.as_deref() == Ok("ok")
        });
        assert!(found, "field {label:?} in window {window:?}: {r:?}");
    }

    /// The selected option of every dropdown in a window's web content.
    pub fn dropdowns(&self, window: &str) -> Vec<String> {
        step(format!("reading dropdowns of {window:?}"));
        osa(
            r#"on run argv
                set out to ""
                tell application "System Events" to tell application process "hearme"
                    set els to entire contents of window (item 1 of argv)
                    repeat with e in els
                        try
                            if role of e is "AXPopUpButton" then set out to out & (value of e as text) & linefeed
                        end try
                    end repeat
                end tell
                return out
            end run"#,
            &[window],
        )
        .map(|s| s.lines().map(String::from).collect())
        .unwrap_or_default()
    }

    /// All visible text in every hearme window — the untitled overlay included.
    pub fn all_text(&self) -> String {
        step("reading text of every hearme window");
        osa(
            r#"set out to ""
            tell application "System Events" to tell application process "hearme"
                repeat with w in (every window)
                    set els to entire contents of w
                    repeat with e in els
                        try
                            if role of e is "AXStaticText" then set out to out & (value of e as text) & linefeed
                        end try
                    end repeat
                end repeat
            end tell
            return out"#,
            &[],
        )
        .unwrap_or_default()
    }

    /// Closes a window if it's showing (e.g. Settings opened by a failing check).
    pub fn close_if_open(&self) -> bool {
        if self.windows().iter().any(|w| w == "hearme") {
            self.close("hearme");
            return self.wait_window("hearme", false, 3);
        }
        false
    }

    /// Presses a window's red close button.
    pub fn close(&self, window: &str) {
        step(format!("closing {window:?}"));
        osa(
            r#"on run argv
                tell application "System Events" to tell application process "hearme"
                    click (first button of window (item 1 of argv) whose subrole is "AXCloseButton")
                end tell
            end run"#,
            &[window],
        )
        .unwrap_or_else(|e| panic!("close {window:?}: {e}"));
    }
}

impl Drop for App {
    fn drop(&mut self) {
        quit();
        restore_login_item();
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

/// With "Press 🌐 key to" set to anything but Do Nothing, fn opens the emoji
/// picker (or switches input source). The picker takes keyboard focus, so
/// the run stalls and later keystrokes/pastes land in the wrong place.
pub fn require_globe_does_nothing() {
    let v = Command::new("defaults")
        .args(["read", "com.apple.HIToolbox", "AppleFnUsageType"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    assert_eq!(
        v, "0",
        "System Settings → Keyboard → \"Press 🌐 key to\" must be \"Do Nothing\" \
         (currently {v:?}); otherwise fn opens the emoji picker, which steals focus mid-test"
    );
}

// ---------------------------------------------------------------- login item

/// The real login-item state when the run started; every App puts it back
/// on drop, since onboarding and the Settings checkbox change it for real.
static LOGIN_ITEM_BEFORE: OnceLock<String> = OnceLock::new();

/// `hearme --login-item status|on|off` against the installed app: asks
/// SMAppService directly, so it's macOS's answer, not the UI's.
pub fn login_item(arg: &str) -> String {
    try_login_item(arg).unwrap_or_else(|e| panic!("hearme --login-item {arg}: {e}"))
}

fn try_login_item(arg: &str) -> Result<String, String> {
    let out = Command::new(format!("{APP}/Contents/MacOS/hearme"))
        .args(["--login-item", arg])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Registered (possibly awaiting approval) — what the checkbox shows.
pub fn login_item_on(status: &str) -> bool {
    matches!(status, "enabled" | "requires_approval")
}

/// Never panics: it runs from Drop and the watchdog.
pub fn restore_login_item() {
    let Some(before) = LOGIN_ITEM_BEFORE.get() else {
        return;
    };
    let want = login_item_on(before);
    let now = try_login_item("status");
    if now.as_deref().map(login_item_on) != Ok(want) {
        if let Err(e) = try_login_item(if want { "on" } else { "off" }) {
            eprintln!("couldn't restore the login item to {before:?}: {e}");
        }
    }
}

pub fn quit() {
    let _ = Command::new("pkill").args(["-x", "hearme"]).status();
    wait_until(3, || {
        !Command::new("pgrep")
            .args(["-x", "hearme"])
            .status()
            .is_ok_and(|s| s.success())
    });
}

// ---------------------------------------------------------------- keyboard

/// Posts real key events at the HID level, so they flow through every event
/// tap (hearme's included) exactly like hardware input. Tracks modifier
/// state so each event carries the right flags.
pub struct Keyboard {
    src: CGEventSource,
    flags: u64,
}

fn device_independent(k: Key) -> u64 {
    match k {
        Key::Shift(_) => 0x0002_0000,
        Key::Ctrl(_) => 0x0004_0000,
        Key::Opt(_) => 0x0008_0000,
        Key::Cmd(_) => 0x0010_0000,
        Key::Fn => 0x0080_0000,
        Key::Code(_) => 0,
    }
}

fn keycode(k: Key) -> u16 {
    use Side::*;
    match k {
        Key::Fn => 63,
        Key::Cmd(Right) => 54,
        Key::Cmd(_) => 55,
        Key::Opt(Right) => 61,
        Key::Opt(_) => 58,
        Key::Ctrl(Right) => 62,
        Key::Ctrl(_) => 59,
        Key::Shift(Right) => 60,
        Key::Shift(_) => 56,
        Key::Code(c) => c,
    }
}

impl Keyboard {
    /// A keyboard that only types while one of `allowed` is the frontmost
    /// app — stray keystrokes and pastes must never land in whatever the
    /// person running the tests has open.
    pub fn into(allowed: &[&str]) -> Keyboard {
        let front = frontmost();
        assert!(
            allowed.contains(&front.as_str()),
            "refusing to type: {front:?} is in front, expected one of {allowed:?}"
        );
        Keyboard {
            src: CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap(),
            flags: 0,
        }
    }

    /// Typing into the typing target.
    #[allow(
        clippy::new_without_default,
        reason = "it checks what's frontmost; Default would hide that"
    )]
    pub fn new() -> Keyboard {
        Self::into(&[TARGET])
    }

    fn parse(name: &str) -> Key {
        let k = Key::parse(name).unwrap_or_else(|| panic!("unknown key {name}"));
        // A bare modifier means the left one, as on a real keyboard.
        match k {
            Key::Shift(Side::Any) => Key::Shift(Side::Left),
            Key::Cmd(Side::Any) => Key::Cmd(Side::Left),
            Key::Opt(Side::Any) => Key::Opt(Side::Left),
            Key::Ctrl(Side::Any) => Key::Ctrl(Side::Left),
            k => k,
        }
    }

    fn post(&mut self, name: &str, down: bool) -> &mut Self {
        step(format!("key {name} {}", if down { "down" } else { "up" }));
        let k = Self::parse(name);
        let ev = CGEvent::new_keyboard_event(self.src.clone(), keycode(k), down).unwrap();
        if k.is_modifier() {
            let bits = k.flag_bit().unwrap() | device_independent(k);
            if down {
                self.flags |= bits;
            } else {
                self.flags &= !k.flag_bit().unwrap();
                // Keep the device-independent bit while the other side is held.
                let still = [
                    Key::Shift(Side::Left),
                    Key::Shift(Side::Right),
                    Key::Cmd(Side::Left),
                    Key::Cmd(Side::Right),
                    Key::Opt(Side::Left),
                    Key::Opt(Side::Right),
                    Key::Ctrl(Side::Left),
                    Key::Ctrl(Side::Right),
                ]
                .iter()
                .any(|o| {
                    device_independent(*o) == device_independent(k)
                        && self.flags & o.flag_bit().unwrap() != 0
                });
                if !still {
                    self.flags &= !device_independent(k);
                }
            }
            ev.set_type(CGEventType::FlagsChanged);
        }
        ev.set_flags(CGEventFlags::from_bits_retain(self.flags));
        ev.post(CGEventTapLocation::HID);
        sleep(30);
        self
    }

    pub fn down(&mut self, name: &str) -> &mut Self {
        self.post(name, true)
    }

    pub fn up(&mut self, name: &str) -> &mut Self {
        self.post(name, false)
    }

    pub fn tap(&mut self, name: &str) -> &mut Self {
        self.down(name).up(name)
    }

    pub fn wait(&mut self, ms: u64) -> &mut Self {
        sleep(ms);
        self
    }
}

// ---------------------------------------------------------------- target

/// Name of the process tests dictate into (src/bin/typing-target.rs).
pub const TARGET: &str = "typing-target";

pub fn frontmost() -> String {
    osa(
        r#"tell application "System Events" to get name of first process whose frontmost is true"#,
        &[],
    )
    .unwrap_or_default()
}

pub fn clipboard() -> String {
    Command::new("pbpaste")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

pub fn set_clipboard(text: &str) {
    use std::io::Write;
    let mut p = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    p.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
    assert!(p.wait().unwrap().success(), "pbcopy failed");
}

/// Brings Finder to the front (its window or, with none open, the desktop):
/// a place with no text field to dictate into.
pub fn focus_finder() {
    step("bringing Finder to the front");
    osa(r#"tell application "Finder" to activate"#, &[])
        .unwrap_or_else(|e| panic!("activating Finder: {e}"));
    assert!(
        wait_until(3, || frontmost() == "Finder"),
        "Finder never came to the front (front: {:?})",
        frontmost()
    );
}

/// A fresh, focused text window to dictate into; quit on drop. Its text is
/// read from the file it mirrors itself to — no scripting of the window.
pub struct Target {
    out: PathBuf,
}

/// Wraps the typing-target binary in a minimal .app: macOS won't bring a
/// bare command-line process to the front, but activates apps launched
/// through LaunchServices (`open`).
fn target_bundle() -> PathBuf {
    let exe = std::env::current_exe().unwrap(); // target/<profile>/deps/app-…
    let dir = exe.parent().unwrap().parent().unwrap();
    let bin = dir.join(TARGET);
    assert!(
        bin.exists(),
        "{bin:?} missing — build with --features os-tests"
    );
    let app = dir.join(format!("{TARGET}.app"));
    let macos = app.join("Contents/MacOS");
    std::fs::create_dir_all(&macos).unwrap();
    std::fs::copy(&bin, macos.join(TARGET)).unwrap();
    std::fs::write(
        app.join("Contents/Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.josean.hearme.e2e-target</string>
<key>CFBundleName</key><string>{TARGET}</string>
<key>CFBundleExecutable</key><string>{TARGET}</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
"#
        ),
    )
    .unwrap();
    app
}

impl Target {
    pub fn open() -> Target {
        step("opening the typing target");
        let _ = Command::new("pkill").args(["-x", TARGET]).status();
        let out =
            std::env::temp_dir().join(format!("hearme-e2e-target-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let ok = Command::new("open")
            .arg("-n")
            .arg(target_bundle())
            .arg("--args")
            .arg(&out)
            .status()
            .unwrap();
        assert!(ok.success(), "couldn't launch the typing target");
        step("waiting for the typing target to be frontmost");
        let front = wait_until(3, || frontmost() == TARGET);
        assert!(
            front,
            "typing target never came to the front (front: {:?})",
            frontmost()
        );
        Target { out }
    }

    pub fn text(&self) -> String {
        std::fs::read_to_string(&self.out).unwrap_or_default()
    }

    /// Waits for `pred` to hold on the window's text; returns the last text seen.
    pub fn wait_for(&self, secs: u64, pred: impl Fn(&str) -> bool) -> String {
        step("waiting for text in the typing target");
        let mut last = String::new();
        wait_until(secs, || {
            last = self.text();
            pred(&last)
        });
        last
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        let _ = Command::new("pkill").args(["-x", TARGET]).status();
        let _ = std::fs::remove_file(&self.out);
    }
}
