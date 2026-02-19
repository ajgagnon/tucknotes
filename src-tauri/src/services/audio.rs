/// Compute the Root Mean Square (RMS) amplitude of a PCM sample buffer.
/// Returns 0.0 for an empty slice.
pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
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
