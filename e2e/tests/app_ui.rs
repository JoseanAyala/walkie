//! The Settings and onboarding windows' own behavior (autosave, History
//! Delete, the polish Test button, live onboarding checks), against the real
//! installed app. Run: e2e/run-app-tests.sh  (same requirements as tests/app.rs)
#![cfg(all(target_os = "macos", feature = "os-tests"))]

//! Each test runs alone and must finish in under 10s (os::TEST_BUDGET).

use walkie_core::config::{PolishProvider, Tone};
use walkie_core::pipeline::polish::{apple_status, describe_apple_status};
use walkie_e2e::os::{begin, drag, sleep, step, App, APP};

fn settings(tab: &str, seed: Option<&str>) -> App {
    settings_with(tab, seed, App::test_config())
}

fn settings_with(tab: &str, seed: Option<&str>, cfg: walkie_core::config::Config) -> App {
    let app = App::new();
    app.write_config(&cfg);
    if let Some(text) = seed {
        app.seed_history(text);
    }
    app.launch();
    app.tray("Settings…");
    assert!(app.wait_window("Walkie", true, 3), "Settings didn't open");
    app.click("Walkie", tab, 1);
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
    let app = settings("General", None);
    let before = app.config().audio.duck_while_recording;
    app.click_checkbox("Walkie", "Lower other audio while dictating");
    step("waiting for the config file to change");
    let mut now = before;
    for _ in 0..20 {
        now = app.config().audio.duck_while_recording;
        if now != before {
            break;
        }
        sleep(100);
    }
    assert_ne!(now, before, "the change wasn't saved\n{}", app.log_text());
    let text = wait_text(&app, "Walkie", "Restart to apply: ducking", true);
    assert!(
        text.contains("Restart to apply: ducking"),
        "ducking is read at launch, so a restart hint should show:\n{text}"
    );
}

#[test]
fn model_picker_says_how_much_ram_it_takes() {
    let _t = begin("model_picker_says_how_much_ram_it_takes");
    let app = settings("General", None); // test config picks "base"
    let text = wait_text(&app, "Walkie", "about 390 MB of RAM", true);
    assert!(
        text.contains("about 390 MB of RAM"),
        "the model's RAM use should be spelled out:\n{text}"
    );
}

#[test]
fn history_delete_removes_the_row() {
    let _t = begin("history_delete_removes_the_row");
    let app = settings("History", Some("delete me from history"));
    let text = wait_text(&app, "Walkie", "delete me from history", true);
    assert!(
        text.contains("delete me from history"),
        "row not shown:\n{text}"
    );
    assert!(
        text.contains("Today"),
        "rows are grouped by day; this one was made today:\n{text}"
    );
    // Delete only shows on hover, but stays reachable
    app.click("Walkie", "Delete", 1);
    let text = wait_text(&app, "Walkie", "delete me from history", false);
    assert!(
        !text.contains("delete me from history"),
        "row still shown:\n{text}"
    );
    assert!(text.contains("Nothing yet."), "no empty state:\n{text}");
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
    app.click("Walkie", "Test", 1);
    let want = "QUICK TEST OF THE POLISH COMMAND";
    let text = wait_text(&app, "Walkie", want, true);
    assert!(text.contains(want), "no polish output:\n{text}");
}

/// What the installed walkie-ai says about this Mac: the tests below expect
/// the app to agree, whether or not Apple Intelligence is on.
fn installed_apple_status() -> String {
    let s = apple_status(&std::path::Path::new(APP).join("Contents/MacOS/walkie-ai"));
    assert_ne!(s, "missing", "Walkie.app has no working walkie-ai");
    s
}

fn apple_config() -> walkie_core::config::Config {
    let mut c = App::test_config();
    c.polish.provider = PolishProvider::Apple;
    c
}

#[test]
fn polish_tab_says_whether_apples_model_is_ready() {
    let _t = begin("polish_tab_says_whether_apples_model_is_ready");
    let status = installed_apple_status();
    let app = settings_with("Polish", None, apple_config());
    let want = describe_apple_status(&status);
    let text = wait_text(&app, "Walkie", want, true);
    assert!(text.contains(want), "expected {want:?} ({status}):\n{text}");
    assert!(
        text.contains("Tone") && text.contains("Hey, are you free for lunch"),
        "Apple shows its tone:\n{text}"
    );
    assert!(!text.contains("Prompt"), "prompts aren't editable:\n{text}");
}

#[test]
fn the_polish_tab_shows_the_saved_tone_and_its_example() {
    let _t = begin("the_polish_tab_shows_the_saved_tone_and_its_example");
    let mut c = apple_config();
    c.polish.tone = Tone::VeryCasual;
    let app = settings_with("Polish", None, c);
    let want = "hey are you free for lunch tomorrow?";
    let text = wait_text(&app, "Walkie", want, true);
    assert!(text.contains(want), "no Very casual example:\n{text}");
    let picked = app.dropdowns("Walkie");
    assert!(
        picked.iter().any(|d| d.starts_with("Very casual")),
        "{picked:?}"
    );
}

