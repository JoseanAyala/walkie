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
    pub cleanup: Cleanup,
    pub polish: Polish,
    pub inject: Inject,
    pub history: HistoryCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Hotkeys {
    pub dictate: String,
    pub polish_modifier: String, // "Shift" | "None"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Cleanup {
    pub enabled: bool,
    pub fillers_en: Vec<String>,
    pub fillers_es: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Polish {
    pub command: String, // empty = disabled
    pub timeout_secs: u64,
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

impl Default for Config {
    fn default() -> Self {
        Self {
            first_run: true,
            language: "auto".into(),
            model: "large-v3-turbo-q5_0".into(),
            hotkeys: Hotkeys::default(),
            cleanup: Cleanup::default(),
            polish: Polish::default(),
            inject: Inject::default(),
            history: HistoryCfg::default(),
        }
    }
}
impl Default for Hotkeys {
    fn default() -> Self {
        Self { dictate: "RightAlt".into(), polish_modifier: "Shift".into() }
    }
}
impl Default for Cleanup {
    fn default() -> Self {
        Self {
            enabled: true,
            fillers_en: vec!["um".into(), "uh".into(), "you know".into()],
            fillers_es: vec!["este".into(), "eh".into(), "o sea".into()],
        }
    }
}
impl Default for Polish {
    fn default() -> Self {
        Self { command: String::new(), timeout_secs: 60 }
    }
}
impl Default for Inject {
    fn default() -> Self {
        Self { strategy: "paste".into(), restore_clipboard_ms: 150 }
    }
}
impl Default for HistoryCfg {
    fn default() -> Self {
        Self { enabled: true }
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}
fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
}
pub fn config_path() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("hearme/config.toml")
}
pub fn models_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("hearme/models")
}
pub fn spool_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("hearme/spool")
}
pub fn db_path() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("hearme/history.sqlite3")
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
        assert_eq!(c.hotkeys.dictate, "RightAlt");
        assert_eq!(c.hotkeys.polish_modifier, "Shift");
        assert!(c.cleanup.enabled);
        assert!(c.cleanup.fillers_en.contains(&"um".to_string()));
        assert!(c.cleanup.fillers_es.contains(&"este".to_string()));
        assert_eq!(c.polish.command, "");
        assert_eq!(c.polish.timeout_secs, 60);
        assert_eq!(c.inject.strategy, "paste");
        assert_eq!(c.inject.restore_clipboard_ms, 150);
        assert!(c.history.enabled);
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
        let mut c = Config::default();
        c.language = "en".into();
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
    fn xdg_paths_shape() {
        // Just shape-check the suffixes; env-dependent prefixes vary.
        assert!(config_path().ends_with("hearme/config.toml"));
        assert!(models_dir().ends_with("hearme/models"));
        assert!(spool_dir().ends_with("hearme/spool"));
        assert!(db_path().ends_with("hearme/history.sqlite3"));
    }
}
