//! Audio post-processing primitives shared across dengjen's TTS pipeline: sample buffers,
//! Hann windowing, and WAV encoding.
#![forbid(unsafe_code)]

pub(crate) mod hanning_window;
mod samples;
mod wave_writer;

pub use hanning_window::AudioOpsError;
pub use samples::{Audio, AudioInfo, AudioSamples};
pub use wave_writer::{write_wave_samples_to_buffer, write_wave_samples_to_file, WaveWriterError};
