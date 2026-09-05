use anyhow::{Context, Result};
use hearme_core::audio::CpalCapture;
use hearme_core::config::{self, models, Config};
use hearme_core::history::History;
use hearme_core::hotkey::listener::{parse_key, spawn_listener};
use hearme_core::hotkey::Output;
use hearme_core::inject::{Injector, PasteInjector, TypeInjector};
use hearme_core::pipeline::session::{Command, Deps, Event, Session, SessionState};
use hearme_core::stt::whisper::WhisperEngine;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use tauri::{AppHandle, Emitter, Manager};

pub fn start(app: AppHandle) -> Result<()> {
    let cfg = Config::load().context("loading config")?;
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
                let _ = app.emit("model-status", "downloading");
                let ap = app.clone();
                if let Err(e) = models::download(&key, &mut |done, total| {
                    let _ = ap.emit("download-progress", done * 100 / total.max(1));
                }) {
                    eprintln!("hearme: model {key} failed to download: {e}");
                    let _ = app.emit("model-status", format!("error: {e}"));
                    return;
                }
            }
            let _ = app.emit("model-status", "loading");
            let path = match models::model_path(&key) {
                Some(p) => p,
                None => {
                    eprintln!("hearme: unknown model {key}");
                    let _ = app.emit("model-status", format!("error: unknown model {key}"));
                    return;
                }
            };
            let stt = match WhisperEngine::load(&path) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("hearme: model {key} failed to load: {e}");
                    let _ = app.emit("model-status", format!("error: {e}"));
                    return;
                }
            };
            let _ = app.emit("model-status", "ready");

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

    // Global hotkey → session commands.
    let hot = parse_key(&cfg.hotkeys.dictate).unwrap_or(rdev::Key::AltGr);
    let use_shift = cfg.hotkeys.polish_modifier.eq_ignore_ascii_case("shift");
    {
        let tx = cmd_tx.clone();
        let app = app.clone();
        let ready = ready.clone();
        spawn_listener(hot, use_shift, move |mode, out| {
            if !ready.load(Ordering::SeqCst) {
                if let Output::Start = out {
                    let _ = app.emit("app-error", "still loading the speech model — please wait");
                }
                return;
            }
            let _ = tx.send(match out {
                Output::Start => Command::Start(mode),
                Output::Finish => Command::Finish,
                Output::CancelDiscard => Command::Cancel,
            });
        })?;
    }

    // Event pump: session events → UI events + tray icon + overlay visibility.
    {
        let app = app.clone();
        std::thread::spawn(move || {
            for ev in evt_rx {
                pump(&app, ev);
            }
        });
    }

    Ok(())
}

fn pump(app: &AppHandle, ev: Event) {
    match ev {
        Event::State(s) => {
            let name = match s {
                SessionState::Idle => "idle",
                SessionState::Recording => "recording",
                SessionState::Transcribing => "transcribing",
                SessionState::Polishing => "polishing",
                SessionState::Injecting => "injecting",
            };
            let _ = app.emit("state", name);
            set_tray(app, name);
            if let Some(w) = app.get_webview_window("overlay") {
                match s {
                    SessionState::Recording => {
                        let _ = position_overlay(&w);
                        let _ = w.show();
                    }
                    SessionState::Idle => {
                        let _ = w.hide();
                    }
                    _ => {}
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
            eprintln!("hearme error: {msg}");
            let _ = app.emit("app-error", msg);
        }
    }
}

fn set_tray(app: &AppHandle, state: &str) {
    let bytes: &[u8] = match state {
        "recording" => include_bytes!("../icons/tray-rec.png"),
        "idle" => include_bytes!("../icons/tray-idle.png"),
        _ => include_bytes!("../icons/tray-busy.png"),
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
