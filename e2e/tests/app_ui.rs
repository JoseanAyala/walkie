//! The Settings and onboarding windows' own behavior (autosave, History
//! Delete, the polish Test button, live onboarding checks), against the real
//! installed app. Run: e2e/run-app-tests.sh  (same requirements as tests/app.rs)
#![cfg(all(target_os = "macos", feature = "os-tests"))]

//! Each test runs alone and must finish in under 10s (os::TEST_BUDGET).

use walkie_core::config::{PolishProvider, Tone};
use walkie_core::pipeline::polish::{apple_status, describe_apple_status};
use walkie_e2e::os::{begin, sleep, step, App, APP};

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
    app.click("Walkie", "Delete", 1);
    let text = wait_text(&app, "Walkie", "delete me from history", false);
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
    c.hotkeys.hands_free = vec!["RightOpt".into()];
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
        "Try it should show the configured hands-free key:\n{text}"
    );
}
