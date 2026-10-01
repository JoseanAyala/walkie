//! Lowers the system output volume while recording ("ducking") and puts it
//! back afterwards — lowered, never muted, so you still hear what's playing.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// The system output volume, 0.0–1.0.
pub trait OutputVolume: Send + Sync {
    /// None when muted or unknown (no output device, no volume control).
    fn get(&self) -> Option<f32>;
    fn set(&self, v: f32);
}

/// Closer than this counts as the same volume (devices quantise the scalar).
const SAME: f32 = 0.01;

struct Saved {
    before: f32,
    ducked: f32,
}

/// Remembers the pre-recording volume between `duck` and `restore`. Shared
/// with the app so quitting mid-recording can restore too.
pub struct Ducker {
    out: Box<dyn OutputVolume>,
    /// The volume during recording, as % of the volume before it.
    percent: AtomicU32,
    saved: Mutex<Option<Saved>>,
}

impl Ducker {
    /// `percent`: the volume during recording, as % of the volume before it.
    pub fn new(out: Box<dyn OutputVolume>, percent: u32) -> Self {
        Self {
            out,
            percent: AtomicU32::new(percent.min(100)),
            saved: Mutex::new(None),
        }
    }

    /// Changes how far the next `duck` lowers the volume.
    pub fn set_percent(&self, percent: u32) {
        self.percent.store(percent.min(100), Ordering::SeqCst);
    }

    pub fn duck(&self) {
        let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        if saved.is_some() {
            return;
        }
        // Muted, silent, or nothing to lower: leave it alone.
        let Some(before) = self.out.get() else { return };
        let target = before * self.percent.load(Ordering::SeqCst) as f32 / 100.0;
        if before < SAME || before - target < SAME {
            return;
        }
        self.out.set(target);
        // Read back what the device actually took, so `restore` can tell our
        // value from one the user picked.
        let ducked = self.out.get().unwrap_or(target);
        *saved = Some(Saved { before, ducked });
    }

    /// Puts the volume back, unless the user changed it since `duck`.
    pub fn restore(&self) {
        let Some(s) = self.saved.lock().unwrap_or_else(|e| e.into_inner()).take() else {
            return;
        };
        match self.out.get() {
            Some(now) if (now - s.ducked).abs() < SAME => self.out.set(s.before),
            _ => {} // changed or muted by the user: theirs now
        }
    }
}

/// An in-memory volume, for tests.
#[derive(Clone)]
pub struct MemVolume(Arc<Mutex<(f32, bool)>>);

impl MemVolume {
    pub fn new(v: f32) -> Self {
        Self(Arc::new(Mutex::new((v, false))))
    }
    pub fn volume(&self) -> f32 {
        self.0.lock().unwrap().0
    }
    /// A change made "by the user" (not through the ducker).
    pub fn user_set(&self, v: f32) {
        self.0.lock().unwrap().0 = v;
    }
    pub fn set_muted(&self, m: bool) {
        self.0.lock().unwrap().1 = m;
    }
}

impl OutputVolume for MemVolume {
    fn get(&self) -> Option<f32> {
        let s = self.0.lock().unwrap();
        (!s.1).then_some(s.0)
    }
    fn set(&self, v: f32) {
        self.0.lock().unwrap().0 = v;
    }
}

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
mod platform;
pub use platform::SystemVolume;

#[cfg(test)]
mod tests {
    use super::*;

    fn ducker(v: &MemVolume, percent: u32) -> Ducker {
        Ducker::new(Box::new(v.clone()), percent)
    }

    #[test]
    fn ducks_to_a_fraction_and_restores_exactly() {
        let v = MemVolume::new(0.8);
        let d = ducker(&v, 30);
        d.duck();
        assert!((v.volume() - 0.24).abs() < 1e-6, "{}", v.volume());
        d.restore();
        assert_eq!(v.volume(), 0.8);
    }

    #[test]
    fn a_new_percent_applies_to_the_next_duck() {
        let v = MemVolume::new(0.8);
        let d = ducker(&v, 30);
        d.set_percent(50);
        d.duck();
        assert!((v.volume() - 0.4).abs() < 1e-6, "{}", v.volume());
    }

    #[test]
    fn user_change_mid_recording_is_not_stomped() {
        let v = MemVolume::new(0.8);
        let d = ducker(&v, 30);
        d.duck();
        v.user_set(0.5);
        d.restore();
        assert_eq!(v.volume(), 0.5);
    }

    #[test]
    fn muted_mid_recording_is_left_alone() {
        let v = MemVolume::new(0.8);
        let d = ducker(&v, 30);
        d.duck();
        v.set_muted(true);
        d.restore();
        assert!((v.volume() - 0.24).abs() < 1e-6);
    }

    #[test]
    fn muted_or_silent_or_full_percent_is_a_noop() {
        let v = MemVolume::new(0.8);
        v.set_muted(true);
        ducker(&v, 30).duck();
        assert_eq!(v.volume(), 0.8);

        let v = MemVolume::new(0.0);
        ducker(&v, 30).duck();
        assert_eq!(v.volume(), 0.0);

        let v = MemVolume::new(0.6);
        ducker(&v, 100).duck();
        assert_eq!(v.volume(), 0.6);
    }

    #[test]
    fn noop_duck_means_restore_does_nothing() {
        // Muted at start, unmuted by the user during recording: nothing of
        // ours to undo.
        let v = MemVolume::new(0.8);
        v.set_muted(true);
        let d = ducker(&v, 30);
        d.duck();
        v.set_muted(false);
        v.user_set(0.7);
        d.restore();
        assert_eq!(v.volume(), 0.7);
    }

    #[test]
    fn double_duck_keeps_the_original_and_restore_is_idempotent() {
        let v = MemVolume::new(0.8);
        let d = ducker(&v, 50);
        d.duck();
        d.duck();
        d.restore();
        assert_eq!(v.volume(), 0.8);
        v.user_set(0.3);
        d.restore();
        assert_eq!(v.volume(), 0.3);
    }
}
