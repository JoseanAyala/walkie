#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

mod commands;
mod glue;
mod logfile;
mod login_item;
mod status;

pub struct TrayHandle(pub Mutex<tauri::tray::TrayIcon>);

/// The tray menu doesn't activate hearme (an Accessory app), so the app you
/// were in keeps focus — but the menu is still closing when the click
/// arrives. Pasting right away can land before it's gone.
const TRAY_PASTE_DELAY: Duration = Duration::from_millis(250);

fn main() {
    if let Some(code) = login_item::cli() {
        std::process::exit(code);
    }
    logfile::init();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::history_recent,
            commands::history_search,
            commands::copy_text,
            commands::open_settings_pane,
            commands::finish_onboarding,
            commands::get_launch_at_login,
            commands::set_launch_at_login,
            commands::get_status,
            commands::record_shortcut,
            commands::restart_app,
            commands::list_input_devices,
        ])
        .on_window_event(|window, event| {
            // The settings/overlay/onboarding windows are declared once in
            // tauri.conf.json and never re-created. Tauri's default behavior
            // for a close request is to destroy the window, which would make
            // it permanently unavailable (e.g. tray → "Settings…" silently
            // doing nothing). Hide instead so it can be shown again later.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let paste = MenuItem::with_id(app, "paste_last", "Paste last transcript", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit hearme", true, None::<&str>)?;
            let menu =
                MenuBuilder::new(app).item(&paste).separator().item(&settings).separator().item(&quit).build()?;

            let tray = TrayIconBuilder::with_id("main")
                .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray-idle.png"))?)
                .icon_as_template(false)
                .menu(&menu)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "paste_last" => glue::paste_last(app, TRAY_PASTE_DELAY),
                    "settings" => {
                        if let Some(w) = app.get_webview_window("settings") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            app.manage(TrayHandle(Mutex::new(tray)));
            glue::start(app.handle().clone())?;
            if hearme_core::config::Config::load()
                .map(|c| c.first_run)
                .unwrap_or(true)
            {
                if let Some(w) = app.get_webview_window("onboarding") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running hearme")
        .run(|app, ev| {
            if let tauri::RunEvent::Exit = ev {
                glue::unduck(app);
            }
        });
}
