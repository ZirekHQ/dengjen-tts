#![no_main]

use libfuzzer_sys::fuzz_target;
use pinyin_phonemizer::normalize_numbers;

// `text` models arbitrary model-generated or attacker-controlled input text reaching the
// phonemization pipeline before any digit/percentage/temperature normalization runs. Neither
// validated nor length-bounded upstream, so this target's only assertion is "never panics" --
// a crash here is a real DoS on the synthesis pipeline, same framing as
// dengjen-tts-piper-fuzz's map_phonemes_to_ids target.
fuzz_target!(|text: String| {
    let _ = normalize_numbers(&text);
});
