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
    let saved = clipboard();
    let app = ready_app(Some("copy me from history"));
    app.tray("Settings…");
    assert!(app.wait_window("walkie", true, 3), "Settings didn't open");
    app.click("walkie", "History", 1);
    app.click("walkie", "Copy", 1);
    let ok = (0..20).any(|_| {
        sleep(100);
        clipboard() == "copy me from history"
    });
    let got = clipboard();
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg("printf %s \"$1\" | pbcopy")
        .arg("-")
        .arg(&saved)
        .status();
    assert!(ok, "clipboard has {got:?}\n{}", app.log_text());
}
