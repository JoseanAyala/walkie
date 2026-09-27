//! Turns physical key events into dictation signals. Pure: no OS calls, so
//! every gesture is unit-testable. The macOS tap (`tap.rs`) feeds it events
//! and obeys its `swallow` verdicts.
//!
//! A binding is *satisfied* when the held keys are exactly its keys — so
//! `Fn+A` never triggers an `Fn` binding. Gestures:
//! - dictate / polish: hold to talk, double-tap to lock (see `machine.rs`)
//! - hands-free: press to start, press it (or dictate) again to stop
//! - while holding dictate, adding keys that satisfy polish or hands-free
//!   upgrades the running recording instead of starting a new one
//! - any other key within HOLD_MIN_MS of starting cancels (you were typing
//!   a shortcut, not dictating)

use super::keys::{self, Key, ESCAPE};
use super::machine::{HotkeyMachine, HOLD_MIN_MS};
use super::{Mode, Output};
use crate::config::Hotkeys;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Dictate,
    Polish,
    HandsFree,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bindings {
    pub dictate: Vec<Key>,
    pub polish: Vec<Key>,
    pub hands_free: Vec<Key>,
}

impl Bindings {
    /// Parses and checks the configured bindings. Every problem is reported;
    /// an unusable binding is disabled rather than failing the whole set.
    pub fn from_config(h: &Hotkeys) -> (Bindings, Vec<String>) {
        let mut errors = Vec::new();
        let mut parse = |label: &str, names: &[String]| {
            keys::parse_binding(names).unwrap_or_else(|e| {
                errors.push(format!("{label} shortcut: {e}"));
                Vec::new()
            })
        };
        let mut b = Bindings {
            dictate: parse("Dictate", &h.dictate),
            polish: parse("Polish", &h.polish),
            hands_free: parse("Hands-free", &h.hands_free),
        };
        let same = |x: &[Key], y: &[Key]| {
            !x.is_empty() && x.len() == y.len() && x.iter().all(|k| y.iter().any(|j| k.matches(*j)))
        };
        if same(&b.polish, &b.dictate) {
            errors.push("Polish shortcut is the same as Dictate — Polish disabled".into());
            b.polish.clear();
        }
        if same(&b.hands_free, &b.dictate) || same(&b.hands_free, &b.polish) {
            errors.push("Hands-free shortcut duplicates another one — Hands-free disabled".into());
            b.hands_free.clear();
        }
        (b, errors)
    }

