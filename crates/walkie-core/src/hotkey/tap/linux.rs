//! The Linux keyboard hook: reads raw `evdev` key events from every
//! keyboard-looking device under `/dev/input` and feeds them to `Engine`,
//! exactly like the macOS tap does with `CGEventTap`.
//!
//! Listen-only for now (see AGENTS.md): we never `EVIOCGRAB` a device, so
//! the `swallow` verdict `Engine::on_key` returns is computed (and logged
//! under `WALKIE_DEBUG_EVENTS`) but never enforced — the focused app always
//! sees the keys too. Honoring it needs a grab plus a `uinput` device that
//! re-emits the keys we decide to keep, so the user's typing doesn't just
//! stop; that's tracked separately.
//!
//! No single compositor-independent "is Accessibility granted" signal exists
//! on Linux the way it does on macOS, so instead of a permission error we
//! report whatever actually stops us from reading a keyboard: nothing
//! readable in `/dev/input` (not in the `input` group) vs. no keyboard-like
//! device present at all.

use super::super::engine::{Engine, Signal};
use super::super::keys::{Key, Side};
use super::TapStatus;
use evdev::{Device, EventSummary};
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The factory default for `hotkeys.dictate` (see `config::Hotkeys`): held
/// alone, Right Ctrl has no other binding on a typical Linux keyboard layout
/// and both bare hands can still reach it.
pub const DEFAULT_DICTATE_KEY: &str = "RightCtrl";

/// The factory default for `hotkeys.paste_last` (see `config::Hotkeys`):
/// macOS's default is `Ctrl+Cmd+V`, but Cmd is Super on Linux, which
/// Hyprland (and most other compositors/WMs) already claims. Right Ctrl+V
/// reuses the dictate key, which the engine's chord-starts-with-dictate-key
/// handling (see `hotkey::engine::Engine::down`) is built to allow: holding
/// Right Ctrl begins a recording, but adding V before release cancels it
/// and pastes instead.
pub const DEFAULT_PASTE_LAST: &[&str] = &["RightCtrl", "V"];

/// How often we re-scan `/dev/input` for devices that appeared (a keyboard
/// plugged in, or `udev` finishing its permission dance after boot) or that
/// a reader thread had to drop. Simpler than wiring up inotify, and the
/// interval only has to be short enough that plugging in a keyboard feels
/// instant, not that every keystroke does.
const RESCAN: Duration = Duration::from_secs(3);

// TODO(linux): grab + uinput re-emit to honor swallow. Today every event is
// `CallbackResult::Keep`-equivalent: we read but never drop, so dictate/
// polish/paste-last chords also reach the focused app's normal key handling.

