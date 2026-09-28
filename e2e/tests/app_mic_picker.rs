//! The microphone picker (Settings → General → Microphone), against the real
//! installed app. Run: e2e/run-app-tests.sh
#![cfg(all(target_os = "macos", feature = "os-tests"))]

//! Each test runs alone and must finish in under 10s (os::TEST_BUDGET).

use walkie_e2e::os::{begin, sleep, App, Keyboard, Target};

/// No real machine has a device by this name.
const UNPLUGGED: &str = "walkie-e2e unplugged mic";

fn app_with_mic(device: &str) -> App {
    let app = App::new();
    let mut c = App::test_config();
    c.audio.input_device = device.into();
    app.write_config(&c);
    app.launch();
    app
}

fn open_general(app: &App) {
    let _ = app.close_if_open();
    app.tray("Settings…");
    assert!(
        app.wait_window("Walkie", true, 3),
        "Settings didn't open from the tray"
    );
    app.click("Walkie", "General", 1);
}

/// The Microphone dropdown's selected option, once the page has filled it.
fn mic_choice(app: &App) -> String {
    let mut seen = Vec::new();
    for _ in 0..20 {
        seen = app.dropdowns("Walkie");
        if let Some(m) = seen
            .iter()
            .find(|v| v.starts_with("System default") || v.contains(UNPLUGGED))
        {
            return m.clone();
        }
        sleep(100);
    }
    panic!("no Microphone dropdown in Settings; dropdowns: {seen:?}");
}

#[test]
fn microphone_defaults_to_following_the_system() {
    let _t = begin("microphone_defaults_to_following_the_system");
    let app = app_with_mic("");
    open_general(&app);
    let m = mic_choice(&app);
    assert!(
        m.starts_with("System default ("),
        "expected the System default option selected: {m:?}"
    );
}

#[test]
fn unplugged_microphone_stays_selected_and_marked_not_connected() {
    let _t = begin("unplugged_microphone_stays_selected_and_marked_not_connected");
    let app = app_with_mic(UNPLUGGED);
    open_general(&app);
    let m = mic_choice(&app);
    assert_eq!(m, format!("{UNPLUGGED} (not connected)"));
    app.click("Walkie", "↻", 1); // a refresh mustn't reset it either
    sleep(300);
    assert_eq!(mic_choice(&app), format!("{UNPLUGGED} (not connected)"));
    assert_eq!(
        app.config().audio.input_device,
        UNPLUGGED,
        "the saved choice was reset"
    );
}

#[test]
fn status_warns_when_the_chosen_microphone_is_missing() {
    let _t = begin("status_warns_when_the_chosen_microphone_is_missing");
    let app = app_with_mic(UNPLUGGED);
    assert_eq!(
        app.status("Input device"),
        Some(false),
        "startup check should flag it:\n{}",
        app.log_text()
    );
    assert!(
        app.log_text()
            .contains(&format!("{UNPLUGGED} is not connected")),
        "{}",
        app.log_text()
    );
    open_general(&app);
    app.click("Walkie", "Status", 1);
    let mut text = String::new();
    for _ in 0..20 {
        text = app.text("Walkie");
        if text.contains("not connected") {
            break;
        }
        sleep(100);
    }
    assert!(
        text.contains(&format!("{UNPLUGGED} is not connected")),
        "Status tab:\n{text}"
    );
}

/// Test mode replaces the mic with a WAV, so this can't exercise the cpal
/// fallback itself — it checks a missing device never blocks dictation.
#[test]
fn dictation_works_with_a_missing_microphone() {
    let _t = begin("dictation_works_with_a_missing_microphone");
    let app = app_with_mic(UNPLUGGED);
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
