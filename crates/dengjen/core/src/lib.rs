#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt;

pub use dengjen_audio_ops::{Audio, AudioInfo, AudioSamples, WaveWriterError};

mod cancellation;
mod synthesis_config;

pub use cancellation::CancellationToken;
pub use synthesis_config::SynthesisConfig;

pub type DengjenResult<T> = Result<T, DengjenError>;
pub type DengjenAudioResult = DengjenResult<Audio>;
pub type AudioStreamIterator<'a> =
    Box<dyn Iterator<Item = DengjenResult<AudioSamples>> + Send + Sync + 'a>;

#[derive(Debug)]
pub enum DengjenError {
    FailedToLoadResource(String),
    PhonemizationError(String),
    InferenceError(String),
    InvalidConfiguration(String),
    UnsupportedOperation(String),
    OperationError(String),
}

impl DengjenError {
    pub fn with_message(message: impl Into<String>) -> Self {
        Self::OperationError(message.into())
    }
}

impl StdError for DengjenError {}

impl fmt::Display for DengjenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FailedToLoadResource(msg) => {
                write!(f, "Failed to load resource: {msg}")
            }
            Self::PhonemizationError(msg)
            | Self::InferenceError(msg)
            | Self::InvalidConfiguration(msg)
            | Self::UnsupportedOperation(msg)
            | Self::OperationError(msg) => write!(f, "{msg}"),
        }
    }
}

impl From<WaveWriterError> for DengjenError {
    fn from(error: WaveWriterError) -> Self {
        Self::OperationError(error.to_string())
    }
}

pub struct Phonemes(Vec<String>);

impl Phonemes {
    pub fn sentences(&self) -> &Vec<String> {
        &self.0
    }

    pub fn to_vec(self) -> Vec<String> {
        self.0
    }

    pub fn num_sentences(&self) -> usize {
        self.0.len()
    }
}

impl From<Vec<String>> for Phonemes {
    fn from(sentences: Vec<String>) -> Self {
        Self(sentences)
    }
}

impl fmt::Display for Phonemes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.join(" "))
    }
}

pub trait DengjenModel {
    fn audio_output_info(&self) -> DengjenResult<AudioInfo>;
    fn phonemize_text(&self, text: &str) -> DengjenResult<Phonemes>;
    fn speak_batch(&self, phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>>;
    fn speak_one_sentence(&self, phonemes: String) -> DengjenAudioResult;

    fn get_default_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>>;
    fn get_fallback_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>>;
    fn set_fallback_synthesis_config(
        &self,
        synthesis_config: &SynthesisConfig,
    ) -> DengjenResult<()>;

    fn get_language(&self) -> DengjenResult<Option<String>> {
        Ok(None)
    }
    fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
        Ok(None)
    }
    fn speaker_id_to_name(&self, sid: &i64) -> DengjenResult<Option<String>> {
        Ok(self
            .get_speakers()?
            .and_then(|speakers| speakers.get(sid).cloned()))
    }
    fn speaker_name_to_id(&self, name: &str) -> DengjenResult<Option<i64>> {
        Ok(self.get_speakers()?.and_then(|speakers| {
            speakers
                .iter()
                .find_map(|(sid, speaker_name)| (speaker_name == name).then_some(*sid))
        }))
    }
    fn properties(&self) -> DengjenResult<HashMap<String, String>> {
        Ok(HashMap::new())
    }

    fn supports_streaming_output(&self) -> bool {
        false
    }
    #[allow(unused_variables)]
    fn stream_synthesis(
        &self,
        phonemes: String,
        chunk_size: usize,
        chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<AudioStreamIterator<'_>> {
        Err(DengjenError::UnsupportedOperation(
            "Streaming synthesis is not supported for this model".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct NullModel {
        speakers: HashMap<i64, String>,
    }

    impl DengjenModel for NullModel {
        fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
            Ok(AudioInfo {
                sample_rate: 22050,
                num_channels: 1,
                sample_width: 2,
            })
        }
        fn phonemize_text(&self, _text: &str) -> DengjenResult<Phonemes> {
            Ok(Phonemes::from(Vec::new()))
        }
        fn speak_batch(&self, _phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
            Ok(Vec::new())
        }
        fn speak_one_sentence(&self, _phonemes: String) -> DengjenAudioResult {
            Err(DengjenError::OperationError("not implemented".to_string()))
        }
        fn get_default_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(None)
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(None)
        }
        fn set_fallback_synthesis_config(
            &self,
            _synthesis_config: &SynthesisConfig,
        ) -> DengjenResult<()> {
            Ok(())
        }
        fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
            Ok((!self.speakers.is_empty()).then_some(&self.speakers))
        }
    }

