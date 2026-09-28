use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use walkie_core::audio::duck::{Ducker, SystemVolume};
use walkie_core::audio::{Capture, CpalCapture, FileCapture};
use walkie_core::config::{self, models, Config};
use walkie_core::history::History;
use walkie_core::hotkey::engine::{Bindings, Engine, Signal};
use walkie_core::hotkey::keys::{self, binding_names, Key};
use walkie_core::hotkey::tap::{self, TapStatus};
use walkie_core::inject::{Injector, MainThread, PasteInjector, TypeInjector};
use walkie_core::pipeline::session::{Command, Deps, Event, Session, SessionState};
use walkie_core::stt::whisper::WhisperEngine;
use walkie_core::stt::{LangHint, SttEngine, Transcript};

use crate::status::{self, HotkeyState, ModelStatus};

/// Emits a model status and remembers it for the Status tab.
/// The OS e2e suite plays a fixture instead of the mic via WALKIE_TEST_AUDIO.
/// Only `test-hooks` builds honour it; a shipped app always uses the mic.
pub fn test_audio() -> Option<std::ffi::OsString> {
    if cfg!(feature = "test-hooks") {
        std::env::var_os("WALKIE_TEST_AUDIO")
    } else {
        None
    }
}

/// walkie-ai, the bridge to Apple's model. The OS e2e suite can swap in a
/// stand-in via WALKIE_AI_BIN; like WALKIE_TEST_AUDIO, only `test-hooks`
/// builds honour it.
pub fn ai_helper() -> std::path::PathBuf {
    match std::env::var_os("WALKIE_AI_BIN") {
        Some(p) if cfg!(feature = "test-hooks") => p.into(),
        _ => walkie_core::pipeline::polish::apple_helper(),
    }
}

fn model_status(app: &AppHandle, s: impl Into<String>) {
    let s = s.into();
    eprintln!("walkie: model {s}");
    if let Some(m) = app.try_state::<ModelStatus>() {
        *m.0.lock().unwrap() = s.clone();
    }
    let _ = app.emit("model-status", s);
}

/// How long the overlay stays visible (showing the last error) after an
/// `Event::Error` before an `Idle` transition is allowed to hide it. Without
/// this, a mid-session error (e.g. a failed polish command) flashes for
/// milliseconds since `Error` is immediately followed by further state
/// transitions down to `Idle`, which used to hide the overlay synchronously.
const ERROR_GRACE: Duration = Duration::from_secs(2);
/// Same, for an `Event::Notice` (e.g. "copied to clipboard" — it tells you
/// what to do next, so it gets a little longer).
const NOTICE_GRACE: Duration = Duration::from_millis(2500);
/// Same, for `Event::Done`: long enough to see "✓ typed", then out of the
/// way. Never cuts short an error's or notice's longer grace.
const DONE_GRACE: Duration = Duration::from_millis(800);

/// The later of an existing linger deadline and a new one.
fn linger_to(current: Option<Instant>, until: Instant) -> Option<Instant> {
    Some(current.map_or(until, |c| c.max(until)))
}

/// Paste-last: the newest transcript (from `Event::Done`; history covers a
/// restart) and the way to the session worker, which owns the injector.
pub struct PasteLast {
    last: Mutex<Option<String>>,
    tx: Mutex<mpsc::Sender<Command>>,
    ready: Arc<AtomicBool>,
}

/// Re-inserts the most recent transcript into the focused app after `delay`.
pub fn paste_last(app: &AppHandle, delay: Duration) {
    let Some(p) = app.try_state::<PasteLast>() else {
        return;
    };
    if !p.ready.load(Ordering::SeqCst) {
        let _ = app.emit("app-error", "still loading the speech model — please wait");
        return;
    }
    let text = p.last.lock().unwrap().clone().or_else(|| {
        History::open(&config::db_path())
            .and_then(|h| h.last_text())
            .map_err(|e| eprintln!("walkie: paste-last couldn't read history: {e}"))
            .ok()
            .flatten()
    });
    let Some(text) = text else {
        let _ = app.emit(
            "app-error",
            "nothing to paste yet — dictate something first",
        );
        return;
    };
    let tx = p.tx.lock().unwrap().clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        let _ = tx.send(Command::Reinject(text));
    });
}

