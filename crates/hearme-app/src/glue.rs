use anyhow::Result;
use hearme_core::audio::CpalCapture;
use hearme_core::config::{self, models, Config};
use hearme_core::history::History;
use hearme_core::hotkey::engine::{Bindings, Engine, Signal};
use hearme_core::hotkey::keys::{self, binding_names};
use hearme_core::hotkey::tap::{self, TapStatus};
use hearme_core::inject::{Injector, PasteInjector, TypeInjector};
use hearme_core::pipeline::session::{Command, Deps, Event, Session, SessionState};
use hearme_core::stt::whisper::WhisperEngine;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::status::{self, HotkeyState, ModelStatus};

/// Emits a model status and remembers it for the Status tab.
fn model_status(app: &AppHandle, s: impl Into<String>) {
    let s = s.into();
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

pub fn start(app: AppHandle) -> Result<()> {
    // A hand-edited config.toml (the README encourages this, e.g. for the
    // polish command) can have a syntax error. Falling back to defaults
    // instead of aborting keeps the app launchable — a broken config would
    // otherwise mean no tray icon, no dialog, nothing.
    let cfg = Config::load().unwrap_or_else(|e| {
        eprintln!("hearme: config error, using defaults: {e}");
        Config::default()
    });
    app.manage(ModelStatus(Mutex::new("starting".into())));
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
    let (evt_tx, evt_rx) = mpsc::channel::<Event>();
    // Set to true only once the worker thread has finished loading the model
    // and is about to start consuming `cmd_rx` via `Session::run`. The hotkey
    // listener checks this before forwarding commands, since it starts
    // immediately and would otherwise queue commands nobody is consuming yet.
    let ready = Arc::new(AtomicBool::new(false));

    // Worker thread: owns every !Send dep. Downloads/loads the model, then
    // runs the session loop until shutdown.
    {
        let cfg = cfg.clone();
        let app = app.clone();
        let ready = ready.clone();
        std::thread::spawn(move || {
            let key = cfg.model.clone();
            if !models::is_downloaded(&key) {
                model_status(&app, "downloading");
                let ap = app.clone();
                if let Err(e) = models::download(&key, &mut |done, total| {
                    let _ = ap.emit("download-progress", done * 100 / total.max(1));
                }) {
                    eprintln!("hearme: model {key} failed to download: {e}");
                    model_status(&app, format!("error: {e}"));
                    return;
                }
            }
            model_status(&app, "loading");
            let path = match models::model_path(&key) {
                Some(p) => p,
                None => {
                    eprintln!("hearme: unknown model {key}");
                    model_status(&app, format!("error: unknown model {key}"));
                    return;
                }
            };
            let stt = match WhisperEngine::load(&path) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("hearme: model {key} failed to load: {e}");
                    model_status(&app, format!("error: {e}"));
                    return;
                }
            };
            model_status(&app, "ready");

            let injector: Box<dyn Injector> = match cfg.inject.strategy.as_str() {
                "type" => Box::new(TypeInjector),
                _ => Box::new(PasteInjector { restore_ms: cfg.inject.restore_clipboard_ms }),
            };
            let history = History::open(&config::db_path())
                .map_err(|e| eprintln!("hearme: history disabled: {e}"))
                .ok();
            let deps = Deps {
                capture: Box::new(CpalCapture::new()),
                stt: Box::new(stt),
                injector,
                history,
                cfg,
            };
            ready.store(true, Ordering::SeqCst);
            Session::new(deps, evt_tx).run(cmd_rx);
        });
    }

    // Keyboard hook → engine → signals. The hook thread only forwards; all
    // handling (session commands, UI events) happens on this thread so a slow
    // emit can never stall the keyboard.
    let (bindings, errors) = Bindings::from_config(&cfg.hotkeys);
    for e in &errors {
        eprintln!("hearme: {e}");
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
    // `last_error_at` and `epoch` are local to this loop (pump is only ever
    // driven sequentially from here), but the delayed-hide timer they enable
    // needs to outlive a single pump() call, so `epoch` is a shared counter
    // rather than a plain local.
    {
        let app = app.clone();
        std::thread::spawn(move || {
            let mut last_error_at: Option<Instant> = None;
            let epoch = Arc::new(AtomicU64::new(0));
            for ev in evt_rx {
                pump(&app, ev, &mut last_error_at, &epoch);
            }
        });
    }

    Ok(())
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
        ref s => match Command::from_signal(s) {
            Some(c) => c,
            None => return,
        },
    };
    if !ready.load(Ordering::SeqCst) {
        if let Command::Start(_) = cmd {
            let _ = app.emit("app-error", "still loading the speech model — please wait");
        }
        return;
    }
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
            eprintln!("hearme: status {} {}: {}", if c.ok { "ok  " } else { "FAIL" }, c.label, c.detail);
        }
        let blocking = checks.iter().any(|c| !c.ok && c.id != "model");
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

fn pump(app: &AppHandle, ev: Event, last_error_at: &mut Option<Instant>, epoch: &Arc<AtomicU64>) {
    match ev {
        Event::State(s) => {
            // Bump first: any timer scheduled by a *previous* Idle event
            // that fires before this new state was known about must see
            // its captured epoch as stale and bail out.
            epoch.fetch_add(1, Ordering::SeqCst);
            set_tray(app, &s);

            if s == SessionState::Idle {
                let remaining =
                    (*last_error_at).and_then(|t| ERROR_GRACE.checked_sub(t.elapsed()));
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
                if s == SessionState::Recording {
                    // A fresh dictation starting means any earlier error's
                    // grace period is no longer relevant — don't let it
                    // delay hiding the overlay for *this* (possibly clean)
                    // recording once it finishes.
                    *last_error_at = None;
                    if let Some(w) = app.get_webview_window("overlay") {
                        let _ = position_overlay(&w);
                        let _ = w.show();
                    }
                }
            }
        }
        Event::Level(v) => {
            let _ = app.emit("level", v);
        }
        Event::Done { text, .. } => {
            let _ = app.emit("transcribed", text);
        }
        Event::Error(msg) => {
            *last_error_at = Some(Instant::now());
            eprintln!("hearme error: {msg}");
            let _ = app.emit("app-error", msg);
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
    let bytes: &[u8] = match state {
        SessionState::Recording => include_bytes!("../icons/tray-rec.png"),
        SessionState::Idle => include_bytes!("../icons/tray-idle.png"),
        SessionState::Transcribing | SessionState::Polishing | SessionState::Injecting => {
            include_bytes!("../icons/tray-busy.png")
        }
    };
    if let Some(h) = app.try_state::<crate::TrayHandle>() {
        if let Ok(img) = tauri::image::Image::from_bytes(bytes) {
            let _ = h.0.lock().unwrap().set_icon(Some(img));
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
