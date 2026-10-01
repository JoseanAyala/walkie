use anyhow::{Context, Result};
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};
use std::sync::Mutex;
use std::time::Duration;

/// Ctrl (+ Shift) + `ch` through a uinput keyboard, which the compositor
/// treats like a real one. enigo's Wayland path can't do chords reliably:
/// it uploads a new keymap for a key it hasn't mapped yet, and the keymap
/// change drops the held Ctrl, so apps got a bare "v". None (enigo is used
/// instead) when /dev/uinput can't be opened.
///
/// The device can only press the keys in `KEYS`. They're physical
/// positions (KEY_V), as from a real keyboard.
pub fn send(ch: char, shift: bool) -> Option<Result<()>> {
    let key = letter(ch)?;
    let mut dev = DEVICE.lock().unwrap_or_else(|e| e.into_inner());
    if dev.is_none() {
        match build() {
            Ok(d) => {
                *dev = Some(d);
                // The compositor picks a new input device up asynchronously;
                // keys sent before that are lost. Once per run.
                std::thread::sleep(SETTLE);
            }
            Err(e) => {
                eprintln!("walkie: no uinput keyboard ({e:#}); falling back to enigo");
                return None;
            }
        }
    }
    let d = dev.as_mut()?;
    Some(emit(d, &chord(key, shift)).context("uinput chord"))
}

static DEVICE: Mutex<Option<VirtualDevice>> = Mutex::new(None);

const SETTLE: Duration = Duration::from_millis(300);

/// Only what `chord` presses. Without KEY_Z/KEY_SPACE the keyboard hook's
/// `looks_like_keyboard` skips this device, so walkie doesn't hear itself.
const KEYS: [KeyCode; 5] = [
    KeyCode::KEY_LEFTCTRL,
    KeyCode::KEY_LEFTSHIFT,
    KeyCode::KEY_A,
    KeyCode::KEY_C,
    KeyCode::KEY_V,
];

fn build() -> Result<VirtualDevice> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for k in KEYS {
        keys.insert(k);
    }
    Ok(VirtualDevice::builder()
        .context("opening /dev/uinput")?
        .name("walkie virtual keyboard")
        .with_keys(&keys)?
        .build()?)
}

fn letter(ch: char) -> Option<KeyCode> {
    match ch {
        'a' => Some(KeyCode::KEY_A),
        'c' => Some(KeyCode::KEY_C),
        'v' => Some(KeyCode::KEY_V),
        _ => None,
    }
}

/// (key, pressed) in order: modifiers down, key down/up, modifiers up.
fn chord(key: KeyCode, shift: bool) -> Vec<(KeyCode, bool)> {
    let mut mods = vec![KeyCode::KEY_LEFTCTRL];
    if shift {
        mods.push(KeyCode::KEY_LEFTSHIFT);
    }
    let mut out: Vec<_> = mods.iter().map(|&m| (m, true)).collect();
    out.push((key, true));
    out.push((key, false));
    out.extend(mods.iter().rev().map(|&m| (m, false)));
    out
}

/// One event (`emit` adds the SYN_REPORT) per step, a moment apart, so the
/// compositor sees the modifiers held before the letter.
fn emit(d: &mut VirtualDevice, steps: &[(KeyCode, bool)]) -> std::io::Result<()> {
    for &(k, down) in steps {
        d.emit(&[InputEvent::new(EventType::KEY.0, k.code(), down as i32)])?;
        std::thread::sleep(Duration::from_millis(8));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyCode as K;

    #[test]
    fn paste_holds_ctrl_around_v() {
        assert_eq!(
            chord(K::KEY_V, false),
            [
                (K::KEY_LEFTCTRL, true),
                (K::KEY_V, true),
                (K::KEY_V, false),
                (K::KEY_LEFTCTRL, false),
            ]
        );
    }

    #[test]
    fn terminal_paste_adds_shift_inside_ctrl() {
        assert_eq!(
            chord(K::KEY_V, true),
            [
                (K::KEY_LEFTCTRL, true),
                (K::KEY_LEFTSHIFT, true),
                (K::KEY_V, true),
                (K::KEY_V, false),
                (K::KEY_LEFTSHIFT, false),
                (K::KEY_LEFTCTRL, false),
            ]
        );
    }

    #[test]
    fn only_the_chord_letters_map() {
        assert_eq!(letter('v'), Some(K::KEY_V));
        assert_eq!(letter('x'), None);
    }

    #[test]
    fn the_hook_will_not_mistake_this_for_a_keyboard() {
        assert!(!KEYS.contains(&K::KEY_Z) && !KEYS.contains(&K::KEY_SPACE));
    }
}