pub fn start(app: AppHandle) -> Result<()> {
    for line in walkie_core::config::migrate_from_hearme() {
        eprintln!("walkie: {line}");
    }
    // A hand-edited config.toml (the README encourages this, e.g. for the
    // polish command) can have a syntax error. Falling back to defaults
    // instead of aborting keeps the app launchable — a broken config would
    // otherwise mean no tray icon, no dialog, nothing.
    let cfg = Config::load().unwrap_or_else(|e| {
        eprintln!("walkie: config error, using defaults: {e}");
        Config::default()
    });
    walkie_core::audio::set_preferred_input(&cfg.audio.input_device);
    app.manage(ModelStatus(Mutex::new("starting".into())));
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
    let (evt_tx, evt_rx) = mpsc::channel::<Event>();
    // Set to true once a speech model is loaded (see `load_model`). The
    // hotkey listener checks this before forwarding commands, so nothing is
    // recorded that couldn't be transcribed.
    let ready = Arc::new(AtomicBool::new(false));
    // Always there: whether a recording ducks is the session's config.
    let ducker = Arc::new(Ducker::new(Box::new(SystemVolume), cfg.audio.duck_percent));
    restore_on_signal(ducker.clone());
    app.manage(Duck(ducker.clone()));
    let (stopped_tx, stopped_rx) = mpsc::channel::<()>();
    app.manage(Worker {
        tx: Mutex::new(cmd_tx.clone()),
        stopped: Mutex::new(stopped_rx),
        ready: ready.clone(),
        model: Mutex::new(String::new()),
    });
    app.manage(PasteLast {
        last: Mutex::new(None),
        tx: Mutex::new(cmd_tx.clone()),
        ready: ready.clone(),
    });
    app.manage(GlobeWarning::default());

    // Worker thread: owns every !Send dep and runs the session loop until
    // shutdown. The speech model arrives separately (`load_model`).
    {
        let cfg = cfg.clone();
        let app = app.clone();
        std::thread::spawn(move || {
            let history = History::open(&config::db_path())
                .map_err(|e| eprintln!("walkie: history disabled: {e}"))
                .ok();
            let capture: Box<dyn Capture> = match test_audio() {
                Some(p) => match FileCapture::open(std::path::Path::new(&p)) {
                    Ok(c) => {
                        eprintln!("walkie: TEST MODE — microphone replaced by {p:?}");
                        Box::new(c)
                    }
                    Err(e) => {
                        eprintln!("walkie: WALKIE_TEST_AUDIO unusable ({e}), using the microphone");
                        Box::new(CpalCapture::new())
                    }
                },
                None => Box::new(CpalCapture::new()),
            };
            let injector_for = {
                let main = main_thread(&app);
                move |c: &config::Inject| injector(c, main.clone())
            };
            let deps = Deps {
                capture,
                stt: Box::new(NoModel),
                injector: injector_for(&cfg.inject),
                history,
                cfg,
            };
            let session = Session::new(deps, evt_tx)
                .with_ai_helper(ai_helper())
                .with_ducker(ducker)
                .with_injector_for(Box::new(injector_for));
            session.run(cmd_rx); // consumes the session: the model is freed here
            let _ = stopped_tx.send(());
        });
    }
    load_model(&app, &cfg.model);

    // Keyboard hook → engine → signals. The hook thread only forwards; all
    // handling (session commands, UI events) happens on this thread so a slow
    // emit can never stall the keyboard.
    let (bindings, errors) = Bindings::from_config(&cfg.hotkeys);
    for e in &errors {
        eprintln!("walkie: {e}");
    }
    let hk = HotkeyState {
        engine: Arc::new(Mutex::new(Engine::new(bindings))),
        tap: Arc::new(TapStatus::default()),
        errors: Mutex::new(errors),
    };
    let (sig_tx, sig_rx) = mpsc::channel::<Signal>();
    #[cfg(target_os = "macos")]
    tap::spawn(hk.engine.clone(), hk.tap.clone(), move |s| {
        let _ = sig_tx.send(s);
    });
    app.manage(hk);
    {
        let tx = cmd_tx.clone();
        let app = app.clone();
        let ready = ready.clone();
        std::thread::spawn(move || {
            for sig in sig_rx {
                handle_signal(&app, &tx, &ready, sig);
            }
        });
    }
    startup_check(app.clone(), cfg.first_run);

    // Event pump: session events → UI events + tray icon + overlay visibility.
    // `linger_until` and `epoch` are local to this loop (pump is only ever
    // driven sequentially from here), but the delayed-hide timer they enable
    // needs to outlive a single pump() call, so `epoch` is a shared counter
    // rather than a plain local.
    {
        let app = app.clone();
        std::thread::spawn(move || {
            let mut linger_until: Option<Instant> = None;
            let epoch = Arc::new(AtomicU64::new(0));
            for ev in evt_rx {
                pump(&app, ev, &mut linger_until, &epoch);
            }
        });
    }

    Ok(())
}

