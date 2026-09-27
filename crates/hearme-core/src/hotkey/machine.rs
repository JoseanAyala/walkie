use super::Output;

pub const HOLD_MIN_MS: u128 = 150;
pub const TAP_WINDOW_MS: u128 = 300;

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Idle,
    MaybeHold { pressed_at: u128 },
    TapPending { released_at: u128 },
    Locked { armed: bool },
    Draining, // Finish sent from Locked; waiting for the key-up of the stop tap
}

pub struct HotkeyMachine {
    state: State,
}

impl HotkeyMachine {
    pub fn new() -> Self {
        Self { state: State::Idle }
    }

    pub fn is_engaged(&self) -> bool {
        self.state != State::Idle
    }

    /// The key is down and it's still undecided whether this is a hold or a tap.
    pub fn is_holding(&self) -> bool {
        matches!(self.state, State::MaybeHold { .. })
    }

    /// A double-tap locked session is running (or its stop tap is draining).
    pub fn is_locked(&self) -> bool {
        matches!(self.state, State::Locked { .. } | State::Draining)
    }

    pub fn reset(&mut self) {
        self.state = State::Idle;
    }

    pub fn press(&mut self, t: u128) -> Option<Output> {
        match self.state {
            State::Idle => {
                self.state = State::MaybeHold { pressed_at: t };
                Some(Output::Start)
            }
            State::TapPending { released_at } if t.saturating_sub(released_at) <= TAP_WINDOW_MS => {
                self.state = State::Locked { armed: false };
                Some(Output::Start)
            }
            State::TapPending { .. } => {
                self.state = State::MaybeHold { pressed_at: t };
                Some(Output::Start)
            }
            State::Locked { armed: true } => {
                self.state = State::Draining;
                Some(Output::Finish)
            }
            // MaybeHold (key auto-repeat), Locked{armed:false}, Draining: ignore
            _ => None,
        }
    }

    pub fn release(&mut self, t: u128) -> Option<Output> {
        match self.state {
            State::MaybeHold { pressed_at } if t.saturating_sub(pressed_at) < HOLD_MIN_MS => {
                self.state = State::TapPending { released_at: t };
                Some(Output::CancelDiscard)
            }
            State::MaybeHold { .. } => {
                self.state = State::Idle;
                Some(Output::Finish)
            }
            State::Locked { armed: false } => {
                self.state = State::Locked { armed: true };
                None
            }
            State::Draining => {
                self.state = State::Idle;
                None
            }
            _ => None,
        }
    }

    /// Call periodically (~50ms) so an unanswered first tap expires.
    pub fn poll(&mut self, now: u128) {
        if let State::TapPending { released_at } = self.state {
            if now.saturating_sub(released_at) > TAP_WINDOW_MS {
                self.state = State::Idle;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::Output::*;

    #[test]
    fn hold_and_release_dictates() {
        let mut m = HotkeyMachine::new();
        assert_eq!(m.press(0), Some(Start));
        assert_eq!(m.release(500), Some(Finish));
        assert!(!m.is_engaged());
    }

    #[test]
    fn quick_tap_cancels() {
        let mut m = HotkeyMachine::new();
        assert_eq!(m.press(0), Some(Start));
        assert_eq!(m.release(100), Some(CancelDiscard));
        assert!(m.is_engaged()); // waiting for possible second tap
    }

    #[test]
    fn double_tap_locks_then_tap_stops() {
        let mut m = HotkeyMachine::new();
        m.press(0);
        m.release(100); // tap 1
        assert_eq!(m.press(250), Some(Start)); // tap 2 inside 300ms window → locked
        assert_eq!(m.release(350), None);      // completes tap 2, stays locked
        assert!(m.is_engaged());
        assert_eq!(m.press(5000), Some(Finish)); // stop tap
        assert_eq!(m.release(5080), None);
        assert!(!m.is_engaged());
    }

    #[test]
    fn late_second_press_starts_fresh_hold() {
        let mut m = HotkeyMachine::new();
        m.press(0);
        m.release(100);
        assert_eq!(m.press(1000), Some(Start)); // window expired → new hold
        assert_eq!(m.release(1400), Some(Finish));
    }

    #[test]
    fn poll_expires_tap_window() {
        let mut m = HotkeyMachine::new();
        m.press(0);
        m.release(100);
        m.poll(401);
        assert!(!m.is_engaged());
    }

    #[test]
    fn key_repeat_press_ignored_while_held() {
        let mut m = HotkeyMachine::new();
        assert_eq!(m.press(0), Some(Start));
        assert_eq!(m.press(50), None); // OS auto-repeat
        assert_eq!(m.release(500), Some(Finish));
    }

    #[test]
    fn stray_release_ignored() {
        let mut m = HotkeyMachine::new();
        assert_eq!(m.release(10), None);
    }
}
