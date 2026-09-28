//! The manual testing checklist, automated against the real installed app.
//! Run: e2e/run-app-tests.sh  (builds + installs, then runs these one at a time)
//!
//! Keyboard tests need Accessibility granted to /Applications/Walkie.app and
//! to the terminal running the tests (to post key events and script the UI).
#![cfg(all(target_os = "macos", feature = "os-tests"))]

//! Each test runs alone and must finish in under 10s (os::TEST_BUDGET);
//! past that the run aborts and reports the step it was stuck on.

use walkie_e2e::os::{begin, clipboard, set_clipboard, sleep, step, App, Keyboard, Target};

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
        app.wait_window("Welcome to Walkie", true, 3),
        "onboarding didn't show: {:?}",
        app.windows()
    );
    app.click("Welcome to Walkie", "I've granted everything — finish", 1);
    assert!(
        app.wait_window("Welcome to Walkie", false, 3),
        "onboarding didn't close"
    );
    assert!(
        app.wait_window("Walkie", true, 3),
        "finishing onboarding should open Settings"
    );
    assert!(
        !app.config().first_run,
        "first_run should be saved as false"
    );

    app.launch(); // same dirs → second run
    sleep(1000); // onboarding shows during startup, before the hook line
    assert!(
        !app.windows().iter().any(|w| w == "Welcome to Walkie"),
        "onboarding reappeared"
    );
}

#[test]
fn opening_the_app_shows_settings() {
    let _t = begin("opening_the_app_shows_settings");
    let app = ready_app();
    assert!(
        app.wait_window("Walkie", true, 3),
        "launching walkie by hand should show Settings"
    );
    app.close("Walkie");
    assert!(app.wait_window("Walkie", false, 3));
    app.reopen();
    assert!(
        app.wait_window("Walkie", true, 3),
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
        app.wait_window("Walkie", true, 3),
        "Settings didn't open from the tray"
    );
    for tab in ["General", "Status", "Polish", "History"] {
        app.click("Walkie", tab, 1);
    }
    app.close("Walkie");
    assert!(app.wait_window("Walkie", false, 3));
    app.tray("Settings…");
    assert!(
        app.wait_window("Walkie", true, 3),
        "Settings couldn't be reopened after closing"
    );
}

/// Regression: exit() with the Whisper model still loaded aborted in ggml's
/// Metal teardown (GGML_ASSERT), so every quit was a crash.
#[test]
fn quitting_from_the_tray_does_not_crash() {
    let _t = begin("quitting_from_the_tray_does_not_crash");
    let app = ready_app();
    assert!(app.wait_log("model ready", 5), "{}", app.log_text());
    app.tray("Quit Walkie");
    let gone = (0..50).any(|_| {
        sleep(100);
        !std::process::Command::new("pgrep")
            .args(["-x", "walkie"])
            .status()
            .is_ok_and(|s| s.success())
    });
    assert!(gone, "walkie didn't quit\n{}", app.log_text());
    let log = app.log_text();
    assert!(!log.contains("GGML_ASSERT"), "crashed on quit:\n{log}");
}

