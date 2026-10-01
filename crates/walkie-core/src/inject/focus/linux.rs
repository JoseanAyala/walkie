use super::Probe;
use std::process::Command;

// TODO(linux): query focus through AT-SPI (the accessibility bus most
// desktop toolkits — GTK, Qt — answer on), the way `macos` does through AX.
pub fn probe() -> Probe {
    Probe::Failed
}

// TODO(linux): AT-SPI's Text interface exposes a selection, where the
// focused app supports it.
pub fn selected_text() -> Option<String> {
    None
}

/// The focused window's class and title, via Hyprland's `hyprctl`. None
/// outside a Hyprland session — there's no portable "frontmost app" query:
/// X11 has `_NET_ACTIVE_WINDOW`, Wayland generally doesn't, and
/// wlr-foreign-toplevel (what another wlroots compositor could offer this
/// through) isn't wired up yet — or if `hyprctl` isn't on `PATH` or errors.
pub fn frontmost() -> Option<(String, String)> {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let out = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_activewindow(&String::from_utf8_lossy(&out.stdout))
}

/// Pulls `class` and `title` out of `hyprctl activewindow -j`'s JSON by
/// scanning for them rather than pulling in a JSON parser: good enough for
/// two known string fields in output we don't otherwise touch, and a miss
/// here only costs picking the wrong paste shortcut (`wants_shift_paste`).
fn parse_activewindow(json: &str) -> Option<(String, String)> {
    let class = json_string_field(json, "class")?;
    let title = json_string_field(json, "title").unwrap_or_default();
    Some((class, title))
}

/// The value of `"key": "value"` in `json`, with JSON's string escapes
/// decoded. Matching on `"key"` (with its quotes) keeps this from also
/// matching a field whose *name* merely contains `key`, e.g. the real
/// `activewindow` output's `"initialClass"` when looking for `"class"`.
fn json_string_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let after_key = json.split_once(needle.as_str())?.1;
    let after_colon = after_key.trim_start().strip_prefix(':')?.trim_start();
    let body = after_colon.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None // unterminated string: malformed input
}

/// Terminals whose own keybindings swallow a plain Ctrl+V (as a literal
/// control character, or for something else of their own) and use
/// Ctrl+Shift+V for paste instead. Matched case-insensitively and by
/// substring against the window class, which is a short name for some
/// ("kitty", "foot") and a reverse-DNS app id for others
/// ("com.mitchellh.ghostty", "org.wezfurlong.wezterm").
const SHIFT_PASTE_TERMINALS: &[&str] = &["ghostty", "kitty", "foot", "alacritty", "wezterm"];

fn is_shift_paste_terminal(class: &str) -> bool {
    let class = class.to_ascii_lowercase();
    SHIFT_PASTE_TERMINALS.iter().any(|t| class.contains(t))
}

/// Whether the focused app needs Ctrl+Shift+V instead of plain Ctrl+V to
/// paste (see `inject::mod`'s `shortcut`). False whenever `frontmost` can't
/// tell — same bias as `probe`: a wrong guess here just means a paste
/// keystroke that doesn't do anything, not a crash or lost text.
pub fn wants_shift_paste() -> bool {
    frontmost().is_some_and(|(class, _title)| is_shift_paste_terminal(&class))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured via `hyprctl activewindow -j` with Ghostty focused (trimmed
    // to the fields that matter here; the real output has more).
    const GHOSTTY_FIXTURE: &str = r#"{
    "address": "0x61eb693f76c0",
    "mapped": true,
    "workspace": {
        "id": 1,
        "name": "1"
    },
    "floating": false,
    "monitor": 0,
    "class": "com.mitchellh.ghostty",
    "title": "la-maquina: dotfiles",
    "initialClass": "com.mitchellh.ghostty",
    "initialTitle": "Ghostty",
    "pid": 17360,
    "xwayland": false
}"#;

    #[test]
    fn parses_the_real_hyprctl_fixture() {
        assert_eq!(
            parse_activewindow(GHOSTTY_FIXTURE),
            Some((
                "com.mitchellh.ghostty".to_string(),
                "la-maquina: dotfiles".to_string()
            ))
        );
    }

    #[test]
    fn decodes_json_escapes_in_the_title() {
        let json = r#"{"class": "firefox", "title": "Say \"hi\"\n\\o/"}"#;
        assert_eq!(
            parse_activewindow(json),
            Some(("firefox".to_string(), "Say \"hi\"\n\\o/".to_string()))
        );
    }

    #[test]
    fn missing_title_defaults_to_empty() {
        assert_eq!(
            parse_activewindow(r#"{"class": "kitty"}"#),
            Some(("kitty".to_string(), String::new()))
        );
    }

    #[test]
    fn missing_class_is_none() {
        assert_eq!(parse_activewindow(r#"{"title": "x"}"#), None);
    }

    #[test]
    fn a_class_like_field_name_does_not_match_the_class_key() {
        // Real hyprctl output always puts `initialClass` right next to
        // `class`; make sure we grab the latter, not a substring of the
        // former.
        let json = r#"{"initialClass": "wrong", "class": "right", "title": "t"}"#;
        assert_eq!(
            parse_activewindow(json),
            Some(("right".to_string(), "t".to_string()))
        );
    }

    #[test]
    fn known_terminal_classes_want_shift_paste() {
        for class in [
            "kitty",
            "foot",
            "Alacritty",
            "com.mitchellh.ghostty",
            "org.wezfurlong.wezterm",
            "WEZTERM",
        ] {
            assert!(is_shift_paste_terminal(class), "{class}");
        }
    }

    #[test]
    fn other_apps_do_not_want_shift_paste() {
        for class in ["firefox", "Code", "org.gnome.TextEditor", ""] {
            assert!(!is_shift_paste_terminal(class), "{class}");
        }
    }
}
