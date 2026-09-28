//! The manual testing checklist, automated against the real installed app.
//! Run: e2e/run-app-tests.sh  (builds + installs, then runs these one at a time)
//!
//! Keyboard tests need Accessibility granted to /Applications/walkie.app and
//! to the terminal running the tests (to post key events and script the UI).
#![cfg(all(target_os = "macos", feature = "os-tests"))]

//! Each test runs alone and must finish in under 10s (os::TEST_BUDGET);
//! past that the run aborts and reports the step it was stuck on.

use walkie_e2e::os::{begin, sleep, App, Keyboard, Target};

fn ready_app() -> App {
    let app = App::new();
    app.write_config(&App::test_config());
    app.launch();
    app
}

// ---- windows, tray, onboarding (no keyboard needed)

#[test]
fn onboarding_shows_on_first_run_and_never_again() {
    let _t = begin("onboarding_shows_on_first_run_and_never_again");
    let app = App::new(); // no config file → first run
    app.launch();
    assert!(
        app.wait_window("Welcome to walkie", true, 3),
        "onboarding didn't show: {:?}",
        app.windows()
    );
    app.click("Welcome to walkie", "I've granted everything — finish", 1);
    assert!(
        app.wait_window("Welcome to walkie", false, 3),
        "onboarding didn't close"
    );
    assert!(
        app.wait_window("walkie", true, 3),
        "finishing onboarding should open Settings"
    );
    assert!(
        !app.config().first_run,
        "first_run should be saved as false"
    );

    app.launch(); // same dirs → second run
    sleep(1000); // onboarding shows during startup, before the hook line
    assert!(
        !app.windows().iter().any(|w| w == "Welcome to walkie"),
        "onboarding reappeared"
    );
}

#[test]
fn opening_the_app_shows_settings() {
    let _t = begin("opening_the_app_shows_settings");
    let app = ready_app();
    assert!(
        app.wait_window("walkie", true, 3),
        "launching walkie by hand should show Settings"
    );
    app.close("walkie");
    assert!(app.wait_window("walkie", false, 3));
    app.reopen();
    assert!(
        app.wait_window("walkie", true, 3),
        "opening walkie again (Dock/Finder) should show Settings"
    );
}

#[test]
fn tray_opens_settings_and_closing_only_hides_it() {
    let _t = begin("tray_opens_settings_and_closing_only_hides_it");
    let app = ready_app();
    let _ = app.close_if_open();
    app.tray("Settings…");
    assert!(
        app.wait_window("walkie", true, 3),
        "Settings didn't open from the tray"
    );
    for tab in ["General", "Status", "Cleanup", "Polish", "History"] {
        app.click("walkie", tab, 1);
    }
    app.close("walkie");
    assert!(app.wait_window("walkie", false, 3));
    app.tray("Settings…");
    assert!(
        app.wait_window("walkie", true, 3),
        "Settings couldn't be reopened after closing"
    );
}

#[test]
fn status_tab_lists_every_check() {
    let _t = begin("status_tab_lists_every_check");
    let app = ready_app();
    app.tray("Settings…");
    assert!(app.wait_window("walkie", true, 3));
    app.click("walkie", "Status", 1);
    let mut text = String::new();
    for _ in 0..20 {
        text = app.text("walkie");
        if text.contains("Speech model") {
            break;
        }
        sleep(100);
    }
    for label in [
        "Microphone",
        "Accessibility",
        "Input Monitoring",
        "Keyboard hook",
        "Shortcuts",
        "Input device",
        "Speech model",
    ] {
        assert!(
            text.contains(label),
            "Status tab is missing {label:?}:\n{text}"
        );
    }
    for label in ["Keyboard hook", "Shortcuts", "Speech model"] {
        assert!(
            app.status(label).is_some(),
            "startup log is missing the {label} check"
        );
    }
}

// ---- dictation through the real keyboard hook

#[test]
fn hold_fn_types_the_transcript_into_the_focused_app() {
    let _t = begin("hold_fn_types_the_transcript_into_the_focused_app");
    let app = ready_app();
    app.require_keyboard();
    let doc = Target::open();
    Keyboard::new().down("Fn").wait(600).up("Fn");
    let t = doc.wait_for(5, |t| t.to_lowercase().contains("hello"));
    assert!(
        t.to_lowercase().contains("hello"),
        "target has {t:?}\n{}",
        app.log_text()
    );
}

#[test]
fn fn_shift_runs_the_polish_command() {
    let _t = begin("fn_shift_runs_the_polish_command");
    let app = ready_app();
    app.require_keyboard();
    let doc = Target::open();
    Keyboard::new()
        .down("Shift")
        .down("Fn")
        .wait(600)
        .up("Fn")
        .up("Shift");
    let t = doc.wait_for(5, |t| t.contains("HELLO"));
    assert!(
        t.contains("HELLO"),
        "expected the upper-casing polish command to run: {t:?}\n{}",
        app.log_text()
    );
}

