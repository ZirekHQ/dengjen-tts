use dengjen_core::{DengjenError, DengjenResult};

/// Ordered longest-pattern-first. Each entry is (espeak IPA substring, Kokoro phoneme symbol).
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

fn espeak_ipa_to_kokoro(ipa: &str) -> String {
    let mut result = ipa.to_string();
    for (from, to) in SUBSTITUTIONS {
        result = result.replace(from, to);
    }
    result
}

pub fn text_to_kokoro_phonemes(text: &str, language: &str) -> DengjenResult<String> {
    let sentences = espeak_phonemizer::text_to_phonemes(text, language, None, false, false)
        .map_err(|e| DengjenError::PhonemizationError(e.to_string()))?;
    Ok(sentences
        .iter()
        .map(|s| espeak_ipa_to_kokoro(s))
        .collect::<Vec<_>>()
        .join(" "))
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

    #[test]
    fn text_to_kokoro_phonemes_returns_error_for_unset_voice() {
        // An unrecognized espeak-ng language code should surface as a
        // PhonemizationError, not panic.
        let result = text_to_kokoro_phonemes("hello", "not-a-real-language-code");
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
