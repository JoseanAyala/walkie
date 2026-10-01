use super::Status;

// TODO(linux): tauri-plugin-autostart (XDG autostart .desktop file under
// ~/.config/autostart) is the portable way to do this; there's no SMAppService
// equivalent, so `status`/`set` would just read/write that file.
pub fn status() -> Status {
    Status::NotFound
}
pub fn launched_at_login() -> Option<bool> {
    None
}
pub fn set(_: bool) -> Result<(), String> {
    Err("launch at login is only supported on macOS".into())
}
