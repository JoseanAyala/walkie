pub mod dsp;
pub mod duck;

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// RMS below which a whole recording is treated as "no signal". Quiet room
/// speech sits well above this; a muted or disconnected input sits at 0.0.
const SILENCE_RMS: f32 = 1e-4;

/// Whether the most recent recording was near-silent (for the status page).
pub static LAST_CAPTURE_SILENT: AtomicBool = AtomicBool::new(false);

/// Name of the system default input device.
pub fn default_input_name() -> Option<String> {
    use cpal::traits::HostTrait;
    cpal::default_host()
        .default_input_device()
        .map(|d| d.to_string())
}

/// Names of every input device currently connected.
pub fn input_device_names() -> Vec<String> {
    use cpal::traits::HostTrait;
    cpal::default_host()
        .input_devices()
        .map(|ds| ds.map(|d| d.to_string()).collect())
        .unwrap_or_default()
}

/// The microphone chosen in settings (`[audio] input_device`); empty means
/// follow the system default. Read at every recording start, so a settings
/// change or a plugged-in device applies to the next dictation.
static PREFERRED_INPUT: Mutex<String> = Mutex::new(String::new());

pub fn set_preferred_input(name: &str) {
    *PREFERRED_INPUT.lock().unwrap() = name.to_string();
}

pub fn preferred_input() -> String {
    PREFERRED_INPUT.lock().unwrap().clone()
}

#[derive(Debug, PartialEq)]
pub enum Resolved {
    /// No preference: use the system default.
    Default,
    /// The preferred device, at this index of the available list.
    Chosen(usize),
    /// The preferred device isn't connected: fall back to the system default.
    Missing,
}

pub fn resolve(preferred: &str, available: &[String]) -> Resolved {
    if preferred.is_empty() {
        return Resolved::Default;
    }
    match available.iter().position(|n| n == preferred) {
        Some(i) => Resolved::Chosen(i),
        None => Resolved::Missing,
    }
}

/// The device a recording would use right now, plus the preferred device's
/// name when it had to be passed over because it isn't connected.
fn pick_input() -> (Option<cpal::Device>, Option<String>) {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    let preferred = preferred_input();
    if preferred.is_empty() {
        return (host.default_input_device(), None);
    }
    let mut devices: Vec<cpal::Device> = host
        .input_devices()
        .map(|ds| ds.collect())
        .unwrap_or_default();
    let names: Vec<String> = devices.iter().map(|d| d.to_string()).collect();
    match resolve(&preferred, &names) {
        Resolved::Chosen(i) => (Some(devices.swap_remove(i)), None),
        Resolved::Default => (host.default_input_device(), None),
        Resolved::Missing => (host.default_input_device(), Some(preferred)),
    }
}

/// For the status page: (name of the device a recording would use, the
/// preferred device if it isn't connected).
pub fn current_input() -> (Option<String>, Option<String>) {
    let (d, missing) = pick_input();
    (d.map(|d| d.to_string()), missing)
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

impl Default for CpalCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl CpalCapture {
    pub fn new() -> Self {
        Self {
            stream: None,
            buf: Arc::new(Mutex::new(Vec::new())),
            rate: 16_000,
            channels: 1,
        }
    }
}

impl Capture for CpalCapture {
    fn start(&mut self, on_level: Box<dyn Fn(f32) + Send>) -> Result<()> {
        use cpal::traits::{DeviceTrait, StreamTrait};
        let (device, missing) = pick_input();
        if let Some(m) = missing {
            eprintln!("walkie: warning: chosen microphone {m:?} isn't connected, using the system default");
        }
        let device = device.context("no input device — check mic permission")?;
        let cfg = device
            .default_input_config()
            .context("no default input config")?;
        // Name the device we're actually recording from. macOS keeps whatever
        // was last set as the system default input, which can be a virtual
        // device (a DAW/mixer routing device, say) that is still selected long
        // after its hardware is unplugged. Those open cleanly and return pure
        // silence, so without this line a silent recording is indistinguishable
        // from a broken mic or a denied permission.
        eprintln!("walkie: recording from {device}");
        self.rate = cfg.sample_rate();
        self.channels = cfg.channels();
        let buf = self.buf.clone();
        buf.lock().unwrap().clear();
        let err_fn = |e| eprintln!("walkie audio error: {e}");
        let mut tick = 0usize;
        let stream = match cfg.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                cfg.config(),
                move |data: &[f32], _| {
                    buf.lock().unwrap().extend_from_slice(data);
                    tick += 1;
                    if tick.is_multiple_of(4) {
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
                    if tick.is_multiple_of(4) {
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
                "walkie: captured {:.1}s of near-silence — check System Settings \
                 → Sound → Input, and that the mic permission is granted",
                mono.len() as f32 / self.rate as f32
            );
        }
        Ok(dsp::resample_16k(&mono, self.rate))
    }
}

/// A WAV file standing in for the microphone: every recording "hears" it.
/// Used by the e2e suites (in-process, and the real app via
/// `WALKIE_TEST_AUDIO=/path.wav`).
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
                r.samples::<i32>()
                    .map(|s| s.map(|v| v as f32 / scale))
                    .collect::<Result<_, _>>()?
            }
            hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>()?,
        };
        let mono = dsp::to_mono(&raw, spec.channels);
        Ok(Self {
            samples: dsp::resample_16k(&mono, spec.sample_rate),
        })
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
mod resolve_tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_preference_follows_the_system_default() {
        assert_eq!(
            resolve("", &names(&["MacBook Pro Microphone"])),
            Resolved::Default
        );
        assert_eq!(resolve("", &[]), Resolved::Default);
    }

    #[test]
    fn connected_preference_is_chosen() {
        let avail = names(&["MacBook Pro Microphone", "Shure MV7"]);
        assert_eq!(resolve("Shure MV7", &avail), Resolved::Chosen(1));
    }

    #[test]
    fn unplugged_preference_falls_back() {
        assert_eq!(
            resolve("Shure MV7", &names(&["MacBook Pro Microphone"])),
            Resolved::Missing
        );
        assert_eq!(resolve("Shure MV7", &[]), Resolved::Missing);
    }

    #[test]
    fn names_match_exactly() {
        let avail = names(&["Shure MV7 (2)"]);
        assert_eq!(resolve("Shure MV7", &avail), Resolved::Missing);
        assert_eq!(resolve("shure mv7 (2)", &avail), Resolved::Missing);
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
        assert!(
            (s.len() as f32 / 16_000.0 - 2.97).abs() < 0.05,
            "{} samples",
            s.len()
        );
        assert!(dsp::rms(&s) > 0.001);
    }
}
