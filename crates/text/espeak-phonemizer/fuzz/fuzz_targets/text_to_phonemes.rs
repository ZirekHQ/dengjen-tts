#![no_main]

use arbitrary::Arbitrary;
use dengjen_espeak_phonemizer::text_to_phonemes;
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Input {
    text: String,
    language: String,
    phoneme_separator: Option<char>,
    remove_lang_switch_flags: bool,
    remove_stress: bool,
}

fuzz_target!(|input: Input| {
    let _ = text_to_phonemes(
        &input.text,
        &input.language,
        input.phoneme_separator,
        input.remove_lang_switch_flags,
        input.remove_stress,
    );
});
