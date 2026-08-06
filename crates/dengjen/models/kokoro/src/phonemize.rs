use dengjen_core::{DengjenError, DengjenResult};

/// Ordered longest-pattern-first. Each entry is (espeak IPA substring, Kokoro phoneme symbol).
#[cfg_attr(not(feature = "espeak"), allow(dead_code))]
const SUBSTITUTIONS: &[(&str, &str)] = &[
    ("aɪ", "I"),
    ("aʊ", "W"),
    ("dʒ", "ʤ"),
    ("eɪ", "A"),
    ("tʃ", "ʧ"),
    ("ɔɪ", "Y"),
    ("oʊ", "O"),
    ("ɚ", "əɹ"),
    ("r", "ɹ"),
    ("x", "k"),
    ("ç", "k"),
    ("ɐ", "ə"),
    ("ɬ", "l"),
    ("ʔ", "t"),
    ("n\u{0329}", "ᵊn"),
    ("ʲ", ""),
    ("ː", ""),
];

#[cfg_attr(not(feature = "espeak"), allow(dead_code))]
fn espeak_ipa_to_kokoro(ipa: &str) -> String {
    let mut result = ipa.to_string();
    for (from, to) in SUBSTITUTIONS {
        result = result.replace(from, to);
    }
    result
}

#[cfg(feature = "espeak")]
pub fn text_to_kokoro_phonemes(text: &str, language: &str) -> DengjenResult<Vec<String>> {
    let sentences = espeak_phonemizer::text_to_phonemes(text, language, None, true, false)
        .map_err(|e| DengjenError::PhonemizationError(e.to_string()))?;
    Ok(sentences.iter().map(|s| espeak_ipa_to_kokoro(s)).collect())
}

