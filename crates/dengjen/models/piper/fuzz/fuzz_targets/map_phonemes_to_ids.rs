#![no_main]

use arbitrary::Arbitrary;
use dengjen_tts_piper::map_phonemes_to_ids;
use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;

// `phoneme_id_map` models an untrusted, on-disk voice config's `phoneme_id_map` field;
// `phonemes` models arbitrary model-generated or attacker-controlled text. Neither is
// validated before reaching `map_phonemes_to_ids` in production, so this target's only
// assertion is "never panics" — a crash here is a real DoS on the synthesis pipeline.
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
