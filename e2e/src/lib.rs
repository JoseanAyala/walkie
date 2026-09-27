//! Harness for the end-to-end tests in `tests/`. Everything real except the
//! two OS edges: the microphone is a WAV fixture and the focused app is a
//! Vec<String>. Keystrokes go through the same `Engine` the macOS tap feeds,
//! signals through the same `Command::from_signal` the app uses, and speech
//! through the real Whisper model.

use hearme_core::audio::Capture;
use hearme_core::config::{models, Config, Hotkeys};
use hearme_core::history::History;
use hearme_core::hotkey::engine::{Bindings, Engine, Signal};
use hearme_core::hotkey::keys::Key;
use hearme_core::inject::Injector;
use hearme_core::pipeline::session::{Command, Deps, Event, Session};
use hearme_core::stt::whisper::WhisperEngine;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../crates/hearme-core/tests/fixtures").join(name)
}

fn load_wav(name: &str) -> Vec<f32> {
    let mut r = hound::WavReader::open(fixture(name)).unwrap();
    r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
}

/// "Microphone" that yields a fixture's audio for any recording.
struct FixtureMic(Vec<f32>);
impl Capture for FixtureMic {
    fn start(&mut self, _on_level: Box<dyn Fn(f32) + Send>) -> anyhow::Result<()> {
        Ok(())
    }
    fn stop(&mut self) -> anyhow::Result<Vec<f32>> {
        Ok(self.0.clone())
    }
}

/// The "focused app": collects what would have been pasted.
struct FocusedApp(Rc<RefCell<Vec<String>>>);
impl Injector for FocusedApp {
    fn inject(&mut self, text: &str) -> anyhow::Result<()> {
        self.0.borrow_mut().push(text.to_string());
        Ok(())
    }
}

pub struct Rig {
    engine: Engine,
    session: Session,
    events: mpsc::Receiver<Event>,
    typed: Rc<RefCell<Vec<String>>>,
    /// Events the keyboard hook would have dropped, as (key name, down).
    pub swallowed: Vec<(String, bool)>,
    pub signals: Vec<Signal>,
    t: u128,
}

pub struct Setup {
    pub hotkeys: Hotkeys,
    pub audio: &'static str,
    pub polish_command: &'static str,
}

impl Default for Setup {
    fn default() -> Self {
        Self { hotkeys: Hotkeys::default(), audio: "en.wav", polish_command: "" }
    }
}

impl Rig {
    pub fn new(setup: Setup) -> Rig {
        let model = models::model_path("base").unwrap();
        assert!(
            model.exists(),
            "e2e needs the base model — run: cargo run -p hearme-core --example fetch_model -- base"
        );
        let (bindings, errors) = Bindings::from_config(&setup.hotkeys);
        assert!(errors.is_empty(), "bad hotkeys in test setup: {errors:?}");
        let mut cfg = Config::default();
        cfg.language = "auto".into();
        cfg.polish.command = setup.polish_command.into();
        let typed = Rc::new(RefCell::new(Vec::new()));
        let (tx, events) = mpsc::channel();
        let deps = Deps {
            capture: Box::new(FixtureMic(load_wav(setup.audio))),
            stt: Box::new(WhisperEngine::load(&model).unwrap()),
            injector: Box::new(FocusedApp(typed.clone())),
            history: Some(History::open_in_memory().unwrap()),
            cfg,
        };
        Rig {
            engine: Engine::new(bindings),
            session: Session::new(deps, tx),
            events,
            typed,
            swallowed: Vec::new(),
            signals: Vec::new(),
            t: 1_000,
        }
    }

    fn key(&mut self, name: &str, down: bool) {
        let k = Key::parse(name).unwrap_or_else(|| panic!("unknown key {name}"));
        // Physical events always carry a side; a bare modifier means the left one.
        let k = match k {
            Key::Shift(hearme_core::hotkey::keys::Side::Any) => Key::from_keycode(56),
            Key::Cmd(hearme_core::hotkey::keys::Side::Any) => Key::from_keycode(55),
            Key::Opt(hearme_core::hotkey::keys::Side::Any) => Key::from_keycode(58),
            Key::Ctrl(hearme_core::hotkey::keys::Side::Any) => Key::from_keycode(59),
            k => k,
        };
        let v = self.engine.on_key(k, down, self.t);
        if v.swallow {
            self.swallowed.push((name.to_string(), down));
        }
        if let Some(s) = v.signal {
            if let Some(cmd) = Command::from_signal(&s) {
                self.session.apply(cmd);
            }
            self.signals.push(s);
        }
    }

    pub fn press(&mut self, name: &str) -> &mut Self {
        self.key(name, true);
        self
    }

    pub fn release(&mut self, name: &str) -> &mut Self {
        self.key(name, false);
        self
    }

    /// Lets time pass (the machine's hold/tap windows are time-based).
    pub fn wait(&mut self, ms: u128) -> &mut Self {
        self.t += ms;
        self.engine.poll(self.t);
        self
    }

    pub fn typed(&self) -> Vec<String> {
        self.typed.borrow().clone()
    }

    pub fn errors(&self) -> Vec<String> {
        self.events
            .try_iter()
            .filter_map(|e| match e {
                Event::Error(m) => Some(m),
                _ => None,
            })
            .collect()
    }
}
