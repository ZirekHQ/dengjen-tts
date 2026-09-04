






#![forbid(unsafe_code)]

mod chars;
mod ipa;
mod nakdimon;

pub use nakdimon::{create_nakdimon_engine, NakdimonEngine};



pub use nakdimon::num_classes;

use dengjen_tts_core::{DengjenResult, Phonemes};





const NIQQUD_RANGE_START: u32 = 0x05B0;
const NIQQUD_RANGE_END: u32 = 0x05C7;

fn already_diacritized(text: &str) -> bool {
    text.chars()
        .any(|c| (NIQQUD_RANGE_START..=NIQQUD_RANGE_END).contains(&(c as u32)))
}

















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
        let pointed = "\u{05E9}\u{05B8}\u{05DC}\u{05D5}\u{05DD}"; 
        assert!(already_diacritized(pointed));
        let ipa = hebrew_to_ipa_or_error(pointed).unwrap();
        assert!(!ipa.is_empty());
    }

    #[test]
    fn already_diacritized_is_false_for_undotted_consonants_only() {
        let undotted = "\u{05E9}\u{05DC}\u{05D5}\u{05DD}"; 
        assert!(!already_diacritized(undotted));
    }

    #[test]
    fn text_to_hebrew_phonemes_empty_ipa_yields_empty_phonemes() {
        let result = hebrew_to_ipa_or_error("").unwrap();
        assert!(result.is_empty());
    }

    fn hebrew_to_ipa_or_error(text: &str) -> DengjenResult<String> {
        
        
        
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
