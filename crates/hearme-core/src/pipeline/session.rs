use crate::audio::duck::Ducker;
use crate::audio::Capture;
use crate::config::{self, Config};
use crate::history::History;
use crate::hotkey::engine::Signal;
use crate::hotkey::Mode;
use crate::inject::{Injected, Injector};
use crate::pipeline::{cleanup, polish};
use crate::stt::{LangHint, SttEngine};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
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
    /// Switch the running recording's mode (e.g. Shift added mid-hold).
    SetMode(Mode),
    Finish,
    Cancel,
    /// Insert this text again (paste-last). Not written to history.
    Reinject(String),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum Event {
    State(SessionState),
    Level(f32),
    Done { text: String, lang: Option<String> },
    Error(String),
    /// Worth telling the user, but nothing went wrong (e.g. copied, not pasted).
    Notice(String),
}

pub const NO_FIELD_NOTICE: &str = "No text field — copied to clipboard (⌘V to paste)";

impl Command {
    /// The session command a hotkey signal maps to. Recorder signals have
    /// none, and neither does PasteLast: the caller supplies the text.
    pub fn from_signal(s: &Signal) -> Option<Command> {
        Some(match s {
            Signal::Start(m) => Command::Start(*m),
            Signal::SetMode(m) => Command::SetMode(*m),
            Signal::Finish => Command::Finish,
            Signal::Cancel => Command::Cancel,
            Signal::Recorded(_) | Signal::RecordCancelled | Signal::PasteLast => return None,
        })
    }
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
    ducker: Option<Arc<Ducker>>,
}

const MIN_UTTERANCE_MS: usize = 300;

impl Session {
    pub fn new(deps: Deps, tx: Sender<Event>) -> Self {
        Self { deps, tx, mode: None, ducker: None }
    }

    /// Lowers other audio while recording (see `audio::duck`).
    pub fn with_ducker(mut self, d: Arc<Ducker>) -> Self {
        self.ducker = Some(d);
        self
    }

    fn unduck(&self) {
        if let Some(d) = &self.ducker {
            d.restore();
        }
    }

    /// Blocks processing commands until Shutdown or channel close.
    pub fn run(mut self, rx: Receiver<Command>) {
        while let Ok(cmd) = rx.recv() {
            match cmd {
                Command::Shutdown => break,
                cmd => self.apply(cmd),
            }
        }
    }

    pub fn apply(&mut self, cmd: Command) {
        match cmd {
            Command::Start(mode) => self.start(mode),
            Command::SetMode(mode) => self.set_mode(mode),
            Command::Finish => self.finish(),
            Command::Cancel => self.cancel(),
            Command::Reinject(text) => self.reinject(&text),
            Command::Shutdown => {}
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
                if let Some(d) = &self.ducker {
                    d.duck();
                }
                self.emit(Event::State(SessionState::Recording));
            }
            Err(e) => {
                self.emit(Event::Error(format!("can't record: {e}")));
                self.emit(Event::State(SessionState::Idle));
            }
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode.is_some() {
            self.mode = Some(mode);
        }
    }

    pub fn cancel(&mut self) {
        if self.mode.take().is_none() {
            return;
        }
        let _ = self.deps.capture.stop();
        self.unduck();
        self.emit(Event::State(SessionState::Idle));
    }

    pub fn reinject(&mut self, text: &str) {
        if self.mode.is_some() {
            self.emit(Event::Error("finish the current dictation before pasting the last one".into()));
            return;
        }
        self.emit(Event::State(SessionState::Injecting));
        if let Err(e) = self.deps.injector.inject(text) {
            self.emit(Event::Error(e.to_string()));
        }
        self.emit(Event::State(SessionState::Idle));
    }

