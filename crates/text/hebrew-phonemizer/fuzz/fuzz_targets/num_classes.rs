#![no_main]

use arbitrary::Arbitrary;
use hebrew_phonemizer::num_classes;
use libfuzzer_sys::fuzz_target;
use ort::value::Shape;

// `shape` models an ONNX model's output tensor shape -- controlled by whichever .onnx file the
// voice config points at, not guaranteed by this crate. `seq_len`/`expected_classes` model the
// caller's own expectations. This target's only assertion is "never panics": num_classes exists
// specifically to reject a malformed shape with an error instead of letting a caller index into
// it, so a crash here means that guarantee itself is broken.
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
