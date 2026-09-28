use crate::login_item::{self, LoginItem};
use crate::status::{self, Check, HotkeyState, ModelStatus};
use std::time::Duration;
use tauri::{Emitter, Manager, State};
use walkie_core::audio;
use walkie_core::config::{self, models, Config, ThemeCfg};
use walkie_core::history::{History, Record};
use walkie_core::hotkey::engine::Bindings;
use walkie_core::pipeline::polish::run_polish;

fn estr(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_config() -> Result<Config, String> {
    Config::load().map_err(estr)
}

/// Saves, then applies the shortcuts, microphone and theme immediately.
/// Problems with a shortcut come back as the error (the rest of the config
/// is still saved).
#[tauri::command]
pub fn save_config(
    app: tauri::AppHandle,
    cfg: Config,
    hk: State<HotkeyState>,
) -> Result<(), String> {
    let before = Config::load().ok().map(|c| c.theme);
    cfg.save().map_err(estr)?;
    if let Some(line) = theme_change(before.as_ref(), &cfg.theme) {
        eprintln!("walkie: {line}");
        // every window (overlay, onboarding) repaints in the new colors
        let _ = app.emit("theme", &cfg.theme);
    }
    audio::set_preferred_input(&cfg.audio.input_device);
    let (bindings, errors) = Bindings::from_config(&cfg.hotkeys);
    hk.engine.lock().unwrap().set_bindings(bindings);
    *hk.errors.lock().unwrap() = errors.clone();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// The log line for a theme change, or None if the theme didn't change.
fn theme_change(before: Option<&ThemeCfg>, now: &ThemeCfg) -> Option<String> {
    (before != Some(now)).then(|| format!("theme {} ({:?})", now.name, now.appearance))
}

#[derive(serde::Serialize)]
pub struct InputDevices {
    pub default: Option<String>,
    pub devices: Vec<String>,
}

#[tauri::command]
pub fn list_input_devices() -> InputDevices {
    InputDevices {
        default: audio::default_input_name(),
        devices: audio::input_device_names(),
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
    crate::glue::unduck(&app);
    app.restart();
}

#[tauri::command]
pub fn history_recent(limit: u32) -> Result<Vec<Record>, String> {
    History::open(&config::db_path())
        .and_then(|h| h.recent(limit))
        .map_err(estr)
}

#[tauri::command]
pub fn history_search(q: String, limit: u32) -> Result<Vec<Record>, String> {
    History::open(&config::db_path())
        .and_then(|h| h.search(&q, limit))
        .map_err(estr)
}

#[tauri::command]
pub fn history_delete(id: i64) -> Result<bool, String> {
    History::open(&config::db_path())
        .and_then(|h| h.delete(id))
        .map_err(estr)
}

#[tauri::command]
pub fn history_clear() -> Result<usize, String> {
    History::open(&config::db_path())
        .and_then(|h| h.clear())
        .map_err(estr)
}

/// What the Polish tab's Test button sends through the command.
pub const POLISH_SAMPLE: &str = "um so this is uh a quick test of the polish command";

/// Runs `command` on [`POLISH_SAMPLE`] off the main thread, so a slow CLI
/// doesn't freeze the window while it thinks.
#[tauri::command]
pub async fn test_polish(command: String, timeout_secs: u64) -> Result<String, String> {
    let timeout = Duration::from_secs(timeout_secs.clamp(1, 300));
    tauri::async_runtime::spawn_blocking(move || run_polish(&command, POLISH_SAMPLE, timeout))
        .await
        .map_err(estr)?
        .map_err(estr)
}

#[derive(serde::Serialize, Debug)]
pub struct ModelChoice {
    pub key: &'static str,
    pub note: &'static str,
    pub size_mb: u64,
    pub memory_mb: u64,
    pub downloaded: bool,
}

#[tauri::command]
pub fn list_models() -> Vec<ModelChoice> {
    models::REGISTRY
        .iter()
        .map(|m| ModelChoice {
            key: m.key,
            note: m.note,
            size_mb: m.approx_bytes / 1_000_000,
            memory_mb: m.memory_bytes / 1_000_000,
            downloaded: models::is_downloaded(m.key),
        })
        .collect()
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut cb| cb.set_text(text))
        .map_err(estr)
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
            "input" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
            }
            "keyboard" => "x-apple.systempreferences:com.apple.Keyboard-Settings.extension",
            "sound" => "x-apple.systempreferences:com.apple.Sound-Settings.extension",
            "loginitems" => "x-apple.systempreferences:com.apple.LoginItems-Settings.extension",
            _ => return Err(format!("unknown pane: {pane}")),
        };
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(estr)?;
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
    let login = if launch_at_login {
        login_item::set(true).err()
    } else {
        None
    };
    if let Some(w) = app.get_webview_window("onboarding") {
        let _ = w.hide();
    }
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    if let Some(e) = login {
        eprintln!("walkie: {e}");
        let _ = app.emit("app-error", e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polish_runs_the_command_on_the_sample() {
        let out = tauri::async_runtime::block_on(test_polish("tr 'a-z' 'A-Z'".into(), 5)).unwrap();
        assert_eq!(out.trim(), POLISH_SAMPLE.to_uppercase());
    }

    #[test]
    fn test_polish_reports_a_missing_command() {
        let err = tauri::async_runtime::block_on(test_polish("  ".into(), 5)).unwrap_err();
        assert!(err.contains("no polish command"), "{err}");
    }

    #[test]
    fn theme_change_is_reported_only_when_the_theme_differs() {
        let a = ThemeCfg::default();
        assert_eq!(theme_change(Some(&a), &a), None);
        let b = ThemeCfg {
            name: "klein".into(),
            appearance: config::Appearance::Dark,
            ..a.clone()
        };
        assert_eq!(
            theme_change(Some(&a), &b).as_deref(),
            Some("theme klein (Dark)")
        );
        assert!(theme_change(None, &a).is_some(), "no saved config yet");
    }

    #[test]
    fn list_models_covers_the_registry() {
        let m = list_models();
        assert_eq!(m.len(), models::REGISTRY.len());
        let turbo = m.iter().find(|m| m.key == "large-v3-turbo-q5_0").unwrap();
        assert_eq!(turbo.size_mb, 574);
        assert_eq!(turbo.memory_mb, 1000);
        assert!(!turbo.note.is_empty());
    }
}
