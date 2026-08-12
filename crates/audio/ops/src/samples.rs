use crate::hanning_window;
use std::path::Path;

const PI: f32 = std::f32::consts::PI;
const I16MIN_F32: f32 = i16::MIN as f32;
const I16MAX_F32: f32 = i16::MAX as f32;
const MAX_WAV_VALUE_I16: f32 = 32767.0;

/// Playback-relevant metadata describing a raw PCM sample stream.
#[derive(Debug, Clone)]
pub struct AudioInfo {
    pub sample_rate: usize,
    pub num_channels: usize,
    pub sample_width: usize,
}

/// A buffer of `f32` PCM samples, typically normalized to `[-1.0, 1.0]`, plus the
/// operations (fades, filters, normalization) used to shape it before encoding to WAV.
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
        let end = sample_range.end.min(self.len());
        self.0.drain(sample_range.start..end).collect()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn to_i16_vec(&self) -> Vec<i16> {
        if self.is_empty() {
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
        let peak = highest.abs().max(lowest.abs()).max(f32::EPSILON);
        let scale = MAX_WAV_VALUE_I16 / peak;
        self.0
            .iter()
            .map(|sample| (sample * scale).clamp(I16MIN_F32, I16MAX_F32) as i16)
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
        if self.is_empty() {
            return;
        }
        let largest = self
            .0
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap()
            .abs();
        let factor = largest.max(max_value) / max_value.abs();
        for sample in self.0.iter_mut() {
            *sample /= factor;
        }
    }
    pub fn apply_hanning_window(&mut self) -> Result<(), crate::AudioOpsError> {
        if self.is_empty() {
            return Ok(());
        }
        let window = hanning_window::get_hann_window(self.0.len())?;
        for (sample, ratio) in self.0.iter_mut().zip(window) {
            *sample *= ratio;
        }
        Ok(())
    }
    pub fn overlap_with(&mut self, other: &mut Self) {
        if !self.is_empty() {
            let self_len = self.0.len();
            let overlap = self_len.min(other.0.len());
            let span = 2.0 * overlap as f32;
            for t in 0..overlap {
                let ratio = (t as f32 * PI / span).sin();
                self.0[self_len - 1 - t] *= ratio;
                other.0[t] *= ratio;
            }
        }
        self.0.append(&mut other.0);
    }
    pub fn fade_in(&mut self, fade_samples: usize) {
        let span = fade_samples.min(self.len());
        let span_f32 = span as f32;
        for i in 0..span {
            let ratio = (i as f32 / span_f32 * PI / 2.0).sin();
            self.0[i] *= ratio;
        }
    }

    pub fn fade_out(&mut self, fade_samples: usize) {
        let length = self.len();
        let span = fade_samples.min(length);
        let span_f32 = span as f32;
        for i in 0..span {
            let ratio = (i as f32 / span_f32 * PI / 2.0).sin();
            self.0[length - 1 - i] *= ratio;
        }
    }
    pub fn crossfade(&mut self, fade_samples: usize) {
        let length = self.len();
        let span = fade_samples.min(length / 2);
        // span - 1 underflows at 0 and divides by zero at 1 — nothing to fade, leave untouched.
        if span < 2 {
            return;
        }
        let span_f32 = (span - 1) as f32;
        for i in 0..span {
            let ratio = (i as f32 / span_f32 * PI / 2.0).sin();
            self.0[i] *= ratio;
            self.0[length - 1 - i] *= ratio;
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
        let kept: Vec<f32> = self.0[sample_range.clone()]
            .iter()
            .copied()
            .filter(|&f| f > 0.0)
            .collect();
        self.0.splice(sample_range, kept);
    }

    pub fn to_decibel(&self) -> Vec<f32> {
        self.0.iter().map(|x| 20.0 * x.abs().log10()).collect()
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

/// A complete decoded utterance: the sample buffer plus format metadata and
/// (optionally) how long the model took to produce it.
#[derive(Debug, Clone)]
#[must_use]
pub struct Audio {
    pub samples: AudioSamples,
    pub info: AudioInfo,
    pub inference_ms: Option<f32>,
}

impl Audio {
    pub fn new(samples: AudioSamples, sample_rate: usize, inference_ms: Option<f32>) -> Self {
        let info = AudioInfo {
            sample_rate,
            num_channels: 1,
            sample_width: 2,
        };
        Self {
            samples,
            info,
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
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut s1 = AudioSamples::from(data.clone());
        s1.fade_in(4);
        assert_eq!(s1.0[0], 0.0);
    }

    #[test]
    fn test_fade_out() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut s1 = AudioSamples::from(data.clone());
        s1.fade_out(4);
        assert_eq!(s1.0[7], 0.0);
    }

    #[test]
    fn test_overlap() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut s1 = AudioSamples::from(data.clone());
        let mut s2 = AudioSamples::from(data.clone());
        s1.overlap_with(&mut s2);
        assert_eq!(s1.len(), data.len() * 2);
        let rs = s1.as_vec();
        assert_eq!(rs[7], 0.0);
        assert_eq!(rs[8], 0.0);
    }

    #[test]
    fn test_lowpass_filter() {
        let data = vec![0.0, 0.1, 2.2, 0.0, 0.5, 0.0, 0.7, 0.0];
        let mut s1 = AudioSamples::from(data.clone());
        s1.lowpass_filter(0..5, 0.5);
        assert_eq!(s1.into_iter().filter(|f| *f == 0.0).count(), 6);
    }

    #[test]
    fn test_highpass_filter() {
        let data = vec![0.0, 0.1, 2.2, 0.0, 0.5, 0.0, 0.7, 0.0];
        let mut s1 = AudioSamples::from(data.clone());
        s1.highpass_filter(0..s1.len(), 0.5);
        assert_eq!(s1.into_iter().filter(|f| *f != 0.0).count(), 2);
    }

    #[test]
    fn test_normalize() {
        let data = vec![0.0, 0.1, 2.2, 0.0, 0.5, 0.0, 0.7, 0.0];
        let mut s1 = AudioSamples::from(data.clone());
        s1.normalize(1.0);
        assert_eq!(
            s1.0.into_iter()
                .max_by(|x, y| x.partial_cmp(y).unwrap())
                .unwrap(),
            1.0
        );
    }

    #[test]
    fn test_strip_silence() {
        let data = vec![0.0, 0.1, 2.2, 0.0, 0.5, 0.0, 0.7, 0.0];
        let mut s1 = AudioSamples::from(data.clone());
        s1.strip_silence(0..s1.len());
        assert_eq!(s1.len(), 4);
    }

    #[test]
    fn to_i16_vec_returns_empty_for_empty_samples() {
        let samples = AudioSamples::from(Vec::<f32>::new());
        assert_eq!(samples.to_i16_vec(), Vec::<i16>::new());
    }

    #[test]
    fn to_i16_vec_scales_all_zero_samples_without_dividing_by_zero() {
        let samples = AudioSamples::from(vec![0.0, 0.0, 0.0]);
        assert_eq!(samples.to_i16_vec(), vec![0, 0, 0]);
    }

    #[test]
    fn take_range_clamps_end_to_available_length() {
        let mut samples = AudioSamples::from(vec![1.0, 2.0, 3.0]);
        let taken = samples.take_range(1..100);
        assert_eq!(taken, vec![2.0, 3.0]);
        assert_eq!(samples.len(), 1);
    }

    #[test]
    fn merge_appends_other_samples_in_order() {
        let mut a = AudioSamples::from(vec![1.0, 2.0]);
        let b = AudioSamples::from(vec![3.0, 4.0]);
        a.merge(b);
        assert_eq!(a.into_vec(), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn apply_hanning_window_tapers_first_sample_to_zero() {
        let mut samples = AudioSamples::from(vec![1.0; 10]);
        samples.apply_hanning_window().unwrap();
        let v = samples.as_vec();
        assert_eq!(v[0], 0.0);
        assert!(v[5] > v[0]);
    }

    #[test]
    fn crossfade_attenuates_both_edges_symmetrically_and_leaves_the_middle_untouched() {
        let mut samples = AudioSamples::from(vec![1.0; 10]);
        samples.crossfade(4);
        let v = samples.as_vec();
        assert_eq!(v[0], v[9]);
        assert_eq!(v[1], v[8]);
        assert!(v[0] < 1.0);
        assert_eq!(v[4], 1.0);
        assert_eq!(v[5], 1.0);
    }

    #[test]
    fn crossfade_clamps_fade_length_to_half_of_total_samples() {
        let mut samples = AudioSamples::from(vec![1.0; 6]);
        samples.crossfade(100);
        let v = samples.as_vec();
        assert_eq!(v.len(), 6);
        assert!(v.iter().all(|f| f.is_finite()));
    }

    #[test]
    fn crossfade_is_a_noop_when_fade_length_resolves_below_two() {
        let original = vec![1.0, 2.0, 3.0, 4.0];
        let mut samples = AudioSamples::from(original.clone());
        samples.crossfade(1);
        assert_eq!(samples.as_vec(), &original);
    }

    #[test]
    fn crossfade_is_a_noop_for_zero_fade_samples() {
        let original = vec![1.0, 2.0, 3.0, 4.0];
        let mut samples = AudioSamples::from(original.clone());
        samples.crossfade(0);
        assert_eq!(samples.as_vec(), &original);
    }

    #[test]
    fn apply_hanning_window_on_empty_samples_is_a_noop() {
        let mut samples = AudioSamples::from(Vec::<f32>::new());
        samples.apply_hanning_window().unwrap();
        assert!(samples.is_empty());
    }

    #[test]
    fn apply_hanning_window_errors_on_single_sample() {
        let mut samples = AudioSamples::from(vec![1.0]);
        assert_eq!(
            samples.apply_hanning_window(),
            Err(crate::AudioOpsError::InvalidWindowLength(1))
        );
    }

    #[test]
    fn to_decibel_converts_full_scale_amplitude_to_zero_db() {
        let samples = AudioSamples::from(vec![1.0, 0.5]);
        let db = samples.to_decibel();
        assert_eq!(db[0], 0.0);
        assert!(db[1] < 0.0);
    }

    #[test]
    fn to_decibel_of_zero_amplitude_is_negative_infinity() {
        let samples = AudioSamples::from(vec![0.0]);
        assert_eq!(samples.to_decibel()[0], f32::NEG_INFINITY);
    }

    #[test]
    fn real_time_factor_returns_none_without_inference_time() {
        let audio = Audio::new(AudioSamples::from(vec![0.0; 100]), 100, None);
        assert_eq!(audio.real_time_factor(), None);
    }

    #[test]
    fn real_time_factor_returns_zero_for_zero_duration_audio() {
        let audio = Audio::new(AudioSamples::from(Vec::new()), 100, Some(5.0));
        assert_eq!(audio.real_time_factor(), Some(0.0));
    }

    #[test]
    fn real_time_factor_divides_inference_ms_by_duration_ms() {
        // 100 samples @ 100Hz = 1000ms duration; 50ms inference => rtf 0.05
        let audio = Audio::new(AudioSamples::from(vec![0.0; 100]), 100, Some(50.0));
        assert_eq!(audio.real_time_factor(), Some(0.05));
    }

    #[test]
    fn crossfade_golden_values_for_a_known_input() {
        let mut samples = AudioSamples::from(vec![1.0; 8]);
        samples.crossfade(2);
        assert_eq!(
            samples.as_vec(),
            &vec![0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]
        );
    }

    #[test]
    fn overlap_with_golden_values_for_a_known_input() {
        let mut s1 = AudioSamples::from(vec![1.0, 2.0]);
        let mut s2 = AudioSamples::from(vec![3.0, 4.0]);
        s1.overlap_with(&mut s2);
        let r = (std::f32::consts::PI / 4.0).sin();
        assert_eq!(s1.as_vec(), &vec![r, 0.0, 0.0, 4.0 * r]);
    }

    #[test]
    fn to_i16_vec_golden_values_for_a_known_input() {
        let samples = AudioSamples::from(vec![-1.0, 0.5, 1.0]);
        assert_eq!(samples.to_i16_vec(), vec![-32767, 16383, 32767]);
    }
}
