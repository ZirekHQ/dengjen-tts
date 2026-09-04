#![no_main]

use libfuzzer_sys::fuzz_target;
use pinyin_phonemizer::normalize_numbers;






fuzz_target!(|text: String| {
    let _ = normalize_numbers(&text);
});
