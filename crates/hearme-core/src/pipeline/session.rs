use crate::audio::Capture;
use crate::config::{self, Config};
use crate::history::History;
use crate::hotkey::Mode;
use crate::inject::Injector;
use crate::pipeline::{cleanup, polish};
use crate::stt::{LangHint, SttEngine};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionState {
    Idle,
    Recording,
    Transcribing,
    Polishing,
    Injecting,
}

#[derive(Debug)]
pub enum Command {
    Start(Mode),
    Finish,
    Cancel,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum Event {
    State(SessionState),
    Level(f32),
    Done { text: String, lang: Option<String> },
    Error(String),
}

pub struct Deps {
    pub capture: Box<dyn Capture>,
    pub stt: Box<dyn SttEngine>,
    pub injector: Box<dyn Injector>,
    pub history: Option<History>,
    pub cfg: Config,
}

pub struct Session {
    pub deps: Deps,
    tx: Sender<Event>,
    mode: Option<Mode>,
}

const MIN_UTTERANCE_MS: usize = 300;

impl Session {
    pub fn new(deps: Deps, tx: Sender<Event>) -> Self {
        Self { deps, tx, mode: None }
    }

    /// Blocks processing commands until Shutdown or channel close.
    pub fn run(mut self, rx: Receiver<Command>) {
        while let Ok(cmd) = rx.recv() {
            match cmd {
                Command::Start(mode) => self.start(mode),
                Command::Finish => self.finish(),
                Command::Cancel => self.cancel(),
                Command::Shutdown => break,
            }
        }
    }

    fn emit(&self, ev: Event) {
        let _ = self.tx.send(ev);
    }

    pub fn start(&mut self, mode: Mode) {
        if self.mode.is_some() {
            return; // already recording
        }
        let level_tx = self.tx.clone();
        match self.deps.capture.start(Box::new(move |lvl| {
            let _ = level_tx.send(Event::Level(lvl));
        })) {
            Ok(()) => {
                self.mode = Some(mode);
                self.emit(Event::State(SessionState::Recording));
            }
            Err(e) => {
                self.emit(Event::Error(format!("can't record: {e}")));
                self.emit(Event::State(SessionState::Idle));
            }
        }
    }

    pub fn cancel(&mut self) {
        if self.mode.take().is_none() {
            return;
        }
        let _ = self.deps.capture.stop();
        self.emit(Event::State(SessionState::Idle));
    }

