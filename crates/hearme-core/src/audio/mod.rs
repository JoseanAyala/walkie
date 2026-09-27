pub mod dsp;

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// RMS below which a whole recording is treated as "no signal". Quiet room
/// speech sits well above this; a muted or disconnected input sits at 0.0.
const SILENCE_RMS: f32 = 1e-4;

/// Whether the most recent recording was near-silent (for the status page).
pub static LAST_CAPTURE_SILENT: AtomicBool = AtomicBool::new(false);

/// Name of the device a recording would use right now.
pub fn default_input_name() -> Option<String> {
    use cpal::traits::HostTrait;
    cpal::default_host().default_input_device().map(|d| d.to_string())
}

pub trait Capture {
    fn start(&mut self, on_level: Box<dyn Fn(f32) + Send>) -> Result<()>;
    fn stop(&mut self) -> Result<Vec<f32>>;
}

pub struct CpalCapture {
    stream: Option<cpal::Stream>,
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
    channels: u16,
}

impl CpalCapture {
    pub fn new() -> Self {
        Self { stream: None, buf: Arc::new(Mutex::new(Vec::new())), rate: 16_000, channels: 1 }
    }
}

impl Capture for CpalCapture {
    fn start(&mut self, on_level: Box<dyn Fn(f32) + Send>) -> Result<()> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let device = host.default_input_device().context("no input device — check mic permission")?;
        let cfg = device.default_input_config().context("no default input config")?;
        // Name the device we're actually recording from. macOS keeps whatever
        // was last set as the system default input, which can be a virtual
        // device (a DAW/mixer routing device, say) that is still selected long
        // after its hardware is unplugged. Those open cleanly and return pure
        // silence, so without this line a silent recording is indistinguishable
        // from a broken mic or a denied permission.
        eprintln!("hearme: recording from {device}");
        self.rate = cfg.sample_rate();
        self.channels = cfg.channels();
        let buf = self.buf.clone();
        buf.lock().unwrap().clear();
        let err_fn = |e| eprintln!("hearme audio error: {e}");
        let mut tick = 0usize;
        let stream = match cfg.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                cfg.config(),
                move |data: &[f32], _| {
                    buf.lock().unwrap().extend_from_slice(data);
                    tick += 1;
                    if tick % 4 == 0 {
                        on_level(dsp::rms(data));
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                cfg.config(),
                move |data: &[i16], _| {
                    let f: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                    buf.lock().unwrap().extend_from_slice(&f);
                    tick += 1;
                    if tick % 4 == 0 {
                        on_level(dsp::rms(&f));
                    }
                },
                err_fn,
                None,
            )?,
            other => anyhow::bail!("unsupported sample format: {other:?}"),
        };
        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) -> Result<Vec<f32>> {
        self.stream.take(); // dropping the stream stops capture
        let raw = std::mem::take(&mut *self.buf.lock().unwrap());
        let mono = dsp::to_mono(&raw, self.channels);
        // Whisper does not return "nothing" for silence — it hallucinates a
        // stock phrase ("Thank you.", "Thanks for watching!"), which reads as a
        // transcription bug rather than a dead input. Say so at the source.
        let silent = !mono.is_empty() && dsp::rms(&mono) < SILENCE_RMS;
        LAST_CAPTURE_SILENT.store(silent, Ordering::Relaxed);
        if silent {
            eprintln!(
                "hearme: captured {:.1}s of near-silence — check System Settings \
                 → Sound → Input, and that the mic permission is granted",
                mono.len() as f32 / self.rate as f32
            );
        }
        Ok(dsp::resample_16k(&mono, self.rate))
    }
}

/// A WAV file standing in for the microphone: every recording "hears" it.
/// Used by the e2e suites (in-process, and the real app via
/// `HEARME_TEST_AUDIO=/path.wav`).
pub struct FileCapture {
    samples: Vec<f32>,
}

impl FileCapture {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let mut r = hound::WavReader::open(path).with_context(|| format!("opening {path:?}"))?;
        let spec = r.spec();
        let raw: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
                r.samples::<i32>().map(|s| s.map(|v| v as f32 / scale)).collect::<Result<_, _>>()?
            }
            hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>()?,
        };
        let mono = dsp::to_mono(&raw, spec.channels);
        Ok(Self { samples: dsp::resample_16k(&mono, spec.sample_rate) })
    }
}

impl Capture for FileCapture {
    fn start(&mut self, on_level: Box<dyn Fn(f32) + Send>) -> Result<()> {
        on_level(dsp::rms(&self.samples));
        Ok(())
    }
    fn stop(&mut self) -> Result<Vec<f32>> {
        Ok(self.samples.clone())
    }
}

#[cfg(test)]
mod file_capture_tests {
    use super::*;

    #[test]
    fn fixture_wav_loads_as_16k_mono() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/en.wav");
        let mut c = FileCapture::open(&path).unwrap();
        c.start(Box::new(|_| {})).unwrap();
        let s = c.stop().unwrap();
        assert!((s.len() as f32 / 16_000.0 - 2.97).abs() < 0.05, "{} samples", s.len());
        assert!(dsp::rms(&s) > 0.001);
    }
}