#[test]
fn status_tab_lists_every_check() {
    let _t = begin("status_tab_lists_every_check");
    let app = ready_app();
    app.tray("Settings…");
    assert!(app.wait_window("Walkie", true, 3));
    app.click("Walkie", "Status", 1);
    let mut text = String::new();
    for _ in 0..20 {
        text = app.text("Walkie");
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

/// The overlay doesn't just vanish: it says the text went in, briefly
/// (glue's DONE_GRACE), then hides.
#[test]
fn the_overlay_says_typed_before_it_hides() {
    let _t = begin("the_overlay_says_typed_before_it_hides");
    let app = ready_app();
    app.require_keyboard();
    let doc = Target::open();
    Keyboard::new().down("Fn").wait(600).up("Fn");
    step("waiting for the overlay to say ✓ DONE");
    let mut seen = String::new();
    for _ in 0..30 {
        seen = app.overlay_text();
        if seen.contains("✓ DONE") {
            break;
        }
        sleep(50);
    }
    assert!(
        seen.contains("✓ DONE"),
        "the overlay never confirmed; it showed {seen:?}\n{}",
        app.log_text()
    );
    assert!(
        doc.text().to_lowercase().contains("hello"),
        "it said typed, so the text should be in"
    );
}

/// A mic that sends nothing (muted, or a headset not sending audio): while
/// still recording, the overlay says it can't hear you.
#[test]
fn a_silent_mic_says_it_cant_hear_you() {
    let _t = begin("a_silent_mic_says_it_cant_hear_you");
    let app = App::new();
    app.write_config(&App::test_config());
    let quiet = app.silence(1);
    app.launch_with(&[("WALKIE_TEST_AUDIO", &quiet)]);
    app.require_keyboard();
    let _doc = Target::open();
    let mut kb = Keyboard::new();
    kb.down("Fn").wait(2100); // past the overlay's DEAF_MS
    step("waiting for the overlay to say it can't hear");
    let mut seen = String::new();
    for _ in 0..20 {
        seen = app.overlay_text();
        if seen.contains("can't hear you") {
            break;
        }
        sleep(50);
    }
    kb.up("Fn");
    assert!(
        seen.contains("can't hear you"),
        "the overlay should say it can't hear; it showed {seen:?}\n{}",
        app.log_text()
    );
}

/// The other side: a mic that hears you never gets the hint, even held past
/// DEAF_MS. (The mic reports its first level before the session says
/// "recording"; the overlay once dropped it and cried wolf.)
#[test]
fn a_live_mic_never_says_it_cant_hear_you() {
    let _t = begin("a_live_mic_never_says_it_cant_hear_you");
    let app = ready_app();
    app.require_keyboard();
    let _doc = Target::open();
    let mut kb = Keyboard::new();
    kb.down("Fn").wait(2300);
    step("reading the overlay");
    let mut seen = String::new();
    for _ in 0..10 {
        seen = app.overlay_text();
        if !seen.is_empty() {
            break;
        }
        sleep(50);
    }
    kb.up("Fn");
    assert!(
        seen.contains("listening") && !seen.contains("can't hear"),
        "the overlay should still be listening; it showed {seen:?}\n{}",
        app.log_text()
    );
}

/// Regression: typed text arrives as keyDowns with no keyUps; the hook read
/// them as a key held forever, so the second hold matched no shortcut.
#[test]
fn typing_strategy_dictates_twice_in_a_row() {
    let _t = begin("typing_strategy_dictates_twice_in_a_row");
    let app = App::new();
    let mut c = App::test_config();
    c.inject.strategy = "type".into();
    app.write_config(&c);
    app.launch();
    app.require_keyboard();
    let doc = Target::open();
    let mut kb = Keyboard::new();
    for n in 1..=2 {
        kb.down("Fn").wait(600).up("Fn");
        let t = doc.wait_for(4, |t| t.to_lowercase().matches("hello").count() >= n);
        assert!(
            t.to_lowercase().matches("hello").count() >= n,
            "dictation {n} typed nothing: {t:?}\n{}",
            app.log_text()
        );
    }
}

/// Shift+Fn, the default polish shortcut: a tap, no recording.
fn tap_polish(kb: &mut Keyboard) {
    kb.down("Shift").down("Fn").wait(80).up("Fn").up("Shift");
}

#[test]
fn fn_shift_polishes_the_whole_field_and_keeps_the_clipboard() {
    let _t = begin("fn_shift_polishes_the_whole_field_and_keeps_the_clipboard");
    let app = ready_app();
    app.require_keyboard();
    set_clipboard("walkie-e2e sentinel");
    let doc = Target::with_text("hello there\nsecond line");
    tap_polish(&mut Keyboard::new());
    let t = doc.wait_for(5, |t| t == "HELLO THERE\nSECOND LINE");
    assert_eq!(
        t,
        "HELLO THERE\nSECOND LINE",
        "the upper-casing polish command should replace it all\n{}",
        app.log_text()
    );
    sleep(500); // the paste's own clipboard restore
    assert_eq!(clipboard(), "walkie-e2e sentinel", "clipboard not restored");
}

#[test]
fn fn_shift_polishes_only_the_selection() {
    let _t = begin("fn_shift_polishes_only_the_selection");
    let app = ready_app();
    app.require_keyboard();
    let doc = Target::with_text("keep this, hello");
    let mut kb = Keyboard::new();
    kb.down("Shift");
    for _ in 0.."hello".len() {
        kb.tap("Left");
    }
    kb.up("Shift").wait(100);
    tap_polish(&mut kb);
    let t = doc.wait_for(5, |t| t.contains("HELLO"));
    assert_eq!(t, "keep this, HELLO", "{}", app.log_text());
}

#[test]
fn fn_shift_in_an_empty_field_says_so() {
    let _t = begin("fn_shift_in_an_empty_field_says_so");
    let app = ready_app();
    app.require_keyboard();
    let doc = Target::open();
    tap_polish(&mut Keyboard::new());
    assert!(
        app.wait_log("Nothing to polish", 3),
        "no notice\n{}",
        app.log_text()
    );
    assert_eq!(doc.text(), "");
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
    assert!(app.wait_window("Walkie", true, 3));
    app.click("Walkie", "General", 1);
    app.click("Walkie", "Record", 1); // Dictate row
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

    app.close("Walkie"); // back to the app you were typing in
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
