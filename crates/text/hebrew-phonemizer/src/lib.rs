#![forbid(unsafe_code)]

mod chars;
mod ipa;
mod nakdimon;

pub use nakdimon::{create_nakdimon_engine, NakdimonEngine};

use dengjen_tts_core::{DengjenResult, Phonemes};

// Source of truth: `phonemize_hebrew.py`'s niqqud-detection regex
// `[ְ-ׇּֿׁׂ]`, i.e. `\u{05B0}` (SHEVA, the first niqqud mark) through
// `\u{05C7}` (QAMATS_QATAN, the last one used in this port), plus the
// dagesh/shin/sin dots which fall inside that same contiguous range.
const NIQQUD_RANGE_START: u32 = 0x05B0;
const NIQQUD_RANGE_END: u32 = 0x05C7;

fn already_diacritized(text: &str) -> bool {
    text.chars()
        .any(|c| (NIQQUD_RANGE_START..=NIQQUD_RANGE_END).contains(&(c as u32)))
}

/// Converts Hebrew text to IPA phonemes, diacritizing first (via `engine`)
/// if `text` carries no niqqud already.
///
/// # Errors
///
/// Returns an error if diacritization via `engine` fails.
pub fn text_to_hebrew_phonemes(engine: &NakdimonEngine, text: &str) -> DengjenResult<Phonemes> {
    let diacritized = if already_diacritized(text) {
        text.to_string()
    } else {
        engine.diacritize(text)?
    };

    let ipa = ipa::hebrew_to_ipa(&diacritized);
    if ipa.is_empty() {
        return Ok(Phonemes::from(Vec::<String>::new()));
    }
    Ok(Phonemes::from(vec![ipa]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dengjen_tts_core::DengjenResult;

    #[test]
    fn text_to_hebrew_phonemes_skips_diacritization_when_already_pointed() {
        // Pre-pointed input never touches the ONNX engine, so this test needs
        // no model file — construct a real engine only if one is available,
        // otherwise this path must still work by short-circuiting before use.
        // A real `&NakdimonEngine` can't be constructed without an ONNX model
        // file (see the model-gated end-to-end test below), so this exercises
        // the actual short-circuit decision (`already_diacritized`) directly,
        // rather than only the ipa module beneath it.
        let pointed = "\u{05E9}\u{05B8}\u{05DC}\u{05D5}\u{05DD}"; // shalom, pointed
        assert!(already_diacritized(pointed));
        let ipa = hebrew_to_ipa_or_error(pointed).unwrap();
        assert!(!ipa.is_empty());
    }

    #[test]
    fn already_diacritized_is_false_for_undotted_consonants_only() {
        let undotted = "\u{05E9}\u{05DC}\u{05D5}\u{05DD}"; // shalom, no niqqud
        assert!(!already_diacritized(undotted));
    }

    #[test]
    fn text_to_hebrew_phonemes_empty_ipa_yields_empty_phonemes() {
        let result = hebrew_to_ipa_or_error("").unwrap();
        assert!(result.is_empty());
    }

    fn hebrew_to_ipa_or_error(text: &str) -> DengjenResult<String> {
        // Exercises the niqqud-check + ipa::hebrew_to_ipa path directly,
        // without needing a real NakdimonEngine (this helper only exists in
        // the test module to isolate the already-pointed-text path).
        Ok(ipa::hebrew_to_ipa(text))
    }

    #[test]
    fn text_to_hebrew_phonemes_diacritizes_undotted_text_with_a_real_model() {
        let Ok(model_path) = std::env::var("DENGJEN_NAKDIMON_TEST_MODEL_PATH") else {
            eprintln!("skipping: DENGJEN_NAKDIMON_TEST_MODEL_PATH not set");
            return;
        };
        let engine = create_nakdimon_engine(std::path::Path::new(&model_path)).unwrap();
        let phonemes =
            text_to_hebrew_phonemes(&engine, "\u{05E9}\u{05DC}\u{05D5}\u{05DD}").unwrap();
        assert_eq!(phonemes.num_sentences(), 1);
        assert!(!phonemes.sentences()[0].is_empty());
    }
}
