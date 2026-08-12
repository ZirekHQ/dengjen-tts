use once_cell::sync::Lazy;
use std::{ collections::HashMap, f32::consts::PI};

#[derive(Debug, PartialEq)]
pub enum AudioOpsError {
    InvalidWindowLength(usize),
}

impl std::fmt::Display for AudioOpsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioOpsError::InvalidWindowLength(length) => {
                write!(f, "invalid window length: {}", length)
            }
        }
    }
}

impl std::error::Error for AudioOpsError {}

const PRECOMPUTED_HANN_WINDOW_LENGTHS: [usize; 7] = [64, 128, 256, 512, 1024, 2048, 4096];

static HANN_WINDOW_LOOKUP_TABLE: Lazy<HashMap<usize, Vec<f32>>> = Lazy::new(|| {
    PRECOMPUTED_HANN_WINDOW_LENGTHS
        .into_iter()
        .map(|length| (length, calculate_hann_window(length)))
        .collect()
});


/// Returns a Hann window of the given length, serving it from a precomputed lookup table
/// when available and computing it on demand otherwise.
///
/// Errors with `AudioOpsError::InvalidWindowLength` when `window_length` is `0` or `1`.
pub fn get_hann_window(window_length: usize) -> Result<Vec<f32>, AudioOpsError> {
    if window_length <= 1 {
        return Err(AudioOpsError::InvalidWindowLength(window_length));
    }
    if let Some(hann_window) = HANN_WINDOW_LOOKUP_TABLE.get(&window_length) {
        Ok(hann_window.clone())
    } else {
        Ok(calculate_hann_window(window_length))
    }
}

/// Computes a Hann window of length `window_length`: `w(n) = 0.5 - 0.5 * cos(2π * n / (N - 1))`
/// for `n` in `0..N`. See https://en.wikipedia.org/wiki/Window_function#Hann_and_Hamming_windows.
///
/// `w` is symmetric about the center (`w(n) == w(N - 1 - n)`), so only the first half is
/// evaluated with `cos`; the second half is filled by mirroring those values.
fn calculate_hann_window(window_length: usize) -> Vec<f32> {
    let mut window = vec![0.0_f32; window_length];
    let angular_step = 2.0 * PI / (window_length - 1) as f32;
    let midpoint = window_length.div_ceil(2);

    for n in 0..midpoint {
        let value = 0.5 - 0.5 * (angular_step * n as f32).cos();
        window[n] = value;
        window[window_length - 1 - n] = value;
    }

    window
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_hann_window_errors_on_zero_length() {
        assert_eq!(
            get_hann_window(0),
            Err(AudioOpsError::InvalidWindowLength(0))
        );
    }

    #[test]
    fn get_hann_window_errors_on_length_one() {
        assert_eq!(
            get_hann_window(1),
            Err(AudioOpsError::InvalidWindowLength(1))
        );
    }

    #[test]
    fn get_hann_window_starts_and_ends_at_zero_and_peaks_near_the_center() {
        let window = get_hann_window(64).unwrap();
        assert_eq!(window.len(), 64);
        assert_eq!(window[0], 0.0);
        assert_eq!(window[63], 0.0);
        let max = window.iter().cloned().fold(f32::MIN, f32::max);
        let max_index = window.iter().position(|&v| v == max).unwrap();
        assert!((28..36).contains(&max_index), "expected peak near center, got index {max_index}");
    }

    #[test]
    fn get_hann_window_lookup_table_matches_direct_computation() {
        // 64 is one of the precomputed lengths; verify the cached table entry
        // wasn't corrupted or stored under the wrong key.
        assert_eq!(get_hann_window(64).unwrap(), calculate_hann_window(64));
    }

    #[test]
    fn get_hann_window_computes_on_the_fly_for_a_length_not_in_the_lookup_table() {
        // 10 is not one of the precomputed lengths (64, 128, 256, 512, 1024, 2048, 4096).
        let window = get_hann_window(10).unwrap();
        assert_eq!(window.len(), 10);
        assert_eq!(window[0], 0.0);
        assert_eq!(window[9], 0.0);
    }
}

