#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

pub struct TrayHandle(pub Mutex<tauri::tray::TrayIcon>);

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit hearme", true, None::<&str>)?;
            let menu = MenuBuilder::new(app).item(&settings).separator().item(&quit).build()?;

            let tray = TrayIconBuilder::with_id("main")
                .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray-idle.png"))?)
                .icon_as_template(false)
                .menu(&menu)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running hearme");
}