#[test]
fn fn_space_is_hands_free_and_the_space_never_types() {
    let _t = begin("fn_space_is_hands_free_and_the_space_never_types");
    let app = ready_app();
    app.require_keyboard();
    let doc = Target::open();
    let mut kb = Keyboard::new();
    kb.down("Fn").wait(60).tap("Space").wait(60).up("Fn");
    sleep(1000);
    assert_eq!(
        doc.text(),
        "",
        "still recording: nothing typed yet (and no space)"
    );
    kb.down("Fn").wait(60).up("Fn");
    let t = doc.wait_for(5, |t| t.to_lowercase().contains("hello"));
    assert!(
        t.to_lowercase().contains("hello"),
        "{t:?}\n{}",
        app.log_text()
    );
    assert!(!t.starts_with(' '), "the Space leaked into the app: {t:?}");
}

/// Regression: a 🌐 tap switches the input source, after which enigo's
/// layout lookup off the main thread was a SIGTRAP mid-paste.
#[test]
fn dictating_after_a_globe_tap_does_not_crash() {
    let _t = begin("dictating_after_a_globe_tap_does_not_crash");
    let app = ready_app();
    app.require_keyboard();
    let doc = Target::open();
    let mut kb = Keyboard::new();
    kb.down("Fn").wait(40).up("Fn").wait(500); // may switch input source / show emoji
    kb.down("Fn").wait(600).up("Fn");
    let t = doc.wait_for(5, |t| t.to_lowercase().contains("hello"));
    assert!(
        t.to_lowercase().contains("hello"),
        "{t:?}\n{}",
        app.log_text()
    );
    kb.down("Fn").wait(40).up("Fn").wait(500); // and once more for luck
    assert!(
        std::process::Command::new("pgrep")
            .args(["-x", "walkie"])
            .output()
            .unwrap()
            .status
            .success(),
        "walkie crashed"
    );
}

#[test]
fn quick_tap_types_nothing() {
    let _t = begin("quick_tap_types_nothing");
    let app = ready_app();
    app.require_keyboard();
    let doc = Target::open();
    Keyboard::new().down("Fn").wait(40).up("Fn");
    sleep(1500);
    assert_eq!(doc.text(), "");
}

#[test]
fn fn_plus_a_letter_is_not_dictation_and_the_letter_types() {
    // Z, not A: 🌐+A is a macOS shortcut (focus the Dock) that eats the key.
    let _t = begin("fn_plus_a_letter_is_not_dictation_and_the_letter_types");
    let app = ready_app();
    app.require_keyboard();
    let doc = Target::open();
    Keyboard::new()
        .down("Fn")
        .wait(20)
        .tap("Z")
        .wait(300)
        .up("Fn");
    sleep(1500);
    let t = doc.text();
    assert!(
        !t.to_lowercase().contains("hello"),
        "Fn+Z started a dictation: {t:?}"
    );
    assert!(
        t.to_lowercase().contains('z'),
        "the Z should still reach the app: {t:?}"
    );
}

#[test]
fn recorded_shortcut_works_immediately_and_its_key_is_swallowed() {
    let _t = begin("recorded_shortcut_works_immediately_and_its_key_is_swallowed");
    let app = ready_app();
    app.require_keyboard();
    // Something safe in front first: the tray doesn't activate walkie, so
    // without this the recording keystrokes would go to whatever app was.
    let first = Target::open();
    app.tray("Settings…");
    assert!(app.wait_window("walkie", true, 3));
    app.click("walkie", "General", 1);
    app.click("walkie", "Record", 1); // Dictate row
    let mut kb = Keyboard::into(&["walkie", walkie_e2e::os::TARGET]);
    kb.wait(300)
        .down("Ctrl")
        .down("Opt")
        .down("D")
        .wait(100)
        .up("D")
        .up("Opt")
        .up("Ctrl");
    let ok = (0..30).any(|_| {
        sleep(100);
        app.config().hotkeys.dictate == ["Ctrl", "Opt", "D"]
    });
    assert!(
        ok,
        "recorded shortcut wasn't saved: {:?}\n{}",
        app.config().hotkeys,
        app.log_text()
    );

    app.close("walkie"); // back to the app you were typing in
    drop(first);
    let doc = Target::open();
    let mut kb = Keyboard::new();
    kb.down("Ctrl")
        .down("Opt")
        .down("D")
        .wait(600)
        .up("D")
        .up("Opt")
        .up("Ctrl");
    let t = doc.wait_for(5, |t| t.to_lowercase().contains("hello"));
    assert!(
        t.to_lowercase().contains("hello"),
        "new shortcut didn't dictate without a restart: {t:?}\nclipboard: {:?}\nfront app: {:?}",
        std::process::Command::new("pbpaste")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string()),
        walkie_e2e::os::frontmost(),
    );
    assert!(!t.contains('∂'), "Opt+D leaked into the app: {t:?}");
}
