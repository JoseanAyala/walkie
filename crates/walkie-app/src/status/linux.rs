use super::Check;

/// No "Press 🌐 key to" setting on Linux; assume it's fine.
pub fn globe_does_nothing() -> bool {
    true
}

/// No mic-permission prompt to trigger on this platform yet.
pub fn ask_for_microphone() -> bool {
    false
}

/// No permission checks here yet; see AGENTS.md.
// TODO(linux): check that /dev/input's event nodes are readable (the
// `input` group, or a udev rule) as a stand-in for Accessibility/Input
// Monitoring, and query PipeWire/PulseAudio for the mic instead of
// AVFoundation's authorization status.
pub fn permission_checks(_tap_running: bool) -> Vec<Check> {
    Vec::new()
}
