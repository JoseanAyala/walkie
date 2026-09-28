//! Themes (Settings → Theme and the light | dark switch), against the real
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
    assert!(app.wait_window("walkie", true, 3), "Settings didn't open");
    app.click("walkie", "Theme", 1);
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
fn picking_a_theme_saves_it_and_tells_every_window() {
    let _t = begin("picking_a_theme_saves_it_and_tells_every_window");
    let app = theme_tab();
    app.click("walkie", "Klein", 1);
    let c = wait_config(&app, "theme klein", |c| c.theme.name == "klein");
    assert_eq!(c.theme.name, "klein", "not saved");
    // logged where the "theme" event goes out to every window
    assert!(
        app.wait_log("walkie: theme klein", 2),
        "no theme change in the log:\n{}",
        app.log_text()
    );
}

#[test]
fn light_dark_theme_switch_pins_the_mode() {
    let _t = begin("light_dark_theme_switch_pins_the_mode");
    let app = theme_tab();
    // the switch in the top bar comes before Appearance's own buttons
    app.click("walkie", "dark", 1);
    let c = wait_config(&app, "dark", |c| c.theme.appearance == Appearance::Dark);
    assert_eq!(c.theme.appearance, Appearance::Dark);
    app.click("walkie", "light", 1);
    let c = wait_config(&app, "light", |c| c.theme.appearance == Appearance::Light);
    assert_eq!(c.theme.appearance, Appearance::Light);
    app.click("walkie", "system", 1);
    let c = wait_config(&app, "system", |c| c.theme.appearance == Appearance::System);
    assert_eq!(c.theme.appearance, Appearance::System);
}

#[test]
fn importing_a_coolors_link_adds_and_picks_a_theme() {
    let _t = begin("importing_a_coolors_link_adds_and_picks_a_theme");
    let app = theme_tab();
    app.set_field(
        "walkie",
        "Colors to import",
        "https://coolors.co/e94b3c-7479d8-2b2a30",
    );
    app.set_field("walkie", "Theme name", "Sunset");
    app.click("walkie", "Save theme", 1);
    let c = wait_config(&app, "theme Sunset", |c| c.theme.name == "Sunset");
    assert_eq!(c.theme.name, "Sunset", "not picked after saving");
    let t = &c.theme.custom[0];
    assert_eq!(
        (
            t.name.as_str(),
            t.base.as_str(),
            t.main.as_str(),
            t.accent.as_str()
        ),
        ("Sunset", "#2b2a30", "#7479d8", "#e94b3c"),
        "roles: the darkest is the base, the most colorful the accent"
    );
    app.click("walkie", "Delete Sunset", 1);
    let c = wait_config(&app, "no imported themes", |c| c.theme.custom.is_empty());
    assert!(c.theme.custom.is_empty(), "not deleted: {:?}", c.theme);
    assert_eq!(
        c.theme.name, "classic",
        "deleting the theme in use falls back"
    );
}
