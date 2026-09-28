pub mod whisper;

use anyhow::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum LangHint {
    Auto,
    Pinned(String),
}

impl LangHint {
    pub fn from_config(s: &str) -> Self {
        match s {
            "auto" | "" => LangHint::Auto,
            other => LangHint::Pinned(other.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub lang: Option<String>,
}

pub trait SttEngine {
    fn transcribe(&mut self, samples_16k: &[f32], lang: &LangHint) -> Result<Transcript>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_config_auto() {
        assert_eq!(LangHint::from_config("auto"), LangHint::Auto);
        assert_eq!(LangHint::from_config(""), LangHint::Auto);
    }

    #[test]
    fn from_config_pinned() {
        assert_eq!(
            LangHint::from_config("en"),
            LangHint::Pinned("en".to_string())
        );
        assert_eq!(
            LangHint::from_config("es"),
            LangHint::Pinned("es".to_string())
        );
    }
}
