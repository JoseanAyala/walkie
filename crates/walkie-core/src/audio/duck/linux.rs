use super::OutputVolume;
use std::process::Command;
use std::sync::OnceLock;

/// The default sink's volume, read/written through `wpctl` (PipeWire) or,
/// if that's missing, `pactl` (the PulseAudio-compat CLI PipeWire also
/// ships). Each `get`/`set` spawns a subprocess — unlike macOS's direct
/// CoreAudio call — but `wpctl get-volume` measures well under 10ms on a
/// PipeWire session, and `Ducker::duck`/`restore` only ever call this a
/// couple of times each, so the recording-start path doesn't notice.
pub struct SystemVolume;

const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const PACTL_SINK: &str = "@DEFAULT_SINK@";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Backend {
    Wpctl,
    Pactl,
    None,
}

fn backend() -> Backend {
    static BACKEND: OnceLock<Backend> = OnceLock::new();
    *BACKEND.get_or_init(|| {
        if has_binary("wpctl") {
            Backend::Wpctl
        } else if has_binary("pactl") {
            Backend::Pactl
        } else {
            Backend::None
        }
    })
}

/// Looked up on PATH rather than run: `wpctl --version` exits 1.
fn has_binary(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(bin).is_file()))
}

fn run(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `wpctl get-volume @DEFAULT_AUDIO_SINK@` prints `Volume: 0.40` or, when
/// muted, `Volume: 0.40 [MUTED]`. Mirrors macOS: muted reads as `None`.
fn parse_wpctl_volume(out: &str) -> Option<f32> {
    let line = out.trim();
    if line.contains("[MUTED]") {
        return None;
    }
    line.strip_prefix("Volume:")?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// `pactl get-sink-mute @DEFAULT_SINK@` prints `Mute: yes` / `Mute: no`.
fn parse_pactl_mute(out: &str) -> Option<bool> {
    match out.trim().strip_prefix("Mute:")?.trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// `pactl get-sink-volume @DEFAULT_SINK@` prints one or more channels, e.g.
/// `Volume: front-left: 26214 /  40% / -23.54 dB, front-right: ...`. Every
/// channel of a mono-volume sink reports the same percentage, so the first
/// one found is the volume.
fn parse_pactl_volume(out: &str) -> Option<f32> {
    let before_pct = out.split('%').next()?;
    let digits: String = before_pct
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let digits: String = digits.chars().rev().collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<f32>().ok().map(|pct| pct / 100.0)
}

impl OutputVolume for SystemVolume {
    fn get(&self) -> Option<f32> {
        match backend() {
            Backend::Wpctl => parse_wpctl_volume(&run("wpctl", &["get-volume", SINK])?),
            Backend::Pactl => {
                let muted = parse_pactl_mute(&run("pactl", &["get-sink-mute", PACTL_SINK])?)?;
                if muted {
                    return None;
                }
                parse_pactl_volume(&run("pactl", &["get-sink-volume", PACTL_SINK])?)
            }
            Backend::None => None,
        }
    }

    fn set(&self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        match backend() {
            Backend::Wpctl => {
                let _ = run("wpctl", &["set-volume", SINK, &format!("{v:.4}")]);
            }
            Backend::Pactl => {
                let pct = (v * 100.0).round() as i32;
                let _ = run(
                    "pactl",
                    &["set-sink-volume", PACTL_SINK, &format!("{pct}%")],
                );
            }
            Backend::None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wpctl_volume_parses() {
        assert_eq!(parse_wpctl_volume("Volume: 0.40\n"), Some(0.40));
        assert_eq!(parse_wpctl_volume("Volume: 1.00\n"), Some(1.00));
    }

    #[test]
    fn wpctl_muted_is_none() {
        assert_eq!(parse_wpctl_volume("Volume: 0.40 [MUTED]\n"), None);
    }

    #[test]
    fn wpctl_garbage_is_none() {
        assert_eq!(parse_wpctl_volume("not a volume\n"), None);
        assert_eq!(parse_wpctl_volume(""), None);
    }

    #[test]
    fn pactl_mute_parses() {
        assert_eq!(parse_pactl_mute("Mute: yes\n"), Some(true));
        assert_eq!(parse_pactl_mute("Mute: no\n"), Some(false));
        assert_eq!(parse_pactl_mute("nonsense\n"), None);
    }

    #[test]
    fn pactl_volume_parses_the_first_channel() {
        let out = "Volume: front-left: 26214 /  40% / -23.54 dB,   front-right: 26214 /  40% / -23.54 dB\n        balance 0.00\n";
        assert_eq!(parse_pactl_volume(out), Some(0.40));
    }

    #[test]
    fn pactl_volume_handles_a_single_channel() {
        assert_eq!(
            parse_pactl_volume("Volume: mono: 65536 / 100% / 0.00 dB\n"),
            Some(1.0)
        );
    }

    #[test]
    fn pactl_volume_garbage_is_none() {
        assert_eq!(parse_pactl_volume("no percentages here\n"), None);
    }
}
