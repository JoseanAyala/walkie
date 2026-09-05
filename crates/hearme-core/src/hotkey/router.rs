use super::machine::HotkeyMachine;
use super::{Mode, Output};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RouterKey {
    Hot,      // the configured dictation key
    Modifier, // the polish modifier (Shift)
    Other,
}

/// Routes raw key events to one of two machines. The mode is chosen by
/// whether the modifier is down at the moment a session starts, and sticks
/// until that machine disengages.
pub struct Router {
    modifier_down: bool,
    active: Option<Mode>,
    dictate: HotkeyMachine,
    polish: HotkeyMachine,
}

impl Router {
    pub fn new() -> Self {
        Self {
            modifier_down: false,
            active: None,
            dictate: HotkeyMachine::new(),
            polish: HotkeyMachine::new(),
        }
    }

    pub fn handle(&mut self, key: RouterKey, down: bool, t: u128) -> Option<(Mode, Output)> {
        match key {
            RouterKey::Modifier => {
                self.modifier_down = down;
                None
            }
            RouterKey::Other => None,
            RouterKey::Hot => {
                let mode = self.active.unwrap_or(if self.modifier_down {
                    Mode::Polish
                } else {
                    Mode::Dictate
                });
                let m = self.machine(mode);
                let out = if down { m.press(t) } else { m.release(t) };
                self.active = if self.machine(mode).is_engaged() { Some(mode) } else { None };
                out.map(|o| (mode, o))
            }
        }
    }

    pub fn poll(&mut self, now: u128) {
        self.dictate.poll(now);
        self.polish.poll(now);
        if let Some(mode) = self.active {
            if !self.machine(mode).is_engaged() {
                self.active = None;
            }
        }
    }

    fn machine(&mut self, mode: Mode) -> &mut HotkeyMachine {
        match mode {
            Mode::Dictate => &mut self.dictate,
            Mode::Polish => &mut self.polish,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::{Mode, Output};

    #[test]
    fn plain_hold_is_dictate() {
        let mut r = Router::new();
        assert_eq!(r.handle(RouterKey::Hot, true, 0), Some((Mode::Dictate, Output::Start)));
        assert_eq!(r.handle(RouterKey::Hot, false, 500), Some((Mode::Dictate, Output::Finish)));
    }

    #[test]
    fn shift_held_at_press_is_polish() {
        let mut r = Router::new();
        r.handle(RouterKey::Modifier, true, 0);
        assert_eq!(r.handle(RouterKey::Hot, true, 10), Some((Mode::Polish, Output::Start)));
        // releasing shift mid-hold does not change the session's mode
        r.handle(RouterKey::Modifier, false, 200);
        assert_eq!(r.handle(RouterKey::Hot, false, 600), Some((Mode::Polish, Output::Finish)));
    }

    #[test]
    fn locked_session_keeps_mode_across_taps() {
        let mut r = Router::new();
        r.handle(RouterKey::Modifier, true, 0);
        r.handle(RouterKey::Hot, true, 10);
        r.handle(RouterKey::Hot, false, 60); // tap 1 (polish)
        r.handle(RouterKey::Modifier, false, 100);
        // second tap without shift still routes to the engaged polish machine
        assert_eq!(r.handle(RouterKey::Hot, true, 200), Some((Mode::Polish, Output::Start)));
    }

    #[test]
    fn other_keys_are_ignored() {
        let mut r = Router::new();
        assert_eq!(r.handle(RouterKey::Other, true, 0), None);
    }

    #[test]
    fn poll_clears_expired_engagement() {
        let mut r = Router::new();
        r.handle(RouterKey::Hot, true, 0);
        r.handle(RouterKey::Hot, false, 100); // tap → TapPending
        r.poll(500);
        // after expiry a shift-chorded press starts a fresh polish session
        r.handle(RouterKey::Modifier, true, 600);
        assert_eq!(r.handle(RouterKey::Hot, true, 610), Some((Mode::Polish, Output::Start)));
    }
}
