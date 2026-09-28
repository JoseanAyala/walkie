use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub first_run: bool,
    pub language: String, // "auto" | "en" | "es" | ISO code
    pub model: String,    // key into config::models::REGISTRY
    pub hotkeys: Hotkeys,
    pub polish: Polish,
    pub inject: Inject,
    pub history: HistoryCfg,
    pub audio: AudioCfg,
    pub theme: ThemeCfg,
}

/// Each binding is the key names held together (see `hotkey::keys`); an
/// empty list disables that action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(from = "RawHotkeys")]
pub struct Hotkeys {
    pub dictate: Vec<String>,
    pub polish: Vec<String>,
    /// A one-shot tap, not a hold: re-inserts the most recent transcript.
    pub paste_last: Vec<String>,
}

/// Accepts the current format and the original one (`dictate = "RightAlt"`
/// plus `polish_modifier = "Shift"`), so existing configs keep their keys.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawHotkeys {
    dictate: Option<OneOrMany>,
    polish: Option<Vec<String>>,
    paste_last: Option<Vec<String>>,
    polish_modifier: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl From<RawHotkeys> for Hotkeys {
    fn from(r: RawHotkeys) -> Self {
        let d = Hotkeys::default();
        let legacy = matches!(r.dictate, Some(OneOrMany::One(_)));
        let dictate = match r.dictate {
            Some(OneOrMany::One(k)) => vec![k],
            Some(OneOrMany::Many(v)) => v,
            None => d.dictate.clone(),
        };
        let with = |extra: &str| {
            let mut v = dictate.clone();
            v.push(extra.to_string());
            v
        };
        let polish = r
            .polish
            .unwrap_or_else(|| match r.polish_modifier.as_deref() {
                Some(m) if m.eq_ignore_ascii_case("none") => vec![],
                Some(m) => with(m),
                None if legacy => with("Shift"),
                None => d.polish.clone(),
            });
        let paste_last = r.paste_last.unwrap_or(d.paste_last);
        Hotkeys {
            dictate,
            polish,
            paste_last,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(from = "RawPolish")]
pub struct Polish {
    pub provider: PolishProvider,
    /// How Apple's model writes (see `pipeline::style`).
    pub tone: Tone,
    /// For `PolishProvider::Command`; empty = disabled.
    pub command: String,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PolishProvider {
    /// A shell command of the user's: the transcript on stdin, stdout typed.
    #[default]
    Command,
    /// Opt-in: Apple's on-device model (macOS 26, Apple Intelligence), via
    /// walkie-ai.
    Apple,
}

/// How Apple's model writes. The prompts behind the tones are built in.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    /// Full capitalization and punctuation.
    #[default]
    Formal,
    /// Capitalized, lighter punctuation, no final period.
    Casual,
    /// All lowercase, lighter punctuation, no final period.
    VeryCasual,
    /// Exclamation marks where they fit.
    Excited,
}

/// Fills what's missing from the defaults, field by field (configs from
/// before providers have no `provider` or `tone`; their `prompt` is
/// dropped, as prompts are no longer editable).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawPolish {
    provider: PolishProvider,
    tone: Tone,
    command: String,
    timeout_secs: Option<u64>,
}

impl From<RawPolish> for Polish {
    fn from(r: RawPolish) -> Self {
        let d = Polish::default();
        Polish {
            provider: r.provider,
            tone: r.tone,
            command: r.command,
            timeout_secs: r.timeout_secs.unwrap_or(d.timeout_secs),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Inject {
    pub strategy: String, // "paste" | "type"
    pub restore_clipboard_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HistoryCfg {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioCfg {
    /// Lower (not mute) other audio while recording.
    pub duck_while_recording: bool,
    /// Volume during recording, as % of the volume before it.
    pub duck_percent: u32,
    /// Microphone name as `audio::input_device_names` reports it; empty =
    /// follow the system default.
    pub input_device: String,
}

/// The UI's look: one design, in light or dark. (Older configs also have
/// `name` and `custom` palettes here; they're ignored.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct ThemeCfg {
    pub appearance: Appearance,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    /// Follow macOS light/dark.
    #[default]
    System,
    Light,
    Dark,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            first_run: true,
            language: "auto".into(),
            model: "large-v3-turbo-q5_0".into(),
            hotkeys: Hotkeys::default(),
            polish: Polish::default(),
            inject: Inject::default(),
            history: HistoryCfg::default(),
            audio: AudioCfg::default(),
            theme: ThemeCfg::default(),
        }
    }
}
impl Default for Hotkeys {
    fn default() -> Self {
        let v = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect();
        Self {
            dictate: v(&["Fn"]),
            polish: v(&["Fn", "Shift"]),
            paste_last: v(&["Ctrl", "Cmd", "V"]),
        }
    }
}
impl Default for Polish {
    fn default() -> Self {
        Self {
            provider: PolishProvider::Command,
            tone: Tone::Formal,
            command: String::new(),
            timeout_secs: 60,
        }
    }
}
impl Default for Inject {
    fn default() -> Self {
        Self {
            strategy: "paste".into(),
            restore_clipboard_ms: 150,
        }
    }
}
impl Default for HistoryCfg {
    fn default() -> Self {
        Self { enabled: true }
    }
}
impl Default for AudioCfg {
    fn default() -> Self {
        Self {
            duck_while_recording: true,
            duck_percent: 70,
            input_device: String::new(),
        }
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
}
pub fn config_path() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("walkie/config.toml")
}
pub fn models_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("walkie/models")
}
pub fn spool_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("walkie/spool")
}
pub fn db_path() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("walkie/history.sqlite3")
}

