use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::path::PathBuf;

pub struct ModelInfo {
    pub key: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    /// Approximate size — only a progress-display fallback when the server
    /// sends no Content-Length. Not used for verification.
    pub approx_bytes: u64,
}

pub const REGISTRY: &[ModelInfo] = &[
    ModelInfo {
        key: "base",
        file: "ggml-base.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        approx_bytes: 148_000_000,
    },
    ModelInfo {
        key: "large-v3-turbo-q5_0",
        file: "ggml-large-v3-turbo-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        approx_bytes: 574_000_000,
    },
];

pub fn find(key: &str) -> Option<&'static ModelInfo> {
    REGISTRY.iter().find(|m| m.key == key)
}

pub fn model_path(key: &str) -> Option<PathBuf> {
    find(key).map(|m| super::models_dir().join(m.file))
}

pub fn is_downloaded(key: &str) -> bool {
    model_path(key).map(|p| p.exists()).unwrap_or(false)
}

pub fn download(key: &str, progress: &mut dyn FnMut(u64, u64)) -> Result<PathBuf> {
    let info = find(key).with_context(|| format!("unknown model key: {key}"))?;
    let dest = model_path(key).unwrap();
    std::fs::create_dir_all(dest.parent().unwrap())?;
    let part = dest.with_extension("part");

    let resp = ureq::get(info.url).call().context("model download request failed")?;
    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(info.approx_bytes);

    let mut reader = resp.into_reader();
    let mut out = std::fs::File::create(&part)?;
    let mut done: u64 = 0;
    let mut chunk = [0u8; 65536];
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        out.write_all(&chunk[..n])?;
        done += n as u64;
        progress(done, total);
    }
    out.flush()?;
    drop(out);
    std::fs::rename(&part, &dest)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_both_models() {
        assert!(find("base").is_some());
        assert!(find("large-v3-turbo-q5_0").is_some());
        assert!(find("nope").is_none());
    }

    #[test]
    fn file_names_are_exact() {
        assert_eq!(find("base").unwrap().file, "ggml-base.bin");
        assert_eq!(find("large-v3-turbo-q5_0").unwrap().file, "ggml-large-v3-turbo-q5_0.bin");
    }

    #[test]
    fn model_path_is_under_models_dir() {
        let p = model_path("base").unwrap();
        assert!(p.ends_with("hearme/models/ggml-base.bin"));
    }
}