    #[test]
    fn null_model_required_methods_return_their_documented_defaults() {
        let model = NullModel::default();
        let info = model.audio_output_info().unwrap();
        assert_eq!(info.sample_rate, 22050);
        assert_eq!(info.num_channels, 1);
        assert_eq!(info.sample_width, 2);
        assert_eq!(model.phonemize_text("hi").unwrap().num_sentences(), 0);
        assert!(model.speak_batch(vec![]).unwrap().is_empty());
        assert!(model.speak_one_sentence("x".to_string()).is_err());
        assert_eq!(model.get_default_synthesis_config().unwrap(), None);
        assert_eq!(model.get_fallback_synthesis_config().unwrap(), None);
        assert!(model
            .set_fallback_synthesis_config(&SynthesisConfig::default())
            .is_ok());
    }

    #[test]
    fn with_message_wraps_a_plain_string_as_an_operation_error() {
        let err = DengjenError::with_message("boom");
        assert!(matches!(err, DengjenError::OperationError(msg) if msg == "boom"));
    }

    #[test]
    fn dengjen_error_from_wave_writer_error_carries_the_message_through() {
        let result = dengjen_audio_ops::write_wave_samples_to_file(
            std::path::Path::new("/dengjen-core-test-nonexistent-dir/out.wav"),
            [].iter(),
            22050,
            1,
            2,
        );
        let wave_err = result.unwrap_err();
        let expected_message = wave_err.to_string();
        let err: DengjenError = wave_err.into();
        assert!(matches!(err, DengjenError::OperationError(msg) if msg == expected_message));
    }

    #[test]
    fn phonemes_sentences_and_to_vec_expose_the_underlying_strings() {
        let phonemes = Phonemes::from(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(
            phonemes.sentences(),
            &vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(phonemes.num_sentences(), 2);
        assert_eq!(phonemes.to_vec(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn default_get_language_returns_none() {
        assert_eq!(NullModel::default().get_language().unwrap(), None);
    }

    #[test]
    fn default_properties_returns_an_empty_map() {
        assert!(NullModel::default().properties().unwrap().is_empty());
    }

    #[test]
    fn default_supports_streaming_output_is_false() {
        assert!(!NullModel::default().supports_streaming_output());
    }

    #[test]
    fn speaker_id_to_name_and_name_to_id_resolve_through_get_speakers_when_present() {
        let mut speakers = HashMap::new();
        speakers.insert(7i64, "alice".to_string());
        let model = NullModel { speakers };

        assert_eq!(
            model.speaker_id_to_name(&7).unwrap(),
            Some("alice".to_string())
        );
        assert_eq!(model.speaker_id_to_name(&8).unwrap(), None);
        assert_eq!(model.speaker_name_to_id("alice").unwrap(), Some(7));
        assert_eq!(model.speaker_name_to_id("bob").unwrap(), None);
    }

    #[test]
    fn error_display_formats_each_variant() {
        assert_eq!(
            DengjenError::FailedToLoadResource("disk full".to_string()).to_string(),
            "Failed to load resource: disk full"
        );
        assert_eq!(
            DengjenError::PhonemizationError("bad text".to_string()).to_string(),
            "bad text"
        );
        assert_eq!(
            DengjenError::InferenceError("model failed".to_string()).to_string(),
            "model failed"
        );
        assert_eq!(
            DengjenError::InvalidConfiguration("bad speaker id".to_string()).to_string(),
            "bad speaker id"
        );
        assert_eq!(
            DengjenError::UnsupportedOperation("not streamable".to_string()).to_string(),
            "not streamable"
        );
        assert_eq!(
            DengjenError::OperationError("boom".to_string()).to_string(),
            "boom"
        );
    }

    #[test]
    fn phonemes_display_joins_sentences_with_a_space() {
        let phonemes = Phonemes::from(vec!["hh ə l ˈoʊ".to_string(), "w ˈɜːld".to_string()]);
        assert_eq!(phonemes.to_string(), "hh ə l ˈoʊ w ˈɜːld");
    }

    #[test]
    fn phonemes_display_is_empty_string_for_no_sentences() {
        let phonemes = Phonemes::from(Vec::<String>::new());
        assert_eq!(phonemes.to_string(), "");
    }

    #[test]
    fn default_stream_synthesis_returns_unsupported_operation_error() {
        let model = NullModel::default();
        let result =
            model.stream_synthesis("phonemes".to_string(), 100, 3, CancellationToken::new());
        assert!(matches!(result, Err(DengjenError::UnsupportedOperation(_))));
    }

    #[test]
    fn default_speaker_id_to_name_returns_none_without_speakers() {
        assert_eq!(NullModel::default().speaker_id_to_name(&0).unwrap(), None);
    }

    #[test]
    fn default_speaker_name_to_id_returns_none_without_speakers() {
        assert_eq!(
            NullModel::default().speaker_name_to_id("foo").unwrap(),
            None
        );
    }
}