/// Starts the hook. Spawns one manager thread that scans `/dev/input` for
/// keyboard-like devices (and keeps re-scanning for hotplug) plus one reader
/// thread per device found; a device disappearing only drops its own thread.
pub fn spawn(
    engine: Arc<Mutex<Engine>>,
    status: Arc<TapStatus>,
    on_signal: impl Fn(Signal) + Send + Sync + 'static,
) {
    let debug = std::env::var_os("WALKIE_DEBUG_EVENTS").is_some();
    let start = Instant::now();
    let on_signal = Arc::new(on_signal);

    {
        let engine = engine.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(50));
            let t = start.elapsed().as_millis();
            engine.lock().unwrap_or_else(|e| e.into_inner()).poll(t);
        });
    }

    let active_paths: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
    // Paths opened and found *not* to be a keyboard (a mouse, a webcam's
    // snapshot key, ...): skip them on future scans instead of reopening
    // every device under /dev/input every `RESCAN` tick. Forgotten once the
    // path drops out of a scan, so a different device replugged at the same
    // path gets a fresh check.
    let rejected_paths: Mutex<HashSet<PathBuf>> = Mutex::new(HashSet::new());
    let active_count = Arc::new(AtomicUsize::new(0));

    std::thread::spawn(move || loop {
        let mut permission_denied = false;
        let candidates = scan_candidate_paths();
        prune_rejected(&mut rejected_paths.lock().unwrap(), &candidates);
        for path in candidates {
            if active_paths.lock().unwrap().contains(&path)
                || rejected_paths.lock().unwrap().contains(&path)
            {
                continue;
            }
            let device = match Device::open(&path) {
                Ok(d) => d,
                Err(e) => {
                    if e.kind() == io::ErrorKind::PermissionDenied {
                        permission_denied = true;
                    }
                    continue;
                }
            };
            let codes: Vec<u16> = device
                .supported_keys()
                .map(|ks| ks.iter().map(|k| k.code()).collect())
                .unwrap_or_default();
            if !looks_like_keyboard(&codes) {
                rejected_paths.lock().unwrap().insert(path.clone());
                continue;
            }

            let name = device.name().unwrap_or("unknown keyboard").to_string();
            eprintln!("walkie: keyboard hook reading {} ({name})", path.display());
            active_paths.lock().unwrap().insert(path.clone());
            active_count.fetch_add(1, Ordering::SeqCst);
            status.running.store(true, Ordering::SeqCst);
            *status.error.lock().unwrap() = None;

            let engine = engine.clone();
            let on_signal = on_signal.clone();
            let status = status.clone();
            let active_paths = active_paths.clone();
            let active_count = active_count.clone();
            let path_for_thread = path.clone();
            std::thread::spawn(move || {
                read_device(device, &engine, &on_signal, start, debug);
                // The device went away (unplugged, suspended, ...): drop it
                // from the active set so the next scan can pick it back up
                // if it comes back under the same path.
                active_paths.lock().unwrap().remove(&path_for_thread);
                let remaining = active_count.fetch_sub(1, Ordering::SeqCst) - 1;
                eprintln!("walkie: keyboard hook lost {}", path_for_thread.display());
                status.running.store(remaining > 0, Ordering::SeqCst);
                *status.error.lock().unwrap() = status_message(remaining, false);
            });
        }

        let active = active_count.load(Ordering::SeqCst);
        status.running.store(active > 0, Ordering::SeqCst);
        if let Some(msg) = status_message(active, permission_denied) {
            let mut error = status.error.lock().unwrap();
            if error.is_none() {
                eprintln!("walkie: keyboard hook: {msg}");
            }
            *error = Some(msg);
        }

        std::thread::sleep(RESCAN);
    });
}

/// Blocks reading one device's events until it errors out (unplugged,
/// suspended, read failure — anything). Each event is fed to `on_key`; the
/// swallow verdict is only logged (see the `TODO(linux)` in `spawn`), never
/// enforced, since we never grab the device.
fn read_device(
    mut device: Device,
    engine: &Arc<Mutex<Engine>>,
    on_signal: &Arc<impl Fn(Signal) + Send + Sync + 'static>,
    start: Instant,
    debug: bool,
) {
    loop {
        let events = match device.fetch_events() {
            Ok(events) => events,
            Err(_) => return,
        };
        for ev in events {
            let EventSummary::Key(_, code, value) = ev.destructure() else {
                continue;
            };
            if is_autorepeat(value) {
                continue;
            }
            let key = key_from_evdev_code(code.code());
            let down = value != 0;
            let t = start.elapsed().as_millis();
            let v = engine
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_key(key, down, t);
            if debug {
                eprintln!(
                    "walkie: key @{t}ms {} {} → {:?}{}",
                    key.name(),
                    if down { "down" } else { "up" },
                    v.signal,
                    if v.swallow {
                        " (would swallow — listen-only on Linux)"
                    } else {
                        ""
                    }
                );
            }
            if let Some(s) = v.signal {
                on_signal(s);
            }
        }
    }
}

/// Drops any remembered-rejected path that didn't show up in this scan's
/// `seen` paths — so a device unplugged and replaced by a different one at
/// the same `/dev/input/eventN` node gets re-checked instead of staying
/// rejected forever based on the device that used to be there.
fn prune_rejected(rejected: &mut HashSet<PathBuf>, seen: &[PathBuf]) {
    rejected.retain(|p| seen.contains(p));
}

/// `/dev/input/event*` paths, in whatever order `read_dir` gives them.
/// Doesn't open anything — `spawn`'s caller does that, so it can tell a
/// permission error apart from "no such device".
fn scan_candidate_paths() -> Vec<PathBuf> {
    let dir: &Path = Path::new("/dev/input");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("event"))
        })
        .collect()
}

/// Is `value` evdev's autorepeat marker? `EV_KEY` values are 0 (up), 1
/// (down) or 2 (the kernel re-sending a held key) — macOS never reaches
/// `Engine::on_key` with a third state (`Engine::on_key` already treats a
/// repeated `down` on an already-held key as a no-op, but there's no reason
/// to even take the lock for those).
fn is_autorepeat(value: i32) -> bool {
    value == 2
}

