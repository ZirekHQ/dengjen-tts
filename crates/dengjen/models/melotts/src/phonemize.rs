#![allow(dead_code)]

use dengjen_tts_core::{DengjenError, DengjenResult};

#[cfg(feature = "espeak")]
pub(crate) fn espeak_phone_tone_pairs(
    text: &str,
    voice: &str,
) -> DengjenResult<Vec<Vec<(String, String)>>> {
    let sentences = espeak_phonemizer::text_to_phonemes(text, voice, None, true, false)
        .map_err(|e| DengjenError::PhonemizationError(e.to_string()))?;
    Ok(sentences
        .into_iter()
        .map(|sentence| vec![(sentence, "_".to_string())])
        .collect())
}

#[cfg(not(feature = "espeak"))]
pub(crate) fn espeak_phone_tone_pairs(
    _text: &str,
    _voice: &str,
) -> DengjenResult<Vec<Vec<(String, String)>>> {
    Err(DengjenError::PhonemizationError(
        "MeloTTS espeak phonemization requires the `espeak` feature (GPL-3.0-or-later, via espeak-ng), but it is disabled".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "espeak")]
    static ESPEAK_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(feature = "espeak")]
    fn lock_espeak() -> std::sync::MutexGuard<'static, ()> {
        ESPEAK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[cfg(feature = "espeak")]
    fn phonemize_or_skip(text: &str, voice: &str) -> Option<Vec<Vec<(String, String)>>> {
        match espeak_phone_tone_pairs(text, voice) {
            Ok(pairs) => Some(pairs),
            Err(DengjenError::PhonemizationError(msg))
                if msg.contains("Failed to initialize eSpeak-ng") =>
            {
                eprintln!(
                    "Skipping: no espeak-ng data available. Set DENGJEN_ESPEAKNG_DATA_DIRECTORY."
                );
                None
            }
            Err(e) => panic!("phonemization failed unexpectedly: {e}"),
        }
    }

    #[cfg(feature = "espeak")]
    #[test]
    fn espeak_phone_tone_pairs_returns_one_sentence_with_the_no_tone_sentinel() {
        let _guard = lock_espeak();
        let Some(result) = phonemize_or_skip("Hello there.", "en-US") else {
            return;
        };
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 1);
        assert_eq!(result[0][0].1, "_");
        assert!(!result[0][0].0.is_empty());
    }

    #[cfg(feature = "espeak")]
    #[test]
    fn espeak_phone_tone_pairs_errors_for_an_unrecognized_voice() {
        let _guard = lock_espeak();
        let result = espeak_phone_tone_pairs("hello", "not-a-real-language-code");
        assert!(matches!(result, Err(DengjenError::PhonemizationError(_))));
    }

    #[cfg(not(feature = "espeak"))]
    #[test]
    fn espeak_phone_tone_pairs_errors_cleanly_when_espeak_disabled() {
        let result = espeak_phone_tone_pairs("hello", "en-US");
        assert!(matches!(result, Err(DengjenError::PhonemizationError(_))));
    }
}
