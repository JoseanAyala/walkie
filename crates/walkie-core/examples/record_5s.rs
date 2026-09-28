//! Manual smoke test: records 5 seconds from the default mic and writes
//! a 16kHz mono WAV. Run: cargo run -p walkie-core --example record_5s

use walkie_core::audio::{Capture, CpalCapture};

fn main() -> anyhow::Result<()> {
    let mut cap = CpalCapture::new();
    println!("recording 5 seconds — speak now…");
    cap.start(Box::new(|lvl| print!("\rlevel: {:>6.3}", lvl)))?;
    std::thread::sleep(std::time::Duration::from_secs(5));
    let samples = cap.stop()?;
    println!(
        "\ncaptured {} samples ({:.1}s at 16kHz)",
        samples.len(),
        samples.len() as f32 / 16000.0
    );
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let path = "/tmp/walkie-record-test.wav";
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in &samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    w.finalize()?;
    println!("wrote {path} — play it with: afplay {path}");
    Ok(())
}