/// The session worker, so settings can reach it and quitting can stop it
/// (see `shutdown`).
pub struct Worker {
    tx: Mutex<mpsc::Sender<Command>>,
    stopped: Mutex<mpsc::Receiver<()>>,
    /// A speech model is loaded.
    ready: Arc<AtomicBool>,
    /// The model last asked for: a load that finishes after another was
    /// asked for is dropped.
    model: Mutex<String>,
}

/// Stands in until the first speech model has loaded (`ready` keeps
/// recordings from reaching it).
struct NoModel;

impl SttEngine for NoModel {
    fn transcribe(&mut self, _: &[f32], _: &LangHint) -> Result<Transcript> {
        anyhow::bail!("the speech model isn't loaded yet")
    }
}

fn injector(c: &config::Inject, main: MainThread) -> Box<dyn Injector> {
    match c.strategy.as_str() {
        "type" => Box::new(TypeInjector { main: Some(main) }),
        _ => Box::new(PasteInjector {
            restore_ms: c.restore_clipboard_ms,
            main: Some(main),
        }),
    }
}

/// Settings changed: the session takes them from its next command on, and
/// a different speech model starts loading (the current one keeps working
/// until it's ready).
pub fn reconfigure(app: &AppHandle, cfg: &Config) {
    let Some(w) = app.try_state::<Worker>() else {
        return;
    };
    let _ =
        w.tx.lock()
            .unwrap()
            .send(Command::Reconfigure(Box::new(cfg.clone())));
    eprintln!("walkie: settings applied");
    if *w.model.lock().unwrap() != cfg.model {
        load_model(app, &cfg.model);
    }
}

/// Downloads (if needed) and loads the speech model `key` off the worker
/// thread, then hands it to the session.
fn load_model(app: &AppHandle, key: &str) {
    let w = app.state::<Worker>();
    *w.model.lock().unwrap() = key.to_string();
    let app = app.clone();
    let key = key.to_string();
    std::thread::spawn(move || {
        let current = || *app.state::<Worker>().model.lock().unwrap() == key;
        if !models::is_downloaded(&key) {
            model_status(&app, "downloading");
            let ap = app.clone();
            if let Err(e) = models::download(&key, &mut |done, total| {
                let _ = ap.emit("download-progress", done * 100 / total.max(1));
            }) {
                eprintln!("walkie: model {key} failed to download: {e}");
                if current() {
                    model_status(&app, format!("error: {e}"));
                }
                return;
            }
        }
        if !current() {
            return;
        }
        model_status(&app, "loading");
        let Some(path) = models::model_path(&key) else {
            eprintln!("walkie: unknown model {key}");
            model_status(&app, format!("error: unknown model {key}"));
            return;
        };
        let stt = match WhisperEngine::load(&path) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("walkie: model {key} failed to load: {e}");
                model_status(&app, format!("error: {e}"));
                return;
            }
        };
        let w = app.state::<Worker>();
        // Asked for another meanwhile: that load takes over.
        if *w.model.lock().unwrap() != key {
            return;
        }
        if w.tx
            .lock()
            .unwrap()
            .send(Command::SetStt(Box::new(stt)))
            .is_ok()
        {
            w.ready.store(true, Ordering::SeqCst);
            model_status(&app, "ready");
        }
    });
}

