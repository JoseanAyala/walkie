//! Light and dark (Settings → General → Appearance), against the real
//! installed app. Run: e2e/run-app-tests.sh theme
#![cfg(all(target_os = "macos", feature = "os-tests"))]

//! Each test runs alone and must finish in under 10s (os::TEST_BUDGET).

use walkie_core::config::{Appearance, Config};
use walkie_e2e::os::{begin, sleep, step, App};

fn theme_tab() -> App {
    let app = App::new();
    app.write_config(&App::test_config());
    app.launch();
    app.tray("Settings…");
    assert!(app.wait_window("Walkie", true, 3), "Settings didn't open");
    app.click("Walkie", "General", 1);
    app
}

/// Polls the saved config until `ok` holds.
fn wait_config(app: &App, what: &str, ok: impl Fn(&Config) -> bool) -> Config {
    step(format!("waiting for the config to have {what}"));
    let mut c = app.config();
    for _ in 0..20 {
        if ok(&c) {
            break;
        }
        sleep(100);
        c = app.config();
    }
    c
}

#[test]
fn theme_appearance_pins_light_or_dark() {
    let _t = begin("theme_appearance_pins_light_or_dark");
    let app = theme_tab();
    app.click("Walkie", "dark", 1);
    let c = wait_config(&app, "dark", |c| c.theme.appearance == Appearance::Dark);
    assert_eq!(c.theme.appearance, Appearance::Dark);
    // logged where the "theme" event goes out to every window
    assert!(
        app.wait_log("walkie: theme Dark", 2),
        "no theme change in the log:\n{}",
        app.log_text()
    );
    app.click("Walkie", "light", 1);
    let c = wait_config(&app, "light", |c| c.theme.appearance == Appearance::Light);
    assert_eq!(c.theme.appearance, Appearance::Light);
    app.click("Walkie", "system", 1);
    let c = wait_config(&app, "system", |c| c.theme.appearance == Appearance::System);
    assert_eq!(c.theme.appearance, Appearance::System);
}
