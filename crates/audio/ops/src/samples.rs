use crate::hanning_window;
use std::path::Path;

const HALF_TURN: f32 = std::f32::consts::PI;
const I16_MIN_AS_F32: f32 = i16::MIN as f32;
const I16_MAX_AS_F32: f32 = i16::MAX as f32;
const WAV_PEAK_MAGNITUDE: f32 = 32767.0;

/// Format metadata for a raw PCM sample stream: rate, channel count, and
/// per-sample byte width.
#[derive(Debug, Clone)]
pub struct AudioInfo {
    pub sample_rate: usize,
    pub num_channels: usize,
    pub sample_width: usize,
}

/// A newtype around a `Vec<f32>` PCM buffer (samples normally fall in
/// `[-1.0, 1.0]`), with the shaping operations (fades, filters, normalization)
/// applied before the buffer is encoded to WAV.
#[derive(Clone, Debug, Default)]
#[must_use]
pub struct AudioSamples(Vec<f32>);

impl AudioSamples {
    pub fn new(samples: Vec<f32>) -> Self {
        Self(samples)
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.0
    }

    pub fn as_vec(&self) -> &Vec<f32> {
        &self.0
    }

    pub fn as_mut_vec(&mut self) -> &mut Vec<f32> {
        &mut self.0
    }

    pub fn into_vec(self) -> Vec<f32> {
        self.0
    }

    pub fn take(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.0)
    }

    pub fn take_range(&mut self, sample_range: std::ops::Range<usize>) -> Vec<f32> {
        let clamped_end = sample_range.end.min(self.0.len());
        self.0.drain(sample_range.start..clamped_end).collect()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn to_i16_vec(&self) -> Vec<i16> {
        if self.0.is_empty() {
            return Vec::new();
        }
        let highest = self
            .0
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let lowest = self
            .0
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let peak_magnitude = highest.abs().max(lowest.abs()).max(f32::EPSILON);
        let gain = WAV_PEAK_MAGNITUDE / peak_magnitude;
        self.0
            .iter()
            .map(|&sample| (sample * gain).clamp(I16_MIN_AS_F32, I16_MAX_AS_F32) as i16)
            .collect()
    }

    pub fn as_wave_bytes(&self) -> Vec<u8> {
        self.to_i16_vec()
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect()
    }

    pub fn merge(&mut self, mut other: Self) {
        self.0.append(&mut other.0);
    }

    pub fn normalize(&mut self, max_value: f32) {
        if self.0.is_empty() {
            return;
        }
        let highest = self
            .0
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let lowest = self
            .0
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let peak_magnitude = highest.abs().max(lowest.abs());
        let divisor = peak_magnitude.max(max_value) / max_value.abs();
        self.0.iter_mut().for_each(|sample| *sample /= divisor);
    }

    pub fn apply_hanning_window(&mut self) -> Result<(), crate::AudioOpsError> {
        if self.0.is_empty() {
            return Ok(());
        }
        let window = hanning_window::get_hann_window(self.0.len())?;
        self.0
            .iter_mut()
            .zip(window)
            .for_each(|(sample, gain)| *sample *= gain);
        Ok(())
    }

    pub fn overlap_with(&mut self, other: &mut Self) {
        if !self.0.is_empty() {
            let tail_len = self.0.len();
            let overlap_len = tail_len.min(other.0.len());
            let ramp_span = 2.0 * overlap_len as f32;
            for offset in 0..overlap_len {
                let gain = (offset as f32 * HALF_TURN / ramp_span).sin();
                self.0[tail_len - 1 - offset] *= gain;
                other.0[offset] *= gain;
            }
        }
        self.0.append(&mut other.0);
    }

    pub fn fade_in(&mut self, fade_samples: usize) {
        let span = fade_samples.min(self.0.len());
        let span_f32 = span as f32;
        self.0
            .iter_mut()
            .take(span)
            .enumerate()
            .for_each(|(i, sample)| {
                *sample *= (i as f32 / span_f32 * HALF_TURN / 2.0).sin();
            });
    }

    pub fn fade_out(&mut self, fade_samples: usize) {
        let span = fade_samples.min(self.0.len());
        let span_f32 = span as f32;
        self.0
            .iter_mut()
            .rev()
            .take(span)
            .enumerate()
            .for_each(|(i, sample)| {
                *sample *= (i as f32 / span_f32 * HALF_TURN / 2.0).sin();
            });
    }

    pub fn crossfade(&mut self, fade_samples: usize) {
        let length = self.0.len();
        let span = fade_samples.min(length / 2);
        // A span under 2 samples has nothing to fade and would divide by
        // zero below (span - 1 would underflow at 0 or hit 0 at 1).
        if span < 2 {
            return;
        }
        let span_f32 = (span - 1) as f32;
        for i in 0..span {
            let gain = (i as f32 / span_f32 * HALF_TURN / 2.0).sin();
            self.0[i] *= gain;
            self.0[length - 1 - i] *= gain;
        }
    }

    pub fn lowpass_filter(&mut self, sample_range: std::ops::Range<usize>, cutoff: f32) {
        for i in sample_range {
            self.0[i] = if self.0[i] < cutoff { self.0[i] } else { 0.0 };
        }
    }

    pub fn highpass_filter(&mut self, sample_range: std::ops::Range<usize>, cutoff: f32) {
        for i in sample_range {
            self.0[i] = if self.0[i] > cutoff { self.0[i] } else { 0.0 };
        }
    }

    pub fn strip_silence(&mut self, sample_range: std::ops::Range<usize>) {
        let retained: Vec<f32> = self.0[sample_range.clone()]
            .iter()
            .copied()
            .filter(|&sample| sample > 0.0)
            .collect();
        self.0.splice(sample_range, retained);
    }

    pub fn to_decibel(&self) -> Vec<f32> {
        self.0
            .iter()
            .map(|sample| 20.0 * sample.abs().log10())
            .collect()
    }
}

