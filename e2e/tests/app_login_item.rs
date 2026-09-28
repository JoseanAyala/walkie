//! Launch at login against the real installed app: the Settings checkbox and
//! the onboarding one change macOS's actual login item (SMAppService), which
//! is checked from outside with `walkie --login-item status`.
//! Run: e2e/run-app-tests.sh. The original login-item state is put back
//! after every test (os::restore_login_item).
#![cfg(all(target_os = "macos", feature = "os-tests"))]

use walkie_e2e::os::{begin, login_item, login_item_on, sleep, step, App};

const LABEL: &str = "Launch walkie at login";

fn wait_login_item(on: bool) -> String {
    step(format!(
        "waiting for the login item to be {}",
        if on { "on" } else { "off" }
    ));
    let mut s = login_item("status");
    for _ in 0..20 {
        if login_item_on(&s) == on {
            break;
        }
        sleep(100);
        s = login_item("status");
    }
    s
}

/// The page reads the state asynchronously; give it a moment to catch up.
fn wait_checkbox(app: &App, window: &str, want: bool) -> Option<bool> {
    step(format!(
        "waiting for the checkbox in {window:?} to show {want}"
    ));
    let mut v = app.checkbox(window, LABEL);
    for _ in 0..20 {
        if v == Some(want) {
            break;
        }
        sleep(100);
        v = app.checkbox(window, LABEL);
    }
    v
}

#[test]
fn settings_checkbox_toggles_the_real_login_item() {
    let _t = begin("settings_checkbox_toggles_the_real_login_item");
    let app = App::new();
    app.write_config(&App::test_config());
    app.launch();
    app.tray("Settings…");
    assert!(app.wait_window("Walkie", true, 3), "Settings didn't open");
    app.click("Walkie", "General", 1);

    let before = login_item_on(&login_item("status"));
    assert_eq!(
        wait_checkbox(&app, "Walkie", before),
        Some(before),
        "checkbox doesn't show macOS's state"
    );

    for want in [!before, before] {
        app.click_checkbox("Walkie", LABEL);
        let s = wait_login_item(want);
        assert_eq!(
            login_item_on(&s),
            want,
            "toggling the checkbox left the login item {s:?}"
        );
        // the UI re-reads the state after applying it
        assert_eq!(wait_checkbox(&app, "Walkie", want), Some(want));
        if s == "requires_approval" {
            assert!(
                app.text("Walkie").contains("approval"),
                "no approval hint shown"
            );
        }
    }
}

#[test]
fn onboarding_turns_launch_at_login_on_by_default() {
    let _t = begin("onboarding_turns_launch_at_login_on_by_default");
    let app = App::new(); // no config file → first run
    login_item("off");
    app.launch();
    assert!(
        app.wait_window("Welcome to Walkie", true, 3),
        "onboarding didn't show"
    );
    assert_eq!(
        wait_checkbox(&app, "Welcome to Walkie", true),
        Some(true),
        "should be checked by default"
    );
    assert!(
        !login_item_on(&login_item("status")),
        "applied before onboarding finished"
    );
    app.click("Welcome to Walkie", "I've granted everything — finish", 1);
    let s = wait_login_item(true);
    assert!(
        login_item_on(&s),
        "finishing onboarding left the login item {s:?}"
    );
}

#[test]
fn onboarding_unchecked_leaves_launch_at_login_off() {
    let _t = begin("onboarding_unchecked_leaves_launch_at_login_off");
    let app = App::new();
    login_item("off");
    app.launch();
    assert!(
        app.wait_window("Welcome to Walkie", true, 3),
        "onboarding didn't show"
    );
    app.click_checkbox("Welcome to Walkie", LABEL);
    assert_eq!(wait_checkbox(&app, "Welcome to Walkie", false), Some(false));
    app.click("Welcome to Walkie", "I've granted everything — finish", 1);
    assert!(
        app.wait_window("Welcome to Walkie", false, 3),
        "onboarding didn't close"
    );
    assert!(!app.config().first_run);
    assert_eq!(login_item("status"), "disabled");
}
