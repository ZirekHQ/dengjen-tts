#![no_main]

use arbitrary::Arbitrary;
use hebrew_phonemizer::num_classes;
use libfuzzer_sys::fuzz_target;
use ort::value::Shape;






#[derive(Debug, Arbitrary)]
struct Input {
    dims: Vec<i64>,
    seq_len: usize,
    expected_classes: usize,
}

fuzz_target!(|input: Input| {
    let shape = Shape::new(input.dims);
    let _ = num_classes(&shape, input.seq_len, input.expected_classes);
});