/// walkie used to be called hearme. Moves each of its folders (config,
/// history, models) to the new name, once: only when the old folder exists
/// and the new one doesn't. Returns a line per folder moved or not movable.
pub fn migrate_from_hearme() -> Vec<String> {
    [
        xdg("XDG_CONFIG_HOME", ".config"),
        xdg("XDG_DATA_HOME", ".local/share"),
        xdg("XDG_CACHE_HOME", ".cache"),
    ]
    .iter()
    .filter_map(|root| rename_once(&root.join("hearme"), &root.join("walkie")))
    .collect()
}

fn rename_once(old: &Path, new: &Path) -> Option<String> {
    if !old.is_dir() || new.exists() {
        return None;
    }
    Some(match std::fs::rename(old, new) {
        Ok(()) => format!("moved {} → {}", old.display(), new.display()),
        Err(e) => format!("couldn't move {} → {}: {e}", old.display(), new.display()),
    })
}

impl Config {
    pub fn load() -> Result<Config> {
        Self::load_from(&config_path())
    }
    pub fn load_from(path: &Path) -> Result<Config> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let s = std::fs::read_to_string(path).with_context(|| format!("reading {path:?}"))?;
        toml::from_str(&s).with_context(|| format!("parsing {path:?}"))
    }
    pub fn save(&self) -> Result<()> {
        self.save_to(&config_path())
    }
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

