use super::{LangHint, SttEngine, Transcript};
use anyhow::{Context, Result};
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct WhisperEngine {
    ctx: WhisperContext,
}

impl WhisperEngine {
    pub fn load(model: &Path) -> Result<Self> {
        let ctx = WhisperContext::new_with_params(
            model,
            WhisperContextParameters::default(), // GPU (Metal) on by default
        )
        .context("loading whisper model")?;
        Ok(Self { ctx })
    }
}

impl SttEngine for WhisperEngine {
    fn transcribe(&mut self, samples_16k: &[f32], lang: &LangHint) -> Result<Transcript> {
        // whisper.cpp behaves poorly on sub-second clips; pad with silence.
        let mut samples = samples_16k.to_vec();
        if samples.len() < 17_600 {
            samples.resize(17_600, 0.0); // 1.1s
        }

        let mut state = self.ctx.create_state().context("whisper state")?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let pinned;
        match lang {
            LangHint::Auto => params.set_language(Some("auto")),
            LangHint::Pinned(l) => {
                pinned = l.clone();
                params.set_language(Some(&pinned));
            }
        }
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_n_threads(
            std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4),
        );

        state.full(params, &samples).context("whisper full()")?;

        // API drift (whisper-rs 0.16.0): full_n_segments()/full_lang_id_from_state()
        // return plain c_int (not Result), and per-segment text is fetched via
        // get_segment(i).to_str() rather than a full_get_segment_text(i) method
        // on WhisperState.
        let n = state.full_n_segments();
        let mut text = String::new();
        for i in 0..n {
            if let Some(seg) = state.get_segment(i) {
                let seg_text = seg.to_str().context("segment text")?;
                text.push_str(seg_text.trim());
                text.push(' ');
            }
        }
        let lang_id = state.full_lang_id_from_state();
        let lang = whisper_rs::get_lang_str(lang_id).map(|s| s.to_string());

        Ok(Transcript { text: text.trim().to_string(), lang })
    }
}
