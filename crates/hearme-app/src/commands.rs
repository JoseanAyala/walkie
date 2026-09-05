use hearme_core::config::{self, Config};
use hearme_core::history::{History, Record};
use tauri::Manager;

fn estr(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_config() -> Result<Config, String> {
    Config::load().map_err(estr)
}

#[tauri::command]
pub fn save_config(cfg: Config) -> Result<(), String> {
    cfg.save().map_err(estr)
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
            _ => return Err(format!("unknown pane: {pane}")),
        };
        std::process::Command::new("open").arg(url).spawn().map_err(estr)?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = pane;
    Ok(())
}

#[tauri::command]
pub fn finish_onboarding(app: tauri::AppHandle) -> Result<(), String> {
    let mut cfg = Config::load().map_err(estr)?;
    cfg.first_run = false;
    cfg.save().map_err(estr)?;
    if let Some(w) = app.get_webview_window("onboarding") {
        let _ = w.hide();
    }
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    Ok(())
}