    pub fn finish(&mut self) {
        let Some(mode) = self.mode.take() else { return };
        self.emit(Event::State(SessionState::Transcribing));

        let stopped = self.deps.capture.stop();
        self.unduck();
        let samples = match stopped {
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
        match self.deps.injector.inject(&final_text) {
            Ok(Injected::Typed) => {}
            Ok(Injected::CopiedNoField) => self.emit(Event::Notice(NO_FIELD_NOTICE.into())),
            Err(e) => self.emit(Event::Error(e.to_string())),
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

/// Shutdown, or a panic mid-session: never leave the volume lowered.
impl Drop for Session {
    fn drop(&mut self) {
        self.unduck();
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
    use crate::audio::duck::MemVolume;
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
        fn inject(&mut self, text: &str) -> anyhow::Result<Injected> {
            if self.fail {
                anyhow::bail!("blocked")
            }
            self.sink.borrow_mut().push(text.to_string());
            Ok(Injected::Typed)
        }
    }

    /// Focus on the desktop: the text goes to the clipboard, not an app.
    struct NoFieldInjector(Rc<RefCell<Vec<String>>>);
    impl Injector for NoFieldInjector {
        fn inject(&mut self, text: &str) -> anyhow::Result<Injected> {
            self.0.borrow_mut().push(text.to_string());
            Ok(Injected::CopiedNoField)
        }
    }

    struct Rig {
        session: Session,
        rx: mpsc::Receiver<Event>,
        injected: Rc<RefCell<Vec<String>>>,
        stt_calls: Rc<RefCell<u32>>,
        volume: MemVolume,
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
        let volume = MemVolume::new(0.8);
        let ducker = Arc::new(Ducker::new(Box::new(volume.clone()), 25));
        Rig { session: Session::new(deps, tx).with_ducker(ducker), rx, injected, stt_calls, volume }
    }

    fn states(rx: &mpsc::Receiver<Event>) -> Vec<String> {
        rx.try_iter()
            .map(|e| match e {
                Event::State(s) => format!("{s:?}"),
                Event::Done { .. } => "Done".into(),
                Event::Error(_) => "Error".into(),
                Event::Level(_) => "Level".into(),
                Event::Notice(_) => "Notice".into(),
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
        assert_eq!(r.volume.volume(), 0.8, "error path must restore the volume");
        std::env::remove_var("XDG_CACHE_HOME");
    }

    #[test]
    fn injection_failure_reports_but_still_records_history() {
        let mut r = rig("hello", Some("en"), (false, true, false));
        r.session.start(Mode::Dictate);
        r.session.finish();
        assert!(states(&r.rx).contains(&"Error".to_string()));
        let rows = r.session.deps.history.as_ref().unwrap().recent(10).unwrap();
        assert_eq!(rows.len(), 1, "transcript should be recorded even though injection failed");
        assert_eq!(rows[0].cleaned, "Hello");
    }

    #[test]
    fn no_text_field_is_a_notice_not_an_error_and_still_records_history() {
        let mut r = rig("hello", Some("en"), (false, false, false));
        let copied = Rc::new(RefCell::new(Vec::new()));
        r.session.deps.injector = Box::new(NoFieldInjector(copied.clone()));
        r.session.start(Mode::Dictate);
        r.session.finish();
        assert_eq!(copied.borrow().as_slice(), ["Hello"]);
        let evs: Vec<Event> = r.rx.try_iter().collect();
        let notices: Vec<&str> =
            evs.iter().filter_map(|e| if let Event::Notice(m) = e { Some(m.as_str()) } else { None }).collect();
        assert_eq!(notices, [NO_FIELD_NOTICE]);
        assert!(!evs.iter().any(|e| matches!(e, Event::Error(_))), "{evs:?}");
        assert!(matches!(evs.last(), Some(Event::State(SessionState::Idle))));
        let rows = r.session.deps.history.as_ref().unwrap().recent(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cleaned, "Hello");
    }

    #[test]
    fn typed_injection_emits_no_notice() {
        let mut r = rig("hello", Some("en"), (false, false, false));
        r.session.start(Mode::Dictate);
        r.session.finish();
        assert!(!states(&r.rx).contains(&"Notice".to_string()));
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
    fn cleaned_to_empty_is_dropped_silently() {
        // "um" is entirely stripped by the default English filler list, so
        // the post-cleanup transcript is empty. This should drop silently
        // (no injection, no Done, no Error) just like a too-short utterance
        // — and it's the same shape of outcome macOS produces when mic
        // permission is silently denied (silence in, empty-after-cleanup out).
        let mut r = rig("um", Some("en"), (false, false, false));
        r.session.start(Mode::Dictate);
        r.session.finish();
        assert!(r.injected.borrow().is_empty());
        assert_eq!(states(&r.rx), ["Recording", "Transcribing", "Idle"]);
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
    fn set_mode_mid_recording_switches_to_polish() {
        let mut r = rig("hello there", Some("en"), (false, false, false));
        r.session.start(Mode::Dictate);
        r.session.set_mode(Mode::Polish);
        r.session.finish();
        assert_eq!(*r.injected.borrow(), vec!["HELLO THERE".to_string()]);
    }

    #[test]
    fn set_mode_while_idle_does_not_start() {
        let mut r = rig("hello", Some("en"), (false, false, false));
        r.session.set_mode(Mode::Polish);
        r.session.finish();
        assert!(r.injected.borrow().is_empty());
    }

    #[test]
    fn finish_without_start_is_a_noop() {
        let mut r = rig("x", None, (false, false, false));
        r.session.finish();
        assert!(states(&r.rx).is_empty());
    }

    #[test]
    fn volume_is_lowered_while_recording_and_restored_after() {
        let mut r = rig("hello", Some("en"), (false, false, false));
        r.session.start(Mode::Dictate);
        assert!((r.volume.volume() - 0.2).abs() < 1e-6, "{}", r.volume.volume());
        r.session.finish();
        assert_eq!(r.volume.volume(), 0.8);
    }

    #[test]
    fn cancel_restores_the_volume() {
        let mut r = rig("x", None, (false, false, false));
        r.session.start(Mode::Dictate);
        r.session.cancel();
        assert_eq!(r.volume.volume(), 0.8);
    }

    #[test]
    fn user_volume_change_mid_recording_survives_finish() {
        let mut r = rig("hello", Some("en"), (false, false, false));
        r.session.start(Mode::Dictate);
        r.volume.user_set(0.5);
        r.session.finish();
        assert_eq!(r.volume.volume(), 0.5);
    }

    #[test]
    fn failed_start_never_touches_the_volume() {
        let mut r = rig("x", None, (false, false, true));
        r.session.start(Mode::Dictate);
        assert_eq!(r.volume.volume(), 0.8);
    }

    #[test]
    fn dropping_a_recording_session_restores_the_volume() {
        let mut r = rig("x", None, (false, false, false));
        r.session.start(Mode::Dictate);
        let volume = r.volume.clone();
        drop(r);
        assert_eq!(volume.volume(), 0.8);
    }

    #[test]
    fn reinject_injects_again_without_touching_history() {
        let mut r = rig("hello", Some("en"), (false, false, false));
        r.session.start(Mode::Dictate);
        r.session.finish();
        states(&r.rx);
        r.session.apply(Command::Reinject("Hello".into()));
        assert_eq!(r.injected.borrow().as_slice(), ["Hello", "Hello"]);
        assert_eq!(states(&r.rx), ["Injecting", "Idle"], "no Done: it isn't a new transcript");
        assert_eq!(r.session.deps.history.as_ref().unwrap().recent(10).unwrap().len(), 1);
    }

    #[test]
    fn reinject_while_recording_is_refused() {
        let mut r = rig("hello", Some("en"), (false, false, false));
        r.session.start(Mode::Dictate);
        r.session.reinject("old");
        assert!(r.injected.borrow().is_empty());
        assert_eq!(states(&r.rx), ["Recording", "Error"], "recording keeps going");
        r.session.finish();
        assert_eq!(r.injected.borrow().as_slice(), ["Hello"]);
    }

    #[test]
    fn reinject_failure_reports_and_goes_idle() {
        let mut r = rig("x", None, (false, true, false));
        r.session.reinject("old");
        assert_eq!(states(&r.rx), ["Injecting", "Error", "Idle"]);
    }
}
