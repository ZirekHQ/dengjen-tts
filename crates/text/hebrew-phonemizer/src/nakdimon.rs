use crate::chars::{char_to_id_map, dagesh_classes, niqqud_classes, normalize, sin_classes, RAFE};
use dengjen_tts_core::{DengjenError, DengjenResult};
use ndarray::Array2;
use ort::session::Session;
use ort::value::{Shape, Tensor};
use std::path::Path;
use std::sync::Mutex;

fn can_dagesh(letter: char) -> bool {
    "\u{05D1}\u{05D2}\u{05D3}\u{05D4}\u{05D5}\u{05D6}\u{05D8}\u{05D9}\u{05DB}\u{05DC}\u{05DE}\u{05E0}\u{05E1}\u{05E4}\u{05E6}\u{05E7}\u{05E9}\u{05EA}\u{05DA}\u{05E3}"
        .contains(letter)
}

fn can_sin(letter: char) -> bool {
    letter == '\u{05E9}'
}

fn can_niqqud(letter: char) -> bool {
    "\u{05D0}\u{05D1}\u{05D2}\u{05D3}\u{05D4}\u{05D5}\u{05D6}\u{05D7}\u{05D8}\u{05D9}\u{05DB}\u{05DC}\u{05DE}\u{05E0}\u{05E1}\u{05E2}\u{05E4}\u{05E6}\u{05E7}\u{05E8}\u{05E9}\u{05EA}\u{05DA}\u{05DF}"
        .contains(letter)
}

/// Rebuilds diacritized text from per-position class ids. Model class id 0 is
/// the mask token ("" upstream — emit nothing); id N (N>=1) is
/// `*_classes()[N - 1]`. Do not prepend another RAFE here —
/// `niqqud_classes()`/`dagesh_classes()`/`sin_classes()` already start with
/// their own real RAFE entry at index 0 of the *class* list, which is
/// distinct from the mask id.
fn merge_diacritics(
    letters: &[char],
    niqqud_ids: &[usize],
    dagesh_ids: &[usize],
    sin_ids: &[usize],
) -> String {
    let niqqud_chars = niqqud_classes();
    let dagesh_chars = dagesh_classes();
    let sin_chars = sin_classes();

    let mut out = String::new();
    for (i, &letter) in letters.iter().enumerate() {
        out.push(letter);
        if can_dagesh(letter) && dagesh_ids[i] > 0 {
            out.push(dagesh_chars[dagesh_ids[i] - 1]);
        }
        if can_sin(letter) && sin_ids[i] > 0 {
            out.push(sin_chars[sin_ids[i] - 1]);
        }
        if can_niqqud(letter) && niqqud_ids[i] > 0 {
            out.push(niqqud_chars[niqqud_ids[i] - 1]);
        }
    }
    out.replace(RAFE, "")
}

/// A Nakdimon niqqud-restoration ONNX session. `Session::run` requires `&mut
/// self`, so the session is behind a `Mutex` to keep `diacritize` callable
/// from `&self` (matching the other inference engines in this workspace).
pub struct NakdimonEngine(Mutex<Session>);

/// Loads a Nakdimon ONNX model from `model_path` for niqqud restoration.
pub fn create_nakdimon_engine(model_path: &Path) -> DengjenResult<NakdimonEngine> {
    let session = Session::builder()
        .map_err(session_init_error)?
        .commit_from_file(model_path)
        .map_err(session_init_error)?;
    Ok(NakdimonEngine(Mutex::new(session)))
}

fn session_init_error(cause: ort::Error) -> DengjenError {
    DengjenError::InferenceError(format!(
        "Failed to initialize the Nakdimon inference session: {cause}"
    ))
}

fn inference_error(cause: impl std::fmt::Display) -> DengjenError {
    DengjenError::InferenceError(format!("Nakdimon inference failed: {cause}"))
}

