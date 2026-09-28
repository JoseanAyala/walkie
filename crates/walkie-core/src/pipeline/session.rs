use crate::audio::duck::Ducker;
use crate::audio::Capture;
use crate::config::{self, Config};
use crate::history::History;
use crate::hotkey::engine::Signal;
use crate::inject::{Injected, Injector};
use crate::pipeline::polish;
use crate::stt::{LangHint, SttEngine};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

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
    Start,
    Finish,
    Cancel,
    /// Polish the focused field's text (its selection, or all of it) in place.
    Polish,
    /// Insert this text again (paste-last). Not written to history.
    Reinject(String),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum Event {
    State(SessionState),
    Level(f32),
    Done {
        text: String,
        lang: Option<String>,
    },
    Error(String),
    /// Worth telling the user, but nothing went wrong (e.g. copied, not pasted).
    Notice(String),
}

pub const NO_FIELD_NOTICE: &str = "No text field — copied to clipboard (⌘V to paste)";
pub const NOTHING_TO_POLISH: &str = "Nothing to polish — click into a text field with some text";

impl Command {
    /// The session command a hotkey signal maps to. Recorder signals have
    /// none, and neither does PasteLast: the caller supplies the text.
    pub fn from_signal(s: &Signal) -> Option<Command> {
        Some(match s {
            Signal::Start => Command::Start,
            Signal::Finish => Command::Finish,
            Signal::Cancel => Command::Cancel,
            Signal::Polish => Command::Polish,
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
    recording: bool,
    ducker: Option<Arc<Ducker>>,
    /// walkie-ai, for the Apple polish provider.
    ai_helper: PathBuf,
}

const MIN_UTTERANCE_MS: usize = 300;

impl Session {
    pub fn new(deps: Deps, tx: Sender<Event>) -> Self {
        Self {
            deps,
            tx,
            recording: false,
            ducker: None,
            ai_helper: polish::apple_helper(),
        }
    }

    /// Uses this walkie-ai instead of the one next to the binary (tests).
    pub fn with_ai_helper(mut self, path: PathBuf) -> Self {
        self.ai_helper = path;
        self
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
            Command::Start => self.start(),
            Command::Finish => self.finish(),
            Command::Cancel => self.cancel(),
            Command::Polish => self.polish(),
            Command::Reinject(text) => self.reinject(&text),
            Command::Shutdown => {}
        }
    }

    fn emit(&self, ev: Event) {
        let _ = self.tx.send(ev);
    }

    pub fn start(&mut self) {
        if self.recording {
            return;
        }
        let level_tx = self.tx.clone();
        match self.deps.capture.start(Box::new(move |lvl| {
            let _ = level_tx.send(Event::Level(lvl));
        })) {
            Ok(()) => {
                self.recording = true;
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

    pub fn cancel(&mut self) {
        if !std::mem::take(&mut self.recording) {
            return;
        }
        let _ = self.deps.capture.stop();
        self.unduck();
        self.emit(Event::State(SessionState::Idle));
    }

    pub fn reinject(&mut self, text: &str) {
        if self.recording {
            self.emit(Event::Error(
                "finish the current dictation before pasting the last one".into(),
            ));
            return;
        }
        self.emit(Event::State(SessionState::Injecting));
        self.inject(text);
        self.emit(Event::State(SessionState::Idle));
    }

    /// Injects and reports how it went: a notice when there was no text
    /// field to type into (the text is on the clipboard), an error on failure.
    fn inject(&mut self, text: &str) {
        match self.deps.injector.inject(text) {
            Ok(Injected::Typed) => {}
            Ok(Injected::CopiedNoField) => self.emit(Event::Notice(NO_FIELD_NOTICE.into())),
            Err(e) => self.emit(Event::Error(e.to_string())),
        }
    }

    /// Grabs the focused field's text, polishes it and puts the result in
    /// its place. On failure the field is left as it was.
    pub fn polish(&mut self) {
        if self.recording {
            self.emit(Event::Error(
                "finish the current dictation before polishing".into(),
            ));
            return;
        }
        self.emit(Event::State(SessionState::Polishing));
        match self.deps.injector.grab() {
            Ok(Some(text)) if !text.trim().is_empty() => self.polish_in_place(&text),
            Ok(_) => self.emit(Event::Notice(NOTHING_TO_POLISH.into())),
            Err(e) => self.emit(Event::Error(format!("couldn't read the text field: {e}"))),
        }
        self.emit(Event::State(SessionState::Idle));
    }

    fn polish_in_place(&mut self, text: &str) {
        let out = match polish::polish(&self.deps.cfg.polish, &self.ai_helper, text) {
            Ok(out) => out,
            Err(e) => {
                self.emit(Event::Error(format!(
                    "polish failed ({e}); text left as is"
                )));
                return;
            }
        };
        self.emit(Event::State(SessionState::Injecting));
        self.inject(&out);
        // Kept like a dictation: the original stays recoverable from History.
        self.remember(text, text, Some(&out), None, 0);
        self.emit(Event::Done {
            text: out,
            lang: None,
        });
    }

    fn remember(
        &self,
        raw: &str,
        cleaned: &str,
        polished: Option<&str>,
        lang: Option<&str>,
        duration_ms: i64,
    ) {
        if !self.deps.cfg.history.enabled {
            return;
        }
        if let Some(h) = &self.deps.history {
            if let Err(e) = h.insert(raw, cleaned, polished, lang, duration_ms) {
                eprintln!("walkie: history insert failed: {e}");
            }
        }
    }

    pub fn finish(&mut self) {
        if !std::mem::take(&mut self.recording) {
            return;
        }
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

        let text = tr.text.trim().to_string();
        if text.is_empty() {
            self.emit(Event::State(SessionState::Idle));
            return;
        }

        self.emit(Event::State(SessionState::Injecting));
        self.inject(&text);
        self.remember(
            &tr.text,
            &text,
            None,
            tr.lang.as_deref(),
            duration_ms as i64,
        );
        self.emit(Event::Done {
            text,
            lang: tr.lang,
        });
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
    use crate::config::PolishProvider;
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
            Ok(Transcript {
                text: self.text.clone(),
                lang: self.lang.clone(),
            })
        }
    }

    struct MockInjector {
        sink: Rc<RefCell<Vec<String>>>,
        fail: bool,
        /// What the focused field holds, for polish to grab.
        field: Option<String>,
    }
    impl Injector for MockInjector {
        fn inject(&mut self, text: &str) -> anyhow::Result<Injected> {
            if self.fail {
                anyhow::bail!("blocked")
            }
            self.sink.borrow_mut().push(text.to_string());
            Ok(Injected::Typed)
        }
        fn grab(&mut self) -> anyhow::Result<Option<String>> {
            if self.fail {
                anyhow::bail!("blocked")
            }
            Ok(self.field.clone())
        }
    }

    /// Focus on the desktop: the text goes to the clipboard, not an app.
    struct NoFieldInjector(Rc<RefCell<Vec<String>>>);
    impl Injector for NoFieldInjector {
        fn inject(&mut self, text: &str) -> anyhow::Result<Injected> {
            self.0.borrow_mut().push(text.to_string());
            Ok(Injected::CopiedNoField)
        }
        fn grab(&mut self) -> anyhow::Result<Option<String>> {
            Ok(None)
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
        cfg.polish.provider = crate::config::PolishProvider::Command;
        cfg.polish.command = "tr 'a-z' 'A-Z'".into(); // deterministic local "LLM"
        let deps = Deps {
            capture: Box::new(MockCapture {
                samples: vec![0.05; 16_000],
                fail_start: capture_fail,
            }),
            stt: Box::new(MockStt {
                text: text.into(),
                lang: lang.map(String::from),
                fail: stt_fail,
                calls: stt_calls.clone(),
            }),
            injector: Box::new(MockInjector {
                sink: injected.clone(),
                fail: inject_fail,
                field: Some("um so hello there".into()),
            }),
            history: Some(crate::history::History::open_in_memory().unwrap()),
            cfg,
        };
        let volume = MemVolume::new(0.8);
        let ducker = Arc::new(Ducker::new(Box::new(volume.clone()), 25));
        Rig {
            session: Session::new(deps, tx).with_ducker(ducker),
            rx,
            injected,
            stt_calls,
            volume,
        }
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
    fn dictate_happy_path_injects_the_transcript() {
        let mut r = rig(" um, hello world.", Some("en"), (false, false, false));
        r.session.start();
        r.session.finish();
        assert_eq!(r.injected.borrow().as_slice(), ["um, hello world."]);
        let evs = states(&r.rx);
        assert_eq!(
            evs,
            ["Recording", "Transcribing", "Injecting", "Done", "Idle"]
        );
    }

    #[test]
    fn polish_replaces_the_fields_text() {
        let mut r = rig("x", None, (false, false, false));
        r.session.polish();
        assert_eq!(r.injected.borrow().as_slice(), ["UM SO HELLO THERE"]);
        assert_eq!(states(&r.rx), ["Polishing", "Injecting", "Done", "Idle"]);
    }

    #[test]
    fn polish_never_records_or_transcribes() {
        let mut r = rig("x", None, (false, false, false));
        r.session.polish();
        assert_eq!(*r.stt_calls.borrow(), 0);
        assert_eq!(r.volume.volume(), 0.8);
    }

    #[test]
    fn polish_keeps_the_original_in_history() {
        let mut r = rig("x", None, (false, false, false));
        r.session.polish();
        let rows = r.session.deps.history.as_ref().unwrap().recent(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cleaned, "um so hello there");
        assert_eq!(rows[0].polished.as_deref(), Some("UM SO HELLO THERE"));
    }

    #[test]
    fn apple_polish_goes_through_the_helper() {
        let dir = tempfile::tempdir().unwrap();
        let helper = dir.path().join("walkie-ai");
        std::fs::write(&helper, "#!/bin/sh\nprintf 'apple: '; cat\n").unwrap();
        std::fs::set_permissions(&helper, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let mut r = rig("x", None, (false, false, false));
        r.session.deps.cfg.polish.provider = PolishProvider::Apple;
        r.session.ai_helper = helper;
        r.session.polish();
        assert_eq!(r.injected.borrow().as_slice(), ["apple: um so hello there"]);
    }

    #[test]
    fn apple_polish_writes_in_the_configured_tone() {
        let dir = tempfile::tempdir().unwrap();
        let helper = dir.path().join("walkie-ai");
        // says whether its prompt asked for the tone
        let script = "#!/bin/sh\ncat >/dev/null\ncase $2 in *lowercase*) echo 'Hi, ok.' ;; *) echo 'Wrong.' ;; esac\n";
        std::fs::write(&helper, script).unwrap();
        std::fs::set_permissions(&helper, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let mut r = rig("x", None, (false, false, false));
        r.session.deps.cfg.polish.provider = PolishProvider::Apple;
        r.session.deps.cfg.polish.tone = crate::config::Tone::VeryCasual;
        r.session.ai_helper = helper;
        r.session.polish();
        assert_eq!(r.injected.borrow().as_slice(), ["hi, ok"]);
    }

    #[test]
    fn polish_failure_leaves_the_field_alone() {
        let mut r = rig("x", None, (false, false, false));
        r.session.deps.cfg.polish.command = "false".into();
        r.session.polish();
        assert!(r.injected.borrow().is_empty());
        assert_eq!(states(&r.rx), ["Polishing", "Error", "Idle"]);
    }

    #[test]
    fn polish_with_nothing_to_grab_is_a_notice() {
        for field in [None, Some("  \n")] {
            let mut r = rig("x", None, (false, false, false));
            r.session.deps.injector = Box::new(MockInjector {
                sink: r.injected.clone(),
                fail: false,
                field: field.map(String::from),
            });
            r.session.polish();
            assert!(r.injected.borrow().is_empty());
            let evs: Vec<Event> = r.rx.try_iter().collect();
            assert!(
                evs.iter()
                    .any(|e| matches!(e, Event::Notice(m) if m == NOTHING_TO_POLISH)),
                "{evs:?}"
            );
            assert!(matches!(evs.last(), Some(Event::State(SessionState::Idle))));
        }
    }

    #[test]
    fn polish_when_the_field_cant_be_read_reports_it() {
        let mut r = rig("x", None, (false, true, false));
        r.session.polish();
        assert_eq!(states(&r.rx), ["Polishing", "Error", "Idle"]);
    }

    #[test]
    fn polish_while_recording_is_refused() {
        let mut r = rig("Hello", Some("en"), (false, false, false));
        r.session.start();
        r.session.polish();
        assert_eq!(
            states(&r.rx),
            ["Recording", "Error"],
            "recording keeps going"
        );
        r.session.finish();
        assert_eq!(r.injected.borrow().as_slice(), ["Hello"]);
    }

    #[test]
    fn stt_failure_spools_and_reports() {
        // Redirect the spool to a tempdir so the test never touches ~/.cache.
        // Safe alongside parallel tests: the other path tests assert suffixes only.
        let spool_root = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", spool_root.path());
        let mut r = rig("x", None, (true, false, false));
        r.session.start();
        r.session.finish();
        assert!(r.injected.borrow().is_empty());
        let evs = states(&r.rx);
        assert!(evs.contains(&"Error".to_string()));
        assert_eq!(evs.last().unwrap(), "Idle");
        assert!(spool_root
            .path()
            .join("walkie/spool/last-failed.wav")
            .exists());
        assert_eq!(r.volume.volume(), 0.8, "error path must restore the volume");
        std::env::remove_var("XDG_CACHE_HOME");
    }

    #[test]
    fn injection_failure_reports_but_still_records_history() {
        let mut r = rig("Hello", Some("en"), (false, true, false));
        r.session.start();
        r.session.finish();
        assert!(states(&r.rx).contains(&"Error".to_string()));
        let rows = r.session.deps.history.as_ref().unwrap().recent(10).unwrap();
        assert_eq!(
            rows.len(),
            1,
            "transcript should be recorded even though injection failed"
        );
        assert_eq!(rows[0].cleaned, "Hello");
    }

    #[test]
    fn no_text_field_is_a_notice_not_an_error_and_still_records_history() {
        let mut r = rig("Hello", Some("en"), (false, false, false));
        let copied = Rc::new(RefCell::new(Vec::new()));
        r.session.deps.injector = Box::new(NoFieldInjector(copied.clone()));
        r.session.start();
        r.session.finish();
        assert_eq!(copied.borrow().as_slice(), ["Hello"]);
        let evs: Vec<Event> = r.rx.try_iter().collect();
        let notices: Vec<&str> = evs
            .iter()
            .filter_map(|e| {
                if let Event::Notice(m) = e {
                    Some(m.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(notices, [NO_FIELD_NOTICE]);
        assert!(!evs.iter().any(|e| matches!(e, Event::Error(_))), "{evs:?}");
        assert!(matches!(evs.last(), Some(Event::State(SessionState::Idle))));
        let rows = r.session.deps.history.as_ref().unwrap().recent(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cleaned, "Hello");
    }

    #[test]
    fn reinject_with_no_text_field_is_a_notice() {
        let mut r = rig("x", None, (false, false, false));
        let copied = Rc::new(RefCell::new(Vec::new()));
        r.session.deps.injector = Box::new(NoFieldInjector(copied.clone()));
        r.session.reinject("old");
        assert_eq!(copied.borrow().as_slice(), ["old"]);
        assert_eq!(states(&r.rx), ["Injecting", "Notice", "Idle"]);
    }

    #[test]
    fn typed_injection_emits_no_notice() {
        let mut r = rig("Hello", Some("en"), (false, false, false));
        r.session.start();
        r.session.finish();
        assert!(!states(&r.rx).contains(&"Notice".to_string()));
    }

    #[test]
    fn capture_start_failure_reports_and_goes_idle() {
        let mut r = rig("x", None, (false, false, true));
        r.session.start();
        let evs = states(&r.rx);
        assert!(evs.contains(&"Error".to_string()));
        assert_eq!(evs.last().unwrap(), "Idle");
        assert_eq!(*r.stt_calls.borrow(), 0);
    }

    #[test]
    fn too_short_utterance_is_dropped() {
        let mut r = rig("x", None, (false, false, false));
        r.session.deps.capture = Box::new(MockCapture {
            samples: vec![0.0; 1000],
            fail_start: false,
        });
        r.session.start();
        r.session.finish();
        assert_eq!(*r.stt_calls.borrow(), 0);
        assert!(r.injected.borrow().is_empty());
    }

    #[test]
    fn an_empty_transcript_is_dropped_silently() {
        // No injection, no Done, no Error, just like a too-short utterance —
        // the same outcome as mic permission silently denied (silence in,
        // nothing out).
        let mut r = rig("  ", Some("en"), (false, false, false));
        r.session.start();
        r.session.finish();
        assert!(r.injected.borrow().is_empty());
        assert_eq!(states(&r.rx), ["Recording", "Transcribing", "Idle"]);
    }

    #[test]
    fn cancel_discards_without_transcribing() {
        let mut r = rig("x", None, (false, false, false));
        r.session.start();
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

    #[test]
    fn volume_is_lowered_while_recording_and_restored_after() {
        let mut r = rig("Hello", Some("en"), (false, false, false));
        r.session.start();
        assert!(
            (r.volume.volume() - 0.2).abs() < 1e-6,
            "{}",
            r.volume.volume()
        );
        r.session.finish();
        assert_eq!(r.volume.volume(), 0.8);
    }

    #[test]
    fn cancel_restores_the_volume() {
        let mut r = rig("x", None, (false, false, false));
        r.session.start();
        r.session.cancel();
        assert_eq!(r.volume.volume(), 0.8);
    }

    #[test]
    fn user_volume_change_mid_recording_survives_finish() {
        let mut r = rig("Hello", Some("en"), (false, false, false));
        r.session.start();
        r.volume.user_set(0.5);
        r.session.finish();
        assert_eq!(r.volume.volume(), 0.5);
    }

    #[test]
    fn failed_start_never_touches_the_volume() {
        let mut r = rig("x", None, (false, false, true));
        r.session.start();
        assert_eq!(r.volume.volume(), 0.8);
    }

    #[test]
    fn dropping_a_recording_session_restores_the_volume() {
        let mut r = rig("x", None, (false, false, false));
        r.session.start();
        let volume = r.volume.clone();
        drop(r);
        assert_eq!(volume.volume(), 0.8);
    }

    #[test]
    fn reinject_injects_again_without_touching_history() {
        let mut r = rig("Hello", Some("en"), (false, false, false));
        r.session.start();
        r.session.finish();
        states(&r.rx);
        r.session.apply(Command::Reinject("Hello".into()));
        assert_eq!(r.injected.borrow().as_slice(), ["Hello", "Hello"]);
        assert_eq!(
            states(&r.rx),
            ["Injecting", "Idle"],
            "no Done: it isn't a new transcript"
        );
        assert_eq!(
            r.session
                .deps
                .history
                .as_ref()
                .unwrap()
                .recent(10)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn reinject_while_recording_is_refused() {
        let mut r = rig("Hello", Some("en"), (false, false, false));
        r.session.start();
        r.session.reinject("old");
        assert!(r.injected.borrow().is_empty());
        assert_eq!(
            states(&r.rx),
            ["Recording", "Error"],
            "recording keeps going"
        );
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