    pub fn finish(&mut self) {
        let Some(mode) = self.mode.take() else { return };
        self.emit(Event::State(SessionState::Transcribing));

        let samples = match self.deps.capture.stop() {
            Ok(s) => s,
            Err(e) => {
                self.emit(Event::Error(format!("capture failed: {e}")));
                self.emit(Event::State(SessionState::Idle));
                return;
            }
        };
        let duration_ms = samples.len() * 1000 / 16_000;
        if duration_ms < MIN_UTTERANCE_MS {
            self.emit(Event::State(SessionState::Idle));
            return;
        }

        let hint = LangHint::from_config(&self.deps.cfg.language);
        let tr = match self.deps.stt.transcribe(&samples, &hint) {
            Ok(t) => t,
            Err(e) => {
                spool(&samples);
                self.emit(Event::Error(format!(
                    "transcription failed: {e} — audio saved to spool"
                )));
                self.emit(Event::State(SessionState::Idle));
                return;
            }
        };

        let cl = &self.deps.cfg.cleanup;
        let fillers: Vec<String> = match tr.lang.as_deref() {
            Some("en") => cl.fillers_en.clone(),
            Some("es") => cl.fillers_es.clone(),
            _ => [cl.fillers_en.clone(), cl.fillers_es.clone()].concat(),
        };
        let cleaned = if cl.enabled { cleanup::clean(&tr.text, &fillers) } else { tr.text.clone() };
        if cleaned.is_empty() {
            self.emit(Event::State(SessionState::Idle));
            return;
        }

        let mut final_text = cleaned.clone();
        let mut polished: Option<String> = None;
        if mode == Mode::Polish {
            self.emit(Event::State(SessionState::Polishing));
            let p = &self.deps.cfg.polish;
            match polish::run_polish(&p.command, &cleaned, Duration::from_secs(p.timeout_secs)) {
                Ok(out) => {
                    final_text = out.clone();
                    polished = Some(out);
                }
                Err(e) => self.emit(Event::Error(format!("polish failed ({e}); using raw transcript"))),
            }
        }

        self.emit(Event::State(SessionState::Injecting));
        if let Err(e) = self.deps.injector.inject(&final_text) {
            self.emit(Event::Error(e.to_string()));
        }

        if self.deps.cfg.history.enabled {
            if let Some(h) = &self.deps.history {
                if let Err(e) = h.insert(
                    &tr.text,
                    &cleaned,
                    polished.as_deref(),
                    tr.lang.as_deref(),
                    duration_ms as i64,
                ) {
                    eprintln!("hearme: history insert failed: {e}");
                }
            }
        }

        self.emit(Event::Done { text: final_text, lang: tr.lang });
        self.emit(Event::State(SessionState::Idle));
    }
}

fn spool(samples: &[f32]) {
    let dir = config::spool_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    if let Ok(mut w) = hound::WavWriter::create(dir.join("last-failed.wav"), spec) {
        for s in samples {
            let _ = w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16);
        }
        let _ = w.finalize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Capture;
    use crate::hotkey::Mode;
    use crate::inject::Injector;
    use crate::stt::{LangHint, SttEngine, Transcript};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::mpsc;

    struct MockCapture {
        samples: Vec<f32>,
        fail_start: bool,
    }
    impl Capture for MockCapture {
        fn start(&mut self, _on_level: Box<dyn Fn(f32) + Send>) -> anyhow::Result<()> {
            if self.fail_start {
                anyhow::bail!("no mic permission")
            }
            Ok(())
        }
        fn stop(&mut self) -> anyhow::Result<Vec<f32>> {
            Ok(self.samples.clone())
        }
    }

    struct MockStt {
        text: String,
        lang: Option<String>,
        fail: bool,
        calls: Rc<RefCell<u32>>,
    }
    impl SttEngine for MockStt {
        fn transcribe(&mut self, _s: &[f32], _l: &LangHint) -> anyhow::Result<Transcript> {
            *self.calls.borrow_mut() += 1;
            if self.fail {
                anyhow::bail!("stt exploded")
            }
            Ok(Transcript { text: self.text.clone(), lang: self.lang.clone() })
        }
    }

    struct MockInjector {
        sink: Rc<RefCell<Vec<String>>>,
        fail: bool,
    }
    impl Injector for MockInjector {
        fn inject(&mut self, text: &str) -> anyhow::Result<()> {
            if self.fail {
                anyhow::bail!("blocked")
            }
            self.sink.borrow_mut().push(text.to_string());
            Ok(())
        }
    }

    struct Rig {
        session: Session,
        rx: mpsc::Receiver<Event>,
        injected: Rc<RefCell<Vec<String>>>,
        stt_calls: Rc<RefCell<u32>>,
    }

    fn rig(text: &str, lang: Option<&str>, opts: (bool, bool, bool)) -> Rig {
        let (stt_fail, inject_fail, capture_fail) = opts;
        let injected = Rc::new(RefCell::new(Vec::new()));
        let stt_calls = Rc::new(RefCell::new(0));
        let (tx, rx) = mpsc::channel();
        let mut cfg = crate::config::Config::default();
        cfg.polish.command = "tr 'a-z' 'A-Z'".into(); // deterministic local "LLM"
        let deps = Deps {
            capture: Box::new(MockCapture { samples: vec![0.05; 16_000], fail_start: capture_fail }),
            stt: Box::new(MockStt {
                text: text.into(),
                lang: lang.map(String::from),
                fail: stt_fail,
                calls: stt_calls.clone(),
            }),
            injector: Box::new(MockInjector { sink: injected.clone(), fail: inject_fail }),
            history: Some(crate::history::History::open_in_memory().unwrap()),
            cfg,
        };
        Rig { session: Session::new(deps, tx), rx, injected, stt_calls }
    }

    fn states(rx: &mpsc::Receiver<Event>) -> Vec<String> {
        rx.try_iter()
            .map(|e| match e {
                Event::State(s) => format!("{s:?}"),
                Event::Done { .. } => "Done".into(),
                Event::Error(_) => "Error".into(),
                Event::Level(_) => "Level".into(),
            })
            .collect()
    }

    #[test]
    fn dictate_happy_path_injects_cleaned_text() {
        let mut r = rig("um, hello world.", Some("en"), (false, false, false));
        r.session.start(Mode::Dictate);
        r.session.finish();
        assert_eq!(r.injected.borrow().as_slice(), ["Hello world."]);
        let evs = states(&r.rx);
        assert_eq!(
            evs,
            ["Recording", "Transcribing", "Injecting", "Done", "Idle"]
        );
    }

    #[test]
    fn polish_mode_pipes_through_command() {
        let mut r = rig("hello", Some("en"), (false, false, false));
        r.session.start(Mode::Polish);
        r.session.finish();
        assert_eq!(r.injected.borrow().as_slice(), ["HELLO"]);
        assert!(states(&r.rx).contains(&"Polishing".to_string()));
    }

    #[test]
    fn polish_failure_falls_back_to_cleaned() {
        let mut r = rig("hello", Some("en"), (false, false, false));
        r.session.deps.cfg.polish.command = "false".into();
        r.session.start(Mode::Polish);
        r.session.finish();
        assert_eq!(r.injected.borrow().as_slice(), ["Hello"]);
        assert!(states(&r.rx).contains(&"Error".to_string()));
    }

    #[test]
    fn stt_failure_spools_and_reports() {
        // Redirect the spool to a tempdir so the test never touches ~/.cache.
        // Safe alongside parallel tests: the other path tests assert suffixes only.
        let spool_root = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", spool_root.path());
        let mut r = rig("x", None, (true, false, false));
        r.session.start(Mode::Dictate);
        r.session.finish();
        assert!(r.injected.borrow().is_empty());
        let evs = states(&r.rx);
        assert!(evs.contains(&"Error".to_string()));
        assert_eq!(evs.last().unwrap(), "Idle");
        assert!(spool_root.path().join("hearme/spool/last-failed.wav").exists());
        std::env::remove_var("XDG_CACHE_HOME");
    }

    #[test]
    fn injection_failure_reports_but_still_records_history() {
        let mut r = rig("hello", Some("en"), (false, true, false));
        r.session.start(Mode::Dictate);
        r.session.finish();
        assert!(states(&r.rx).contains(&"Error".to_string()));
    }

    #[test]
    fn capture_start_failure_reports_and_goes_idle() {
        let mut r = rig("x", None, (false, false, true));
        r.session.start(Mode::Dictate);
        let evs = states(&r.rx);
        assert!(evs.contains(&"Error".to_string()));
        assert_eq!(evs.last().unwrap(), "Idle");
        assert_eq!(*r.stt_calls.borrow(), 0);
    }

    #[test]
    fn too_short_utterance_is_dropped() {
        let mut r = rig("x", None, (false, false, false));
        r.session.deps.capture = Box::new(MockCapture { samples: vec![0.0; 1000], fail_start: false });
        r.session.start(Mode::Dictate);
        r.session.finish();
        assert_eq!(*r.stt_calls.borrow(), 0);
        assert!(r.injected.borrow().is_empty());
    }

    #[test]
    fn cancel_discards_without_transcribing() {
        let mut r = rig("x", None, (false, false, false));
        r.session.start(Mode::Dictate);
        r.session.cancel();
        assert_eq!(*r.stt_calls.borrow(), 0);
        assert_eq!(states(&r.rx), ["Recording", "Idle"]);
    }

    #[test]
    fn finish_without_start_is_a_noop() {
        let mut r = rig("x", None, (false, false, false));
        r.session.finish();
        assert!(states(&r.rx).is_empty());
    }
}
