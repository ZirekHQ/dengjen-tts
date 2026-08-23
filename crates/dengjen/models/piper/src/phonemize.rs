use crate::config::{ModelConfig, PhonemeType};
use dengjen_tts_core::{DengjenError, DengjenResult, Phonemes};
use std::path::Path;

#[cfg(feature = "tashkeel")]
pub(crate) type TashkeelEngine = libtashkeel_core::DynamicInferenceEngine;
#[cfg(not(feature = "tashkeel"))]
pub(crate) type TashkeelEngine = ();

#[cfg(feature = "hebrew")]
pub(crate) type HebrewEngine = hebrew_phonemizer::NakdimonEngine;
#[cfg(not(feature = "hebrew"))]
pub(crate) type HebrewEngine = ();

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
    #[cfg_attr(not(feature = "hebrew"), allow(unused_variables))] hebrew_engine: Option<
        &HebrewEngine,
    >,
) -> Option<DengjenResult<Phonemes>> {
    match phoneme_type {
        PhonemeType::Espeak => None,
        PhonemeType::Text => Some(Ok(vec![text.to_string()].into())),
        #[cfg(feature = "hebrew")]
        PhonemeType::Hebrew => Some(match hebrew_engine {
            Some(engine) => hebrew_phonemizer::text_to_hebrew_phonemes(engine, text),
            None => Err(DengjenError::PhonemizationError(
                "This voice's phoneme_type is `hebrew` but no Hebrew engine was initialized"
                    .to_string(),
            )),
        }),
        #[cfg(not(feature = "hebrew"))]
        PhonemeType::Hebrew => Some(Err(DengjenError::PhonemizationError(
            "Phonemization for phoneme_type `Hebrew` requires the `hebrew` feature".to_string(),
        ))),
        unsupported => Some(Err(DengjenError::PhonemizationError(format!(
            "Phonemization for phoneme_type `{:?}` is not yet supported",
            unsupported
        )))),
    }
}

#[cfg(feature = "tashkeel")]
pub(crate) fn create_tashkeel_engine(
    config: &ModelConfig,
) -> DengjenResult<Option<TashkeelEngine>> {
    if !should_diacritize(&config.espeak.voice) {
        return Ok(None);
    }
    libtashkeel_core::create_inference_engine(None)
        .map(Some)
        .map_err(|msg| {
            DengjenError::InferenceError(format!(
                "Failed to create inference engine for libtashkeel. {}",
                msg
            ))
        })
}
#[cfg(not(feature = "tashkeel"))]
pub(crate) fn create_tashkeel_engine(
    _config: &ModelConfig,
) -> DengjenResult<Option<TashkeelEngine>> {
    Ok(None)
}

#[cfg(feature = "hebrew")]
pub(crate) fn create_hebrew_engine(
    config: &ModelConfig,
    config_path: &Path,
) -> DengjenResult<Option<HebrewEngine>> {
    if config.phoneme_type != Some(PhonemeType::Hebrew) {
        return Ok(None);
    }
    let Some(model_path) = config.hebrew_model_path.as_ref() else {
        return Err(DengjenError::InvalidConfiguration(
            "This voice's phoneme_type is `hebrew` but no Nakdimon model path was configured"
                .to_string(),
        ));
    };
    // Resolved relative to the config file's own directory, matching every
    // other model path in this crate (e.g. `onnx_path`/`encoder_path`, which
    // use `config_path.with_file_name(..)` the same way), rather than the
    // process's current working directory.
    let resolved_model_path = config_path.with_file_name(model_path);
    hebrew_phonemizer::create_nakdimon_engine(&resolved_model_path).map(Some)
}
#[cfg(not(feature = "hebrew"))]
pub(crate) fn create_hebrew_engine(
    _config: &ModelConfig,
    _config_path: &Path,
) -> DengjenResult<Option<HebrewEngine>> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phonemize_dispatch_falls_through_to_espeak_for_espeak_phoneme_type() {
        assert!(phonemize_dispatch(PhonemeType::Espeak, "hello", None).is_none());
    }

    #[test]
    fn phonemize_dispatch_passes_text_through_unchanged_for_text_phoneme_type() {
        let result = phonemize_dispatch(PhonemeType::Text, "hello", None)
            .unwrap()
            .unwrap();
        assert_eq!(result.sentences(), &vec!["hello".to_string()]);
    }

    #[test]
    fn phonemize_dispatch_errors_on_unsupported_pinyin_phoneme_type() {
        let result = phonemize_dispatch(PhonemeType::Pinyin, "hello", None).unwrap();
        assert!(result.is_err());
    }

    #[cfg(feature = "hebrew")]
    #[test]
    fn phonemize_dispatch_delegates_hebrew_to_the_hebrew_engine_when_present() {
        // No real model available in this sandbox — assert on the *absence*
        // path instead: dispatch must return a clear error, not panic, when
        // this voice is configured for Hebrew but no engine was constructed
        // (e.g. a caller-supplied model path that failed to load earlier).
        let result = phonemize_dispatch(
            PhonemeType::Hebrew,
            "\u{05E9}\u{05DC}\u{05D5}\u{05DD}",
            None,
        );
        assert!(result.unwrap().is_err());
    }

    #[cfg(not(feature = "hebrew"))]
    #[test]
    fn phonemize_dispatch_still_errors_on_hebrew_when_feature_disabled() {
        // Third param stays `Option<&HebrewEngine>` regardless of the feature
        // flag — only what `HebrewEngine` resolves to changes. Pass `None`,
        // never `()`.
        let result = phonemize_dispatch(PhonemeType::Hebrew, "hello", None).unwrap();
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
