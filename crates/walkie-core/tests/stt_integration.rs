//! Real-model STT tests. Gated: cargo test -p walkie-core --features stt-tests
//! Requires: cargo run -p walkie-core --example fetch_model -- base
#![cfg(feature = "stt-tests")]

use walkie_core::config::models;
use walkie_core::stt::{whisper::WhisperEngine, LangHint, SttEngine};

fn load_wav(path: &str) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).unwrap();
    r.samples::<i16>()
        .map(|s| s.unwrap() as f32 / 32768.0)
        .collect()
}

fn engine() -> WhisperEngine {
    let path = models::model_path("base").unwrap();
    assert!(
        path.exists(),
        "test model missing — run: cargo run -p walkie-core --example fetch_model -- base"
    );
    WhisperEngine::load(&path).unwrap()
}

#[test]
fn transcribes_english() {
    let mut e = engine();
    let t = e
        .transcribe(&load_wav("tests/fixtures/en.wav"), &LangHint::Auto)
        .unwrap();
    assert!(t.text.to_lowercase().contains("hello"), "got: {}", t.text);
    assert_eq!(t.lang.as_deref(), Some("en"));
}

#[test]
fn transcribes_spanish() {
    let mut e = engine();
    let t = e
        .transcribe(&load_wav("tests/fixtures/es.wav"), &LangHint::Auto)
        .unwrap();
    assert!(t.text.to_lowercase().contains("hola"), "got: {}", t.text);
    assert_eq!(t.lang.as_deref(), Some("es"));
}

#[test]
fn pinned_language_is_respected() {
    let mut e = engine();
    let t = e
        .transcribe(
            &load_wav("tests/fixtures/en.wav"),
            &LangHint::Pinned("en".into()),
        )
        .unwrap();
    assert!(t.text.to_lowercase().contains("hello"), "got: {}", t.text);
}