/// The Test button with Apple selected, through a stand-in walkie-ai
/// (WALKIE_AI_BIN): the real model's first call after idle can take 4–8s,
/// too slow for the budget. commands.rs tests the real model directly.
#[test]
fn polish_test_button_runs_apples_model() {
    let _t = begin("polish_test_button_runs_apples_model");
    let app = App::new();
    let helper = app.root.join("walkie-ai");
    std::fs::write(
        &helper,
        "#!/bin/sh\ncase $1 in\n  status) echo available ;;\n  respond) printf 'apple says: '; cat ;;\nesac\n",
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    app.write_config(&apple_config());
    app.launch_with(&[("WALKIE_AI_BIN", &helper)]);
    app.tray("Settings…");
    assert!(app.wait_window("Walkie", true, 3), "Settings didn't open");
    app.click("Walkie", "Polish", 1);
    app.click("Walkie", "Test", 1);
    let want = "apple says: um so this is";
    let text = wait_text(&app, "Walkie", want, true);
    assert!(
        text.contains(want),
        "the Test button didn't use walkie-ai:\n{text}"
    );
}

#[test]
fn status_lists_apple_intelligence_when_polish_uses_it() {
    let _t = begin("status_lists_apple_intelligence_when_polish_uses_it");
    let status = installed_apple_status();
    let app = App::new();
    app.write_config(&apple_config());
    app.launch();
    assert_eq!(
        app.status("Apple Intelligence"),
        Some(status == "available"),
        "{}",
        app.log_text()
    );
}

#[test]
fn onboarding_shows_live_checks_and_the_configured_shortcuts() {
    let _t = begin("onboarding_shows_live_checks_and_the_configured_shortcuts");
    let app = App::new();
    let mut c = App::test_config();
    c.first_run = true;
    c.hotkeys.polish = vec!["RightOpt".into()];
    app.write_config(&c);
    app.launch();
    assert!(
        app.wait_window("Welcome to Walkie", true, 3),
        "onboarding didn't show: {:?}",
        app.windows()
    );
    // accessibility is granted to the app for this suite, so its row is OK
    let text = wait_text(&app, "Welcome to Walkie", "OK", true);
    assert!(text.contains("Accessibility"), "{text}");
    assert!(text.contains("OK"), "no live check result:\n{text}");
    assert!(
        text.contains("right ⌥"),
        "Try it should show the configured polish key:\n{text}"
    );
}

/// The title bar is hidden (the desk runs under the traffic lights), so the
/// page itself has to move the window: the top strip and the bare desk.
#[test]
fn settings_window_drags_by_its_top_strip_and_desk() {
    let _t = begin("settings_window_drags_by_its_top_strip_and_desk");
    let app = settings("General", None);
    for (what, at) in [
        // right of the WALKIE.OS1 chip, level with the traffic lights
        ("top strip", (300.0, 20.0)),
        // the desk between the Menu and About windows
        ("desk", (80.0, 300.0)),
    ] {
        let (x, y) = app.position("Walkie");
        drag((x + at.0, y + at.1), (40.0, 30.0));
        step(format!("waiting for the window to follow the {what}"));
        let mut now = (x, y);
        for _ in 0..20 {
            now = app.position("Walkie");
            if now != (x, y) {
                break;
            }
            sleep(100);
        }
        assert_eq!(
            now,
            (x + 40.0, y + 30.0),
            "dragging the {what} should move the window with the mouse"
        );
    }
}

/// walkie draws its own dropdowns: picking from one must save, like the
/// native popup did. (Its keyboard rules are unit-tested in listbox.ts; a
/// menu-bar app can't be made frontmost to type into from here.)
#[test]
fn a_dropdown_picks_with_the_mouse_and_saves() {
    let _t = begin("a_dropdown_picks_with_the_mouse_and_saves");
    let mut c = App::test_config();
    c.language = "auto".into();
    let app = settings_with("General", None, c);
    app.click_popup("Walkie", "Auto-detect (en/es)");
    app.click_list_item("Walkie", "English");
    step("waiting for the config to say en");
    let mut now = String::new();
    for _ in 0..20 {
        now = app.config().language;
        if now == "en" {
            break;
        }
        sleep(100);
    }
    assert_eq!(
        now,
        "en",
        "clicking English should pick it\n{}",
        app.log_text()
    );
    assert!(
        app.dropdowns("Walkie").iter().any(|d| d == "English"),
        "the dropdown should now show English: {:?}",
        app.dropdowns("Walkie")
    );
}

/// Settings can't be squeezed below the size its layout needs
/// (tauri.conf.json's minWidth/minHeight).
#[test]
fn settings_window_stops_shrinking_at_its_minimum() {
    let _t = begin("settings_window_stops_shrinking_at_its_minimum");
    let app = settings("General", None);
    let (w, h) = app.resize("Walkie", 300, 200);
    assert_eq!((w, h), (680, 440), "the window went below its minimum");
}
