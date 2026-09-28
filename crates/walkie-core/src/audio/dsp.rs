pub fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let ch = channels as usize;
    samples
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Linear-interpolation resampler. Speech-adequate; swap for a windowed-sinc
/// implementation only if STT accuracy ever measurably suffers.
pub fn resample_16k(mono: &[f32], rate: u32) -> Vec<f32> {
    if rate == 16_000 || mono.is_empty() {
        return mono.to_vec();
    }
    let ratio = rate as f64 / 16_000.0;
    let out_len = (mono.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let i0 = pos as usize;
            let frac = (pos - i0 as f64) as f32;
            let a = mono[i0.min(mono.len() - 1)];
            let b = *mono.get(i0 + 1).unwrap_or(&a);
            a + (b - a) * frac
        })
        .collect()
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_passthrough() {
        assert_eq!(to_mono(&[0.1, 0.2], 1), vec![0.1, 0.2]);
    }

    #[test]
    fn stereo_averages_pairs() {
        let out = to_mono(&[0.0, 1.0, 0.5, 0.5], 2);
        assert_eq!(out, vec![0.5, 0.5]);
    }

    #[test]
    fn resample_48k_halves_to_16k_length() {
        let input: Vec<f32> = (0..48000).map(|i| (i as f32 / 48000.0).sin()).collect();
        let out = resample_16k(&input, 48000);
        assert!((out.len() as i64 - 16000).abs() <= 1);
    }

    #[test]
    fn resample_16k_is_identity() {
        let input = vec![0.1, 0.2, 0.3];
        assert_eq!(resample_16k(&input, 16000), input);
    }

    #[test]
    fn rms_of_silence_is_zero_and_of_ones_is_one() {
        assert_eq!(rms(&[0.0; 100]), 0.0);
        assert!((rms(&[1.0; 100]) - 1.0).abs() < 1e-6);
        assert_eq!(rms(&[]), 0.0);
    }
}
