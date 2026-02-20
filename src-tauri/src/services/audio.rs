use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

/// Compute the Root Mean Square (RMS) amplitude of a PCM sample buffer.
/// Returns 0.0 for an empty slice.
pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Resample mono f32 PCM audio to 16 kHz using sinc interpolation.
/// Returns the input unchanged if already at 16 kHz.
pub fn resample_to_16khz(samples: Vec<f32>, from_rate: u32) -> Vec<f32> {
    if from_rate == 16000 || samples.is_empty() {
        return samples;
    }
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedIn::<f32>::new(
        16000.0 / from_rate as f64,
        2.0,
        params,
        samples.len(),
        1,
    )
    .expect("failed to create resampler");

    let result = resampler
        .process(&[&samples], None)
        .expect("resampling failed");
    result.into_iter().next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_slice_returns_zero() {
        assert_eq!(compute_rms(&[]), 0.0);
    }

    #[test]
    fn silence_returns_zero() {
        assert_eq!(compute_rms(&[0.0, 0.0, 0.0, 0.0]), 0.0);
    }

    #[test]
    fn unit_signal() {
        // RMS of [1, -1, 1, -1] = sqrt((1+1+1+1)/4) = 1.0
        let rms = compute_rms(&[1.0, -1.0, 1.0, -1.0]);
        assert!((rms - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn single_sample() {
        let rms = compute_rms(&[0.5]);
        assert!((rms - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn realistic_mic_level() {
        // Typical quiet mic ambient: samples around 0.001
        let samples: Vec<f32> = vec![0.001; 512];
        let rms = compute_rms(&samples);
        assert!((rms - 0.001).abs() < 1e-6);
    }

    #[test]
    fn mixed_signal() {
        // RMS of [0.0, 0.4] = sqrt((0 + 0.16) / 2) = sqrt(0.08)
        let rms = compute_rms(&[0.0, 0.4]);
        let expected = (0.08f32).sqrt();
        assert!((rms - expected).abs() < 1e-6);
    }
}
