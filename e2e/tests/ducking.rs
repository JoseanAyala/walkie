//! Other audio is lowered while recording and put back on every way out.
//! Run: cargo test -p hearme-e2e --test ducking

use hearme_e2e::{Rig, Setup};

const BEFORE: f32 = 0.8;
const DUCKED: f32 = 0.8 * 0.3;

fn near(v: f32, want: f32) -> bool {
    (v - want).abs() < 1e-4
}

#[test]
fn hold_fn_lowers_the_volume_and_release_restores_it() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn").wait(1500);
    assert!(near(r.volume.volume(), DUCKED), "recording: {}", r.volume.volume());
    r.wait(1500).release("Fn");
    assert!(r.typed()[0].to_lowercase().contains("hello"), "{:?}", r.typed());
    assert_eq!(r.volume.volume(), BEFORE);
}

#[test]
fn hands_free_stays_lowered_until_stopped() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn").wait(40).press("Space").wait(60).release("Space").release("Fn");
    r.wait(3000);
    assert!(near(r.volume.volume(), DUCKED), "hands-free: {}", r.volume.volume());
    r.press("Fn");
    assert_eq!(r.volume.volume(), BEFORE);
    r.release("Fn");
}

#[test]
fn fn_plus_a_letter_cancels_and_restores() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn").wait(30).press("A").release("A").wait(500).release("Fn");
    assert!(r.typed().is_empty(), "{:?}", r.typed());
    assert_eq!(r.volume.volume(), BEFORE);
}

#[test]
fn quick_tap_restores() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn").wait(60).release("Fn").wait(1000);
    assert_eq!(r.volume.volume(), BEFORE);
}

#[test]
fn volume_changed_mid_recording_is_kept() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn").wait(1500);
    r.volume.user_set(0.5);
    r.wait(1500).release("Fn");
    assert_eq!(r.volume.volume(), 0.5);
}
