use crate::login_item::{self, LoginItem};
use crate::status::{self, Check, HotkeyState, ModelStatus};
use hearme_core::config::{self, Config};
use hearme_core::history::{History, Record};
use hearme_core::hotkey::engine::Bindings;
use tauri::{Emitter, Manager, State};

fn estr(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_config() -> Result<Config, String> {
    Config::load().map_err(estr)
}

/// Saves, then applies the shortcuts immediately. Problems with a shortcut
/// come back as the error (the rest of the config is still saved).
#[tauri::command]
pub fn save_config(cfg: Config, hk: State<HotkeyState>) -> Result<(), String> {
    cfg.save().map_err(estr)?;
    let (bindings, errors) = Bindings::from_config(&cfg.hotkeys);
    hk.engine.lock().unwrap().set_bindings(bindings);
    *hk.errors.lock().unwrap() = errors.clone();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[tauri::command]
pub fn get_status(hk: State<HotkeyState>, model: State<ModelStatus>) -> Vec<Check> {
    status::collect(&hk, &model)
}

/// The next chord pressed anywhere comes back as a `shortcut-recorded`
/// (or `shortcut-error` / `shortcut-cancelled`) event.
#[tauri::command]
pub fn record_shortcut(hk: State<HotkeyState>) -> Result<(), String> {
    if !hk.tap.running.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("the keyboard hook isn't running — see the Status tab".into());
    }
    hk.engine.lock().unwrap().start_recording();
    Ok(())
}

#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

#[tauri::command]
pub fn history_recent(limit: u32) -> Result<Vec<Record>, String> {
    History::open(&config::db_path()).and_then(|h| h.recent(limit)).map_err(estr)
}

#[tauri::command]
pub fn history_search(q: String, limit: u32) -> Result<Vec<Record>, String> {
    History::open(&config::db_path()).and_then(|h| h.search(&q, limit)).map_err(estr)
}

#[tauri::command]
pub fn open_settings_pane(pane: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let url = match pane.as_str() {
            "mic" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
            "accessibility" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            "input" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
            "keyboard" => "x-apple.systempreferences:com.apple.Keyboard-Settings.extension",
            "sound" => "x-apple.systempreferences:com.apple.Sound-Settings.extension",
            "loginitems" => "x-apple.systempreferences:com.apple.LoginItems-Settings.extension",
            _ => return Err(format!("unknown pane: {pane}")),
        };
        std::process::Command::new("open").arg(url).spawn().map_err(estr)?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = pane;
    Ok(())
}

#[tauri::command]
pub fn get_launch_at_login() -> LoginItem {
    login_item::status().into()
}

/// Returns the state macOS reports afterwards, not the one asked for.
#[tauri::command]
pub fn set_launch_at_login(enabled: bool) -> Result<LoginItem, String> {
    login_item::set(enabled).map(Into::into)
}

#[tauri::command]
pub fn finish_onboarding(app: tauri::AppHandle, launch_at_login: bool) -> Result<(), String> {
    let mut cfg = Config::load().map_err(estr)?;
    cfg.first_run = false;
    cfg.save().map_err(estr)?;
    // Onboarding still finishes if this fails; Settings shows why.
    let login = if launch_at_login { login_item::set(true).err() } else { None };
    if let Some(w) = app.get_webview_window("onboarding") {
        let _ = w.hide();
    }
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    if let Some(e) = login {
        eprintln!("hearme: {e}");
        let _ = app.emit("app-error", e);
    }
    Ok(())
}