    fn get(&self, a: Action) -> &[Key] {
        match a {
            Action::Dictate => &self.dictate,
            Action::Polish => &self.polish,
            Action::HandsFree => &self.hands_free,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Start(Mode),
    /// The running recording switches mode (dictate → polish).
    SetMode(Mode),
    Finish,
    Cancel,
    /// Shortcut recorder result: the keys held together, normalized.
    Recorded(Vec<Key>),
    RecordCancelled,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Verdict {
    pub signal: Option<Signal>,
    /// Drop the event so the focused app never sees it.
    pub swallow: bool,
}

pub struct Engine {
    bindings: Bindings,
    held: Vec<Key>,
    machine: HotkeyMachine,
    /// Mode of the current (or double-tap-pending) session.
    mode: Option<Mode>,
    /// The key whose press started the current hold; releasing it ends it.
    anchor: Option<Key>,
    pressed_at: u128,
    hands_free: bool,
    /// Set after a stray-key cancel: ignore everything until all keys are up.
    inhibit: bool,
    swallowed: HashSet<Key>,
    recording: Option<Vec<Key>>,
}

fn mode_of(a: Action) -> Mode {
    match a {
        Action::Polish => Mode::Polish,
        _ => Mode::Dictate,
    }
}

impl Engine {
    pub fn new(bindings: Bindings) -> Self {
        Self {
            bindings,
            held: Vec::new(),
            machine: HotkeyMachine::new(),
            mode: None,
            anchor: None,
            pressed_at: 0,
            hands_free: false,
            inhibit: false,
            swallowed: HashSet::new(),
            recording: None,
        }
    }

    pub fn bindings(&self) -> &Bindings {
        &self.bindings
    }

    /// Takes effect on the next key event; a session already running keeps going.
    pub fn set_bindings(&mut self, b: Bindings) {
        self.bindings = b;
    }

    /// The next chord pressed is reported as `Signal::Recorded` instead of
    /// triggering anything. Esc alone cancels.
    pub fn start_recording(&mut self) {
        self.recording = Some(Vec::new());
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    pub fn on_key(&mut self, k: Key, down: bool, t: u128) -> Verdict {
        if down {
            if self.held.contains(&k) {
                // auto-repeat: follow the verdict of the original press
                return Verdict { signal: None, swallow: self.swallowed.contains(&k) };
            }
            self.held.push(k);
            if self.recording.is_some() {
                return self.record_down(k);
            }
            if self.inhibit {
                return Verdict::default();
            }
            let signal = self.down(k, t);
            // Only regular keys are ever swallowed: dropping a modifier's
            // flags-changed event desyncs the OS's modifier state (stuck keys).
            let swallow = signal_consumed(&signal) && !k.is_modifier();
            if swallow {
                self.swallowed.insert(k);
            }
            Verdict { signal: signal.into_signal(), swallow }
        } else {
            self.held.retain(|h| *h != k);
            let swallow = self.swallowed.remove(&k);
            if self.recording.is_some() {
                return Verdict { signal: self.record_up(), swallow };
            }
            if self.inhibit {
                if self.held.is_empty() {
                    self.inhibit = false;
                }
                return Verdict { signal: None, swallow };
            }
            Verdict { signal: self.up(k, t), swallow }
        }
    }

    /// Call every ~50ms so an unanswered first tap expires.
    pub fn poll(&mut self, now: u128) {
        self.machine.poll(now);
        if !self.machine.is_engaged() && !self.hands_free && self.anchor.is_none() {
            self.mode = None;
        }
    }

    fn satisfied(&self) -> Option<Action> {
        [Action::HandsFree, Action::Polish, Action::Dictate].into_iter().find(|&a| {
            let b = self.bindings.get(a);
            !b.is_empty()
                && b.len() == self.held.len()
                && b.iter().all(|bk| self.held.iter().any(|h| bk.matches(*h)))
                && self.held.iter().all(|h| b.iter().any(|bk| bk.matches(*h)))
        })
    }

    fn down(&mut self, k: Key, t: u128) -> Down {
        let sat = self.satisfied();

        if self.hands_free {
            return match sat {
                Some(Action::Dictate | Action::HandsFree) => {
                    self.hands_free = false;
                    self.mode = None;
                    Down::Consumed(Some(Signal::Finish))
                }
                _ => Down::Ignored,
            };
        }

        if self.anchor.is_some() && self.machine.is_holding() {
            return match sat {
                Some(Action::Polish) if self.mode == Some(Mode::Dictate) => {
                    self.mode = Some(Mode::Polish);
                    Down::Consumed(Some(Signal::SetMode(Mode::Polish)))
                }
                Some(Action::HandsFree) => {
                    // The recording keeps running; it just no longer needs holding.
                    self.machine.reset();
                    self.anchor = None;
                    self.hands_free = true;
                    Down::Consumed(None)
                }
                _ if t.saturating_sub(self.pressed_at) < HOLD_MIN_MS => {
                    self.machine.reset();
                    self.anchor = None;
                    self.mode = None;
                    self.inhibit = true;
                    Down::Ignored.with(Signal::Cancel)
                }
                _ => Down::Ignored,
            };
        }

        match sat {
            Some(Action::HandsFree) if !self.machine.is_locked() => {
                self.machine.reset();
                self.mode = Some(Mode::Dictate);
                self.hands_free = true;
                Down::Consumed(Some(Signal::Start(Mode::Dictate)))
            }
            Some(a @ (Action::Dictate | Action::Polish)) => {
                let m = mode_of(a);
                if self.machine.is_engaged() && self.mode != Some(m) {
                    if self.machine.is_locked() {
                        return Down::Ignored; // the other mode's locked session owns the keys
                    }
                    self.machine.reset(); // a pending tap of the other mode just expires
                }
                self.anchor = Some(k);
                self.pressed_at = t;
                let out = self.machine.press(t);
                if self.machine.is_holding() || self.machine.is_locked() {
                    self.mode = Some(m);
                }
                Down::Consumed(match out {
                    Some(Output::Start) => Some(Signal::Start(m)),
                    Some(Output::Finish) => Some(Signal::Finish),
                    _ => None,
                })
            }
            _ => Down::Ignored,
        }
    }

    fn up(&mut self, k: Key, t: u128) -> Option<Signal> {
        if self.anchor != Some(k) {
            return None;
        }
        self.anchor = None;
        let out = self.machine.release(t);
        if !self.machine.is_engaged() {
            self.mode = None;
        }
        match out {
            Some(Output::Finish) => Some(Signal::Finish),
            Some(Output::CancelDiscard) => Some(Signal::Cancel),
            _ => None,
        }
    }

    fn record_down(&mut self, k: Key) -> Verdict {
        if k == ESCAPE && self.held.len() == 1 {
            self.recording = None;
            self.swallowed.insert(k);
            return Verdict { signal: Some(Signal::RecordCancelled), swallow: true };
        }
        let rec = self.recording.as_mut().unwrap();
        if self.held.len() > rec.len() {
            *rec = self.held.clone();
        }
        // Keep the keys out of the settings window while recording.
        let swallow = !k.is_modifier();
        if swallow {
            self.swallowed.insert(k);
        }
        Verdict { signal: None, swallow }
    }

    fn record_up(&mut self) -> Option<Signal> {
        if !self.held.is_empty() {
            return None;
        }
        let keys = self.recording.take().unwrap_or_default();
        Some(Signal::Recorded(keys::normalize(&keys)))
    }
}

/// What a key press did: `Consumed` means it drove a binding (so a regular
/// key should be swallowed), with or without a signal.
enum Down {
    Consumed(Option<Signal>),
    Ignored,
    IgnoredWith(Signal),
}

impl Down {
    fn with(self, s: Signal) -> Down {
        match self {
            Down::Ignored => Down::IgnoredWith(s),
            d => d,
        }
    }
    fn into_signal(self) -> Option<Signal> {
        match self {
            Down::Consumed(s) => s,
            Down::Ignored => None,
            Down::IgnoredWith(s) => Some(s),
        }
    }
}

fn signal_consumed(d: &Down) -> bool {
    matches!(d, Down::Consumed(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::keys::Side::*;
    use Signal::*;

    const SPACE: Key = Key::Code(49);
    const A: Key = Key::Code(0);
    const D: Key = Key::Code(2);
    const LSHIFT: Key = Key::Shift(Left);

    fn wispr() -> Engine {
        Engine::new(Bindings {
            dictate: vec![Key::Fn],
            polish: vec![Key::Fn, Key::Shift(Any)],
            hands_free: vec![Key::Fn, SPACE],
        })
    }

    /// Feeds (key, down, t) events; returns the non-empty signals.
    fn run(e: &mut Engine, evs: &[(Key, bool, u128)]) -> Vec<Signal> {
        evs.iter().filter_map(|&(k, d, t)| e.on_key(k, d, t).signal).collect()
    }

    #[test]
    fn default_config_parses_to_wispr_bindings() {
        let (b, errs) = Bindings::from_config(&Hotkeys::default());
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(b, wispr().bindings);
    }

    #[test]
    fn bad_and_duplicate_bindings_are_disabled_with_reasons() {
        let v = |k: &[&str]| k.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (b, errs) = Bindings::from_config(&Hotkeys {
            dictate: v(&["RightCmd"]),
            polish: v(&["RightCmd"]),
            hands_free: v(&["A"]),
        });
        assert_eq!(b.dictate, vec![Key::Cmd(Right)]);
        assert!(b.polish.is_empty() && b.hands_free.is_empty());
        assert_eq!(errs.len(), 2, "{errs:?}");
    }

    #[test]
    fn hold_fn_dictates() {
        let mut e = wispr();
        assert_eq!(run(&mut e, &[(Key::Fn, true, 0), (Key::Fn, false, 800)]), vec![
            Start(Mode::Dictate),
            Finish
        ]);
    }

    #[test]
    fn quick_tap_cancels_and_double_tap_locks() {
        let mut e = wispr();
        let s = run(&mut e, &[
            (Key::Fn, true, 0),
            (Key::Fn, false, 80),   // tap 1 → cancel
            (Key::Fn, true, 200),   // tap 2 → locked start
            (Key::Fn, false, 260),
            (Key::Fn, true, 5000),  // stop tap
            (Key::Fn, false, 5060),
        ]);
        assert_eq!(s, vec![Start(Mode::Dictate), Cancel, Start(Mode::Dictate), Finish]);
    }

    #[test]
    fn shift_then_fn_is_polish() {
        let mut e = wispr();
        let s = run(&mut e, &[
            (LSHIFT, true, 0),
            (Key::Fn, true, 10),
            (LSHIFT, false, 300), // releasing Shift doesn't end it
            (Key::Fn, false, 900),
        ]);
        assert_eq!(s, vec![Start(Mode::Polish), Finish]);
    }

    #[test]
    fn adding_shift_mid_hold_upgrades_to_polish() {
        let mut e = wispr();
        let s = run(&mut e, &[
            (Key::Fn, true, 0),
            (LSHIFT, true, 400),
            (LSHIFT, false, 600),
            (Key::Fn, false, 900),
        ]);
        assert_eq!(s, vec![Start(Mode::Dictate), SetMode(Mode::Polish), Finish]);
    }

    #[test]
    fn fn_space_goes_hands_free_and_fn_stops_it() {
        let mut e = wispr();
        let start = e.on_key(Key::Fn, true, 0);
        assert_eq!(start.signal, Some(Start(Mode::Dictate)));
        let space = e.on_key(SPACE, true, 50);
        assert!(space.swallow, "Space must not reach the focused app");
        assert_eq!(space.signal, None, "same recording continues");
        assert!(e.on_key(SPACE, false, 120).swallow, "its key-up is swallowed too");
        assert_eq!(e.on_key(Key::Fn, false, 150).signal, None, "releasing no longer ends it");
        assert_eq!(e.on_key(Key::Fn, true, 9000).signal, Some(Finish));
        assert_eq!(e.on_key(Key::Fn, false, 9050).signal, None);
        assert_eq!(e.on_key(Key::Fn, true, 12000).signal, Some(Start(Mode::Dictate)));
    }

    #[test]
    fn space_then_fn_starts_hands_free_directly() {
        let mut e = wispr();
        assert!(!e.on_key(SPACE, true, 0).swallow); // Space alone is just typing
        assert_eq!(e.on_key(Key::Fn, true, 20).signal, Some(Start(Mode::Dictate)));
        e.on_key(Key::Fn, false, 100);
        e.on_key(SPACE, false, 120);
        assert_eq!(e.on_key(Key::Fn, true, 3000).signal, Some(Finish));
    }

    #[test]
    fn stray_key_right_after_start_cancels_and_passes_through() {
        let mut e = wispr();
        e.on_key(Key::Fn, true, 0);
        let a = e.on_key(A, true, 40);
        assert_eq!(a.signal, Some(Cancel));
        assert!(!a.swallow);
        // nothing retriggers until everything is released
        assert_eq!(run(&mut e, &[(A, false, 60), (Key::Fn, false, 100)]), vec![]);
        assert_eq!(e.on_key(Key::Fn, true, 500).signal, Some(Start(Mode::Dictate)));
    }

    #[test]
    fn stray_key_during_a_real_hold_is_ignored() {
        let mut e = wispr();
        e.on_key(Key::Fn, true, 0);
        let a = e.on_key(A, true, 1000);
        assert_eq!(a, Verdict::default());
        e.on_key(A, false, 1050);
        assert_eq!(e.on_key(Key::Fn, false, 2000).signal, Some(Finish));
    }

    #[test]
    fn fn_with_another_modifier_is_not_dictate() {
        let mut e = wispr();
        let s = run(&mut e, &[(Key::Cmd(Left), true, 0), (Key::Fn, true, 10), (Key::Fn, false, 900)]);
        assert_eq!(s, vec![]);
    }

    #[test]
    fn combo_binding_swallows_its_regular_key() {
        let mut e = Engine::new(Bindings {
            dictate: vec![Key::Ctrl(Any), Key::Opt(Any), D],
            ..Default::default()
        });
        assert!(!e.on_key(Key::Ctrl(Left), true, 0).swallow);
        assert!(!e.on_key(Key::Opt(Right), true, 10).swallow);
        let d = e.on_key(D, true, 20);
        assert_eq!(d.signal, Some(Start(Mode::Dictate)));
        assert!(d.swallow);
        assert!(e.on_key(D, true, 60).swallow, "auto-repeat swallowed too");
        let up = e.on_key(D, false, 900);
        assert_eq!(up.signal, Some(Finish));
        assert!(up.swallow);
    }

    #[test]
    fn sided_binding_ignores_the_other_side() {
        let mut e = Engine::new(Bindings { dictate: vec![Key::Cmd(Right)], ..Default::default() });
        assert_eq!(run(&mut e, &[(Key::Cmd(Left), true, 0), (Key::Cmd(Left), false, 900)]), vec![]);
        assert_eq!(run(&mut e, &[(Key::Cmd(Right), true, 1000), (Key::Cmd(Right), false, 1900)]), vec![
            Start(Mode::Dictate),
            Finish
        ]);
    }

    #[test]
    fn empty_binding_is_disabled() {
        let mut e = Engine::new(Bindings { dictate: vec![Key::Fn], ..Default::default() });
        let s = run(&mut e, &[(LSHIFT, true, 0), (Key::Fn, true, 10), (Key::Fn, false, 900)]);
        assert_eq!(s, vec![], "no polish binding → Shift+Fn does nothing");
    }

    #[test]
    fn rebinding_takes_effect_immediately() {
        let mut e = wispr();
        e.set_bindings(Bindings { dictate: vec![Key::Cmd(Right)], ..Default::default() });
        assert_eq!(run(&mut e, &[(Key::Fn, true, 0), (Key::Fn, false, 900)]), vec![]);
        assert_eq!(e.on_key(Key::Cmd(Right), true, 1000).signal, Some(Start(Mode::Dictate)));
    }

    #[test]
    fn recorder_captures_the_largest_chord_and_suppresses_triggers() {
        let mut e = wispr();
        e.start_recording();
        assert_eq!(e.on_key(Key::Fn, true, 0).signal, None, "Fn must not start dictation");
        e.on_key(Key::Ctrl(Left), true, 10);
        assert!(e.on_key(D, true, 20).swallow);
        e.on_key(D, false, 100);
        e.on_key(Key::Ctrl(Left), false, 110);
        let v = e.on_key(Key::Fn, false, 120);
        assert_eq!(v.signal, Some(Recorded(vec![Key::Fn, Key::Ctrl(Any), D])));
        assert!(!e.is_recording());
        assert_eq!(e.on_key(Key::Fn, true, 500).signal, Some(Start(Mode::Dictate)));
    }

    #[test]
    fn recorder_keeps_side_for_lone_modifiers() {
        let mut e = wispr();
        e.start_recording();
        e.on_key(Key::Cmd(Right), true, 0);
        assert_eq!(e.on_key(Key::Cmd(Right), false, 50).signal, Some(Recorded(vec![Key::Cmd(Right)])));
    }

    #[test]
    fn escape_cancels_recording() {
        let mut e = wispr();
        e.start_recording();
        let v = e.on_key(ESCAPE, true, 0);
        assert_eq!(v.signal, Some(RecordCancelled));
        assert!(v.swallow);
        assert!(e.on_key(ESCAPE, false, 50).swallow);
        assert!(!e.is_recording());
    }

    #[test]
    fn poll_expires_a_pending_tap_so_polish_can_start() {
        let mut e = wispr();
        run(&mut e, &[(Key::Fn, true, 0), (Key::Fn, false, 80)]);
        e.poll(500);
        let s = run(&mut e, &[(LSHIFT, true, 600), (Key::Fn, true, 610), (Key::Fn, false, 1400)]);
        assert_eq!(s, vec![Start(Mode::Polish), Finish]);
    }
}