/// Validates a `(1, seq_len, expected_classes)` output shape against the
/// actual input `seq_len` and the calling table's class count, erroring
/// instead of panicking if the model produced a tensor of an unexpected
/// rank, a batch size other than 1, a sequence length that disagrees with
/// the input we gave it, or a class-count axis that doesn't match
/// `expected_classes` (the corresponding `*_classes()` table's length, plus
/// one for the mask token). Checked in that order: rank first, since a
/// wrong-rank tensor makes every later index meaningless to report on.
///
/// Tying the validated class count to `expected_classes` up front is what
/// keeps `merge_diacritics`'s `table[id - 1]` indexing in bounds: every id
/// `argmax_per_position` can produce is `< expected_classes`, so `id - 1` is
/// always a valid index into a table of length `expected_classes - 1`.
pub fn num_classes(shape: &Shape, seq_len: usize, expected_classes: usize) -> DengjenResult<usize> {
    if shape.len() != 3 {
        return Err(inference_error(format!(
            "expected a rank-3 output tensor, got shape {shape:?}"
        )));
    }
    let Some(&num_classes) = shape.get(2) else {
        return Err(inference_error(format!(
            "expected a rank-3 output tensor, got shape {shape:?}"
        )));
    };
    if shape.first() != Some(&1) {
        return Err(inference_error(format!(
            "expected batch size 1, model produced shape {shape:?}"
        )));
    }
    if shape.get(1) != Some(&(seq_len as i64)) {
        return Err(inference_error(format!(
            "expected output sequence length {seq_len}, model produced shape {shape:?}"
        )));
    }
    // ort::value::Shape stores dimensions as i64 and permits -1 to mark a dynamic dimension;
    // `as usize` would wrap a negative value instead of rejecting it, and could then compare
    // equal to a caller-supplied expected_classes of usize::MAX.
    let num_classes = usize::try_from(num_classes).map_err(|_| {
        inference_error(format!(
            "expected a non-negative class dimension, model produced shape {shape:?}"
        ))
    })?;
    if num_classes != expected_classes {
        return Err(inference_error(format!(
            "expected {expected_classes} classes, model produced shape {shape:?}"
        )));
    }
    Ok(num_classes)
}

/// Argmaxes the last axis of a flattened `(seq_len, num_classes)` logits
/// buffer, returning one class id per sequence position.
fn argmax_per_position(data: &[f32], seq_len: usize, num_classes: usize) -> Vec<usize> {
    (0..seq_len)
        .map(|pos| {
            let row = &data[pos * num_classes..(pos + 1) * num_classes];
            row.iter()
                .enumerate()
                // Ties break toward the LAST max value here, vs. upstream's
                // `np.argmax` (first); accepted as unlikely to matter for
                // real float32 model logits.
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(idx, _)| idx)
                .unwrap_or(0)
        })
        .collect()
}

/// Strips any pre-existing niqqud/cantillation marks so the model always
/// sees bare consonants, matching upstream's `remove_niqqud`, which its own
/// `diacritize` calls first for exactly this reason. Without this, points
/// already present in the input would fall through `normalize()`'s
/// unknown-character fallback (`'O'`) instead of being recognized, silently
/// degrading model input rather than erroring.
fn remove_niqqud(text: &str) -> String {
    text.chars()
        .filter(|&c| !(0x05B0..=0x05C7).contains(&(c as u32)))
        .collect()
}