impl From<AudioSamples> for Vec<f32> {
    fn from(samples: AudioSamples) -> Self {
        samples.into_vec()
    }
}

impl From<Vec<f32>> for AudioSamples {
    fn from(raw: Vec<f32>) -> Self {
        Self::new(raw)
    }
}

impl IntoIterator for AudioSamples {
    type Item = f32;
    type IntoIter = std::vec::IntoIter<f32>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// One decoded utterance: its sample buffer, format metadata, and (if known)
/// how long the model took to produce it.
#[derive(Debug, Clone)]
#[must_use]
pub struct Audio {
    pub samples: AudioSamples,
    pub info: AudioInfo,
    pub inference_ms: Option<f32>,
}

impl Audio {
    pub fn new(samples: AudioSamples, sample_rate: usize, inference_ms: Option<f32>) -> Self {
        Self {
            samples,
            info: AudioInfo {
                sample_rate,
                num_channels: 1,
                sample_width: 2,
            },
            inference_ms,
        }
    }

    pub fn into_vec(self) -> Vec<f32> {
        self.samples.into_vec()
    }

    pub fn as_wave_bytes(&self) -> Vec<u8> {
        self.samples.as_wave_bytes()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn duration_ms(&self) -> f32 {
        self.len() as f32 / self.info.sample_rate as f32 * 1000.0
    }

    pub fn inference_ms(&self) -> Option<f32> {
        self.inference_ms
    }

    pub fn real_time_factor(&self) -> Option<f32> {
        let inference_ms = self.inference_ms?;
        let duration_ms = self.duration_ms();
        if duration_ms == 0.0 {
            return Some(0.0);
        }
        Some(inference_ms / duration_ms)
    }

    pub fn save_to_file(&self, filename: &Path) -> Result<(), crate::WaveWriterError> {
        crate::write_wave_samples_to_file(
            filename,
            self.samples.to_i16_vec().iter(),
            self.info.sample_rate as u32,
            self.info.num_channels as u32,
            self.info.sample_width as u32,
        )
    }
}

impl IntoIterator for Audio {
    type Item = f32;
    type IntoIter = std::vec::IntoIter<f32>;