/// How long quitting waits for the worker (it may be mid-transcription).
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

/// Quitting ends in `exit()`, whose C++ static destructors include
/// ggml's Metal device: it aborts (GGML_ASSERT on its residency sets) if the
/// Whisper model's GPU buffers are still alive. So free the model first by
/// stopping the worker that owns it.
pub fn shutdown(app: &AppHandle) {
    unduck(app);
    let Some(w) = app.try_state::<Worker>() else {
        return;
    };
    let _ = w.tx.lock().unwrap().send(Command::Shutdown);
    // Err(Disconnected) is fine too: the worker already ended (model error).
    let waited = w.stopped.lock().unwrap().recv_timeout(SHUTDOWN_WAIT);
    if let Err(mpsc::RecvTimeoutError::Timeout) = waited {
        eprintln!("walkie: worker still busy at quit; exiting anyway");
    }
}

/// The ducker, so quitting mid-recording can put the volume back.
pub struct Duck(Arc<Ducker>);

/// Restores the volume if a recording is ducking it. For every way out of
/// the app that bypasses the session (quitting).
pub fn unduck(app: &AppHandle) {
    if let Some(d) = app.try_state::<Duck>() {
        d.0.restore();
    }
}

static SIGNAL_PIPE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

extern "C" fn on_signal(sig: libc::c_int) {
    let b = sig as u8;
    // SAFETY: write(2) is async-signal-safe; the fd stays open for the process' life.
    unsafe {
        libc::write(
            SIGNAL_PIPE.load(Ordering::Relaxed),
            &b as *const u8 as *const _,
            1,
        )
    };
}

/// `kill`/Ctrl-C skip Tauri's exit path, so a recording killed that way
/// would leave the volume lowered. A signal handler can't safely call
/// CoreAudio, so it hands the signal to a thread that restores, then dies
/// of the signal as it would have.
fn restore_on_signal(d: Arc<Ducker>) {
    let mut fds = [0; 2];
    // SAFETY: plain libc calls on fds we own.
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return;
        }
        SIGNAL_PIPE.store(fds[1], Ordering::SeqCst);
        for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::signal(
                sig,
                on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
            );
        }
    }
    std::thread::spawn(move || {
        let mut b = 0u8;
        // SAFETY: reads one byte into `b`.
        if unsafe { libc::read(fds[0], &mut b as *mut u8 as *mut _, 1) } == 1 {
            d.restore();
            unsafe {
                libc::signal(b as libc::c_int, libc::SIG_DFL);
                libc::raise(b as libc::c_int);
            }
        }
    });
}

/// Keystroke synthesis has to run on the main thread (see `inject::MainThread`).
fn main_thread(app: &AppHandle) -> MainThread {
    let app = app.clone();
    Arc::new(move |job| {
        if let Err(e) = app.run_on_main_thread(job) {
            eprintln!("walkie: couldn't reach the main thread to type: {e}");
        }
    })
}

/// Set when a shortcut with Fn fired while macOS still gives 🌐 a job of its
/// own (emoji, input source): the overlay says so once that run is over.
#[derive(Default)]
pub struct GlobeWarning(AtomicBool);

pub const GLOBE_WARNING: &str =
    "Fn also did macOS's 🌐 action — set Keyboard → “Press 🌐 key to” → Do Nothing";

/// Whether the shortcut `keys` also triggers macOS's own 🌐 action.
fn globe_clash(keys: &[Key], globe_does_nothing: impl FnOnce() -> bool) -> bool {
    keys.contains(&Key::Fn) && !globe_does_nothing()
}

fn check_globe(app: &AppHandle, cmd: &Command) {
    let hk = app.state::<HotkeyState>();
    let keys = {
        let engine = hk.engine.lock().unwrap();
        let b = engine.bindings();
        match cmd {
            Command::Start => b.dictate.clone(),
            Command::Polish => b.polish.clone(),
            _ => return,
        }
    };
    if globe_clash(&keys, status::globe_does_nothing) {
        app.state::<GlobeWarning>().0.store(true, Ordering::SeqCst);
    }
}