pub mod models;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_spec_values() {
        let c = Config::default();
        assert!(c.first_run);
        assert_eq!(c.language, "auto");
        assert_eq!(c.model, "large-v3-turbo-q5_0");
        assert_eq!(c.hotkeys.dictate, ["Fn"]);
        assert_eq!(c.hotkeys.polish, ["Fn", "Shift"]);
        assert_eq!(c.hotkeys.paste_last, ["Ctrl", "Cmd", "V"]);
        assert_eq!(c.polish.provider, PolishProvider::Command);
        assert_eq!(c.polish.tone, Tone::Formal);
        assert_eq!(c.polish.command, "");
        assert_eq!(c.polish.timeout_secs, 60);
        assert_eq!(c.inject.strategy, "paste");
        assert_eq!(c.inject.restore_clipboard_ms, 150);
        assert!(c.history.enabled);
        assert_eq!(c.audio.input_device, "");
        assert!(c.audio.duck_while_recording);
        assert_eq!(c.audio.duck_percent, 70);
        assert_eq!(c.theme.appearance, Appearance::System);
    }

    #[test]
    fn toml_roundtrip() {
        let c = Config::default();
        let s = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn partial_toml_fills_defaults() {
        let back: Config = toml::from_str("language = \"es\"").unwrap();
        assert_eq!(back.language, "es");
        assert_eq!(back.model, "large-v3-turbo-q5_0");
    }

    #[test]
    fn save_load_roundtrip_in_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let c = Config {
            language: "en".into(),
            ..Default::default()
        };
        c.save_to(&path).unwrap();
        let back = Config::load_from(&path).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn load_missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let back = Config::load_from(&dir.path().join("nope.toml")).unwrap();
        assert_eq!(back, Config::default());
    }

    #[test]
    fn legacy_single_key_hotkeys_migrate() {
        let c: Config =
            toml::from_str("[hotkeys]\ndictate = \"RightCmd\"\npolish_modifier = \"Shift\"")
                .unwrap();
        assert_eq!(c.hotkeys.dictate, ["RightCmd"]);
        assert_eq!(c.hotkeys.polish, ["RightCmd", "Shift"]);
        assert_eq!(c.hotkeys.paste_last, Hotkeys::default().paste_last);
        let s = toml::to_string_pretty(&c).unwrap();
        assert!(
            !s.contains("polish_modifier"),
            "old field is dropped on save"
        );
    }

    #[test]
    fn polish_without_provider_keeps_using_its_command() {
        let c: Config = toml::from_str("[polish]\ncommand = \"claude -p hi\"").unwrap();
        assert_eq!(c.polish.provider, PolishProvider::Command);
        assert_eq!(c.polish.command, "claude -p hi");
        assert_eq!(c.polish.timeout_secs, 60);
        assert_eq!(c.polish.tone, Tone::Formal);
    }

    #[test]
    fn an_old_edited_prompt_gives_way_to_the_tone() {
        let c: Config = toml::from_str(
            "[polish]\nprovider = \"apple\"\nprompt = \"tidy\"\ntone = \"very_casual\"",
        )
        .unwrap();
        assert_eq!(c.polish.tone, Tone::VeryCasual);
        assert!(!toml::to_string_pretty(&c).unwrap().contains("prompt"));
    }

    #[test]
    fn apple_is_only_used_when_chosen() {
        let c: Config = toml::from_str("[polish]\ncommand = \"\"").unwrap();
        assert_eq!(c.polish.provider, PolishProvider::Command);
        let c: Config =
            toml::from_str("[polish]\nprovider = \"apple\"\ncommand = \"cat\"").unwrap();
        assert_eq!(c.polish.provider, PolishProvider::Apple);
    }

    #[test]
    fn legacy_polish_modifier_none_disables_polish() {
        let c: Config =
            toml::from_str("[hotkeys]\ndictate = \"RightAlt\"\npolish_modifier = \"None\"")
                .unwrap();
        assert!(c.hotkeys.polish.is_empty());
    }

    #[test]
    fn new_format_and_partial_hotkeys() {
        let c: Config = toml::from_str("[hotkeys]\ndictate = [\"Ctrl\", \"Opt\", \"D\"]").unwrap();
        assert_eq!(c.hotkeys.dictate, ["Ctrl", "Opt", "D"]);
        assert_eq!(c.hotkeys.polish, Hotkeys::default().polish);
        assert_eq!(c.hotkeys.paste_last, Hotkeys::default().paste_last);
    }

    #[test]
    fn paste_last_can_be_disabled_and_round_trips() {
        let c: Config = toml::from_str("[hotkeys]\npaste_last = []").unwrap();
        assert!(c.hotkeys.paste_last.is_empty());
        let back: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert!(
            back.hotkeys.paste_last.is_empty(),
            "disabled must stay disabled, not revert to default"
        );
    }

    #[test]
    fn audio_input_device_roundtrips() {
        let c: Config = toml::from_str("[audio]\ninput_device = \"Shure MV7\"").unwrap();
        assert_eq!(c.audio.input_device, "Shure MV7");
        let back: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(back.audio.input_device, "Shure MV7");
    }

    #[test]
    fn configs_with_the_old_palettes_still_load() {
        let c: Config = toml::from_str(
            "[theme]\nname = \"mine\"\nappearance = \"dark\"\n\
             [[theme.custom]]\nname = \"mine\"\nbase = \"#2b2a30\"\n\
             main = \"#7479d8\"\naccent = \"#e94b3c\"",
        )
        .unwrap();
        assert_eq!(c.theme.appearance, Appearance::Dark);
        let back: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(back.theme, c.theme);
    }

    #[test]
    fn xdg_paths_shape() {
        // Just shape-check the suffixes; env-dependent prefixes vary.
        assert!(config_path().ends_with("walkie/config.toml"));
        assert!(models_dir().ends_with("walkie/models"));
        assert!(spool_dir().ends_with("walkie/spool"));
        assert!(db_path().ends_with("walkie/history.sqlite3"));
    }
}
