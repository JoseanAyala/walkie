//! The Settings and onboarding windows' own behavior (autosave, History
//! Delete, the polish Test button, live onboarding checks), against the real
//! installed app. Run: e2e/run-app-tests.sh  (same requirements as tests/app.rs)
#![cfg(all(target_os = "macos", feature = "os-tests"))]

//! Each test runs alone and must finish in under 10s (os::TEST_BUDGET).

use walkie_e2e::os::{begin, sleep, step, App};

fn settings(tab: &str, seed: Option<&str>) -> App {
    let app = App::new();
    app.write_config(&App::test_config());
    if let Some(text) = seed {
        app.seed_history(text);
    }
    app.launch();
    app.tray("Settings…");
    assert!(app.wait_window("walkie", true, 3), "Settings didn't open");
    app.click("walkie", tab, 1);
    app
}

/// Polls the window's text until `needle` is (or isn't) there.
fn wait_text(app: &App, window: &str, needle: &str, present: bool) -> String {
    step(format!(
        "waiting for {needle:?} to be {} in {window:?}",
        if present { "shown" } else { "gone" }
    ));
    let mut text = app.text(window);
    for _ in 0..20 {
        if text.contains(needle) == present {
            break;
        }
        sleep(100);
        text = app.text(window);
    }
    text
}

#[test]
fn changing_a_setting_saves_it_without_a_save_button() {
    let _t = begin("changing_a_setting_saves_it_without_a_save_button");
    let app = settings("Cleanup", None);
    let before = app.config().cleanup.enabled;
    app.click_checkbox("walkie", "Enable cleanup");
    step("waiting for the config file to change");
    let mut now = before;
    for _ in 0..20 {
        now = app.config().cleanup.enabled;
        if now != before {
            break;
        }
        sleep(100);
    }
    assert_ne!(now, before, "the change wasn't saved\n{}", app.log_text());
    let text = wait_text(&app, "walkie", "Restart to apply: cleanup", true);
    assert!(
        text.contains("Restart to apply: cleanup"),
        "cleanup is read at launch, so a restart hint should show:\n{text}"
    );
}

#[test]
fn model_picker_explains_disk_and_memory() {
    let _t = begin("model_picker_explains_disk_and_memory");
    let app = settings("General", None); // test config picks "base"
    let text = wait_text(&app, "walkie", "about 390 MB of RAM", true);
    assert!(
        text.contains("about 390 MB of RAM") && text.contains("148 MB"),
        "the model's disk and memory use should be spelled out:\n{text}"
    );
}

#[test]
fn history_delete_removes_the_row() {
    let _t = begin("history_delete_removes_the_row");
    let app = settings("History", Some("delete me from history"));
    let text = wait_text(&app, "walkie", "delete me from history", true);
    assert!(
        text.contains("delete me from history"),
        "row not shown:\n{text}"
    );
    app.click("walkie", "Delete", 1);
    let text = wait_text(&app, "walkie", "delete me from history", false);
    assert!(
        !text.contains("delete me from history"),
        "row still shown:\n{text}"
    );
    assert!(
        text.contains("no dictations yet"),
        "no empty state:\n{text}"
    );
    assert!(
        app.history().is_empty(),
        "the row is still in the database: {:?}",
        app.history()
    );
}

#[test]
fn polish_test_button_shows_the_commands_output() {
    let _t = begin("polish_test_button_shows_the_commands_output");
    let app = settings("Polish", None); // test config: tr 'a-z' 'A-Z'
    app.click("walkie", "Test", 1);
    let want = "QUICK TEST OF THE POLISH COMMAND";
    let text = wait_text(&app, "walkie", want, true);
    assert!(text.contains(want), "no polish output:\n{text}");
}

#[test]
fn onboarding_shows_live_checks_and_the_configured_shortcuts() {
    let _t = begin("onboarding_shows_live_checks_and_the_configured_shortcuts");
    let app = App::new();
    let mut c = App::test_config();
    c.first_run = true;
    c.hotkeys.hands_free = vec!["RightOpt".into()];
    app.write_config(&c);
    app.launch();
    assert!(
        app.wait_window("Welcome to walkie", true, 3),
        "onboarding didn't show: {:?}",
        app.windows()
    );
    // accessibility is granted to the app for this suite, so its row is OK
    let text = wait_text(&app, "Welcome to walkie", "OK", true);
    assert!(text.contains("Accessibility"), "{text}");
    assert!(text.contains("OK"), "no live check result:\n{text}");
    assert!(
        text.contains("right ⌥"),
        "Try it should show the configured hands-free key:\n{text}"
    );
}