fn handle_signal(app: &AppHandle, tx: &mpsc::Sender<Command>, ready: &AtomicBool, sig: Signal) {
    let cmd = match sig {
        Signal::Recorded(ref k) => {
            match keys::validate(k) {
                Ok(()) => {
                    let _ = app.emit("shortcut-recorded", binding_names(k));
                }
                Err(e) => {
                    let _ = app.emit("shortcut-error", e);
                }
            }
            return;
        }
        Signal::RecordCancelled => {
            let _ = app.emit("shortcut-cancelled", ());
            return;
        }
        // Fires once the chord is fully released, so no delay is needed.
        Signal::PasteLast => return paste_last(app, Duration::ZERO),
        ref s => match Command::from_signal(s) {
            Some(c) => c,
            None => return,
        },
    };
    if !ready.load(Ordering::SeqCst) {
        if matches!(cmd, Command::Start | Command::Polish) {
            let _ = app.emit("app-error", "still loading the speech model — please wait");
        }
        return;
    }
    check_globe(app, &cmd);
    let _ = tx.send(cmd);
}

/// Logs every status check once, and opens Settings on the Status tab if
/// something is wrong — so a broken setup is visible at launch instead of
/// discovered by "the shortcut does nothing". Skipped on first run, where
/// onboarding walks through the same permissions.
fn startup_check(app: AppHandle, first_run: bool) {
    std::thread::spawn(move || {
        // Give the hook a moment to come up before judging it.
        std::thread::sleep(Duration::from_millis(1500));
        let checks = status::collect(&app.state::<HotkeyState>(), &app.state::<ModelStatus>());
        for c in &checks {
            eprintln!(
                "walkie: status {} {}: {}",
                if c.ok { "ok  " } else { "FAIL" },
                c.label,
                c.detail
            );
        }
        let blocking = checks
            .iter()
            .any(|c| !c.ok && !["model", "device-missing", "apple-ai"].contains(&c.id));
        if blocking && !first_run {
            if let Some(w) = app.get_webview_window("settings") {
                let _ = app.emit("show-tab", "status");
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    });
}

fn state_name(s: SessionState) -> &'static str {
    match s {
        SessionState::Idle => "idle",
        SessionState::Recording => "recording",
        SessionState::Transcribing => "transcribing",
        SessionState::Polishing => "polishing",
        SessionState::Injecting => "injecting",
    }
}

fn pump(app: &AppHandle, ev: Event, linger_until: &mut Option<Instant>, epoch: &Arc<AtomicU64>) {
    match ev {
        Event::State(s) => {
            // Bump first: any timer scheduled by a *previous* Idle event
            // that fires before this new state was known about must see
            // its captured epoch as stale and bail out.
            epoch.fetch_add(1, Ordering::SeqCst);
            set_tray(app, &s);
            let globe = app.try_state::<GlobeWarning>();
            if s == SessionState::Idle && globe.is_some_and(|g| g.0.swap(false, Ordering::SeqCst)) {
                *linger_until = Some(Instant::now() + ERROR_GRACE);
                eprintln!("walkie error: {GLOBE_WARNING}");
                let _ = app.emit("app-error", GLOBE_WARNING);
            }

            if s == SessionState::Idle {
                let remaining = (*linger_until)
                    .and_then(|t| t.checked_duration_since(Instant::now()))
                    .filter(|d| !d.is_zero());
                match remaining {
                    Some(delay) => defer_idle(app.clone(), delay, epoch.clone()),
                    None => {
                        let _ = app.emit("state", "idle");
                        if let Some(w) = app.get_webview_window("overlay") {
                            let _ = w.hide();
                        }
                    }
                }
            } else {
                let _ = app.emit("state", state_name(s));
                // Polishing straight from idle: the polish shortcut.
                if s == SessionState::Recording || s == SessionState::Polishing {
                    // A fresh dictation (or polish) starting means any earlier
                    // error's grace period is no longer relevant — don't let
                    // it delay hiding the overlay for *this* (possibly clean)
                    // one once it finishes.
                    *linger_until = None;
                    if let Some(w) = app.get_webview_window("overlay") {
                        let _ = position_overlay(&w);
                        crate::overlay::show(&w);
                    }
                }
            }
        }
        Event::Level(v) => {
            let _ = app.emit("level", v);
        }
        Event::Done { text, .. } => {
            *linger_until = linger_to(*linger_until, Instant::now() + DONE_GRACE);
            if let Some(p) = app.try_state::<PasteLast>() {
                *p.last.lock().unwrap() = Some(text.clone());
            }
            let _ = app.emit("transcribed", text);
        }
        Event::Error(msg) => {
            *linger_until = Some(Instant::now() + ERROR_GRACE);
            eprintln!("walkie error: {msg}");
            let _ = app.emit("app-error", msg);
        }
        Event::Notice(msg) => {
            *linger_until = Some(Instant::now() + NOTICE_GRACE);
            eprintln!("walkie: notice: {msg}");
            let _ = app.emit("app-notice", msg);
        }
    }
}

/// Hides the overlay (and tells it we're idle) after `delay`, unless a newer
/// state event has since bumped `epoch` past the value captured here — e.g.
/// a new `Recording` started before the error's grace period elapsed.
fn defer_idle(app: AppHandle, delay: Duration, epoch: Arc<AtomicU64>) {
    let at_epoch = epoch.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if epoch.load(Ordering::SeqCst) != at_epoch {
            return;
        }
        let _ = app.emit("state", "idle");
        if let Some(w) = app.get_webview_window("overlay") {
            let _ = w.hide();
        }
    });
}

