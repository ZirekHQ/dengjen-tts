use crate::config::{ModelConfig, PhonemeType};
use dengjen_core::{DengjenError, DengjenResult, Phonemes};

#[cfg(feature = "tashkeel")]
pub(crate) type TashkeelEngine = libtashkeel_core::DynamicInferenceEngine;
#[cfg(not(feature = "tashkeel"))]
pub(crate) type TashkeelEngine = ();

#[cfg(feature = "tashkeel")]
pub(crate) fn should_diacritize(voice: &str) -> bool {
    voice == "ar"
}
#[cfg(not(feature = "tashkeel"))]
#[cfg_attr(not(feature = "espeak"), allow(dead_code))]
pub(crate) fn should_diacritize(_voice: &str) -> bool {
    false
}

// Returns `None` when the caller should fall through to the espeak-based phonemization
// path; `Some(_)` when this phoneme_type is fully handled here.
pub(crate) fn phonemize_dispatch(
    phoneme_type: PhonemeType,
    text: &str,
) -> Option<DengjenResult<Phonemes>> {
    match phoneme_type {
        PhonemeType::Espeak => None,
        PhonemeType::Text => Some(Ok(vec![text.to_string()].into())),
        other => Some(Err(DengjenError::PhonemizationError(format!(
            "Phonemization for phoneme_type `{:?}` is not yet supported",
            other
        )))),
    }
}

#[cfg(feature = "tashkeel")]
pub(crate) fn create_tashkeel_engine(config: &ModelConfig) -> DengjenResult<Option<TashkeelEngine>> {
    if should_diacritize(&config.espeak.voice) {
        match libtashkeel_core::create_inference_engine(None) {
            Ok(engine) => Ok(Some(engine)),
            Err(msg) => Err(DengjenError::InferenceError(format!(
                "Failed to create inference engine for libtashkeel. {}",
                msg
            ))),
        }
    } else {
        Ok(None)
    }
}
#[cfg(not(feature = "tashkeel"))]
pub(crate) fn create_tashkeel_engine(
    _config: &ModelConfig,
) -> DengjenResult<Option<TashkeelEngine>> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phonemize_dispatch_falls_through_to_espeak_for_espeak_phoneme_type() {
        assert!(phonemize_dispatch(PhonemeType::Espeak, "hello").is_none());
    }

    #[test]
    fn phonemize_dispatch_passes_text_through_unchanged_for_text_phoneme_type() {
        let result = phonemize_dispatch(PhonemeType::Text, "hello").unwrap().unwrap();
        assert_eq!(result.sentences(), &vec!["hello".to_string()]);
    }

    #[test]
    fn phonemize_dispatch_errors_on_unsupported_pinyin_phoneme_type() {
        let result = phonemize_dispatch(PhonemeType::Pinyin, "hello").unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn phonemize_dispatch_errors_on_unsupported_hebrew_phoneme_type() {
        let result = phonemize_dispatch(PhonemeType::Hebrew, "hello").unwrap();
        assert!(result.is_err());
    }

    #[cfg(feature = "tashkeel")]
    #[test]
    fn should_diacritize_true_for_arabic_voice_when_tashkeel_enabled() {
        assert!(should_diacritize("ar"));
    }

    #[cfg(feature = "tashkeel")]
    #[test]
    fn should_diacritize_false_for_non_arabic_voice_when_tashkeel_enabled() {
        assert!(!should_diacritize("en-us"));
    }

    #[cfg(not(feature = "tashkeel"))]
    #[test]
    fn should_diacritize_always_false_when_tashkeel_disabled() {
        assert!(!should_diacritize("ar"));
        assert!(!should_diacritize("en-us"));
    }
}
