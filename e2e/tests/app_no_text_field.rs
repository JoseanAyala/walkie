//! "No text field" against the real installed app: dictating where nothing
//! is editable leaves the transcript on the clipboard and says so, while a
//! real text field is still typed into as before.
//! Run: cargo test -p walkie-e2e --features os-tests --test app_no_text_field -- --test-threads=1
//! (after e2e/run-app-tests.sh has installed the app).
#![cfg(all(target_os = "macos", feature = "os-tests"))]

//! Each test runs alone and must finish in under 10s (os::TEST_BUDGET).

use walkie_e2e::os::{
    begin, clipboard, focus_finder, set_clipboard, sleep, step, App, Keyboard, Target,
};

const NOTICE: &str = "No text field";

fn ready_app() -> App {
    let app = App::new();
    app.write_config(&App::test_config());
    app.launch();
    app
}

#[test]
fn a_text_field_is_typed_into_and_the_clipboard_restored() {
    let _t = begin("a_text_field_is_typed_into_and_the_clipboard_restored");
    let app = ready_app();
    app.require_keyboard();
    set_clipboard("walkie-e2e sentinel");
    let doc = Target::open();
    Keyboard::new().down("Fn").wait(600).up("Fn");
    let t = doc.wait_for(5, |t| t.to_lowercase().contains("hello"));
    assert!(
        t.to_lowercase().contains("hello"),
        "misdetected as no text field? target has {t:?}\n{}",
        app.log_text()
    );
    sleep(500); // past restore_clipboard_ms
    assert_eq!(
        clipboard(),
        "walkie-e2e sentinel",
        "the old clipboard should be restored after a paste"
    );
    assert!(
        !app.log_text().contains(NOTICE),
        "no notice expected:\n{}",
        app.log_text()
    );
}

#[test]
fn finder_gets_the_transcript_on_the_clipboard_and_a_notice() {
    let _t = begin("finder_gets_the_transcript_on_the_clipboard_and_a_notice");
    let app = ready_app();
    app.require_keyboard();
    set_clipboard("walkie-e2e sentinel");
    focus_finder();
    Keyboard::into(&["Finder"]).down("Fn").wait(600).up("Fn");
    assert!(
        app.wait_log(NOTICE, 5),
        "no notice logged:\n{}",
        app.log_text()
    );
    let overlay = app.all_text();
    assert!(
        overlay.contains(NOTICE),
        "overlay should show the notice, shows {overlay:?}"
    );
    assert!(
        !overlay.contains('⚠'),
        "a notice isn't an error: {overlay:?}"
    );
    step("checking the clipboard");
    let cb = clipboard();
    assert!(
        cb.to_lowercase().contains("hello"),
        "clipboard should hold the transcript, has {cb:?}"
    );
    sleep(500); // a paste would have restored the old clipboard by now
    assert_eq!(clipboard(), cb, "the transcript must stay on the clipboard");
    assert!(
        !app.log_text().contains("walkie error"),
        "{}",
        app.log_text()
    );
}
