//! "Lower other audio while dictating", against the real installed app and
//! the real system volume. Run: e2e/run-app-tests.sh (with --test app_ducking)
//!
//! Each test sets a known output volume first and puts yours back after,
//! even when it fails.
#![cfg(all(target_os = "macos", feature = "os-tests"))]

use hearme_e2e::os::{begin, osa, quit, sleep, step, App, Keyboard, Target};

/// The output volume (0–100) as the menu bar shows it.
fn volume() -> i64 {
    osa("output volume of (get volume settings)", &[])
        .unwrap()
        .parse()
        .unwrap()
}

fn set_volume(v: i64) {
    step(format!("setting output volume to {v}"));
    osa(
        &format!("set volume output volume {v} without output muted"),
        &[],
    )
    .unwrap();
}

/// Restores the volume (and mute) the person running the tests had.
struct KeepVolume(String);

impl KeepVolume {
    fn at(v: i64) -> KeepVolume {
        let muted = osa("output muted of (get volume settings)", &[]).unwrap_or_default();
        let keep = KeepVolume(format!(
            "set volume output volume {} {} output muted",
            volume(),
            if muted == "true" { "with" } else { "without" }
        ));
        set_volume(v);
        keep
    }
}

impl Drop for KeepVolume {
    fn drop(&mut self) {
        quit();
        let _ = osa(&self.0, &[]);
    }
}

fn wait_volume(secs: u64, pred: impl Fn(i64) -> bool) -> i64 {
    step("waiting for the output volume");
    for _ in 0..secs * 10 {
        let v = volume();
        if pred(v) {
            return v;
        }
        sleep(100);
    }
    volume()
}

fn app_with(duck: bool) -> App {
    let app = App::new();
    let mut c = App::test_config();
    c.audio.duck_while_recording = duck;
    c.audio.duck_percent = 30;
    app.write_config(&c);
    app.launch();
    app.require_keyboard();
    app
}

/// Starts a hands-free recording, so the test can look at the volume while
/// nothing is held.
fn start_hands_free() -> Keyboard {
    let mut kb = Keyboard::new();
    kb.down("Fn").wait(60).tap("Space").wait(60).up("Fn");
    kb
}

#[test]
fn recording_lowers_the_volume_and_stopping_restores_it() {
    let _t = begin("recording_lowers_the_volume_and_stopping_restores_it");
    let _keep = KeepVolume::at(50);
    let app = app_with(true);
    let doc = Target::open();
    let mut kb = start_hands_free();
    let v = wait_volume(2, |v| v < 50);
    assert!(
        (13..=17).contains(&v),
        "expected ~15 (30% of 50) while recording, got {v}\n{}",
        app.log_text()
    );
    kb.down("Fn").wait(60).up("Fn");
    let v = wait_volume(2, |v| v == 50);
    assert_eq!(
        v,
        50,
        "volume not restored after stopping\n{}",
        app.log_text()
    );
    doc.wait_for(5, |t| t.to_lowercase().contains("hello"));
}

#[test]
fn volume_changed_during_recording_is_kept() {
    let _t = begin("volume_changed_during_recording_is_kept");
    let _keep = KeepVolume::at(50);
    let app = app_with(true);
    let _doc = Target::open();
    let mut kb = start_hands_free();
    wait_volume(2, |v| v < 50);
    set_volume(70);
    kb.down("Fn").wait(60).up("Fn");
    sleep(800);
    assert_eq!(
        volume(),
        70,
        "hearme overwrote a volume the user picked\n{}",
        app.log_text()
    );
}

#[test]
fn quitting_mid_recording_restores_the_volume() {
    let _t = begin("quitting_mid_recording_restores_the_volume");
    let _keep = KeepVolume::at(50);
    let app = app_with(true);
    let _doc = Target::open();
    start_hands_free();
    let v = wait_volume(2, |v| v < 50);
    assert!(v < 50, "never lowered\n{}", app.log_text());
    quit(); // SIGTERM, like `kill` or logging out
    assert_eq!(
        wait_volume(2, |v| v == 50),
        50,
        "volume left lowered after quitting mid-recording"
    );
}

#[test]
fn turned_off_leaves_the_volume_alone() {
    let _t = begin("turned_off_leaves_the_volume_alone");
    let _keep = KeepVolume::at(50);
    let app = app_with(false);
    let _doc = Target::open();
    let mut kb = start_hands_free();
    sleep(800);
    assert_eq!(
        volume(),
        50,
        "ducking is off but the volume changed\n{}",
        app.log_text()
    );
    kb.down("Fn").wait(60).up("Fn");
}
