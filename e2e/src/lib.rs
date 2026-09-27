//! Harness for the end-to-end tests in `tests/`. Everything real except the
//! two OS edges: the microphone is a WAV fixture and the focused app is a
//! Vec<String>. Keystrokes go through the same `Engine` the macOS tap feeds,
//! signals through the same `Command::from_signal` the app uses, and speech
//! through the real Whisper model.

#[cfg(all(target_os = "macos", feature = "os-tests"))]
pub mod os;

use hearme_core::audio::duck::{Ducker, MemVolume};
use hearme_core::audio::FileCapture;
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
use std::sync::{mpsc, Arc};

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../crates/hearme-core/tests/fixtures").join(name)
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
    /// The system output volume: starts at 0.8, ducked to 30% while recording.
    pub volume: MemVolume,
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
            capture: Box::new(FileCapture::open(&fixture(setup.audio)).unwrap()),
            stt: Box::new(WhisperEngine::load(&model).unwrap()),
            injector: Box::new(FocusedApp(typed.clone())),
            history: Some(History::open_in_memory().unwrap()),
            cfg,
        };
        let volume = MemVolume::new(0.8);
        let ducker = Arc::new(Ducker::new(Box::new(volume.clone()), 30));
        Rig {
            engine: Engine::new(bindings),
            session: Session::new(deps, tx).with_ducker(ducker),
            events,
            typed,
            swallowed: Vec::new(),
            signals: Vec::new(),
            volume,
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
            if s == Signal::PasteLast {
                // What the app falls back to after a restart: history's newest row.
                let last = self.session.deps.history.as_ref().and_then(|h| h.last_text().unwrap());
                if let Some(text) = last {
                    self.session.apply(Command::Reinject(text));
                }
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