#[cfg(not(feature = "espeak"))]
pub fn text_to_kokoro_phonemes(_text: &str, _language: &str) -> DengjenResult<Vec<String>> {
    Err(DengjenError::PhonemizationError(
        "Kokoro phonemization requires the `espeak` feature (GPL-3.0-or-later, via espeak-ng), but it is disabled".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected raw IPA values below were captured by actually running this repo's
    // vendored espeak-ng (via espeak_phonemizer::text_to_phonemes) during planning,
    // not invented - see plan Task 3 for how to reproduce.
    #[test]
    fn espeak_ipa_to_kokoro_composes_ai_diphthong() {
        // espeak IPA for "time" is "tˈaɪm" (verified against real espeak-ng)
        assert_eq!(espeak_ipa_to_kokoro("tˈaɪm"), "tˈIm");
    }

    #[test]
    fn espeak_ipa_to_kokoro_composes_dz_affricate() {
        // espeak IPA for "job" is "dʒˈɑːb" (verified against real espeak-ng);
        // the length mark on ɑː is also stripped.
        assert_eq!(espeak_ipa_to_kokoro("dʒˈɑːb"), "ʤˈɑb");
    }

    #[test]
    fn espeak_ipa_to_kokoro_composes_oi_diphthong() {
        // espeak IPA for "toy" is "tˈɔɪ" (verified against real espeak-ng)
        assert_eq!(espeak_ipa_to_kokoro("tˈɔɪ"), "tˈY");
    }

    #[test]
    fn espeak_ipa_to_kokoro_composes_au_diphthong() {
        // espeak IPA for "house" is "hˈaʊs" (verified against real espeak-ng)
        assert_eq!(espeak_ipa_to_kokoro("hˈaʊs"), "hˈWs");
    }

    #[test]
    fn espeak_ipa_to_kokoro_leaves_plain_phonemes_unchanged() {
        // espeak IPA for "test" is "tˈɛst" (verified against real espeak-ng) - no
        // diphthongs/affricates/length-marks present, so nothing should change.
        assert_eq!(espeak_ipa_to_kokoro("tˈɛst"), "tˈɛst");
    }

    // espeak-ng keeps the selected voice in process-global state, so concurrent
    // tests calling it clobber each other's voice selection.
    #[cfg(feature = "espeak")]
    static ESPEAK_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(feature = "espeak")]
    fn lock_espeak() -> std::sync::MutexGuard<'static, ()> {
        ESPEAK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `None` means espeak-ng has no data directory here, so the caller must skip
    /// rather than fail - any other error is a real failure and panics.
    #[cfg(feature = "espeak")]
    fn phonemize_or_skip(text: &str, language: &str) -> Option<Vec<String>> {
        match text_to_kokoro_phonemes(text, language) {
            Ok(sentences) => Some(sentences),
            Err(DengjenError::PhonemizationError(msg))
                if msg.contains("Failed to initialize eSpeak-ng") =>
            {
                eprintln!(
                    "Skipping: no espeak-ng data available. Set DENGJEN_ESPEAKNG_DATA_DIRECTORY to the directory containing `espeak-ng-data`."
                );
                None
            }
            Err(e) => panic!("phonemization failed unexpectedly: {e}"),
        }
    }

    #[cfg(feature = "espeak")]
    #[test]
    fn text_to_kokoro_phonemes_returns_error_for_unset_voice() {
        let _guard = lock_espeak();
        // An unrecognized espeak-ng language code should surface as a
        // PhonemizationError, not panic.
        let result = text_to_kokoro_phonemes("hello", "not-a-real-language-code");
        assert!(matches!(result, Err(DengjenError::PhonemizationError(_))));
    }

    #[cfg(feature = "espeak")]
    #[test]
    fn text_to_kokoro_phonemes_returns_one_entry_per_sentence() {
        let _guard = lock_espeak();
        // Real espeak-ng output for this input, observed while writing the test:
        // ["həlˈO ðˈɛɹ.", "ʤˈɛnəɹɹəl kɛnˈObI."] - two entries, one per sentence.
        let Some(result) = phonemize_or_skip("Hello there. General Kenobi.", "en-US") else {
            return;
        };
        assert_eq!(result.len(), 2, "expected one entry per sentence, got {result:?}");
        assert!(result.iter().all(|s| !s.trim().is_empty()));
    }

    #[cfg(feature = "espeak")]
    #[test]
    fn text_to_kokoro_phonemes_strips_language_switch_flags() {
        let _guard = lock_espeak();
        // Same mixed-script input as espeak-phonemizer's own test_lang_switch_flags,
        // which emits `(en)`/`(ar)` switch markers; they must be stripped before
        // reaching the tokenizer, where they would become audible garbage. Asserting
        // on the parentheses rather than the literal flags, since the IPA-to-Kokoro
        // substitutions rewrite the letters inside them.
        let Some(result) = phonemize_or_skip("Hello معناها مرحباً", "ar") else {
            return;
        };
        let joined = result.join("");
        assert!(!joined.contains('('), "unstripped lang-switch flag in {joined:?}");
    }

    #[cfg(not(feature = "espeak"))]
    #[test]
    fn text_to_kokoro_phonemes_returns_error_when_espeak_disabled() {
        // With the `espeak` feature off, the crate must still compile and fail
        // cleanly at call time rather than being unable to build at all.
        let result = text_to_kokoro_phonemes("hello", "en-US");
        assert!(matches!(result, Err(DengjenError::PhonemizationError(_))));
    }

    #[test]
    fn espeak_ipa_to_kokoro_composes_syllabic_consonant() {
        // espeak IPA for "button" is "bˈʌʔn\u{0329}." (verified against real
        // espeak-ng) - the trailing combining U+0329 marks the syllabic nasal;
        // it composes with the preceding "n" into Kokoro's "ᵊn" convention, and
        // the glottal stop is also mapped to "t" by the existing rule.
        assert_eq!(espeak_ipa_to_kokoro("bˈʌʔn\u{0329}."), "bˈʌtᵊn.");
    }
}
