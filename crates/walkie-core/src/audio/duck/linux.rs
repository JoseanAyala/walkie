use super::OutputVolume;

/// No ducking on this platform yet (see AGENTS.md): `get` always reports
/// "nothing to lower", so `Ducker::duck` is a no-op and `restore` has
/// nothing to undo.
// TODO(linux): read/set the default sink's volume via `wpctl` (PipeWire) or
// libpulse, the way `macos::SystemVolume` does with CoreAudio.
pub struct SystemVolume;

impl OutputVolume for SystemVolume {
    fn get(&self) -> Option<f32> {
        None
    }
    fn set(&self, _v: f32) {}
}