/// Treats a device as a real keyboard (vs. a mouse's couple of buttons, a
/// power button, a webcam's snapshot key, ...) if it can emit the full
/// letter row plus Space — nothing with just a handful of keys passes this.
fn looks_like_keyboard(codes: &[u16]) -> bool {
    const KEY_A: u16 = 30;
    const KEY_Z: u16 = 44;
    const KEY_SPACE: u16 = 57;
    codes.contains(&KEY_A) && codes.contains(&KEY_Z) && codes.contains(&KEY_SPACE)
}

/// What `TapStatus.error` should say after a scan, given how many keyboards
/// are currently being read and whether this scan saw a permission error
/// while trying to open a candidate device.
fn status_message(active: usize, permission_denied: bool) -> Option<String> {
    if active > 0 {
        return None;
    }
    if permission_denied {
        Some("can't read /dev/input — add yourself to the input group".into())
    } else {
        Some("no keyboard found in /dev/input".into())
    }
}

/// Maps a raw evdev key code (`linux/input-event-codes.h`, e.g. `KEY_A` =
/// 30) to the shared `Key`. Modifiers map straight to their sided variant;
/// every other key maps to the same `Key::Code` number the macOS tap would
/// produce for the physically equivalent key (see `keys::NAMED`), so a
/// binding like `"A"` or `"F5"` means the same key and shows the same name
/// on both platforms. A code with no macOS equivalent (Insert, the keypad,
/// ...) still round-trips — it just shows as `KeyNNNN` instead of a name.
pub fn key_from_evdev_code(code: u16) -> Key {
    match code {
        29 => return Key::Ctrl(Side::Left),
        97 => return Key::Ctrl(Side::Right),
        42 => return Key::Shift(Side::Left),
        54 => return Key::Shift(Side::Right),
        56 => return Key::Opt(Side::Left),
        100 => return Key::Opt(Side::Right),
        125 => return Key::Cmd(Side::Left),
        126 => return Key::Cmd(Side::Right),
        0x1d0 => return Key::Fn, // KEY_FN; rarely reported, EC-handled on most laptops
        _ => {}
    }
    // (evdev code, macOS virtual keycode) for every key both platforms name
    // the same way — see `keys::NAMED` for what each macOS code displays as.
    const MAC_EQUIVALENT: &[(u16, u16)] = &[
        // letters
        (30, 0),
        (48, 11),
        (46, 8),
        (32, 2),
        (18, 14),
        (33, 3),
        (34, 5),
        (35, 4),
        (23, 34),
        (36, 38),
        (37, 40),
        (38, 37),
        (50, 46),
        (49, 45),
        (24, 31),
        (25, 35),
        (16, 12),
        (19, 15),
        (31, 1),
        (20, 17),
        (22, 32),
        (47, 9),
        (17, 13),
        (45, 7),
        (21, 16),
        (44, 6),
        // digits (top row)
        (2, 18),
        (3, 19),
        (4, 20),
        (5, 21),
        (6, 23),
        (7, 22),
        (8, 26),
        (9, 28),
        (10, 25),
        (11, 29),
        // whitespace / punctuation
        (57, 49),
        (28, 36),
        (15, 48),
        (14, 51),
        (1, 53),
        (41, 50),
        (12, 27),
        (13, 24),
        (26, 33),
        (27, 30),
        (43, 42),
        (39, 41),
        (40, 39),
        (51, 43),
        (52, 47),
        (53, 44),
        // arrows / navigation
        (105, 123),
        (106, 124),
        (108, 125),
        (103, 126),
        (102, 115),
        (107, 119),
        (104, 116),
        (109, 121),
        (111, 117),
        // Caps Lock — not bindable (see `keys::validate`), but round-trips
        (58, 57),
        // function keys
        (59, 122),
        (60, 120),
        (61, 99),
        (62, 118),
        (63, 96),
        (64, 97),
        (65, 98),
        (66, 100),
        (67, 101),
        (68, 109),
        (87, 103),
        (88, 111),
        (183, 105),
        (184, 107),
        (185, 113),
        (186, 106),
        (187, 64),
        (188, 79),
        (189, 80),
        (190, 90),
    ];
    match MAC_EQUIVALENT.iter().find(|(ev, _)| *ev == code) {
        Some((_, mac)) => Key::Code(*mac),
        None => Key::Code(code + 1000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::keys;

    #[test]
    fn default_dictate_key_parses_to_right_ctrl() {
        assert_eq!(
            keys::Key::parse(DEFAULT_DICTATE_KEY),
            Some(Key::Ctrl(Side::Right))
        );
    }

    #[test]
    fn evdev_modifiers_map_to_sided_keys() {
        assert_eq!(key_from_evdev_code(29), Key::Ctrl(Side::Left));
        assert_eq!(key_from_evdev_code(97), Key::Ctrl(Side::Right));
        assert_eq!(key_from_evdev_code(42), Key::Shift(Side::Left));
        assert_eq!(key_from_evdev_code(54), Key::Shift(Side::Right));
        assert_eq!(key_from_evdev_code(56), Key::Opt(Side::Left));
        assert_eq!(key_from_evdev_code(100), Key::Opt(Side::Right));
        assert_eq!(key_from_evdev_code(125), Key::Cmd(Side::Left));
        assert_eq!(key_from_evdev_code(126), Key::Cmd(Side::Right));
        assert_eq!(key_from_evdev_code(0x1d0), Key::Fn);
    }

    #[test]
    fn evdev_regular_keys_share_mac_names() {
        // KEY_A, KEY_SPACE, KEY_F5 — each should parse/display exactly like
        // the macOS tap would for the same physical key.
        assert_eq!(key_from_evdev_code(30).name(), "A");
        assert_eq!(key_from_evdev_code(57).name(), "Space");
        assert_eq!(key_from_evdev_code(63).name(), "F5");
        assert_eq!(key_from_evdev_code(28).name(), "Return");
    }

    #[test]
    fn evdev_code_with_no_mac_equivalent_still_round_trips() {
        let k = key_from_evdev_code(110); // KEY_INSERT
        assert_eq!(k, Key::Code(1110));
        assert_eq!(keys::Key::parse(&k.name()), Some(k));
    }

    #[test]
    fn autorepeat_is_only_value_two() {
        assert!(!is_autorepeat(0));
        assert!(!is_autorepeat(1));
        assert!(is_autorepeat(2));
    }

    #[test]
    fn keyboard_classification_needs_the_full_letter_row() {
        assert!(looks_like_keyboard(&[30, 44, 57, 1, 2, 3])); // a real keyboard
        assert!(!looks_like_keyboard(&[]));
        assert!(!looks_like_keyboard(&[272, 273, 274])); // a mouse's buttons
        assert!(!looks_like_keyboard(&[116])); // KEY_POWER alone
        assert!(!looks_like_keyboard(&[30, 57])); // has A and Space, not Z
    }

    #[test]
    fn status_message_only_when_nothing_is_active() {
        assert_eq!(status_message(1, true), None);
        assert_eq!(
            status_message(0, true).unwrap(),
            "can't read /dev/input — add yourself to the input group"
        );
        assert!(status_message(0, false).unwrap().contains("no keyboard"));
    }

    #[test]
    fn prune_rejected_drops_paths_missing_from_the_scan() {
        let mut rejected: HashSet<PathBuf> = [
            PathBuf::from("/dev/input/event1"),
            PathBuf::from("/dev/input/event2"),
        ]
        .into_iter()
        .collect();
        // event1 is still there (still not a keyboard); event2 vanished —
        // forget it so a different device at that path gets re-checked.
        prune_rejected(&mut rejected, &[PathBuf::from("/dev/input/event1")]);
        assert_eq!(
            rejected,
            [PathBuf::from("/dev/input/event1")].into_iter().collect()
        );
    }

    #[test]
    fn prune_rejected_is_a_no_op_when_nothing_disappeared() {
        let mut rejected: HashSet<PathBuf> =
            [PathBuf::from("/dev/input/event0")].into_iter().collect();
        prune_rejected(
            &mut rejected,
            &[
                PathBuf::from("/dev/input/event0"),
                PathBuf::from("/dev/input/event1"),
            ],
        );
        assert_eq!(
            rejected,
            [PathBuf::from("/dev/input/event0")].into_iter().collect()
        );
    }

    #[test]
    fn candidate_paths_are_event_nodes_only() {
        // /dev/input always exists on a running Linux box in CI; this just
        // checks the filter, not that any device is present.
        for p in scan_candidate_paths() {
            assert!(p
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("event"));
        }
    }
}
