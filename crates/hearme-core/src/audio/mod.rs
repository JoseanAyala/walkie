pub mod dsp;

use anyhow::{Context, Result};
use std::sync::{Arc, Mutex};

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
        Ok(dsp::resample_16k(&mono, self.rate))
    }
}
