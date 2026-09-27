//! Dictating with nothing editable focused: the transcript is copied, not
//! lost, and the user gets a notice (not an error). Run: cargo test -p hearme-e2e
//! Needs: cargo run -p hearme-core --example fetch_model -- base

use hearme_core::pipeline::session::NO_FIELD_NOTICE;
use hearme_e2e::{Rig, Setup};

#[test]
fn no_text_field_copies_the_transcript_and_says_so() {
    let mut r = Rig::new(Setup { no_text_field: true, ..Default::default() });
    r.press("Fn").wait(3000).release("Fn");
    let (errors, notices) = r.messages();
    assert!(r.typed().is_empty(), "nothing should be typed: {:?}", r.typed());
    let copied = r.copied();
    assert_eq!(copied.len(), 1, "{copied:?} (errors: {errors:?})");
    assert!(copied[0].to_lowercase().contains("hello"), "{copied:?}");
    assert_eq!(notices, [NO_FIELD_NOTICE]);
    assert!(errors.is_empty(), "a missing text field isn't an error: {errors:?}");
    assert_eq!(r.history(), copied, "history should still record the transcript");
}

#[test]
fn a_text_field_gets_typed_into_without_a_notice() {
    let mut r = Rig::new(Setup::default());
    r.press("Fn").wait(3000).release("Fn");
    let (_, notices) = r.messages();
    assert_eq!(r.typed().len(), 1, "{:?}", r.typed());
    assert!(r.copied().is_empty());
    assert!(notices.is_empty(), "{notices:?}");
}
