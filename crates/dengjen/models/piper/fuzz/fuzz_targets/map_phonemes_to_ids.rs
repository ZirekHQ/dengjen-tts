#![no_main]

use arbitrary::Arbitrary;
use dengjen_tts_piper::map_phonemes_to_ids;
use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;





#[derive(Debug, Arbitrary)]
struct Input {
    phoneme_id_map: HashMap<String, Vec<i64>>,
    phonemes: String,
    pad_id: i64,
    bos_id: i64,
    eos_id: i64,
}

fuzz_target!(|input: Input| {
    let _ = map_phonemes_to_ids(
        &input.phoneme_id_map,
        &input.phonemes,
        input.pad_id,
        input.bos_id,
        input.eos_id,
    );
});