impl NakdimonEngine {
    /// Restores niqqud (vowel points) in `text` using the loaded Nakdimon model.
    pub fn diacritize(&self, text: &str) -> DengjenResult<String> {
        let text = remove_niqqud(text);
        let letters: Vec<char> = text.chars().collect();
        if letters.is_empty() {
            return Ok(text);
        }
        let char_ids = char_to_id_map();
        let input_ids: Vec<f32> = letters
            .iter()
            .map(|&c| {
                let normalized = normalize(c);
                *char_ids.get(&normalized).unwrap_or(&0) as f32
            })
            .collect();
        let seq_len = letters.len();
        let input =
            Array2::<f32>::from_shape_vec((1, seq_len), input_ids).map_err(inference_error)?;

        let mut session = self.0.lock().map_err(inference_error)?;
        let outputs = session
            .run(ort::inputs![
                Tensor::from_array(input).map_err(inference_error)?
            ])
            .map_err(inference_error)?;
        if outputs.len() < 3 {
            return Err(inference_error(format!(
                "expected 3 output tensors (niqqud, dagesh, sin), model produced {}",
                outputs.len()
            )));
        }

        let (n_shape, n_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(inference_error)?;
        let (d_shape, d_data) = outputs[1]
            .try_extract_tensor::<f32>()
            .map_err(inference_error)?;
        let (s_shape, s_data) = outputs[2]
            .try_extract_tensor::<f32>()
            .map_err(inference_error)?;

        let niqqud_ids = argmax_per_position(
            n_data,
            seq_len,
            num_classes(n_shape, seq_len, niqqud_classes().len() + 1)?,
        );
        let dagesh_ids = argmax_per_position(
            d_data,
            seq_len,
            num_classes(d_shape, seq_len, dagesh_classes().len() + 1)?,
        );
        let sin_ids = argmax_per_position(
            s_data,
            seq_len,
            num_classes(s_shape, seq_len, sin_classes().len() + 1)?,
        );

        Ok(merge_diacritics(
            &letters,
            &niqqud_ids,
            &dagesh_ids,
            &sin_ids,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_diacritics_appends_dagesh_sin_niqqud_in_that_order() {
        // "ב" (bet) with dagesh class 1 (DAGESH_LETTER) and niqqud class for
        // PATAH — expect dagesh mark, then niqqud mark, appended after the
        // letter (bet cannot take a sin dot, so sin class must be ignored).
        let letters = vec!['\u{05D1}']; // bet
        let niqqud_ids = vec![find_niqqud_class_id('\u{05B7}')]; // patah
        let dagesh_ids = dagesh_id_for_dagesh_letter();
        let sin_ids = vec![2usize]; // would be sin-dot if bet could take one
        let merged = merge_diacritics(&letters, &niqqud_ids, &dagesh_ids, &sin_ids);
        assert_eq!(merged, "\u{05D1}\u{05BC}\u{05B7}");
    }

    #[test]
    fn merge_diacritics_skips_niqqud_for_letters_that_cannot_take_it() {
        // Latin/space characters are not in `can_niqqud`'s letter set.
        let letters = vec![' '];
        let merged = merge_diacritics(&letters, &[0], &[0], &[0]);
        assert_eq!(merged, " ");
    }

    fn find_niqqud_class_id(target: char) -> usize {
        crate::chars::niqqud_classes()
            .iter()
            .position(|&c| c == target)
            .expect("target must be a real niqqud class")
            + 1 // +1 for the prepended mask token, matching _NIQQUD_CHARS
    }

    // dagesh_ids uses the same "+1 for the prepended mask token" convention as
    // niqqud_ids above (verified against upstream's `_DAGESH_CHARS = [""] +
    // DAGESH`, where `DAGESH = [RAFE, DAGESH_LETTER]`): model id 2 maps to
    // `dagesh_classes()[2 - 1]` == DAGESH_LETTER.
    fn dagesh_id_for_dagesh_letter() -> Vec<usize> {
        vec![2]
    }

    #[test]
    fn remove_niqqud_strips_points_and_leaves_base_letters_untouched() {
        // shalom fully pointed: shin-yemanit + patah, lamed + holam, vav, mem-sofit + qamats
        let pointed = "\u{05E9}\u{05C1}\u{05B7}\u{05DC}\u{05B9}\u{05D5}\u{05DD}\u{05B8}";
        assert_eq!(remove_niqqud(pointed), "\u{05E9}\u{05DC}\u{05D5}\u{05DD}");
    }

    #[test]
    fn num_classes_accepts_the_expected_model_output_shape() {
        let shape = Shape::new(vec![1, 4, 7]);

        assert_eq!(num_classes(&shape, 4, 7).unwrap(), 7);
    }

    #[test]
    fn num_classes_rejects_wrong_rank_batch_sequence_and_class_dimensions() {
        let cases = [
            (Shape::new(vec![4, 7]), 4, 7),
            (Shape::new(vec![2, 4, 7]), 4, 7),
            (Shape::new(vec![1, 3, 7]), 4, 7),
            (Shape::new(vec![1, 4, 6]), 4, 7),
            // Rank 4, but axes 0-2 happen to match batch/seq/class: `shape.get(2)` alone would
            // have silently accepted this and returned a plausible-looking wrong class count.
            (Shape::new(vec![1, 4, 7, 1]), 4, 7),
        ];

        for (shape, seq_len, expected_classes) in cases {
            assert!(num_classes(&shape, seq_len, expected_classes).is_err());
        }
    }

    #[test]
    fn num_classes_rejects_a_negative_class_dimension_instead_of_wrapping_to_usize_max() {
        // ort::value::Shape permits -1 to mark a dynamic dimension. `as usize` would wrap -1
        // to usize::MAX, which then compares equal to an expected_classes of usize::MAX --
        // exactly the value this regression case supplies.
        let shape = Shape::new(vec![1, 4, -1]);
        assert!(num_classes(&shape, 4, usize::MAX).is_err());
    }

    #[test]
    fn diacritize_restores_niqqud_with_a_real_model() {
        let Ok(model_path) = std::env::var("DENGJEN_NAKDIMON_TEST_MODEL_PATH") else {
            eprintln!("skipping: DENGJEN_NAKDIMON_TEST_MODEL_PATH not set");
            return;
        };
        let engine = create_nakdimon_engine(std::path::Path::new(&model_path)).unwrap();
        let result = engine.diacritize("שלום").unwrap();
        assert_ne!(result, "שלום", "expected niqqud marks to be added");
    }
}