fn set_tray(app: &AppHandle, state: &SessionState) {
    // Idle is a template (macOS tints it to match the menu bar); the active
    // states keep their accent color.
    let (bytes, template): (&[u8], bool) = match state {
        SessionState::Recording => (include_bytes!("../icons/tray-rec.png"), false),
        SessionState::Idle => (include_bytes!("../icons/tray-idle.png"), true),
        SessionState::Transcribing | SessionState::Polishing | SessionState::Injecting => {
            (include_bytes!("../icons/tray-busy.png"), false)
        }
    };
    if let Some(h) = app.try_state::<crate::TrayHandle>() {
        if let Ok(img) = tauri::image::Image::from_bytes(bytes) {
            let _ =
                h.0.lock()
                    .unwrap()
                    .set_icon_with_as_template(Some(img), template);
        }
    }
}

fn position_overlay(w: &tauri::WebviewWindow) -> tauri::Result<()> {
    if let Some(mon) = w.primary_monitor()? {
        let m = mon.size();
        let s = w.outer_size()?;
        let pos = mon.position();
        w.set_position(tauri::PhysicalPosition::new(
            pos.x + ((m.width.saturating_sub(s.width)) / 2) as i32,
            pos.y + (m.height.saturating_sub(s.height + 120)) as i32,
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fn_shortcuts_clash_with_the_globe_key_until_it_does_nothing() {
        assert!(globe_clash(&[Key::Fn], || false));
        assert!(!globe_clash(&[Key::Fn], || true));
        let never_asked = || panic!("no Fn: no need to check");
        assert!(!globe_clash(&[Key::Code(49)], never_asked));
    }

    #[test]
    fn done_lingers_but_never_shortens_an_errors_grace() {
        let now = Instant::now();
        let done = now + DONE_GRACE;
        assert_eq!(linger_to(None, done), Some(done));
        let error = now + ERROR_GRACE;
        assert_eq!(linger_to(Some(error), done), Some(error));
        let stale = now - ERROR_GRACE;
        assert_eq!(linger_to(Some(stale), done), Some(done));
    }
}

#[cfg(all(test, not(feature = "test-hooks")))]
mod test_hooks_tests {
    #[test]
    fn release_builds_ignore_test_audio() {
        std::env::set_var("WALKIE_TEST_AUDIO", "/tmp/fixture.wav");
        assert_eq!(super::test_audio(), None);
        std::env::remove_var("WALKIE_TEST_AUDIO");
    }
}