    fn into_iter(self) -> Self::IntoIter {
        self.samples.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fade_in() {
        let mut buffer = AudioSamples::from(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        buffer.fade_in(4);
        assert_eq!(buffer.as_slice()[0], 0.0);
    }

    #[test]
    fn test_fade_out() {
        let mut buffer = AudioSamples::from(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        buffer.fade_out(4);
        let last = buffer.len() - 1;
        assert_eq!(buffer.as_slice()[last], 0.0);
    }

    #[test]
    fn test_overlap() {
        let base = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut head = AudioSamples::from(base.clone());
        let mut tail = AudioSamples::from(base.clone());
        head.overlap_with(&mut tail);
        assert_eq!(head.len(), base.len() * 2);
        let merged = head.as_vec();
        assert_eq!(merged[7], 0.0);
        assert_eq!(merged[8], 0.0);
    }

    #[test]
    fn test_lowpass_filter() {
        let mut buffer = AudioSamples::from(vec![0.0, 0.1, 2.2, 0.0, 0.5, 0.0, 0.7, 0.0]);
        buffer.lowpass_filter(0..5, 0.5);
        let zeroed = buffer.into_iter().filter(|&sample| sample == 0.0).count();
        assert_eq!(zeroed, 6);
    }

    #[test]
    fn test_highpass_filter() {
        let mut buffer = AudioSamples::from(vec![0.0, 0.1, 2.2, 0.0, 0.5, 0.0, 0.7, 0.0]);
        let whole = 0..buffer.len();
        buffer.highpass_filter(whole, 0.5);
        let surviving = buffer.into_iter().filter(|&sample| sample != 0.0).count();
        assert_eq!(surviving, 2);
    }

    #[test]
    fn test_normalize() {
        let mut buffer = AudioSamples::from(vec![0.0, 0.1, 2.2, 0.0, 0.5, 0.0, 0.7, 0.0]);
        buffer.normalize(1.0);
        let peak = buffer
            .into_vec()
            .into_iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        assert_eq!(peak, 1.0);
    }

    #[test]
    fn normalize_scales_by_true_peak_magnitude_for_a_negative_skewed_buffer() {
        let mut buffer = AudioSamples::from(vec![-5.0, 0.1]);
        buffer.normalize(1.0);
        let peak_magnitude = buffer
            .into_vec()
            .into_iter()
            .map(f32::abs)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        assert_eq!(peak_magnitude, 1.0);
    }

    #[test]
    fn test_strip_silence() {
        let mut buffer = AudioSamples::from(vec![0.0, 0.1, 2.2, 0.0, 0.5, 0.0, 0.7, 0.0]);
        let whole = 0..buffer.len();
        buffer.strip_silence(whole);
        assert_eq!(buffer.len(), 4);
    }

    #[test]
    fn to_i16_vec_returns_empty_for_empty_samples() {
        let empty = AudioSamples::from(Vec::<f32>::new());
        assert_eq!(empty.to_i16_vec(), Vec::<i16>::new());
    }

    #[test]
    fn to_i16_vec_scales_all_zero_samples_without_dividing_by_zero() {
        let silence = AudioSamples::from(vec![0.0, 0.0, 0.0]);
        assert_eq!(silence.to_i16_vec(), vec![0, 0, 0]);
    }

    #[test]
    fn take_range_clamps_end_to_available_length() {
        let mut buffer = AudioSamples::from(vec![1.0, 2.0, 3.0]);
        let removed = buffer.take_range(1..100);
        assert_eq!(removed, vec![2.0, 3.0]);
        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn merge_appends_other_samples_in_order() {
        let mut first = AudioSamples::from(vec![1.0, 2.0]);
        let second = AudioSamples::from(vec![3.0, 4.0]);
        first.merge(second);
        assert_eq!(first.into_vec(), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn apply_hanning_window_tapers_first_sample_to_zero() {
        let mut buffer = AudioSamples::from(vec![1.0; 10]);
        buffer.apply_hanning_window().unwrap();
        let windowed = buffer.as_vec();
        assert_eq!(windowed[0], 0.0);
        assert!(windowed[5] > windowed[0]);
    }

    #[test]
    fn crossfade_attenuates_both_edges_symmetrically_and_leaves_the_middle_untouched() {
        let mut buffer = AudioSamples::from(vec![1.0; 10]);
        buffer.crossfade(4);
        let faded = buffer.as_vec();
        assert_eq!(faded[0], faded[9]);
        assert_eq!(faded[1], faded[8]);
        assert!(faded[0] < 1.0);
        assert_eq!(faded[4], 1.0);
        assert_eq!(faded[5], 1.0);
    }

    #[test]
    fn crossfade_clamps_fade_length_to_half_of_total_samples() {
        let mut buffer = AudioSamples::from(vec![1.0; 6]);
        buffer.crossfade(100);
        let faded = buffer.as_vec();
        assert_eq!(faded.len(), 6);
        assert!(faded.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn crossfade_is_a_noop_when_fade_length_resolves_below_two() {
        let unfaded = vec![1.0, 2.0, 3.0, 4.0];
        let mut buffer = AudioSamples::from(unfaded.clone());
        buffer.crossfade(1);
        assert_eq!(buffer.as_vec(), &unfaded);
    }

    #[test]
    fn crossfade_is_a_noop_for_zero_fade_samples() {
        let unfaded = vec![1.0, 2.0, 3.0, 4.0];
        let mut buffer = AudioSamples::from(unfaded.clone());
        buffer.crossfade(0);
        assert_eq!(buffer.as_vec(), &unfaded);
    }

    #[test]
    fn apply_hanning_window_on_empty_samples_is_a_noop() {
        let mut buffer = AudioSamples::from(Vec::<f32>::new());
        buffer.apply_hanning_window().unwrap();
        assert!(buffer.is_empty());
    }

    #[test]
    fn apply_hanning_window_errors_on_single_sample() {
        let mut buffer = AudioSamples::from(vec![1.0]);
        assert_eq!(
            buffer.apply_hanning_window(),
            Err(crate::AudioOpsError::InvalidWindowLength(1))
        );
    }

    #[test]
    fn to_decibel_converts_full_scale_amplitude_to_zero_db() {
        let buffer = AudioSamples::from(vec![1.0, 0.5]);
        let decibels = buffer.to_decibel();
        assert_eq!(decibels[0], 0.0);
        assert!(decibels[1] < 0.0);
    }

    #[test]
    fn to_decibel_of_zero_amplitude_is_negative_infinity() {
        let silence = AudioSamples::from(vec![0.0]);
        assert_eq!(silence.to_decibel()[0], f32::NEG_INFINITY);
    }

    #[test]
    fn real_time_factor_returns_none_without_inference_time() {
        let clip = Audio::new(AudioSamples::from(vec![0.0; 100]), 100, None);
        assert_eq!(clip.real_time_factor(), None);
    }

    #[test]
    fn real_time_factor_returns_zero_for_zero_duration_audio() {
        let clip = Audio::new(AudioSamples::from(Vec::new()), 100, Some(5.0));
        assert_eq!(clip.real_time_factor(), Some(0.0));
    }

    #[test]
    fn real_time_factor_divides_inference_ms_by_duration_ms() {
        // 100 samples at 100Hz span 1000ms; a 50ms inference gives rtf 0.05.
        let clip = Audio::new(AudioSamples::from(vec![0.0; 100]), 100, Some(50.0));
        assert_eq!(clip.real_time_factor(), Some(0.05));
    }

    #[test]
    fn crossfade_golden_values_for_a_known_input() {
        let mut buffer = AudioSamples::from(vec![1.0; 8]);
        buffer.crossfade(2);
        assert_eq!(
            buffer.as_vec(),
            &vec![0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]
        );
    }

    #[test]
    fn overlap_with_golden_values_for_a_known_input() {
        let mut head = AudioSamples::from(vec![1.0, 2.0]);
        let mut tail = AudioSamples::from(vec![3.0, 4.0]);
        head.overlap_with(&mut tail);
        let ramp = (std::f32::consts::PI / 4.0).sin();
        assert_eq!(head.as_vec(), &vec![ramp, 0.0, 0.0, 4.0 * ramp]);
    }

    #[test]
    fn to_i16_vec_golden_values_for_a_known_input() {
        let buffer = AudioSamples::from(vec![-1.0, 0.5, 1.0]);
        assert_eq!(buffer.to_i16_vec(), vec![-32767, 16383, 32767]);
    }
}
