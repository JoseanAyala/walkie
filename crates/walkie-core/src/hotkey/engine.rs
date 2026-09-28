//! Turns physical key events into dictation signals. Pure: no OS calls, so
//! every gesture is unit-testable. The macOS tap (`tap.rs`) feeds it events
//! and obeys its `swallow` verdicts.
//!
//! A binding is *satisfied* when the held keys are exactly its keys — so
//! `Fn+A` never triggers an `Fn` binding. Gestures:
//! - dictate / polish: hold to talk, double-tap to lock (see `machine.rs`)
//! - while holding dictate, adding keys that satisfy polish upgrades the
//!   running recording instead of starting a new one
//! - any other key within STRAY_CANCEL_MS of starting cancels (you were
//!   typing a shortcut, not dictating)
//! - paste-last: a one-shot tap that never records; it fires once every key
//!   of the chord is up, so the paste it triggers isn't mixed with (or
//!   swallowed as) the chord's own keys

use super::keys::{self, Key, ESCAPE};
use super::machine::HotkeyMachine;
use super::{Mode, Output};
use crate::config::Hotkeys;
use std::collections::HashSet;

/// A regular key this soon after a hold starts means a shortcut (fn+Z), not
/// dictation. Longer than the hold threshold on purpose: when the 🌐 key has
/// a system action, macOS holds back the next key ~120ms to see whether fn
/// was tapped, so a quick fn+Z reaches us ~170ms apart (measured by the OS
/// e2e suite).
pub const STRAY_CANCEL_MS: u128 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Dictate,
    Polish,
    PasteLast,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bindings {
    pub dictate: Vec<Key>,
    pub polish: Vec<Key>,
    pub paste_last: Vec<Key>,
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
            paste_last: parse("Paste last", &h.paste_last),
        };
        let same = |x: &[Key], y: &[Key]| {
            !x.is_empty() && x.len() == y.len() && x.iter().all(|k| y.iter().any(|j| k.matches(*j)))
        };
        if same(&b.polish, &b.dictate) {
            errors.push("Polish shortcut is the same as Dictate — Polish disabled".into());
            b.polish.clear();
        }
        if same(&b.paste_last, &b.dictate) || same(&b.paste_last, &b.polish) {
            errors.push("Paste-last shortcut duplicates another one — Paste last disabled".into());
            b.paste_last.clear();
        }
        // The paste itself is a synthesized ⌘V, which the tap sees too.
        if same(&b.paste_last, &[Key::Cmd(keys::Side::Any), Key::Code(9)]) {
            errors.push(
                "Paste-last can't be ⌘V (it would trigger itself) — Paste last disabled".into(),
            );
            b.paste_last.clear();
        }
        (b, errors)
    }

    fn get(&self, a: Action) -> &[Key] {
        match a {
            Action::Dictate => &self.dictate,
            Action::Polish => &self.polish,
            Action::PasteLast => &self.paste_last,
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
    /// Re-insert the most recent transcript. Never starts a recording.
    PasteLast,
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
    /// Set after a stray-key cancel: ignore everything until all keys are up.
    inhibit: bool,
    swallowed: HashSet<Key>,
    recording: Option<Vec<Key>>,
    /// The paste-last chord was pressed; fire once every key is up.
    paste_pending: bool,
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
            inhibit: false,
            swallowed: HashSet::new(),
            recording: None,
            paste_pending: false,
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
        self.paste_pending = false;
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    pub fn on_key(&mut self, k: Key, down: bool, t: u128) -> Verdict {
        if down {
            if self.held.contains(&k) {
                // auto-repeat: follow the verdict of the original press
                return Verdict {
                    signal: None,
                    swallow: self.swallowed.contains(&k),
                };
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
            Verdict {
                signal: signal.into_signal(),
                swallow,
            }
        } else {
            self.held.retain(|h| *h != k);
            let swallow = self.swallowed.remove(&k);
            if self.recording.is_some() {
                return Verdict {
                    signal: self.record_up(),
                    swallow,
                };
            }
            if self.inhibit {
                if self.held.is_empty() {
                    self.inhibit = false;
                }
                return Verdict {
                    signal: None,
                    swallow,
                };
            }
            let mut signal = self.up(k, t);
            if self.paste_pending && self.held.is_empty() {
                self.paste_pending = false;
                signal = signal.or(Some(Signal::PasteLast));
            }
            Verdict { signal, swallow }
        }
    }

    /// Call every ~50ms so an unanswered first tap expires.
    pub fn poll(&mut self, now: u128) {
        self.machine.poll(now);
        if !self.machine.is_engaged() && self.anchor.is_none() {
            self.mode = None;
        }
    }

    fn satisfied(&self) -> Option<Action> {
        [Action::Polish, Action::Dictate, Action::PasteLast]
            .into_iter()
            .find(|&a| {
                let b = self.bindings.get(a);
                !b.is_empty()
                    && b.len() == self.held.len()
                    && b.iter().all(|bk| self.held.iter().any(|h| bk.matches(*h)))
                    && self.held.iter().all(|h| b.iter().any(|bk| bk.matches(*h)))
            })
    }

    fn down(&mut self, k: Key, t: u128) -> Down {
        let sat = self.satisfied();

        if self.anchor.is_some() && self.machine.is_holding() {
            return match sat {
                Some(Action::Polish) if self.mode == Some(Mode::Dictate) => {
                    self.mode = Some(Mode::Polish);
                    Down::Consumed(Some(Signal::SetMode(Mode::Polish)))
                }
                _ if !k.is_modifier() && t.saturating_sub(self.pressed_at) < STRAY_CANCEL_MS => {
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
            // Not while a locked recording runs: the paste would land mid-dictation.
            Some(Action::PasteLast) if !self.machine.is_locked() => {
                self.paste_pending = true;
                Down::Consumed(None)
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
            return Verdict {
                signal: Some(Signal::RecordCancelled),
                swallow: true,
            };
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
        Verdict {
            signal: None,
            swallow,
        }
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

    const A: Key = Key::Code(0);
    const D: Key = Key::Code(2);
    const V: Key = Key::Code(9);
    const LSHIFT: Key = Key::Shift(Left);
    const LCTRL: Key = Key::Ctrl(Left);
    const LCMD: Key = Key::Cmd(Left);

    fn wispr() -> Engine {
        Engine::new(Bindings {
            dictate: vec![Key::Fn],
            polish: vec![Key::Fn, Key::Shift(Any)],
            paste_last: vec![Key::Ctrl(Any), Key::Cmd(Any), V],
        })
    }

    /// Feeds (key, down, t) events; returns the non-empty signals.
    fn run(e: &mut Engine, evs: &[(Key, bool, u128)]) -> Vec<Signal> {
        evs.iter()
            .filter_map(|&(k, d, t)| e.on_key(k, d, t).signal)
            .collect()
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
            paste_last: v(&["A"]),
        });
        assert_eq!(b.dictate, vec![Key::Cmd(Right)]);
        assert!(b.polish.is_empty() && b.paste_last.is_empty());
        assert_eq!(errs.len(), 2, "{errs:?}");
    }

    #[test]
    fn hold_fn_dictates() {
        let mut e = wispr();
        assert_eq!(
            run(&mut e, &[(Key::Fn, true, 0), (Key::Fn, false, 800)]),
            vec![Start(Mode::Dictate), Finish]
        );
    }

    #[test]
    fn quick_tap_cancels_and_double_tap_locks() {
        let mut e = wispr();
        let s = run(
            &mut e,
            &[
                (Key::Fn, true, 0),
                (Key::Fn, false, 80), // tap 1 → cancel
                (Key::Fn, true, 200), // tap 2 → locked start
                (Key::Fn, false, 260),
                (Key::Fn, true, 5000), // stop tap
                (Key::Fn, false, 5060),
            ],
        );
        assert_eq!(
            s,
            vec![Start(Mode::Dictate), Cancel, Start(Mode::Dictate), Finish]
        );
    }

    #[test]
    fn shift_then_fn_is_polish() {
        let mut e = wispr();
        let s = run(
            &mut e,
            &[
                (LSHIFT, true, 0),
                (Key::Fn, true, 10),
                (LSHIFT, false, 300), // releasing Shift doesn't end it
                (Key::Fn, false, 900),
            ],
        );
        assert_eq!(s, vec![Start(Mode::Polish), Finish]);
    }

    #[test]
    fn adding_shift_mid_hold_upgrades_to_polish() {
        let mut e = wispr();
        let s = run(
            &mut e,
            &[
                (Key::Fn, true, 0),
                (LSHIFT, true, 400),
                (LSHIFT, false, 600),
                (Key::Fn, false, 900),
            ],
        );
        assert_eq!(s, vec![Start(Mode::Dictate), SetMode(Mode::Polish), Finish]);
    }

    #[test]
    fn stray_key_right_after_start_cancels_and_passes_through() {
        let mut e = wispr();
        e.on_key(Key::Fn, true, 0);
        let a = e.on_key(A, true, 40);
        assert_eq!(a.signal, Some(Cancel));
        assert!(!a.swallow);
        // nothing retriggers until everything is released
        assert_eq!(
            run(&mut e, &[(A, false, 60), (Key::Fn, false, 100)]),
            vec![]
        );
        assert_eq!(
            e.on_key(Key::Fn, true, 500).signal,
            Some(Start(Mode::Dictate))
        );
    }

    #[test]
    fn stray_key_delayed_by_the_globe_key_still_cancels() {
        let mut e = wispr();
        e.on_key(Key::Fn, true, 2841);
        assert_eq!(e.on_key(Key::Code(6), true, 3007).signal, Some(Cancel)); // Z, 166ms later
    }

    #[test]
    fn a_stray_modifier_does_not_cancel() {
        let mut e = wispr();
        e.on_key(Key::Fn, true, 0);
        assert_eq!(e.on_key(Key::Cmd(Left), true, 40).signal, None);
        e.on_key(Key::Cmd(Left), false, 60);
        assert_eq!(e.on_key(Key::Fn, false, 900).signal, Some(Finish));
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
        let s = run(
            &mut e,
            &[
                (Key::Cmd(Left), true, 0),
                (Key::Fn, true, 10),
                (Key::Fn, false, 900),
            ],
        );
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
        let mut e = Engine::new(Bindings {
            dictate: vec![Key::Cmd(Right)],
            ..Default::default()
        });
        assert_eq!(
            run(
                &mut e,
                &[(Key::Cmd(Left), true, 0), (Key::Cmd(Left), false, 900)]
            ),
            vec![]
        );
        assert_eq!(
            run(
                &mut e,
                &[
                    (Key::Cmd(Right), true, 1000),
                    (Key::Cmd(Right), false, 1900)
                ]
            ),
            vec![Start(Mode::Dictate), Finish]
        );
    }

    #[test]
    fn empty_binding_is_disabled() {
        let mut e = Engine::new(Bindings {
            dictate: vec![Key::Fn],
            ..Default::default()
        });
        let s = run(
            &mut e,
            &[
                (LSHIFT, true, 0),
                (Key::Fn, true, 10),
                (Key::Fn, false, 900),
            ],
        );
        assert_eq!(s, vec![], "no polish binding → Shift+Fn does nothing");
    }

    #[test]
    fn rebinding_takes_effect_immediately() {
        let mut e = wispr();
        e.set_bindings(Bindings {
            dictate: vec![Key::Cmd(Right)],
            ..Default::default()
        });
        assert_eq!(
            run(&mut e, &[(Key::Fn, true, 0), (Key::Fn, false, 900)]),
            vec![]
        );
        assert_eq!(
            e.on_key(Key::Cmd(Right), true, 1000).signal,
            Some(Start(Mode::Dictate))
        );
    }

    #[test]
    fn recorder_captures_the_largest_chord_and_suppresses_triggers() {
        let mut e = wispr();
        e.start_recording();
        assert_eq!(
            e.on_key(Key::Fn, true, 0).signal,
            None,
            "Fn must not start dictation"
        );
        e.on_key(Key::Ctrl(Left), true, 10);
        assert!(e.on_key(D, true, 20).swallow);
        e.on_key(D, false, 100);
        e.on_key(Key::Ctrl(Left), false, 110);
        let v = e.on_key(Key::Fn, false, 120);
        assert_eq!(v.signal, Some(Recorded(vec![Key::Fn, Key::Ctrl(Any), D])));
        assert!(!e.is_recording());
        assert_eq!(
            e.on_key(Key::Fn, true, 500).signal,
            Some(Start(Mode::Dictate))
        );
    }

    #[test]
    fn recorder_keeps_side_for_lone_modifiers() {
        let mut e = wispr();
        e.start_recording();
        e.on_key(Key::Cmd(Right), true, 0);
        assert_eq!(
            e.on_key(Key::Cmd(Right), false, 50).signal,
            Some(Recorded(vec![Key::Cmd(Right)]))
        );
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
        let s = run(
            &mut e,
            &[
                (LSHIFT, true, 600),
                (Key::Fn, true, 610),
                (Key::Fn, false, 1400),
            ],
        );
        assert_eq!(s, vec![Start(Mode::Polish), Finish]);
    }

    #[test]
    fn ctrl_cmd_v_pastes_last_once_everything_is_up() {
        let mut e = wispr();
        assert!(!e.on_key(LCTRL, true, 0).swallow);
        assert!(!e.on_key(LCMD, true, 10).swallow);
        let v = e.on_key(V, true, 20);
        assert_eq!(v.signal, None, "fires on release, not press");
        assert!(v.swallow, "the V must not reach the focused app");
        assert!(e.on_key(V, true, 60).swallow, "auto-repeat swallowed too");
        let up = e.on_key(V, false, 100);
        assert!(up.swallow && up.signal.is_none(), "modifiers still held");
        assert_eq!(e.on_key(LCMD, false, 110).signal, None);
        assert_eq!(e.on_key(LCTRL, false, 120).signal, Some(PasteLast));
    }

    #[test]
    fn paste_last_never_starts_a_recording() {
        let mut e = wispr();
        let s = run(
            &mut e,
            &[
                (LCTRL, true, 0),
                (LCMD, true, 10),
                (V, true, 20),
                (V, false, 2000),
                (LCMD, false, 2010),
                (LCTRL, false, 2020),
            ],
        );
        assert_eq!(s, vec![PasteLast]);
        e.poll(3000);
        assert_eq!(
            e.on_key(Key::Fn, true, 3100).signal,
            Some(Start(Mode::Dictate)),
            "dictation unaffected"
        );
    }

    #[test]
    fn the_paste_it_triggers_does_not_retrigger_it() {
        let mut e = wispr();
        run(
            &mut e,
            &[
                (LCTRL, true, 0),
                (LCMD, true, 10),
                (V, true, 20),
                (V, false, 60),
                (LCMD, false, 70),
                (LCTRL, false, 80),
            ],
        );
        // what the injector then posts: ⌘V
        assert_eq!(e.on_key(LCMD, true, 100), Verdict::default());
        assert_eq!(e.on_key(V, true, 110), Verdict::default());
        assert_eq!(run(&mut e, &[(V, false, 120), (LCMD, false, 130)]), vec![]);
    }

    #[test]
    fn plain_cmd_v_and_ctrl_v_pass_through() {
        let mut e = wispr();
        e.on_key(LCMD, true, 0);
        assert_eq!(e.on_key(V, true, 10), Verdict::default());
        assert_eq!(run(&mut e, &[(V, false, 20), (LCMD, false, 30)]), vec![]);
        e.on_key(LCTRL, true, 100);
        assert_eq!(e.on_key(V, true, 110), Verdict::default());
        assert_eq!(run(&mut e, &[(V, false, 120), (LCTRL, false, 130)]), vec![]);
    }

    #[test]
    fn paste_last_is_ignored_while_recording() {
        let mut e = wispr();
        run(
            &mut e,
            &[
                (Key::Fn, true, 0),
                (Key::Fn, false, 60),
                (Key::Fn, true, 150),
                (Key::Fn, false, 200),
            ],
        );
        e.on_key(LCTRL, true, 1000);
        e.on_key(LCMD, true, 1010);
        assert!(!e.on_key(V, true, 1020).swallow);
        assert_eq!(
            run(
                &mut e,
                &[(V, false, 1030), (LCMD, false, 1040), (LCTRL, false, 1050)]
            ),
            vec![]
        );
    }

    #[test]
    fn empty_paste_last_binding_is_disabled() {
        let mut e = Engine::new(Bindings {
            dictate: vec![Key::Fn],
            ..Default::default()
        });
        e.on_key(LCTRL, true, 0);
        e.on_key(LCMD, true, 10);
        assert!(!e.on_key(V, true, 20).swallow);
        assert_eq!(
            run(
                &mut e,
                &[(V, false, 30), (LCMD, false, 40), (LCTRL, false, 50)]
            ),
            vec![]
        );
    }

    #[test]
    fn paste_last_duplicating_another_binding_or_cmd_v_is_disabled() {
        let v = |k: &[&str]| k.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (b, errs) = Bindings::from_config(&Hotkeys {
            paste_last: v(&["Fn", "Shift"]),
            ..Hotkeys::default()
        });
        assert!(b.paste_last.is_empty() && !b.polish.is_empty());
        assert_eq!(errs.len(), 1, "{errs:?}");
        let (b, errs) = Bindings::from_config(&Hotkeys {
            paste_last: v(&["Cmd", "V"]),
            ..Hotkeys::default()
        });
        assert!(b.paste_last.is_empty());
        assert_eq!(errs.len(), 1, "{errs:?}");
    }

    #[test]
    fn recorder_captures_a_paste_last_chord_without_pasting() {
        let mut e = wispr();
        e.start_recording();
        let s = run(
            &mut e,
            &[
                (LCTRL, true, 0),
                (LCMD, true, 10),
                (V, true, 20),
                (V, false, 60),
                (LCMD, false, 70),
                (LCTRL, false, 80),
            ],
        );
        assert_eq!(s, vec![Recorded(vec![Key::Ctrl(Any), Key::Cmd(Any), V])]);
    }
}
