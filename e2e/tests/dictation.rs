//! Keystrokes in, text out — with the default Wispr-style shortcuts unless a
//! test says otherwise. Run: cargo test -p walkie-e2e
//! Needs: cargo run -p walkie-core --example fetch_model -- base

use walkie_core::config::Hotkeys;
use walkie_e2e::{Rig, Setup};

fn one(rig: &Rig) -> String {
    let t = rig.typed();
    assert_eq!(
        t.len(),
        1,
        "expected exactly one paste, got {t:?} (errors: {:?})",
        rig.errors()
    );
    t[0].to_lowercase()
}

#[test]
fn hold_fn_types_english() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn").wait(3000).release("Fn");
    assert!(one(&r).contains("hello"), "{:?}", r.typed());
}

#[test]
fn hold_fn_types_spanish() {
    let mut r = Rig::new(Setup {
        audio: "es.wav",
        ..Default::default()
    });
    r.press("Fn").wait(3000).release("Fn");
    assert!(one(&r).contains("hola"), "{:?}", r.typed());
}

#[test]
fn fn_shift_polishes() {
    let mut r = Rig::new(Setup {
        polish_command: "tr 'a-z' 'A-Z'",
        ..Default::default()
    });
    r.press("Shift")
        .press("Fn")
        .wait(3000)
        .release("Fn")
        .release("Shift");
    let t = r.typed();
    assert_eq!(t.len(), 1, "{t:?}");
    assert!(
        t[0].contains("HELLO"),
        "polish command should have run: {t:?}"
    );
}

#[test]
fn fn_shift_polishes_with_apples_model() {
    let mut r = Rig::new(Setup {
        // walkie-ai's contract: `respond <prompt>`, the transcript on stdin
        apple_helper: Some(
            r#"[ "$1" = respond ] && [ -n "$2" ] && { printf 'apple: '; tr a-z A-Z; }"#,
        ),
        ..Default::default()
    });
    r.press("Shift")
        .press("Fn")
        .wait(3000)
        .release("Fn")
        .release("Shift");
    let t = r.typed();
    assert_eq!(t.len(), 1, "{t:?} (errors: {:?})", r.errors());
    assert!(t[0].starts_with("apple: HELLO"), "{t:?}");
}

#[test]
fn adding_shift_mid_hold_polishes_the_same_recording() {
    let mut r = Rig::new(Setup {
        polish_command: "tr 'a-z' 'A-Z'",
        ..Default::default()
    });
    r.press("Fn")
        .wait(500)
        .press("Shift")
        .wait(2500)
        .release("Shift")
        .release("Fn");
    assert!(r.typed()[0].contains("HELLO"), "{:?}", r.typed());
}

#[test]
fn fn_space_hands_free_until_fn() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn")
        .wait(40)
        .press("Space")
        .wait(60)
        .release("Space")
        .release("Fn");
    assert!(r.typed().is_empty(), "still recording after release");
    r.wait(5000).press("Fn");
    assert!(one(&r).contains("hello"));
    r.release("Fn");
    assert_eq!(
        r.swallowed,
        vec![("Space".to_string(), true), ("Space".to_string(), false)],
        "the Space must never reach the focused app"
    );
}

#[test]
fn quick_tap_types_nothing() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn").wait(60).release("Fn").wait(1000);
    assert!(r.typed().is_empty());
}

#[test]
fn double_tap_locks_until_the_next_tap() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn")
        .wait(60)
        .release("Fn")
        .wait(100)
        .press("Fn")
        .wait(60)
        .release("Fn");
    r.wait(4000);
    assert!(r.typed().is_empty(), "locked: still recording");
    r.press("Fn").wait(60).release("Fn");
    assert!(one(&r).contains("hello"));
}

#[test]
fn fn_plus_a_letter_is_a_shortcut_not_dictation() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn")
        .wait(30)
        .press("A")
        .release("A")
        .wait(2000)
        .release("Fn");
    assert!(r.typed().is_empty(), "{:?}", r.typed());
    assert!(
        r.swallowed.is_empty(),
        "the letter must still reach the app"
    );
}

#[test]
fn custom_combo_binding_works_and_swallows_its_key() {
    let v = |k: &[&str]| k.iter().map(|s| s.to_string()).collect();
    let mut r = Rig::new(Setup {
        hotkeys: Hotkeys {
            dictate: v(&["Ctrl", "Opt", "D"]),
            polish: vec![],
            hands_free: vec![],
            paste_last: vec![],
        },
        ..Default::default()
    });
    r.press("Ctrl")
        .press("Opt")
        .press("D")
        .wait(3000)
        .release("D")
        .release("Opt")
        .release("Ctrl");
    assert!(one(&r).contains("hello"));
    assert_eq!(
        r.swallowed,
        vec![("D".to_string(), true), ("D".to_string(), false)]
    );
}

#[test]
fn ctrl_cmd_v_pastes_the_last_transcript_again() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn").wait(3000).release("Fn");
    let first = one(&r);
    r.wait(1000)
        .press("Ctrl")
        .press("Cmd")
        .press("V")
        .wait(80)
        .release("V")
        .release("Cmd")
        .release("Ctrl");
    let t = r.typed();
    assert_eq!(t.len(), 2, "{t:?} (errors: {:?})", r.errors());
    assert_eq!(
        t[1].to_lowercase(),
        first,
        "the same text, not a new transcription"
    );
    assert_eq!(
        r.swallowed,
        vec![("V".to_string(), true), ("V".to_string(), false)],
        "the V must not type"
    );
    let starts = r
        .signals
        .iter()
        .filter(|s| matches!(s, walkie_core::hotkey::engine::Signal::Start(_)))
        .count();
    assert_eq!(
        starts, 1,
        "paste-last must not start a recording: {:?}",
        r.signals
    );
}

#[test]
fn ctrl_cmd_v_with_nothing_dictated_types_nothing() {
    let mut r = Rig::new(Setup::default());
    r.press("Ctrl")
        .press("Cmd")
        .press("V")
        .wait(80)
        .release("V")
        .release("Cmd")
        .release("Ctrl");
    assert!(r.typed().is_empty(), "{:?}", r.typed());
}

#[test]
fn legacy_right_cmd_config_still_dictates() {
    let cfg: walkie_core::config::Config =
        toml_from("[hotkeys]\ndictate = \"RightCmd\"\npolish_modifier = \"Shift\"");
    let mut r = Rig::new(Setup {
        hotkeys: cfg.hotkeys,
        ..Default::default()
    });
    r.press("RightCmd").wait(3000).release("RightCmd");
    assert!(one(&r).contains("hello"));
}

fn toml_from(s: &str) -> walkie_core::config::Config {
    let dir = std::env::temp_dir().join(format!("walkie-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("config.toml");
    std::fs::write(&p, s).unwrap();
    walkie_core::config::Config::load_from(&p).unwrap()
}
