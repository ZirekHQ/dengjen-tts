use std::any::Any;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;


pub use audio_ops::{
    Audio,
    AudioInfo,
    AudioSamples,
    WaveWriterError
};


pub type DengjenResult<T> = Result<T, DengjenError>;
pub type DengjenAudioResult = DengjenResult<Audio>;
pub type AudioStreamIterator<'a> = Box<dyn Iterator<Item = DengjenResult<AudioSamples>> + Send + Sync + 'a>;

#[derive(Debug)]
pub enum DengjenError {
    FailedToLoadResource(String),
    PhonemizationError(String),
    OperationError(String),
}

impl DengjenError {
    pub fn with_message(message: impl Into<String>) -> Self {
        Self::OperationError(message.into())
    }
}
impl Error for DengjenError {}

impl fmt::Display for DengjenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let err_message = match self {
            DengjenError::FailedToLoadResource(msg) => {
                format!("Failed to load resource from. Error `{}`", msg)
            }
            DengjenError::PhonemizationError(msg) => msg.to_string(),
            DengjenError::OperationError(msg) => msg.to_string(),
        };
        write!(f, "{}", err_message)
    }
}

impl From<WaveWriterError> for DengjenError {
    fn from(error: WaveWriterError) -> Self {
        DengjenError::OperationError(error.to_string())
    }
}

/// A wrapper type that holds sentence phonemes
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
    fn from(other: Vec<String>) -> Self {
        Self(other)
    }
}

impl std::fmt::Display for Phonemes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join(" "))
    }
}


pub trait DengjenModel {
    fn audio_output_info(&self) -> DengjenResult<AudioInfo>;
    fn phonemize_text(&self, text: &str) -> DengjenResult<Phonemes>;
    fn speak_batch(&self, phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>>;
    fn speak_one_sentence(&self, phonemes: String) -> DengjenAudioResult;

    fn get_default_synthesis_config(&self) -> DengjenResult<Box<dyn Any>>;
    fn get_fallback_synthesis_config(&self) -> DengjenResult<Box<dyn Any>>;
    fn set_fallback_synthesis_config(&self, synthesis_config: &dyn Any) -> DengjenResult<()>;

    fn get_language(&self) -> DengjenResult<Option<String>> {
        Ok(None)
    }
    fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
        Ok(None)
    }
    fn speaker_id_to_name(&self, sid: &i64) -> DengjenResult<Option<String>> {
        Ok(self
            .get_speakers()?
            .and_then(|speakers| speakers.get(sid))
            .cloned())
    }
    fn speaker_name_to_id(&self, name: &str) -> DengjenResult<Option<i64>> {
        Ok(self.get_speakers()?.and_then(|speakers| {
            for (sid, sname) in speakers {
                if sname == name {
                    return Some(*sid);
                }
            }
            None
        }))
    }
    fn properties(&self) -> DengjenResult<HashMap<String, String>> {
        Ok(HashMap::with_capacity(0))
    }

    fn supports_streaming_output(&self) -> bool {
        false
    }
    fn stream_synthesis(
        &self,
        #[allow(unused_variables)] phonemes: String,
        #[allow(unused_variables)] chunk_size: usize,
        #[allow(unused_variables)] chunk_padding: usize,
    ) -> DengjenResult<AudioStreamIterator<'_>> {
        Err(DengjenError::OperationError(
                "Streaming synthesis is not supported for this model".to_string(),
            ))
    }
}

