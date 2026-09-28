//! Paste last transcript (shortcut + tray) and the History tab's Copy
//! buttons, against the real installed app.
//! Run: e2e/run-app-tests.sh  (same requirements as tests/app.rs)
#![cfg(all(target_os = "macos", feature = "os-tests"))]

//! Each test runs alone and must finish in under 10s (os::TEST_BUDGET).

use walkie_e2e::os::{begin, sleep, App, Keyboard, Target};

fn ready_app(seed: Option<&str>) -> App {
    let app = App::new();
    app.write_config(&App::test_config());
    if let Some(text) = seed {
        app.seed_history(text);
    }
    app.launch();
    app
}

#[test]
fn ctrl_cmd_v_pastes_the_newest_history_row_after_a_restart() {
    let _t = begin("ctrl_cmd_v_pastes_the_newest_history_row_after_a_restart");
    let app = ready_app(Some("seeded from an earlier run"));
    app.require_keyboard();
    let doc = Target::open();
    Keyboard::new()
        .down("Ctrl")
        .down("Cmd")
        .tap("V")
        .up("Cmd")
        .up("Ctrl");
    let t = doc.wait_for(3, |t| t.contains("seeded from an earlier run"));
    assert_eq!(
        t,
        "seeded from an earlier run",
        "the V must not type either\n{}",
        app.log_text()
    );
}

/// With right ⌘ as the dictate key, Ctrl + ⌘ + V often starts with that ⌘
/// alone, which begins a recording. The chord must still paste the last
/// transcript, and the V must not reach the app (it would paste whatever
/// is on the clipboard).
#[test]
fn paste_last_chord_starting_with_the_dictate_key_pastes() {
    let _t = begin("paste_last_chord_starting_with_the_dictate_key_pastes");
    let app = App::new();
    let mut c = App::test_config();
    c.hotkeys.dictate = vec!["RightCmd".into()];
    app.write_config(&c);
    app.seed_history("seeded from an earlier run");
    app.launch();
    app.require_keyboard();
    walkie_e2e::os::set_clipboard("not this: the clipboard");
    let doc = Target::open();
    Keyboard::new()
        .down("RightCmd")
        .wait(300)
        .down("Ctrl")
        .tap("V")
        .up("RightCmd")
        .up("Ctrl");
    let t = doc.wait_for(3, |t| t.contains("seeded from an earlier run"));
    assert_eq!(t, "seeded from an earlier run", "{}", app.log_text());
}

#[test]
fn tray_pastes_the_last_dictation_again() {
    let _t = begin("tray_pastes_the_last_dictation_again");
    let app = ready_app(None);
    app.require_keyboard();
    let doc = Target::open();
    Keyboard::new().down("Fn").wait(600).up("Fn");
    let first = doc.wait_for(5, |t| t.to_lowercase().contains("hello"));
    assert!(
        first.to_lowercase().contains("hello"),
        "dictation didn't type: {first:?}\n{}",
        app.log_text()
    );
    app.tray("Paste last transcript");
    let t = doc.wait_for(3, |t| t.len() >= 2 * first.len());
    assert_eq!(
        t,
        format!("{first}{first}"),
        "the tray should paste the same text into the app that had focus (front: {:?})\n{}",
        walkie_e2e::os::frontmost(),
        app.log_text()
    );
}

fn clipboard() -> String {
    let out = std::process::Command::new("pbpaste").output().unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn history_copy_button_puts_the_text_on_the_clipboard() {
    let _t = begin("history_copy_button_puts_the_text_on_the_clipboard");
    let app = ready_app(Some("copy me from history"));
    app.tray("Settings…");
    assert!(app.wait_window("Walkie", true, 3), "Settings didn't open");
    app.click("Walkie", "History", 1);
    app.click("Walkie", "Copy", 1);
    let ok = (0..20).any(|_| {
        sleep(100);
        clipboard() == "copy me from history"
    });
    let got = clipboard();
    assert!(ok, "clipboard has {got:?}\n{}", app.log_text());
}
