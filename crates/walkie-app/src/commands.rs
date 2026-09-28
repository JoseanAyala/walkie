use crate::glue;
use crate::login_item::{self, LoginItem};
use crate::status::{self, Check, HotkeyState, ModelStatus};
use tauri::{Emitter, Manager, State};
use walkie_core::audio;
use walkie_core::config::{self, models, Config, Polish, ThemeCfg};
use walkie_core::history::{History, Record};
use walkie_core::hotkey::engine::Bindings;
use walkie_core::pipeline::polish;

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
    (before != Some(now)).then(|| format!("theme {:?}", now.appearance))
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
    crate::glue::shutdown(&app);
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

/// What the Polish tab's Test button polishes.
pub const POLISH_SAMPLE: &str = "um so this is uh a quick test of the polish command";

/// Polishes [`POLISH_SAMPLE`] with `polish` (the tab's unsaved settings)
/// off the main thread, so a slow model doesn't freeze the window.
#[tauri::command]
pub async fn test_polish(mut polish: Polish) -> Result<String, String> {
    polish.timeout_secs = polish.timeout_secs.clamp(1, 300);
    tauri::async_runtime::spawn_blocking(move || {
        polish::polish(&polish, &glue::ai_helper(), POLISH_SAMPLE)
    })
    .await
    .map_err(estr)?
    .map_err(estr)
}

#[derive(serde::Serialize, Debug)]
pub struct AppleAi {
    /// A `polish::apple_status` word: "available", "off", …
    pub status: String,
    pub detail: &'static str,
}

/// Whether Apple's on-device model can polish right now. The Polish tab
/// asks when it shows Apple, so a ready model also starts loading: Test is
/// then quick.
#[tauri::command]
pub async fn apple_ai_status() -> AppleAi {
    let status = tauri::async_runtime::spawn_blocking(|| {
        let helper = glue::ai_helper();
        let status = polish::apple_status(&helper);
        if status == "available" {
            polish::prewarm_apple(&helper);
        }
        status
    })
    .await
    .unwrap_or_else(|_| "missing".into());
    AppleAi {
        detail: polish::describe_apple_status(&status),
        status,
    }
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
    // Before macOS has asked, the microphone pane has no walkie row to
    // switch on: show the prompt a first dictation would instead.
    if pane == "mic" && status::ask_for_microphone() {
        return Ok(());
    }
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
            "ai" => "x-apple.systempreferences:com.apple.Siri-Settings.extension",
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

    fn command(c: &str) -> Polish {
        Polish {
            provider: config::PolishProvider::Command,
            command: c.into(),
            timeout_secs: 5,
            ..Polish::default()
        }
    }

    #[test]
    fn test_polish_runs_the_command_on_the_sample() {
        let out = tauri::async_runtime::block_on(test_polish(command("tr 'a-z' 'A-Z'"))).unwrap();
        assert_eq!(out.trim(), POLISH_SAMPLE.to_uppercase());
    }

    #[test]
    fn test_polish_reports_a_missing_command() {
        let err = tauri::async_runtime::block_on(test_polish(command("  "))).unwrap_err();
        assert!(err.contains("no polish command"), "{err}");
    }

    /// Apple's real model (`make test-ai`), on a Mac that has it: a
    /// dictated question comes back tidied, not answered.
    #[cfg(feature = "ai-tests")]
    #[test]
    fn apple_polish_tidies_a_question_without_answering_it() {
        let helper = glue::ai_helper();
        let status = polish::apple_status(&helper);
        if status != "available" {
            eprintln!("skipped: Apple's model is {status} here");
            return;
        }
        // an idle model takes 4–8s to load; the check below is the warm path
        let _ = std::process::Command::new(&helper).arg("prewarm").status();
        let cfg = Polish {
            provider: config::PolishProvider::Apple,
            timeout_secs: 9, // under the 10s test budget, with a clear error
            ..Polish::default()
        };
        let out = polish::polish(&cfg, &helper, "what time is the meeting tomorrow").unwrap();
        assert!(out.to_lowercase().contains("meeting tomorrow"), "{out}");
        assert!(out.ends_with('?'), "{out}");
    }

    /// The real walkie-ai, which build.rs put next to the test binary:
    /// whatever this Mac says, it's a known status with a description.
    #[test]
    fn apple_ai_status_asks_the_bundled_helper() {
        let s = tauri::async_runtime::block_on(apple_ai_status());
        assert_ne!(
            s.status, "missing",
            "walkie-ai wasn't built next to the binary"
        );
        assert!(!s.detail.is_empty());
    }

    #[test]
    fn theme_change_is_reported_only_when_the_theme_differs() {
        let a = ThemeCfg::default();
        assert_eq!(theme_change(Some(&a), &a), None);
        let b = ThemeCfg {
            appearance: config::Appearance::Dark,
        };
        assert_eq!(theme_change(Some(&a), &b).as_deref(), Some("theme Dark"));
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
